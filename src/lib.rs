use rusqlite::Result;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::types::{ToSql, ToSqlOutput};
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

pub mod export;
pub mod hooks;
pub mod migration;
pub mod persistence;
pub mod report;
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
    pub fn as_str(&self) -> &'static str {
        match self {
            InterruptionKind::Internal => "internal",
            InterruptionKind::External => "external",
        }
    }
}

impl FromStr for InterruptionKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "internal" => Ok(InterruptionKind::Internal),
            "external" => Ok(InterruptionKind::External),
            other => Err(format!(
                "unknown interruption kind '{}'; expected 'internal' or 'external'",
                other
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InterruptLog {
    pub uuid: SqlUuid,
    pub schedulable_uuid: SqlUuid,
    pub kind: InterruptionKind,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
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

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Active => "active",
            Status::Stale => "stale",
            Status::Finished => "finished",
            Status::Cancelled => "cancelled",
            Status::New => "new",
        }
    }
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
pub(crate) fn pid_is_alive(pid: u32) -> bool {
    unsafe {
        if libc::kill(pid as i32, 0) == 0 {
            true
        } else {
            // ESRCH means "no such process"; anything else (e.g. EPERM) means it exists.
            std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
        }
    }
}

/// Kill a process by PID. Sends SIGTERM first, waits briefly, then SIGKILL if still alive.
pub(crate) fn kill_process(pid: u32) {
    // Send SIGTERM for a graceful shutdown
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    // Give it a moment to exit cleanly
    std::thread::sleep(std::time::Duration::from_millis(500));
    // If still alive, escalate to SIGKILL
    if pid_is_alive(pid) {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
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
/// - `HH:MM` 24-hour clock — interpreted as today at that time, or yesterday
///   if that time is in the future (we never apply actions in the future)
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

    // HH:MM — today at that time, or yesterday if that time is in the future
    if let Some(ts) = parse_hhmm_local(s) {
        return Ok(ts);
    }

    // Unix timestamp (bare integer)
    if let Ok(ts) = s.parse::<i64>() {
        return Ok(ts);
    }

    Err(format!(
        "cannot parse '{}' as a timestamp; expected RFC 3339 (e.g. 2026-05-29T14:30:00Z), HH:MM, or Unix timestamp",
        s
    ))
}

/// Parse an `HH:MM` string into a Unix timestamp in local time.
///
/// Returns the timestamp for that time **today** if it's not in the future,
/// or **yesterday** if the wall-clock time has already passed today.
/// This enforces the rule that we never apply actions about the future.
fn parse_hhmm_local(s: &str) -> Option<i64> {
    use chrono::{Local, TimeZone};

    let (hours, minutes) = s.split_once(':').and_then(|(h, m)| {
        let h: u32 = h.parse().ok()?;
        let m: u32 = m.parse().ok()?;
        if h <= 23 && m <= 59 {
            Some((h, m))
        } else {
            None
        }
    })?;

    let now = Local::now();
    let today = now.date_naive();

    // Timestamp for today at HH:MM
    let today_naive = today.and_hms_opt(hours, minutes, 0)?;
    let today_ts = Local
        .from_local_datetime(&today_naive)
        .earliest()?
        .timestamp();

    if today_ts <= now.timestamp() {
        // Today at that time is in the past or right now — use today
        Some(today_ts)
    } else {
        // Future — use yesterday instead
        let yesterday = today - chrono::Duration::days(1);
        let yesterday_naive = yesterday.and_hms_opt(hours, minutes, 0)?;
        Local
            .from_local_datetime(&yesterday_naive)
            .earliest()
            .map(|dt| dt.timestamp())
    }
}

/// Given a list of UUIDs, find the shortest prefix length (minimum 6) that makes
/// all entries unique, and return the abbreviated strings using that same length.
pub fn abbreviate_uuids(uuids: &[SqlUuid]) -> Vec<String> {
    if uuids.is_empty() {
        return vec![];
    }

    let strings: Vec<String> = uuids.iter().map(|u| u.to_string()).collect();
    let n = strings.len();

    for prefix_len in 6..=32 {
        let unique: HashSet<&str> = strings.iter().map(|s| &s[..prefix_len]).collect();
        if unique.len() == n {
            return strings
                .iter()
                .map(|s| s[..prefix_len].to_string())
                .collect();
        }
    }

    // Full UUIDs as fallback (should never be reached with UUIDv4)
    strings
}

/// Format a timestamp as `HH:MM` (short form, for use in day reports).
pub fn format_time(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};
    if timestamp == 0 {
        return "N/A".to_string();
    }
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| timestamp.to_string())
}

/// Return the current Unix timestamp (seconds since epoch).
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("SystemTime before UNIX EPOCH!")
        .as_secs() as i64
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
