use super::{Kind, Schedulable, SqlUuid, Status};
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
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistenceError::CannotSave(e) => write!(f, "Cannot save: {}", e),
            PersistenceError::CannotUpdate(e) => write!(f, "Cannot update: {}", e),
            PersistenceError::CannotFind(e) => write!(f, "Cannot find: {}", e),
            PersistenceError::AlreadyRunning(pid) => write!(f, "Already running as {}", pid),
        }
    }
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
        crate::migration::run(&db);
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
            |row| Ok(Schedulable {
            uuid,
            kind: Kind::from(row.get(1).expect("unable to fetch kind")).expect("unable to convert kind"),
            pid: row.get(2).unwrap_or(0),
            duration: row.get(3).expect("unable to convert duration"),
            started_at: row.get(4).expect("unable to convert started_at"),
            finished_at: row.get(5).unwrap_or(0),
            cancelled_at: row.get(6).unwrap_or(0),
            interruptions: row.get(7).unwrap_or(0),
        })) {
            Ok(val) => Ok(val),
            Err(e) => Err(PersistenceError::CannotFind(format!("{}", e)))
        }
    }

    pub fn today(&self) -> Result<Vec<Schedulable>, PersistenceError> {
        use chrono::Local;

        let today = Local::now();
        let start_of_day = today
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .and_then(|dt| dt.and_local_timezone(Local).earliest())
            .map(|dt| dt.timestamp())
            .unwrap_or(0);
        let end_of_day = today
            .date_naive()
            .and_hms_opt(23, 59, 59)
            .and_then(|dt| dt.and_local_timezone(Local).earliest())
            .map(|dt| dt.timestamp())
            .unwrap_or(i64::MAX);

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
            .query_map(params![start_of_day, end_of_day], |row| {
                let uuid_str: String = row.get(0)?;
                let kind_str: String = row.get(1)?;
                Ok(Schedulable {
                    uuid: SqlUuid(Uuid::parse_str(&uuid_str).unwrap_or_default()),
                    kind: Kind::from(kind_str)
                        .unwrap_or_else(|e| panic!("invalid kind in DB: {}", e.offender)),
                    pid: row.get(2).unwrap_or(0),
                    duration: row.get(3).unwrap_or(0),
                    started_at: row.get(4).unwrap_or(0),
                    finished_at: row.get(5).unwrap_or(0),
                    cancelled_at: row.get(6).unwrap_or(0),
                    interruptions: row.get(7).unwrap_or(0),
                })
            })
            .map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| PersistenceError::CannotFind(format!("{}", e)))?);
        }
        Ok(result)
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

    /// Find the most recently finished pomodoro across all time.
    pub fn most_recently_finished_pomodoro(&self) -> Result<Option<Schedulable>, PersistenceError> {
        match self.db.query_row(
            "SELECT uuid, kind, pid, duration, started_at, finished_at, cancelled_at, interruptions \
             FROM schedulables \
             WHERE kind = 'pomodoro' AND finished_at != 0 \
             ORDER BY finished_at DESC \
             LIMIT 1",
            [],
            |row| {
                let uuid_str: String = row.get(0)?;
                let kind_str: String = row.get(1)?;
                Ok(Schedulable {
                    uuid: SqlUuid(
                        Uuid::parse_str(&uuid_str).expect("parsing UUID from database"),
                    ),
                    kind: Kind::from(kind_str)
                        .unwrap_or_else(|e| panic!("invalid kind in DB: {}", e.offender)),
                    pid: row.get(2).unwrap_or(0),
                    duration: row.get(3).unwrap_or(0),
                    started_at: row.get(4).unwrap_or(0),
                    finished_at: row.get(5).unwrap_or(0),
                    cancelled_at: row.get(6).unwrap_or(0),
                    interruptions: row.get(7).unwrap_or(0),
                })
            },
        ) {
            Ok(val) => Ok(Some(val)),
            Err(QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(PersistenceError::CannotFind(format!("{}", e))),
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
