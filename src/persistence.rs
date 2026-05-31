use super::{Annotation, InterruptLog, InterruptionKind, Kind, Schedulable, SqlUuid, Status};
use rusqlite::Connection;
use rusqlite::Error::QueryReturnedNoRows;
use rusqlite::OpenFlags;
use rusqlite::params;
use std::fmt;
use url::Url;
use uuid::Uuid;

pub struct Repository {
    db: Connection,
}

#[derive(PartialEq, Eq, Debug)]
pub enum PersistenceError {
    CannotSave(String),
    CannotUpdate(String),
    CannotFind(String),
    AlreadyRunning(u32),
    OverlappingTimeRange,
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistenceError::CannotSave(e) => write!(f, "Cannot save: {}", e),
            PersistenceError::CannotUpdate(e) => write!(f, "Cannot update: {}", e),
            PersistenceError::CannotFind(e) => write!(f, "Cannot find: {}", e),
            PersistenceError::AlreadyRunning(pid) => write!(f, "Already running as {}", pid),
            PersistenceError::OverlappingTimeRange => {
                write!(f, "Time range overlaps with an existing entry (Rule #1)")
            }
        }
    }
}

// ── Row mappers ──────────────────────────────────────────────────

/// Map a `schedulables` row to a `Schedulable`. Panics on data-integrity
/// errors (invalid UUID or kind in the database).
fn row_to_schedulable(row: &rusqlite::Row<'_>) -> rusqlite::Result<Schedulable> {
    let uuid_str: String = row.get(0)?;
    let kind_str: String = row.get(1)?;
    Ok(Schedulable {
        uuid: SqlUuid(Uuid::parse_str(&uuid_str).unwrap_or_else(|e| {
            panic!("invalid UUID in database: {}", e);
        })),
        kind: Kind::from(kind_str).unwrap_or_else(|e| {
            panic!("invalid kind in database: {}", e.offender);
        }),
        pid: row.get(2).unwrap_or(0),
        duration: row.get(3).unwrap_or(0),
        started_at: row.get(4).unwrap_or(0),
        finished_at: row.get(5).unwrap_or(0),
        cancelled_at: row.get(6).unwrap_or(0),
        interruptions: row.get(7).unwrap_or(0),
    })
}

/// Map an `annotations` row to an `Annotation`.
fn row_to_annotation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Annotation> {
    let uuid_str: String = row.get(0)?;
    let sched_uuid_str: String = row.get(1)?;
    Ok(Annotation {
        uuid: SqlUuid(Uuid::parse_str(&uuid_str).unwrap_or_default()),
        schedulable_uuid: SqlUuid(Uuid::parse_str(&sched_uuid_str).unwrap_or_default()),
        body: row.get(2)?,
        created_at: row.get(3)?,
    })
}

/// Map an `interrupt_log` row to an `InterruptLog`.
fn row_to_interrupt_log(row: &rusqlite::Row<'_>) -> rusqlite::Result<InterruptLog> {
    let uuid_str: String = row.get(0)?;
    let sched_uuid_str: String = row.get(1)?;
    let kind_str: String = row.get(2)?;
    Ok(InterruptLog {
        uuid: SqlUuid(Uuid::parse_str(&uuid_str).unwrap_or_default()),
        schedulable_uuid: SqlUuid(Uuid::parse_str(&sched_uuid_str).unwrap_or_default()),
        kind: InterruptionKind::from(&kind_str).expect("invalid interrupt kind in DB"),
        created_at: row.get(3)?,
    })
}

impl Repository {
    pub fn new(location: &str) -> Self {
        let db = Connection::open_with_flags(
            location,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI,
        )
        .expect("opening database connection");
        // Foreign key enforcement must be OFF during migrations because
        // V6 drops and recreates the schedulables table, and V4 has already
        // created the annotations table with a FK reference to schedulables.
        // With FKs ON, SQLite would reject the DROP TABLE when annotation
        // rows exist. Enforcement is re-enabled after migrations complete.
        db.execute_batch("PRAGMA foreign_keys = OFF;")
            .expect("disabling foreign key enforcement for migration");
        crate::migration::run(&db);
        db.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enabling foreign key enforcement");
        Self { db }
    }

    pub fn from_url(location: &Url) -> Self {
        Self::new(location.as_str())
    }

    pub fn active(&self) -> Result<Option<Schedulable>, PersistenceError> {
        match self.db.query_row(
            "SELECT uuid from schedulables where pid IS NOT NULL",
            [],
            |row| row.get(0),
        ) {
            Ok(val) => match self.find_by_uuid(val) {
                Ok(schedulable) => Ok(Some(schedulable)),
                Err(e) => Err(e),
            },
            Err(QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(PersistenceError::CannotFind(format!("{}", e))),
        }
    }

    pub fn find_by_uuid(&self, uuid: SqlUuid) -> Result<Schedulable, PersistenceError> {
        let uuid_s = uuid.to_string();

        match self.db.query_row(
            "SELECT uuid, kind, pid, duration, started_at, finished_at, cancelled_at, interruptions from schedulables where uuid=?1",
            params![uuid_s],
            row_to_schedulable,
        ) {
            Ok(mut s) => {
                s.uuid = uuid;
                Ok(s)
            }
            Err(e) => Err(PersistenceError::CannotFind(format!("{}", e)))
        }
    }

    /// Increment the interruption counter for the schedulable with the given UUID.
    /// Returns the updated schedulable.
    pub fn record_interrupt(&self, uuid: SqlUuid) -> Result<Schedulable, PersistenceError> {
        let uuid_s = uuid.to_string();

        match self.db.execute(
            "UPDATE schedulables SET interruptions = interruptions + 1 WHERE uuid == ?1",
            params![uuid_s],
        ) {
            Ok(rows_affected) if rows_affected > 0 => self.find_by_uuid(uuid),
            Ok(_) => Err(PersistenceError::CannotFind(format!(
                "schedulable {} not found",
                uuid_s
            ))),
            Err(e) => Err(PersistenceError::CannotUpdate(format!("{}", e))),
        }
    }

    /// Find the most recently ended schedulable (finished or cancelled) of any kind.
    pub fn most_recently_ended(&self) -> Result<Option<Schedulable>, PersistenceError> {
        match self.db.query_row(
            "SELECT uuid, kind, pid, duration, started_at, finished_at, cancelled_at, interruptions \
             FROM schedulables \
             WHERE finished_at IS NOT NULL OR cancelled_at IS NOT NULL \
             ORDER BY COALESCE(finished_at, cancelled_at) DESC \
             LIMIT 1",
            [],
            row_to_schedulable,
        ) {
            Ok(val) => Ok(Some(val)),
            Err(QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(PersistenceError::CannotFind(format!("{}", e))),
        }
    }

    pub fn save_annotation(&self, annotation: &Annotation) -> Result<Annotation, PersistenceError> {
        let uuid = annotation.uuid.to_string();
        let schedulable_uuid = annotation.schedulable_uuid.to_string();

        match self.db.execute(
            "INSERT INTO annotations (uuid, schedulable_uuid, body, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![uuid, schedulable_uuid, annotation.body, annotation.created_at],
        ) {
            Ok(_) => Ok(Annotation {
                uuid: annotation.uuid,
                schedulable_uuid: annotation.schedulable_uuid,
                body: annotation.body.clone(),
                created_at: annotation.created_at,
            }),
            Err(e) => Err(PersistenceError::CannotSave(format!("{}", e))),
        }
    }

    pub fn find_annotation_by_uuid(&self, uuid: SqlUuid) -> Result<Annotation, PersistenceError> {
        let uuid_s = uuid.to_string();

        match self.db.query_row(
            "SELECT uuid, schedulable_uuid, body, created_at FROM annotations WHERE uuid=?1",
            params![uuid_s],
            row_to_annotation,
        ) {
            Ok(val) => Ok(val),
            Err(e) => Err(PersistenceError::CannotFind(format!("{}", e))),
        }
    }

    pub fn annotations_for(
        &self,
        schedulable_uuid: SqlUuid,
    ) -> Result<Vec<Annotation>, PersistenceError> {
        let uuid_s = schedulable_uuid.to_string();

        let mut stmt = self
            .db
            .prepare(
                "SELECT uuid, schedulable_uuid, body, created_at FROM annotations WHERE schedulable_uuid=?1 ORDER BY created_at ASC",
            )
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let rows = stmt
            .query_map(params![uuid_s], row_to_annotation)
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?);
        }
        Ok(result)
    }

    /// Find the most recently finished pomodoro across all time.
    pub fn most_recently_finished_pomodoro(&self) -> Result<Option<Schedulable>, PersistenceError> {
        match self.db.query_row(
            "SELECT uuid, kind, pid, duration, started_at, finished_at, cancelled_at, interruptions \
             FROM schedulables \
             WHERE kind = 'pomodoro' AND finished_at != 0 \
             ORDER BY finished_at DESC \
             LIMIT 1",
            [],
            row_to_schedulable,
        ) {
            Ok(val) => Ok(Some(val)),
            Err(QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(PersistenceError::CannotFind(format!("{}", e))),
        }
    }

    /// Find the most recently finished break across all time.
    pub fn most_recently_finished_break(&self) -> Result<Option<Schedulable>, PersistenceError> {
        match self.db.query_row(
            "SELECT uuid, kind, pid, duration, started_at, finished_at, cancelled_at, interruptions \
             FROM schedulables \
             WHERE kind = 'break' AND finished_at != 0 \
             ORDER BY finished_at DESC \
             LIMIT 1",
            [],
            row_to_schedulable,
        ) {
            Ok(val) => Ok(Some(val)),
            Err(QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(PersistenceError::CannotFind(format!("{}", e))),
        }
    }

    /// Find a schedulable by abbreviated UUID prefix.
    /// Returns an error if the prefix matches zero or more than one row.
    pub fn find_by_uuid_prefix(&self, prefix: &str) -> Result<Schedulable, PersistenceError> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT uuid, kind, pid, duration, started_at, finished_at, cancelled_at, interruptions \
                 FROM schedulables \
                 WHERE uuid LIKE ?1",
            )
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        // The oldest UUID prefixes in the DB may be shorter than 6 chars for very old entries,
        // so we match the prefix followed by '%'
        let pattern = format!("{}%%", prefix);
        let rows: Vec<Schedulable> = stmt
            .query_map(params![pattern], row_to_schedulable)
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        match rows.len() {
            0 => Err(PersistenceError::CannotFind(format!(
                "no schedulable matches prefix '{}'",
                prefix
            ))),
            1 => Ok(rows.into_iter().next().unwrap()),
            n => Err(PersistenceError::CannotFind(format!(
                "'{}' is ambiguous; matches {} schedulables",
                prefix, n
            ))),
        }
    }

    /// Find the Nth most recently finished pomodoro across all time.
    /// n = 1 is the most recent, n = 2 the second most recent, etc.
    pub fn nth_most_recently_finished_pomodoro(
        &self,
        n: u32,
    ) -> Result<Option<Schedulable>, PersistenceError> {
        let offset = n.saturating_sub(1);
        let mut stmt = self
            .db
            .prepare(
                "SELECT uuid, kind, pid, duration, started_at, finished_at, cancelled_at, interruptions \
                 FROM schedulables \
                 WHERE kind = 'pomodoro' AND finished_at != 0 \
                 ORDER BY finished_at DESC \
                 LIMIT 1 OFFSET ?1",
            )
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        match stmt.query_row(params![offset], row_to_schedulable) {
            Ok(val) => Ok(Some(val)),
            Err(QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(PersistenceError::CannotFind(format!("{}", e))),
        }
    }

    /// Find a schedulable (of any kind) whose time range contains the given timestamp.
    /// The timestamp must be >= started_at, and for finished/cancelled entries must also
    /// be <= finished_at or cancelled_at respectively. Active entries (no end time) will
    /// match any timestamp >= started_at.
    /// When multiple entries match, the one with the latest start time wins.
    pub fn find_by_timestamp(&self, ts: i64) -> Result<Option<Schedulable>, PersistenceError> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT uuid, kind, pid, duration, started_at, finished_at, cancelled_at, interruptions \
                 FROM schedulables \
                 WHERE started_at <= ?1 \
                   AND (finished_at IS NULL OR finished_at >= ?1) \
                   AND (cancelled_at IS NULL OR cancelled_at >= ?1) \
                 ORDER BY started_at DESC \
                 LIMIT 1",
            )
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        match stmt.query_row(params![ts], row_to_schedulable) {
            Ok(val) => Ok(Some(val)),
            Err(QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(PersistenceError::CannotFind(format!("{}", e))),
        }
    }

    /// Save an interrupt log entry.
    pub fn save_interrupt(&self, log: &InterruptLog) -> Result<InterruptLog, PersistenceError> {
        let uuid = log.uuid.to_string();
        let schedulable_uuid = log.schedulable_uuid.to_string();

        match self.db.execute(
            "INSERT INTO interrupt_log (uuid, schedulable_uuid, kind, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![uuid, schedulable_uuid, log.kind.as_str(), log.created_at],
        ) {
            Ok(_) => Ok(InterruptLog {
                uuid: log.uuid,
                schedulable_uuid: log.schedulable_uuid,
                kind: log.kind,
                created_at: log.created_at,
            }),
            Err(e) => Err(PersistenceError::CannotSave(format!("{}", e))),
        }
    }

    /// Fetch interrupt logs for a specific schedulable, ordered by created_at.
    pub fn interrupts_for(
        &self,
        schedulable_uuid: SqlUuid,
    ) -> Result<Vec<InterruptLog>, PersistenceError> {
        let uuid_s = schedulable_uuid.to_string();

        let mut stmt = self
            .db
            .prepare(
                "SELECT uuid, schedulable_uuid, kind, created_at \
             FROM interrupt_log \
             WHERE schedulable_uuid=?1 \
             ORDER BY created_at ASC",
            )
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let rows = stmt
            .query_map(params![uuid_s], row_to_interrupt_log)
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?);
        }
        Ok(result)
    }

    /// Fetch interrupt logs within a time range (inclusive), ordered by created_at.
    pub fn interrupts_between(
        &self,
        start: i64,
        end: i64,
    ) -> Result<Vec<InterruptLog>, PersistenceError> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT uuid, schedulable_uuid, kind, created_at \
             FROM interrupt_log \
             WHERE created_at >= ?1 AND created_at <= ?2 \
             ORDER BY created_at ASC",
            )
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let rows = stmt
            .query_map(params![start, end], row_to_interrupt_log)
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?);
        }
        Ok(result)
    }

    /// Fetch all schedulables within a time range (inclusive), ordered by started_at.
    pub fn annotations_between(
        &self,
        start: i64,
        end: i64,
    ) -> Result<Vec<Annotation>, PersistenceError> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT uuid, schedulable_uuid, body, created_at \
             FROM annotations \
             WHERE created_at >= ?1 AND created_at <= ?2 \
             ORDER BY created_at ASC",
            )
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let rows = stmt
            .query_map(params![start, end], row_to_annotation)
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?);
        }
        Ok(result)
    }

    /// Fetch the most recent `limit` entries, ordered by started_at descending.
    pub fn list(&self, limit: i64) -> Result<Vec<Schedulable>, PersistenceError> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT uuid, kind, pid, duration, started_at, finished_at, cancelled_at, interruptions \
             FROM schedulables \
             ORDER BY started_at DESC \
             LIMIT ?1",
            )
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let rows = stmt
            .query_map(params![limit], row_to_schedulable)
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?);
        }
        Ok(result)
    }

    pub fn entries_between(
        &self,
        start: i64,
        end: i64,
    ) -> Result<Vec<Schedulable>, PersistenceError> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT uuid, kind, pid, duration, started_at, finished_at, cancelled_at, interruptions \
             FROM schedulables \
             WHERE started_at >= ?1 AND started_at <= ?2 \
             ORDER BY started_at ASC",
            )
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let rows = stmt
            .query_map(params![start, end], row_to_schedulable)
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?);
        }
        Ok(result)
    }

    /// Count the number of finished pomodori since the last long break (or since midnight today,
    /// whichever is more recent). Used to determine the suggested break duration.
    ///
    /// Uses the real system clock to determine "today". For deterministic behaviour in tests,
    /// see [`consecutive_pomodoro_count_at`].
    pub fn consecutive_pomodoro_count(&self) -> Result<i64, PersistenceError> {
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        self.consecutive_pomodoro_count_at(now_ts)
    }

    /// Like [`consecutive_pomodoro_count`], but uses the given Unix timestamp as the reference
    /// for determining the current day. This allows deterministic testing.
    pub fn consecutive_pomodoro_count_at(&self, now_ts: i64) -> Result<i64, PersistenceError> {
        use chrono::{Local, TimeZone};

        // Get midnight of the day containing now_ts
        let now_dt = Local.timestamp_opt(now_ts, 0).single().unwrap();
        let midnight = now_dt.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let midnight_ts = Local
            .from_local_datetime(&midnight)
            .single()
            .unwrap()
            .timestamp();

        // Find the most recent long break's finished_at (since midnight)
        let last_long_break_ts: i64 = self
            .db
            .query_row(
                "SELECT COALESCE(MAX(finished_at), 0) FROM schedulables \
                 WHERE kind = 'break' AND finished_at != 0 AND duration >= 10 AND finished_at >= ?1",
                params![midnight_ts],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let since = std::cmp::max(midnight_ts, last_long_break_ts);

        // Count finished pomodori since that timestamp
        self.db
            .query_row(
                "SELECT COUNT(*) FROM schedulables \
                 WHERE kind = 'pomodoro' AND finished_at != 0 AND finished_at >= ?1",
                params![since],
                |row| row.get(0),
            )
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))
    }

    /// Directly insert a finished pomodoro (for external log).
    /// The entry is inserted with pid=NULL, finished_at set, and the no-overlap trigger
    /// (Rule #1) is checked.
    pub fn save_external_finished(&self, s: &Schedulable) -> Result<Schedulable, PersistenceError> {
        let uuid = s.uuid.to_string();

        match self.db.execute(
            "INSERT INTO schedulables (uuid, kind, pid, duration, started_at, finished_at, interruptions) \
             VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6)",
            params![uuid, s.kind, s.duration, s.started_at, s.finished_at, s.interruptions],
        ) {
            Ok(_) => self.find_by_uuid(s.uuid),
            Err(e) => {
                let msg = format!("{}", e);
                if msg.contains("Rule #1") {
                    return Err(PersistenceError::OverlappingTimeRange);
                }
                Err(PersistenceError::CannotSave(msg))
            }
        }
    }

    pub fn save(&self, s: &Schedulable) -> Result<Schedulable, PersistenceError> {
        let uuid = s.uuid.to_string();

        match s.status() {
            Status::New => {Err(PersistenceError::CannotSave(format!("{} has not been started; cannot save", s)))},
            Status::Active | Status::Stale => {
                match self.db.execute(
                    "INSERT INTO schedulables (pid, kind, uuid, duration, started_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![s.pid, s.kind, uuid, s.duration, s.started_at],
                ) {
                    Ok(_) => {
                        Ok(self.find_by_uuid(s.uuid).expect("Could not find the inserted"))
                    },
                    Err(e) => {
                         if let Ok(option) = self.active() {
                             match option {
                                 Some(existing) => return Err(PersistenceError::AlreadyRunning(existing.pid)),
                                 None => return Err(PersistenceError::CannotSave(format!("{} could not be inseted as active, but there was no active Pomodoro or break found, either.", s))),
                             }
                         };
                        Err(PersistenceError::CannotSave(format!("{}", e)))
                    }
                }
            }
            Status::Cancelled => {
                match self.db.execute(
                        "UPDATE schedulables SET pid = NULL, cancelled_at = ?2 WHERE uuid == ?1;",
                        params![uuid, s.cancelled_at],
                    ){
                    Ok(_) => {
                        Ok(self.find_by_uuid(s.uuid).expect("Could not find the cancelled"))
                    },
                    Err(e) => {Err(PersistenceError::CannotUpdate(format!("{}", e)))}
                }
            }
            Status::Finished => {
                match self.db.execute(
                        "UPDATE schedulables SET pid = NULL, finished_at = ?2 WHERE uuid == ?1;",
                        params![uuid, s.finished_at],
                    ){
                    Ok(_) => {
                        Ok(self.find_by_uuid(s.uuid).expect("Could not find the finished"))
                    },
                    Err(e) => {Err(PersistenceError::CannotUpdate(format!("{}", e)))}
                }
            }
        }
    }
}
