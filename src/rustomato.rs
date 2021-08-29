use rusqlite::types::{ToSql, ToSqlOutput};
use rusqlite::Result;
use std::fmt;
use uuid::Uuid;

pub mod persistence;
pub mod scheduling;

pub enum Kind {
    Pomodoro,
    Break,
}

// Neither Pomodoro nor Break
#[derive(Debug)]
pub struct UnknownKind {
    offender: String
}

pub struct Schedulable {
    pid: u32,
    kind: Kind,
    uuid: Uuid,
    duration: u64,
    started_at: u64,
    finished_at: u64,
    cancelled_at: u64,
}

pub enum Status {
    Active,
    Cancelled,
    Finished,
}

impl Kind {
    pub fn from(str: String) -> Result<Self, UnknownKind> {
        match str.to_lowercase().as_str() {
            "pomodoro" => Ok(Kind::Pomodoro),
            "break" => Ok(Kind::Break),
            _ => Err(UnknownKind{offender: str}),
        }
    }
}

impl Schedulable {
    pub fn new(pid: u32, kind: Kind, duration: u64) -> Self {
        Self {
            pid: pid,
            kind: kind,
            uuid: Uuid::new_v4(),
            duration: duration,
            started_at: 0,
            finished_at: 0,
            cancelled_at: 0,
        }
    }

    pub fn status(&self) -> Status {
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

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Pomodoro => {
                write!(f, "pomodoro")
            }
            Kind::Break => {
                write!(f, "break")
            }
        }
    }
}

impl std::fmt::Debug for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
         f.debug_struct("Kind")
         .finish()
    }
}

impl ToSql for Kind {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>> {
        match self {
            Kind::Pomodoro => Ok(ToSqlOutput::from("pomodoro")),
            Kind::Break => Ok(ToSqlOutput::from("break")),
        }
    }
}

impl fmt::Display for Schedulable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status() {
            Status::Active => {
                write!(
                    f,
                    "{} {}; active since {}",
                    self.kind, self.uuid, self.started_at // TODO print prettier timestamp
                )
            }
            Status::Cancelled => {
                write!(
                    f,
                    "{} {}; cancelled at {}",
                    self.kind, self.uuid, self.cancelled_at // TODO print prettier timestamp
                )
            }
            Status::Finished => {
                write!(
                    f,
                    "{} {}; finished at {}",
                    self.kind, self.uuid, self.finished_at // TODO print prettier timestamp
                )
            }
        }
    }
}
