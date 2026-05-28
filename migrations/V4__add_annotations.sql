CREATE TABLE IF NOT EXISTS annotations (
  uuid              TEXT NOT NULL PRIMARY KEY,
  schedulable_uuid  TEXT NOT NULL,
  body              TEXT NOT NULL,
  created_at        INTEGER NOT NULL,
  FOREIGN KEY (schedulable_uuid) REFERENCES schedulables(uuid)
);
