use super::persistence::{Repository, PersistenceError};
use super::Schedulable;
use std::fmt;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{thread, time::Duration, time::Instant};

pub struct Scheduler {
  repo: Repository,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum SchedulingError {
  ExecutionError,
  AlreadyRunning,
}

impl fmt::Display for SchedulingError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      SchedulingError::ExecutionError => write!(f, "cannot execute schedulable"),
      SchedulingError::AlreadyRunning => write!(f, "another pomodoro or break is already running"),
    }
  }
}

impl Scheduler {
  pub fn new(repo: Repository) -> Self {
    Self { repo: repo }
  }

  pub fn run(&self, mut schedulable: Schedulable) -> Result<Schedulable, SchedulingError> {
    schedulable.started_at = now();

    let mut schedulable = match self.repo.save(&schedulable) {
      Ok(v) => v,
      Err(e) => {

        match e {
          PersistenceError::AlreadyRunning => return Err(SchedulingError::AlreadyRunning),
          _ => return Err(SchedulingError::ExecutionError)
        }
      }
    };

    match waiter(schedulable.duration).recv() {
      Ok(cancelled) => {
        if cancelled {
          schedulable.cancelled_at = now();
        } else {
          schedulable.finished_at = now();
        }

        // Handle save error more detailed
        self.repo.save(&schedulable).expect("Unable to persist");

        return Ok(schedulable);
      }
      Err(_) => {
        return Err(SchedulingError::ExecutionError);
      }
    }
  }
}

fn waiter(duration: u64) -> Receiver<bool> {
  let (control_tx, control_rx) = channel();
  let (result_tx, result_rx) = channel::<bool>();

  ctrlc::set_handler(move || {
    control_tx
      .send(())
      .expect("Could not send signal on control channel.")
  })
  .expect("Error setting Ctrl-C handler");

  thread::spawn({
    move || {
      let mut done = false;
      let duration = Duration::new(60 * duration, 0);
      let start = Instant::now();

      while !done {
        if start.elapsed() > duration {
          done = true;
          result_tx.send(false).expect("could not send result");
        }

        match control_rx.try_recv() {
          Ok(_) => {
            done = true;
            result_tx.send(true).expect("could not send result")
          }
          Err(TryRecvError::Disconnected) => {
            println!("Error: channel disconnected");
            done = true;
          }
          Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(25)),
        }
      }
    }
  })
  .join()
  .unwrap();
  return result_rx;
}

fn now() -> u64 {
  match SystemTime::now().duration_since(UNIX_EPOCH) {
    Ok(n) => n.as_secs(),
    Err(_) => panic!("SystemTime before UNIX EPOCH!"),
  }
}
