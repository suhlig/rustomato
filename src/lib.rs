use psutil::process::Process;
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
    pub offender: String,
}

#[derive(Clone, Copy)]
pub struct SqlUuid(Uuid);

impl Default for SqlUuid {
    fn default() -> Self {
        Self(Uuid::new_v4())
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
        write!(f, "{}", self.0.to_simple().to_string())
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
    Stale,
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
            pid,
            kind,
            uuid: SqlUuid::default(),
            duration,
            started_at: 0,
            finished_at: 0,
            cancelled_at: 0,
        }
    }

    pub fn status(&self) -> Status {
        if self.cancelled_at != 0 {
            Status::Cancelled
        } else if self.finished_at != 0 {
            Status::Finished
        } else if self.started_at != 0 {
            match Process::new(self.pid) {
                Ok(_) => Status::Active,
                Err(_) => Status::Stale,
            }
        } else {
            Status::New
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
        use chrono::{Local, TimeZone};

        fn format_timestamp(timestamp: u64) -> String {
            if timestamp == 0 {
                return "N/A".to_string();
            }
            Local
                .timestamp_opt(timestamp as i64, 0)
                .single()
                .map(|dt| dt.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| timestamp.to_string())
        }

        match self.status() {
            Status::New => {
                write!(f, "{} ({} min)", self.kind, self.duration)
            }
            Status::Active => {
                write!(
                    f,
                    "{} {} is active since {}",
                    self.kind,
                    self.uuid,
                    format_timestamp(self.started_at)
                )
            }
            Status::Stale => {
                write!(
                    f,
                    "{} {} is stale (pid {} does not exist)",
                    self.kind, self.uuid, self.pid,
                )
            }
            Status::Cancelled => {
                write!(
                    f,
                    "{} {} was cancelled at {}",
                    self.kind,
                    self.uuid,
                    format_timestamp(self.cancelled_at)
                )
            }
            Status::Finished => {
                write!(
                    f,
                    "{} {} was finished at {}",
                    self.kind,
                    self.uuid,
                    format_timestamp(self.finished_at)
                )
            }
        }
    }
}
