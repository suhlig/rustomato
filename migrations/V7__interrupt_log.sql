CREATE TABLE IF NOT EXISTS interrupt_log (
  uuid              TEXT NOT NULL PRIMARY KEY,
  schedulable_uuid  TEXT NOT NULL,
  kind              TEXT NOT NULL CHECK (kind IN ('internal', 'external')),
  created_at        INTEGER NOT NULL,
  FOREIGN KEY (schedulable_uuid) REFERENCES schedulables(uuid)
);
