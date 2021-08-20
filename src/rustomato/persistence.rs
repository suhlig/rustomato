use rusqlite::{params, Connection};
use std::path::Path;
use super::{Schedulable, Status};

pub struct Repository {
    db: Connection
}

impl Repository {
    pub fn new(location: &Path) -> Self {
        let db = Connection::open(location).expect("Failed to open database");

        Self {
            db: db
        }
    }

    pub fn save(&self, s: &Schedulable) {
        match s.status() {
            Status::New => {
                self.db.execute(
                    "INSERT INTO schedulables (uuid) VALUES (?1)",
                    params![s.uuid.to_simple().to_string()],
                )
                .expect("Failed to insert schedulable");
            }
            Status::Active => {
                self.db.execute(
                    "UPDATE
                        schedulables
                    SET
                        started_at = strftime('%s','now')
                    WHERE
                        uuid == ?1
                    ;" ,
                    params![s.uuid.to_simple().to_string()],
                )
                .expect("Failed to insert schedulable");
            }
            Status::Cancelled => {
                self.db.execute(
                    "UPDATE
                        schedulables
                    SET
                        cancelled_at = strftime('%s','now')
                    WHERE
                        uuid == ?1
                    ;" ,
                    params![s.uuid.to_simple().to_string()],
                )
                .expect("Failed to insert schedulable");
            }
            Status::Finished => {
                self.db.execute(
                    "UPDATE
                        schedulables
                    SET
                        finished_at = strftime('%s','now')
                    WHERE
                        uuid == ?1
                    ;" ,
                    params![s.uuid.to_simple().to_string()],
                )
                .expect("Failed to insert schedulable");
            }
        }
    }
}
