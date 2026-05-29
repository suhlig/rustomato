use rusqlite::Result;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::types::{ToSql, ToSqlOutput};
use std::fmt;
use uuid::Uuid;

pub mod hooks;
pub mod migration;
pub mod persistence;
pub mod scheduling;

#[derive(Debug)]
pub struct Annotation {
    pub uuid: SqlUuid,
    pub schedulable_uuid: SqlUuid,
    pub body: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Pomodoro,
    Break,
}

// Neither Pomodoro nor Break
#[derive(Debug)]
pub struct UnknownKind {
    pub offender: String,
}

#[derive(Debug, Clone, Copy)]
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
        write!(f, "{}", self.0.simple())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptionKind {
    Internal,
    External,
}

impl InterruptionKind {
    pub fn from(str: &str) -> Result<Self, String> {
        match str.to_lowercase().as_str() {
            "internal" => Ok(InterruptionKind::Internal),
            "external" => Ok(InterruptionKind::External),
            other => Err(format!(
                "unknown interruption kind '{}'; expected 'internal' or 'external'",
                other
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            InterruptionKind::Internal => "internal",
            InterruptionKind::External => "external",
        }
    }
}

#[derive(Debug)]
pub struct Schedulable {
    pub pid: u32,
    pub kind: Kind,
    pub uuid: SqlUuid,
    pub duration: i64, // TODO Use duration with a unit
    pub started_at: i64,
    pub finished_at: i64,
    pub cancelled_at: i64,
    pub interruptions: i64,
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

/// Returns true if the given PID exists on the system (cross-platform via POSIX kill(0)).
fn pid_is_alive(pid: u32) -> bool {
    unsafe {
        if libc::kill(pid as i32, 0) == 0 {
            true
        } else {
            // ESRCH means "no such process"; anything else (e.g. EPERM) means it exists.
            std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
        }
    }
}

impl Schedulable {
    pub fn new(pid: u32, kind: Kind, duration: i64) -> Self {
        Self {
            pid,
            kind,
            uuid: SqlUuid::default(),
            duration,
            started_at: 0,
            finished_at: 0,
            cancelled_at: 0,
            interruptions: 0,
        }
    }

    pub fn status(&self) -> Status {
        if self.cancelled_at != 0 {
            Status::Cancelled
        } else if self.finished_at != 0 {
            Status::Finished
        } else if self.started_at != 0 {
            if pid_is_alive(self.pid) {
                Status::Active
            } else {
                Status::Stale
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
        match self.status() {
            Status::New => {
                write!(f, "{} ({} min)", self.kind, self.duration)
            }
            Status::Active => {
                let interrupt_info = if self.interruptions > 0 {
                    let noun = if self.interruptions == 1 {
                        "interruption"
                    } else {
                        "interruptions"
                    };
                    format!(" ({} {})", self.interruptions, noun)
                } else {
                    String::new()
                };
                write!(
                    f,
                    "{} {} is active since {}{}",
                    self.kind,
                    self.uuid,
                    format_timestamp(self.started_at),
                    interrupt_info
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
                let interrupt_info = if self.interruptions > 0 {
                    let noun = if self.interruptions == 1 {
                        "interruption"
                    } else {
                        "interruptions"
                    };
                    format!(" ({} {})", self.interruptions, noun)
                } else {
                    String::new()
                };
                write!(
                    f,
                    "{} {} was finished at {}{}",
                    self.kind,
                    self.uuid,
                    format_timestamp(self.finished_at),
                    interrupt_info
                )
            }
        }
    }
}

/// Parse a timestamp string into a Unix timestamp (seconds since epoch).
///
/// Accepts:
/// - RFC 3339 / ISO 8601 with timezone offset (e.g. `2026-05-29T14:30:00Z` or `2026-05-29T14:30:00+02:00`)
/// - ISO 8601 without timezone (interpreted as local time)
/// - A bare integer interpreted as a Unix timestamp
pub fn parse_timestamp(s: &str) -> Result<i64, String> {
    use chrono::Local;
    use chrono::TimeZone;

    // RFC 3339 / ISO 8601 with timezone
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.timestamp());
    }
    // ISO 8601 with timezone via parse_from_str
    if let Ok(dt) = chrono::DateTime::parse_from_str(s, "%+") {
        return Ok(dt.timestamp());
    }

    // ISO 8601 without timezone – interpret as local time
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        && let Some(local) = Local.from_local_datetime(&naive).earliest()
    {
        return Ok(local.timestamp());
    }
    // Also accept space-separated ISO 8601 (no T)
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        && let Some(local) = Local.from_local_datetime(&naive).earliest()
    {
        return Ok(local.timestamp());
    }

    // Unix timestamp (bare integer)
    if let Ok(ts) = s.parse::<i64>() {
        return Ok(ts);
    }

    Err(format!(
        "cannot parse '{}' as a timestamp; expected RFC 3339 (e.g. 2026-05-29T14:30:00Z) or Unix timestamp",
        s
    ))
}

pub fn format_timestamp(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};

    if timestamp == 0 {
        return "N/A".to_string();
    }
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| timestamp.to_string())
}
