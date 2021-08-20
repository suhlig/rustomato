use super::{Schedulable, Status};
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Repository {
    db: Connection,
}

impl Repository {
    pub fn new(location: &Path) -> Self {
        let db = Connection::open(location).expect("Failed to open database");

        Self { db: db }
    }

    pub fn save(&self, s: &Schedulable) {
        match s.status() {
            Status::Active => {
                self.db.execute(
                    "INSERT INTO schedulables (uuid, started_at) VALUES (?1, strftime('%s','now'))",
                    params![s.uuid.to_simple().to_string()],
                )
                .expect("Failed to insert schedulable");
            }
            Status::Cancelled => {
                self.db
                    .execute(
                        "UPDATE
                        schedulables
                    SET
                        cancelled_at = strftime('%s','now')
                    WHERE
                        uuid == ?1
                    ;",
                        params![s.uuid.to_simple().to_string()],
                    )
                    .expect("Failed to insert schedulable");
            }
            Status::Finished => {
                self.db
                    .execute(
                        "UPDATE
                        schedulables
                    SET
                        finished_at = strftime('%s','now')
                    WHERE
                        uuid == ?1
                    ;",
                        params![s.uuid.to_simple().to_string()],
                    )
                    .expect("Failed to insert schedulable");
            }
        }
    }
}
