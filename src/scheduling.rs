use super::hooks::{self, HookContext, HookEvent};
use super::persistence::{PersistenceError, Repository};
use super::{Kind, Schedulable};
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

    pub fn run(&self, mut schedulable: Schedulable) -> Result<Schedulable, SchedulingError> {
        schedulable.started_at = now();

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
