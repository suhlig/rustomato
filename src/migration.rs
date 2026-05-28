use rusqlite::{Connection, params};

const MIGRATIONS: &[(&str, &str)] = &[
    ("V1__initial", include_str!("../migrations/V1__initial.sql")),
    (
        "V2__add_constraint",
        include_str!("../migrations/V2__add_constraint.sql"),
    ),
    (
        "V3__add_interruptions",
        include_str!("../migrations/V3__add_interruptions.sql"),
    ),
    (
        "V4__add_annotations",
        include_str!("../migrations/V4__add_annotations.sql"),
    ),
];

pub fn run(conn: &Connection) {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS _migrations (name TEXT NOT NULL PRIMARY KEY);")
        .expect("creating _migrations table");

    for (name, sql) in MIGRATIONS {
        let already_run: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM _migrations WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !already_run {
            let wrapped = format!("BEGIN;\n{sql}\nCOMMIT;");
            conn.execute_batch(&wrapped)
                .unwrap_or_else(|e| panic!("running migration {name}: {e}"));
            conn.execute("INSERT INTO _migrations (name) VALUES (?1)", params![name])
                .unwrap_or_else(|e| panic!("recording migration {name}: {e}"));
        }
    }
}
