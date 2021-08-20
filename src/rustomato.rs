use std::fmt;
use uuid::Uuid;

pub mod persistence;
pub mod scheduling;

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
            Status::Active => {
                write!(f, "{}; active since {}", self.uuid, self.started_at)
            }
            Status::New => {
                write!(f, "{}; new", self.uuid)
            }
            Status::Cancelled => {
                write!(f, "{}; cancelled at {}", self.uuid, self.cancelled_at)
            }
            Status::Finished => {
                write!(f, "{}; finished at {}", self.uuid, self.finished_at)
            }
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

