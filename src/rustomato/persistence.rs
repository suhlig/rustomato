use super::{Kind, Schedulable, Status};
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::fmt;
use uuid::Uuid;

pub struct Repository {
    db: Connection,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum PersistenceError {
    CannotSave,
    CannotUpdate,
    CannotFind,
    AlreadyRunning,
}

impl fmt::Display for PersistenceError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
        PersistenceError::CannotSave => write!(f, "Cannot save"),
        PersistenceError::CannotUpdate => write!(f, "Cannot update"),
        PersistenceError::CannotFind => write!(f, "Cannot find"),
        PersistenceError::AlreadyRunning => write!(f, "Already running"),
    }
  }
}

impl Repository {
    pub fn new(location: &PathBuf) -> Self {
        Self {
            db: Connection::open(location).expect("Failed to open database"),
        }
    }

    pub fn has_running(&self) -> Result<bool, PersistenceError> {
        let result: u64 = self
            .db
            .query_row(
                "SELECT count(*) as active from active where pid IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .expect("Unable to query for active schedulables");

        return match result {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(PersistenceError::AlreadyRunning),
        };
    }

    pub fn find_by_uuid(&self, uuid: Uuid) -> Result<Schedulable, PersistenceError> {
        let uuid_s = uuid.to_simple().to_string();

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
            Err(_) => Err(PersistenceError::CannotFind)
        };
    }

    pub fn save(&self, s: &Schedulable) -> Result<Schedulable, PersistenceError> {
        let uuid = s.uuid.to_simple().to_string();

        match s.status() {
            Status::Active => {
                match self.db.execute(
                    "INSERT INTO schedulables (pid, kind, uuid, duration, started_at) VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))",
                    params![s.pid, s.kind, uuid, s.duration],
                ) {
                    Ok(_) => {
                        return Ok(self.find_by_uuid(s.uuid).expect("Could not find the inserted"))
                    },
                    Err(_) => {
                        if self.has_running()? {
                            return Err(PersistenceError::AlreadyRunning);
                        } else {
                            return Err(PersistenceError::CannotSave);
                        }
                    }
                }
            }
            Status::Cancelled => {
                match self.db.execute(
                        "UPDATE schedulables SET pid = NULL, cancelled_at = strftime('%s','now') WHERE uuid == ?1;",
                        params![uuid],
                    ){
                    Ok(_) => {
                        return Ok(self.find_by_uuid(s.uuid).expect("Could not find the updated"))
                    },
                    Err(_) => {return Err(PersistenceError::CannotUpdate)}
                }
            }
            Status::Finished => {
                match self.db.execute(
                        "UPDATE schedulables SET pid = NULL, finished_at = strftime('%s','now') WHERE uuid == ?1;",
                        params![uuid],
                    ){
                    Ok(_) => {
                        return Ok(self.find_by_uuid(s.uuid).expect("Could not find the updated"))
                    },
                    Err(_) => {return Err(PersistenceError::CannotUpdate)}
                }
            }
        }
    }
}
