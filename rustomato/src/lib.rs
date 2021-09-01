use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::types::{ToSql, ToSqlOutput};
use rusqlite::Result;
use std::fmt;
use uuid::Uuid;

pub mod persistence;
pub mod scheduling;

#[derive(Debug)]
pub enum Kind {
    Pomodoro,
    Break,
}

// Neither Pomodoro nor Break
#[derive(Debug)]
pub struct UnknownKind {
    offender: String,
}

#[derive(Clone, Copy, Debug)]
pub struct SqlUuid(Uuid);

impl SqlUuid {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn to_string(&self) -> String {
        self.0.to_simple().to_string()
    }
}

impl FromSql for SqlUuid {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Text(s) => {
                let s = std::str::from_utf8(s).map_err(|e| FromSqlError::Other(Box::new(e)))?;
                match Uuid::parse_str(s) {
                    Ok(val) => Ok(SqlUuid(val)),
                    Err(e) => Err(FromSqlError::Other(Box::new(e))),
                }
            }
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl fmt::Display for SqlUuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

pub struct Schedulable {
    pub pid: u32,
    pub kind: Kind,
    pub uuid: SqlUuid,
    pub duration: u64, // TODO Use duration with a unit
    pub started_at: u64,
    pub finished_at: u64,
    pub cancelled_at: u64,
}

pub enum Status {
    New,
    Active,
    Cancelled,
    Finished,
}

impl Kind {
    pub fn from(str: String) -> Result<Self, UnknownKind> {
        match str.to_lowercase().as_str() {
            "pomodoro" => Ok(Kind::Pomodoro),
            "break" => Ok(Kind::Break),
            _ => Err(UnknownKind { offender: str }),
        }
    }
}

impl Schedulable {
    pub fn new(pid: u32, kind: Kind, duration: u64) -> Self {
        Self {
            pid: pid,
            kind: kind,
            uuid: SqlUuid::new(),
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
                if self.started_at != 0 {
                    return Status::Active;
                } else {
                    return Status::New;
                }
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
            Status::New => {
                write!(f, "{} ({} min)", self.kind, self.duration,)
            }
            Status::Active => {
                write!(
                    f,
                    "{} {} active since {}",
                    self.kind,
                    self.uuid,
                    self.started_at // TODO print prettier timestamp
                )
            }
            Status::Cancelled => {
                write!(
                    f,
                    "{} {} cancelled at {}",
                    self.kind,
                    self.uuid,
                    self.cancelled_at // TODO print prettier timestamp
                )
            }
            Status::Finished => {
                write!(
                    f,
                    "{} {} finished at {}",
                    self.kind,
                    self.uuid,
                    self.finished_at // TODO print prettier timestamp
                )
            }
        }
    }
}

impl std::fmt::Debug for Schedulable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Schedulable")
        .field("pid", &self.pid)
        .field("kind", &self.kind)
        .field("uuid", &self.uuid)
        .field("duration", &self.duration)
        .field("started_at", &self.started_at)
        .field("finished_at", &self.finished_at)
        .field("cancelled_at", &self.cancelled_at)
        .finish()
    }
}
