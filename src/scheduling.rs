use super::hooks::{self, HookContext, HookEvent};
use super::persistence::{PersistenceError, Repository};
use super::{Annotation, InterruptLog, InterruptionKind, Kind, Schedulable, SqlUuid};
use indicatif::{ProgressBar, ProgressStyle};
use std::fmt;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::mpsc::channel;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{thread, time::Duration, time::Instant};

static CTRLC_INIT: Once = Once::new();
static CTRLC_PRESSED: AtomicBool = AtomicBool::new(false);

/// Install the single Ctrl-C handler for the process lifetime.
fn init_ctrlc_handler() {
    CTRLC_INIT.call_once(|| {
        ctrlc::set_handler(|| {
            CTRLC_PRESSED.store(true, Ordering::SeqCst);
        })
        .expect("Error setting Ctrl-C handler");
    });
}

pub struct Scheduler {
    repo: Repository,
    root: PathBuf,
    verbose: bool,
    no_hooks: bool,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum SchedulingError {
    ExecutionError,
    AlreadyRunning(u32),
    HookRejected,
    NoActiveSchedulable,
    NothingToAnnotate,
    NothingToCancel,
    CannotResolveTarget(String),
}

impl fmt::Display for SchedulingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchedulingError::ExecutionError => write!(f, "cannot execute schedulable"),
            SchedulingError::AlreadyRunning(pid) => {
                write!(
                    f,
                    "another pomodoro or break is already running (pid {}). Wait for it to end, cancel it, or use --force",
                    pid
                )
            }
            SchedulingError::HookRejected => {
                write!(f, "a before-hook rejected the operation")
            }
            SchedulingError::NoActiveSchedulable => {
                write!(f, "nothing active to interrupt")
            }

            SchedulingError::NothingToAnnotate => {
                write!(f, "nothing active or previously done to annotate")
            }
            SchedulingError::CannotResolveTarget(msg) => {
                write!(f, "{}", msg)
            }
            SchedulingError::NothingToCancel => {
                write!(f, "nothing active to cancel")
            }
        }
    }
}

impl Scheduler {
    pub fn new(repo: Repository, root: PathBuf, verbose: bool, no_hooks: bool) -> Self {
        Self {
            repo,
            root,
            verbose,
            no_hooks,
        }
    }

    /// Run a hook, optionally modifying the `HookContext` before execution.
    fn run_hook_with(
        &self,
        event: HookEvent,
        schedulable: &Schedulable,
        modify: impl FnOnce(&mut HookContext),
    ) -> Result<(), SchedulingError> {
        let mut ctx = HookContext::from_schedulable(&self.root, schedulable, self.verbose);
        modify(&mut ctx);
        hooks::run_hook(event, &ctx, self.no_hooks).map_err(|e| {
            eprintln!("Error: Hook {} failed: {}", event, e);
            SchedulingError::HookRejected
        })
    }

    /// Convenience wrapper to run a hook without extra context.
    fn run_hook(&self, event: HookEvent, schedulable: &Schedulable) -> Result<(), SchedulingError> {
        self.run_hook_with(event, schedulable, |_| {})
    }

    /// Run an after-hook, optionally modifying the `HookContext`. Failures are only logged.
    fn run_hook_after_with(
        &self,
        event: HookEvent,
        schedulable: &Schedulable,
        modify: impl FnOnce(&mut HookContext),
    ) {
        if let Err(e) = self.run_hook_with(event, schedulable, modify)
            && self.verbose
        {
            eprintln!("Warning: after-hook {} reported: {}", event, e);
        }
    }

    /// Convenience wrapper to run an after-hook without extra context.
    fn run_hook_after(&self, event: HookEvent, schedulable: &Schedulable) {
        self.run_hook_after_with(event, schedulable, |_| {});
    }

    /// Log an externally completed pomodoro.
    pub fn log(&self, schedulable: &Schedulable) -> Result<Schedulable, SchedulingError> {
        // --- before-log-pomodoro ---
        self.run_hook(HookEvent::BeforeLogPomodoro, schedulable)?;

        let saved = self
            .repo
            .save_external_finished(schedulable)
            .map_err(|e| match e {
                PersistenceError::OverlappingTimeRange => {
                    eprintln!("Error: {}.", e);
                    SchedulingError::ExecutionError
                }
                _ => {
                    eprintln!("Error: {}.", e);
                    SchedulingError::ExecutionError
                }
            })?;

        // --- after-log-pomodoro ---
        self.run_hook_after(HookEvent::AfterLogPomodoro, &saved);

        Ok(saved)
    }

    /// Log an externally completed break (one that wasn't tracked via the timer).
    pub fn log_break(&self, schedulable: &Schedulable) -> Result<Schedulable, SchedulingError> {
        // --- before-log-break ---
        self.run_hook(HookEvent::BeforeLogBreak, schedulable)?;

        let saved = self
            .repo
            .save_external_finished(schedulable)
            .map_err(|e| match e {
                PersistenceError::OverlappingTimeRange => {
                    eprintln!("Error: {}.", e);
                    SchedulingError::ExecutionError
                }
                _ => {
                    eprintln!("Error: {}.", e);
                    SchedulingError::ExecutionError
                }
            })?;

        // --- after-log-break ---
        self.run_hook_after(HookEvent::AfterLogBreak, &saved);

        Ok(saved)
    }

    /// Record an interruption. Uses the unified target resolution:
    /// tries the active pomodoro first (`0`), then falls back to
    /// the most recent pomodoro (`-1`).
    pub fn interrupt(&self, kind: InterruptionKind) -> Result<Schedulable, SchedulingError> {
        let target = self
            .resolve_target("0", Some(Kind::Pomodoro))
            .or_else(|_| self.resolve_target("-1", Some(Kind::Pomodoro)))?;

        // Run before-interrupt hook
        self.run_hook_with(HookEvent::BeforeInterruptPomodoro, &target, |ctx| {
            ctx.interrupt_kind = Some(kind.as_str().to_string());
        })?;

        // Increment the counter
        let updated = self
            .repo
            .record_interrupt(target.uuid)
            .map_err(|_| SchedulingError::ExecutionError)?;

        // Save to interrupt log
        let interrupt_log = InterruptLog {
            uuid: SqlUuid::default(),
            schedulable_uuid: target.uuid,
            kind,
            created_at: now(),
        };
        self.repo
            .save_interrupt(&interrupt_log)
            .map_err(|_| SchedulingError::ExecutionError)?;

        // Run after-interrupt hook
        self.run_hook_after_with(HookEvent::AfterInterruptPomodoro, &updated, |ctx| {
            ctx.interrupt_kind = Some(kind.as_str().to_string());
        });

        Ok(updated)
    }

    /// Record an interruption on a specific target (resolved via `--target`).
    pub fn interrupt_target(
        &self,
        kind: InterruptionKind,
        raw_target: &str,
    ) -> Result<Schedulable, SchedulingError> {
        let target = self.resolve_target(raw_target, Some(Kind::Pomodoro))?;

        if target.kind != Kind::Pomodoro {
            return Err(SchedulingError::CannotResolveTarget(format!(
                "'{}' is a break; interruptions can only be recorded on pomodori",
                raw_target
            )));
        }

        // Run before-interrupt hook
        self.run_hook_with(HookEvent::BeforeInterruptPomodoro, &target, |ctx| {
            ctx.interrupt_kind = Some(kind.as_str().to_string());
        })?;

        // Increment the counter
        let updated = self
            .repo
            .record_interrupt(target.uuid)
            .map_err(|_| SchedulingError::ExecutionError)?;

        // Save to interrupt log
        let interrupt_log = InterruptLog {
            uuid: SqlUuid::default(),
            schedulable_uuid: target.uuid,
            kind,
            created_at: now(),
        };
        self.repo
            .save_interrupt(&interrupt_log)
            .map_err(|_| SchedulingError::ExecutionError)?;

        // Run after-interrupt hook
        self.run_hook_after_with(HookEvent::AfterInterruptPomodoro, &updated, |ctx| {
            ctx.interrupt_kind = Some(kind.as_str().to_string());
        });

        Ok(updated)
    }

    /// Close out a schedulable — cancel pomodoro, finish break.
    /// Runs before/after hooks and persists.
    fn close_out(&self, schedulable: &mut Schedulable) -> Result<(), SchedulingError> {
        match schedulable.kind {
            Kind::Pomodoro => {
                self.run_hook(HookEvent::BeforeCancelPomodoro, schedulable)?;
                schedulable.cancelled_at = now();
                self.repo
                    .save(schedulable)
                    .expect("Unable to persist cancelled pomodoro");
                self.run_hook_after(HookEvent::AfterCancelPomodoro, schedulable);
            }
            Kind::Break => {
                self.run_hook(HookEvent::BeforeFinishBreak, schedulable)?;
                schedulable.finished_at = now();
                self.repo
                    .save(schedulable)
                    .expect("Unable to persist finished break");
                self.run_hook_after(HookEvent::AfterFinishBreak, schedulable);
            }
        }
        Ok(())
    }

    /// Cancel the currently active schedulable.
    /// Pomodoro → cancel (cancelled_at). Break → finish (finished_at).
    /// Returns `NothingToCancel` if nothing is active.
    pub fn cancel(&self) -> Result<Schedulable, SchedulingError> {
        let active = self
            .repo
            .active()
            .map_err(|_| SchedulingError::ExecutionError)?;

        let mut schedulable = active.ok_or(SchedulingError::NothingToCancel)?;
        self.close_out(&mut schedulable)?;
        Ok(schedulable)
    }

    /// Cancel a specific pomodoro or break identified by `--target`.
    ///
    /// For a pomodoro: sets `cancelled_at` (even if it was previously finished).
    /// For a break: sets `finished_at` ("cancel" on a break finishes it).
    ///
    /// Returns an error if the target is already in the terminal state
    /// (pomodoro already cancelled, break already finished).
    pub fn cancel_target(&self, raw_target: &str) -> Result<Schedulable, SchedulingError> {
        let mut target = self.resolve_target(raw_target, None)?;

        match target.kind {
            Kind::Pomodoro => {
                if target.cancelled_at != 0 {
                    return Err(SchedulingError::CannotResolveTarget(
                        "pomodoro is already cancelled".to_string(),
                    ));
                }
                self.run_hook(HookEvent::BeforeCancelPomodoro, &target)?;
                target.cancelled_at = now();
                target.finished_at = 0;
                self.repo
                    .save(&target)
                    .map_err(|_| SchedulingError::ExecutionError)?;
                self.run_hook_after(HookEvent::AfterCancelPomodoro, &target);
                Ok(target)
            }
            Kind::Break => {
                if target.finished_at != 0 {
                    return Err(SchedulingError::CannotResolveTarget(
                        "break is already finished".to_string(),
                    ));
                }
                self.run_hook(HookEvent::BeforeFinishBreak, &target)?;
                target.finished_at = now();
                target.cancelled_at = 0;
                self.repo
                    .save(&target)
                    .map_err(|_| SchedulingError::ExecutionError)?;
                self.run_hook_after(HookEvent::AfterFinishBreak, &target);
                Ok(target)
            }
        }
    }

    /// Access the underlying repository (used in tests).
    pub fn repo(&self) -> &Repository {
        &self.repo
    }

    /// Annotate the active schedulable, or the most recently ended one.
    pub fn annotate(&self, text: &str) -> Result<Annotation, SchedulingError> {
        let active = self
            .repo
            .active()
            .map_err(|_| SchedulingError::ExecutionError)?;

        let target = match active {
            Some(s) => s,
            None => {
                // Nothing active — annotate the most recently ended
                self.repo
                    .most_recently_ended()
                    .map_err(|_| SchedulingError::ExecutionError)?
                    .ok_or(SchedulingError::NothingToAnnotate)?
            }
        };

        self.save_annotation_for(&target, text)
    }

    /// Annotate a schedulable of the given kind. If active and matches kind,
    /// annotates it; otherwise falls back to the most recently finished of that kind.
    pub fn annotate_for_kind(&self, text: &str, kind: Kind) -> Result<Annotation, SchedulingError> {
        let active = self
            .repo
            .active()
            .map_err(|_| SchedulingError::ExecutionError)?;

        let target = match active {
            Some(s) if s.kind == kind => s,
            _ => {
                // Active is a different kind or nothing active —
                // find most recently finished of the desired kind
                match kind {
                    Kind::Pomodoro => self
                        .repo
                        .most_recently_finished_pomodoro()
                        .map_err(|_| SchedulingError::ExecutionError)?
                        .ok_or(SchedulingError::NothingToAnnotate)?,
                    Kind::Break => self
                        .repo
                        .most_recently_finished_break()
                        .map_err(|_| SchedulingError::ExecutionError)?
                        .ok_or(SchedulingError::NothingToAnnotate)?,
                }
            }
        };

        self.save_annotation_for(&target, text)
    }

    /// Resolve a `--target` specifier to a `Schedulable`, then annotate it.
    /// `kind` filters which kind of schedulable `-N` resolves to.
    pub fn annotate_target(
        &self,
        text: &str,
        raw_target: &str,
        kind: Option<Kind>,
    ) -> Result<Annotation, SchedulingError> {
        let target = self.resolve_target(raw_target, kind)?;
        self.save_annotation_for(&target, text)
    }

    /// Resolve a target string to a Schedulable.
    ///
    /// - `"0"` → entry with a PID (active or stale). Error if none.
    /// - `"-N"` (1..=9) → Nth most recently started, optionally filtered by `kind`.
    /// - Otherwise tries HH:MM, RFC 3339, then UUID prefix (unchanged).
    pub fn resolve_target(
        &self,
        raw: &str,
        kind: Option<Kind>,
    ) -> Result<Schedulable, SchedulingError> {
        // 0 → entry with a PID (active or stale)
        if raw == "0" {
            let active = self
                .repo
                .active()
                .map_err(|_| SchedulingError::ExecutionError)?
                .ok_or(SchedulingError::NoActiveSchedulable)?;
            if let Some(k) = kind
                && active.kind != k
            {
                return Err(SchedulingError::CannotResolveTarget(format!(
                    "active entry is a {}, not {}",
                    active.kind, k
                )));
            }
            return Ok(active);
        }

        // -N (1..=9): Nth most recently started, optionally filtered by kind.
        // Excludes the active entry (which has its own target `0`), so that
        // `-1` always means "the one before the currently running one".
        if let Some(n) = parse_negative_index(raw) {
            let exclude = self.repo.active().ok().flatten().map(|s| s.uuid);
            return self
                .repo
                .nth_most_recently_started(n, kind, exclude)
                .map_err(|_| SchedulingError::ExecutionError)?
                .ok_or_else(|| {
                    SchedulingError::CannotResolveTarget(format!("no entry at position -{}", n))
                });
        }

        // Timestamp (HH:MM, RFC 3339, ISO 8601, or Unix timestamp)
        if let Ok(ts) = super::parse_timestamp(raw)
            && let Some(s) = self
                .repo
                .find_by_timestamp(ts)
                .map_err(|_| SchedulingError::ExecutionError)?
        {
            return Ok(s);
        }

        // UUID prefix (abbreviated or full)
        if let Ok(s) = self.repo.find_by_uuid_prefix(raw) {
            return Ok(s);
        }

        Err(SchedulingError::CannotResolveTarget(format!(
            "cannot resolve '{}' to a pomodoro or break; try a UUID prefix, -1..-9, or a timestamp",
            raw
        )))
    }

    /// Save an annotation for the given target, running before/after hooks.
    fn save_annotation_for(
        &self,
        target: &Schedulable,
        text: &str,
    ) -> Result<Annotation, SchedulingError> {
        let before_event = match target.kind {
            Kind::Pomodoro => HookEvent::BeforeAnnotatePomodoro,
            Kind::Break => HookEvent::BeforeAnnotateBreak,
        };

        self.run_hook_with(before_event, target, |ctx| {
            ctx.annotation = Some(text.to_string());
        })?;

        // Save the annotation
        let annotation = Annotation {
            uuid: SqlUuid::default(),
            schedulable_uuid: target.uuid,
            body: text.to_string(),
            created_at: now(),
        };
        let saved = self
            .repo
            .save_annotation(&annotation)
            .map_err(|_| SchedulingError::ExecutionError)?;

        // Run after hook
        let after_event = match target.kind {
            Kind::Pomodoro => HookEvent::AfterAnnotatePomodoro,
            Kind::Break => HookEvent::AfterAnnotateBreak,
        };
        self.run_hook_after_with(after_event, target, |ctx| {
            ctx.annotation = Some(text.to_string());
        });

        Ok(saved)
    }

    pub fn run(
        &self,
        mut schedulable: Schedulable,
        force: bool,
    ) -> Result<Schedulable, SchedulingError> {
        schedulable.started_at = now();

        // --- force: kill any existing active schedulable, then close it out ---
        if force && let Ok(Some(mut active)) = self.repo.active() {
            // Kill the process if it's still alive
            if super::pid_is_alive(active.pid) {
                eprintln!("Killing active pid {} ...", active.pid);
                super::kill_process(active.pid);
            }
            self.close_out(&mut active)?;
        }

        // --- before-start-{kind} ---
        let event = match schedulable.kind {
            Kind::Pomodoro => HookEvent::BeforeStartPomodoro,
            Kind::Break => HookEvent::BeforeStartBreak,
        };
        self.run_hook(event, &schedulable)?;

        // --- insert into database (active) ---
        let mut schedulable = match self.repo.save(&schedulable) {
            Ok(v) => v,
            Err(e) => match e {
                PersistenceError::AlreadyRunning(pid) => {
                    return Err(SchedulingError::AlreadyRunning(pid));
                }
                _ => return Err(SchedulingError::ExecutionError),
            },
        };

        // --- after-start-{kind} ---
        let event = match schedulable.kind {
            Kind::Pomodoro => HookEvent::AfterStartPomodoro,
            Kind::Break => HookEvent::AfterStartBreak,
        };
        self.run_hook_after(event, &schedulable);

        if self.verbose {
            let uuid_str = format!("{}", schedulable.uuid);
            eprintln!("  Started {} {}", schedulable.kind, &uuid_str[..8]);
        }

        // --- wait for timer or Ctrl-C ---
        let cancelled = match waiter(schedulable.duration, schedulable.kind).recv() {
            Ok(cancelled) => cancelled,
            Err(_) => return Err(SchedulingError::ExecutionError),
        };

        match schedulable.kind {
            Kind::Pomodoro if cancelled => {
                // Ctrl-C during a pomodoro → cancel
                self.run_hook(HookEvent::BeforeCancelPomodoro, &schedulable)?;

                schedulable.cancelled_at = now();
                self.repo.save(&schedulable).expect("Unable to persist");

                self.run_hook_after(HookEvent::AfterCancelPomodoro, &schedulable);

                Ok(schedulable)
            }
            Kind::Pomodoro => {
                // Timer expired → finish
                self.run_hook(HookEvent::BeforeFinishPomodoro, &schedulable)?;

                schedulable.finished_at = now();
                self.repo.save(&schedulable).expect("Unable to persist");

                self.run_hook_after(HookEvent::AfterFinishPomodoro, &schedulable);

                Ok(schedulable)
            }
            Kind::Break => {
                // Both timer expiry and Ctrl-C during a break → finish
                self.run_hook(HookEvent::BeforeFinishBreak, &schedulable)?;

                schedulable.finished_at = now();
                self.repo.save(&schedulable).expect("Unable to persist");

                self.run_hook_after(HookEvent::AfterFinishBreak, &schedulable);

                Ok(schedulable)
            }
        }
    }
}

/// Hides the terminal cursor while alive; restores it on drop.
struct CursorGuard;

impl CursorGuard {
    fn hide() -> Option<Self> {
        if std::io::stderr().is_terminal() {
            use std::io::Write;
            let _ = write!(std::io::stderr(), "\x1b[?25l");
            Some(CursorGuard)
        } else {
            None
        }
    }
}

impl Drop for CursorGuard {
    fn drop(&mut self) {
        use std::io::Write;
        let _ = write!(std::io::stderr(), "\x1b[?25h");
    }
}

fn waiter(duration: i64, kind: Kind) -> Receiver<bool> {
    init_ctrlc_handler();
    let (result_tx, result_rx) = channel::<bool>();

    // Show the progress bar only when attached to a terminal (stderr)
    let pb = std::io::stderr().is_terminal().then(|| {
        let bar = ProgressBar::new((60 * duration) as u64);
        bar.set_style(
            ProgressStyle::with_template("{msg} [{wide_bar}]")
                .unwrap()
                .progress_chars("=> "),
        );
        bar
    });

    let label = match kind {
        Kind::Pomodoro => "Pomodoro",
        Kind::Break => "Break",
    };

    thread::spawn({
        move || {
            let _cursor = CursorGuard::hide();
            let total = Duration::new((60 * duration) as u64, 0);
            let start = Instant::now();

            loop {
                if start.elapsed() >= total {
                    if let Some(ref pb) = pb {
                        pb.finish_and_clear();
                    }
                    let _ = result_tx.send(false);
                    return;
                }

                let elapsed = start.elapsed();
                let remaining = total.saturating_sub(elapsed);
                let em = elapsed.as_secs() / 60;
                let es = elapsed.as_secs() % 60;
                let rm = remaining.as_secs() / 60;
                let rs = remaining.as_secs() % 60;

                if let Some(ref pb) = pb {
                    pb.set_message(format!(
                        "{} {:02}:{:02} / {:02}:{:02}",
                        label, em, es, rm, rs,
                    ));
                    pb.set_position(elapsed.as_secs());
                }

                if CTRLC_PRESSED.swap(false, Ordering::SeqCst) {
                    if let Some(ref pb) = pb {
                        pb.finish_and_clear();
                    }
                    let _ = result_tx.send(true);
                    return;
                }

                thread::sleep(Duration::from_millis(25));
            }
        }
    })
    .join()
    .unwrap();
    result_rx
}

fn now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(n) => n.as_secs() as i64,
        Err(_) => panic!("SystemTime before UNIX EPOCH!"),
    }
}

/// Parse `-N` where N is 1..=9 and return `Some(N)`, or `None`.
fn parse_negative_index(raw: &str) -> Option<u32> {
    if raw.len() == 2 && raw.starts_with('-') {
        let digit = raw.as_bytes()[1];
        if (b'1'..=b'9').contains(&digit) {
            return Some((digit - b'0') as u32);
        }
    }
    None
}
