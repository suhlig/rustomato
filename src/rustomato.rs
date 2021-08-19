use std::fmt;
use std::result::Result;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{thread, time::Duration, time::Instant};
use uuid::Uuid;

pub struct Schedulable {
    pub uuid: Uuid,
    duration: u64,
    started_at: u64,
    finished_at: u64,
    cancelled_at: u64,
}

pub enum Status {
    New,
    Active,
    Cancelled,
    Finished,
}

impl Schedulable {
    pub fn new(duration: u64) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            duration: duration,
            started_at: 0,
            finished_at: 0,
            cancelled_at: 0,
        }
    }

    pub fn status(&self) -> Status {
        if self.started_at == 0 {
            return Status::New;
        } else {
            if self.cancelled_at != 0 {
                return Status::Cancelled;
            } else {
                if self.finished_at != 0 {
                    return Status::Finished;
                } else {
                    return Status::Active;
                }
            }
        }
    }
}

impl fmt::Display for Schedulable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status() {
            Status::Active => { write!(f, "{}; active since {}", self.uuid, self.started_at) },
            Status::New => { write!(f, "{}; new", self.uuid) },
            Status::Cancelled => { write!(f, "{}; cancelled at {}", self.uuid, self.cancelled_at) },
            Status::Finished => { write!(f, "{}; finished at {}", self.uuid, self.finished_at) },
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum SchedulingError {
    ExecutionError,
}

impl fmt::Display for SchedulingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error executing schedulable")
    }
}

pub fn run(mut schedulable: Schedulable) -> Result<Schedulable, SchedulingError> {
    // TODO Assert that DB does not have schedulable.uuid yet

    schedulable.started_at = now();

    // TODO Save schedulable in DB

    match waiter(schedulable.duration).recv() {
        Ok(cancelled) => {
            if cancelled {
                schedulable.cancelled_at = now();
            } else {
                schedulable.finished_at = now();
            }

            // TODO Save schedulable in DB

            return Ok(schedulable);
        }
        Err(_) => {
            return Err(SchedulingError::ExecutionError);
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
