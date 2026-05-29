use super::hooks::{self, HookContext, HookEvent};
use super::persistence::{PersistenceError, Repository};
use super::{Annotation, InterruptLog, InterruptionKind, Kind, Schedulable, SqlUuid};
use pbr::ProgressBar;
use std::fmt;
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

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum SchedulingError {
    ExecutionError,
    AlreadyRunning(u32),
    HookRejected,
    NoActiveSchedulable,
    NoFinishedPomodoro,
    NothingToAnnotate,
}

impl fmt::Display for SchedulingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchedulingError::ExecutionError => write!(f, "cannot execute schedulable"),
            SchedulingError::AlreadyRunning(_) => {
                write!(f, "another Pomodoro or break is already running")
            }
            SchedulingError::HookRejected => {
                write!(f, "a before-hook rejected the operation")
            }
            SchedulingError::NoActiveSchedulable => {
                write!(f, "nothing active to interrupt")
            }
            SchedulingError::NoFinishedPomodoro => {
                write!(
                    f,
                    "a break is active but there is no finished pomodoro to interrupt"
                )
            }
            SchedulingError::NothingToAnnotate => {
                write!(f, "nothing active or previously done to annotate")
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

    fn run_hook(&self, event: HookEvent, schedulable: &Schedulable) -> Result<(), SchedulingError> {
        let ctx = HookContext::from_schedulable(&self.root, schedulable, self.verbose);
        hooks::run_hook(event, &ctx, self.no_hooks).map_err(|e| {
            eprintln!("Error: Hook {} failed: {}", event, e);
            SchedulingError::HookRejected
        })
    }

    fn run_hook_after(&self, event: HookEvent, schedulable: &Schedulable) {
        if let Err(e) = self.run_hook(event, schedulable)
            && self.verbose
        {
            eprintln!("Warning: after-hook {} reported: {}", event, e);
        }
    }

    /// Run a hook with the interrupt kind set on the context.
    fn run_hook_with_interrupt_kind(
        &self,
        event: HookEvent,
        schedulable: &Schedulable,
        kind: &InterruptionKind,
    ) -> Result<(), SchedulingError> {
        let mut ctx = HookContext::from_schedulable(&self.root, schedulable, self.verbose);
        ctx.interrupt_kind = Some(kind.as_str().to_string());
        hooks::run_hook(event, &ctx, self.no_hooks).map_err(|e| {
            eprintln!("Error: Hook {} failed: {}", event, e);
            SchedulingError::HookRejected
        })
    }

    fn run_hook_after_with_interrupt_kind(
        &self,
        event: HookEvent,
        schedulable: &Schedulable,
        kind: &InterruptionKind,
    ) {
        if let Err(e) = self.run_hook_with_interrupt_kind(event, schedulable, kind)
            && self.verbose
        {
            eprintln!("Warning: after-hook {} reported: {}", event, e);
        }
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

    /// Record an interruption on the active pomodoro, or on the most recently finished
    /// pomodoro if a break is active.
    pub fn interrupt(&self, kind: InterruptionKind) -> Result<Schedulable, SchedulingError> {
        let active = self
            .repo
            .active()
            .map_err(|_| SchedulingError::ExecutionError)?;

        let target = match active {
            Some(s) if s.kind == Kind::Pomodoro => s,
            Some(_) => {
                // Break is active, find most recently finished pomodoro
                self.repo
                    .most_recently_finished_pomodoro()
                    .map_err(|_| SchedulingError::ExecutionError)?
                    .ok_or(SchedulingError::NoFinishedPomodoro)?
            }
            None => return Err(SchedulingError::NoActiveSchedulable),
        };

        // Run before-interrupt hook
        self.run_hook_with_interrupt_kind(HookEvent::BeforeInterruptPomodoro, &target, &kind)?;

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
        self.run_hook_after_with_interrupt_kind(HookEvent::AfterInterruptPomodoro, &updated, &kind);

        Ok(updated)
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

        // Build hook context with annotation text
        let mut ctx = HookContext::from_schedulable(&self.root, &target, self.verbose);
        ctx.annotation = Some(text.to_string());

        let before_event = match target.kind {
            Kind::Pomodoro => HookEvent::BeforeAnnotatePomodoro,
            Kind::Break => HookEvent::BeforeAnnotateBreak,
        };

        hooks::run_hook(before_event, &ctx, self.no_hooks).map_err(|e| {
            eprintln!("Error: Hook {} failed: {}", before_event, e);
            SchedulingError::HookRejected
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
        ctx.annotation = Some(text.to_string());
        if let Err(e) = hooks::run_hook(after_event, &ctx, self.no_hooks)
            && self.verbose
        {
            eprintln!("Warning: after-hook {} reported: {}", after_event, e);
        }

        Ok(saved)
    }

    pub fn run(
        &self,
        mut schedulable: Schedulable,
        force: bool,
    ) -> Result<Schedulable, SchedulingError> {
        schedulable.started_at = now();

        // --- force: kill any existing active schedulable, then cancel/finish it ---
        if force && let Ok(Some(active)) = self.repo.active() {
            // Kill the process if it's still alive
            if super::pid_is_alive(active.pid) {
                eprintln!("Killing active pid {} ...", active.pid);
                super::kill_process(active.pid);
            }

            match active.kind {
                Kind::Pomodoro => {
                    self.run_hook(HookEvent::BeforeCancelPomodoro, &active)?;
                    let mut cancelled = active;
                    cancelled.cancelled_at = now();
                    self.repo
                        .save(&cancelled)
                        .expect("Unable to persist force-cancelled pomodoro");
                    self.run_hook_after(HookEvent::AfterCancelPomodoro, &cancelled);
                }
                Kind::Break => {
                    self.run_hook(HookEvent::BeforeFinishBreak, &active)?;
                    let mut finished = active;
                    finished.finished_at = now();
                    self.repo
                        .save(&finished)
                        .expect("Unable to persist force-finished break");
                    self.run_hook_after(HookEvent::AfterFinishBreak, &finished);
                }
            }
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

        // --- wait for timer or Ctrl-C ---
        let cancelled = match waiter(schedulable.duration).recv() {
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

fn waiter(duration: i64) -> Receiver<bool> {
    init_ctrlc_handler();
    let (result_tx, result_rx) = channel::<bool>();

    // TODO Only if attached to a terminal
    let mut pb = ProgressBar::new((60 * duration) as u64);

    pb.show_speed = false;
    pb.show_counter = false;
    pb.show_time_left = false;
    pb.show_tick = false;

    thread::spawn({
        move || {
            let total = Duration::new((60 * duration) as u64, 0);
            let start = Instant::now();

            loop {
                if start.elapsed() >= total {
                    let _ = result_tx.send(false);
                    return;
                }

                pb.set(start.elapsed().as_secs());

                if CTRLC_PRESSED.swap(false, Ordering::SeqCst) {
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
