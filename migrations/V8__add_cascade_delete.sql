-- V8: Add ON DELETE CASCADE to foreign keys on annotations and interrupt_log.
--
-- When a schedulable is deleted, its annotations and interrupt log entries
-- are automatically removed. This keeps the database consistent without
-- requiring manual cleanup in application code.

-- Recreate annotations with ON DELETE CASCADE
CREATE TABLE IF NOT EXISTS annotations_new (
  uuid              TEXT NOT NULL PRIMARY KEY,
  schedulable_uuid  TEXT NOT NULL,
  body              TEXT NOT NULL,
  created_at        INTEGER NOT NULL,
  FOREIGN KEY (schedulable_uuid) REFERENCES schedulables(uuid) ON DELETE CASCADE
);

INSERT INTO annotations_new SELECT * FROM annotations;
DROP TABLE annotations;
ALTER TABLE annotations_new RENAME TO annotations;

-- Recreate interrupt_log with ON DELETE CASCADE
CREATE TABLE IF NOT EXISTS interrupt_log_new (
  uuid              TEXT NOT NULL PRIMARY KEY,
  schedulable_uuid  TEXT NOT NULL,
  kind              TEXT NOT NULL CHECK (kind IN ('internal', 'external')),
  created_at        INTEGER NOT NULL,
  FOREIGN KEY (schedulable_uuid) REFERENCES schedulables(uuid) ON DELETE CASCADE
);

INSERT INTO interrupt_log_new SELECT * FROM interrupt_log;
DROP TABLE interrupt_log;
ALTER TABLE interrupt_log_new RENAME TO interrupt_log;
