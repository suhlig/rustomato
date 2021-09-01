use super::{Kind, Schedulable, SqlUuid, Status};
use refinery::embed_migrations;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::Error::QueryReturnedNoRows;
use rusqlite::OpenFlags;
use std::fmt;
use url::Url;

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
    pub fn from_str(location: &str) -> Self {
        embed_migrations!("migrations");
        let mut conn = Connection::open_with_flags(
            location,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI,
        )
        .expect("opening database connection");
        migrations::runner().run(&mut conn).unwrap();
        Self { db: conn }
    }

    pub fn from_url(location: &Url) -> Self {
        Self::from_str(location.as_str())
    }

    pub fn active(&self) -> Result<Option<Schedulable>, PersistenceError> {
        match self.db.query_row(
            "SELECT uuid from schedulables where pid IS NOT NULL",
            [],
            |row| row.get(0).into(), // TODO Do we need the into?
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

        return match self.db.query_row(
            "SELECT uuid, kind, pid, duration, started_at, finished_at, cancelled_at from schedulables where uuid=?1",
            params![uuid_s],
            |row| Ok(Schedulable {
            uuid: uuid,
            kind: Kind::from(row.get(1).expect("unable to fetch kind")).expect("unable to convert kind"),
            pid: match row.get(2) {
                Ok(val) => val,
                Err(_) => 0,
            },
            duration: row.get(3).expect("unable to convert duration"),
            started_at: row.get(4).expect("unable to convert started_at"),
            finished_at: match row.get(5) {
                Ok(val) => val,
                Err(_) => 0,
            },
            cancelled_at: match row.get(6) {
                Ok(val) => val,
                Err(_) => 0,
            },
        })) {
            Ok(val) => Ok(val),
            Err(e) => Err(PersistenceError::CannotFind(format!("{}", e)))
        };
    }

    pub fn save(&self, s: &Schedulable) -> Result<Schedulable, PersistenceError> {
        let uuid = s.uuid.to_string();

        match s.status() {
            Status::New => {Err(PersistenceError::CannotSave(format!("{} has not been started; cannot save", s)))},
            Status::Active => {
                match self.db.execute(
                    "INSERT INTO schedulables (pid, kind, uuid, duration, started_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![s.pid, s.kind, uuid, s.duration, s.started_at],
                ) {
                    Ok(_) => {
                        Ok(self.find_by_uuid(s.uuid).expect("Could not find the inserted"))
                    },
                    Err(e) => {
                        match self.active() {
                            Ok(option) => {
                                match option {
                                    Some(existing) => return Err(PersistenceError::AlreadyRunning(existing.pid)),
                                    None => return Err(PersistenceError::CannotSave(format!("{} could not be inseted as active, but there was no active Pomodoro or break found, either.", s))),
                                }
                            },
                            Err(_) => (), // not sure if we care at this point
                        };
                        return Err(PersistenceError::CannotSave(format!("{}", e)))
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
