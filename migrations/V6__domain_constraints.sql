-- V6: Domain model constraints
--
--  1. duration > 0 and duration <= 480 (8 hours)
--  2. interruptions >= 0
--  3. Enable foreign key enforcement
--

-- First, repair any existing data that would violate new constraints.
-- This makes the migration safe for existing databases.

UPDATE schedulables
   SET duration = 25
 WHERE duration IS NULL OR duration <= 0;

UPDATE schedulables
   SET duration = 480
 WHERE duration > 480;

UPDATE schedulables
   SET interruptions = 0
 WHERE interruptions IS NULL OR interruptions < 0;

-- Recreate the table with the new constraints.
-- SQLite does not support ALTER TABLE ADD CHECK, so we rebuild.

CREATE TABLE IF NOT EXISTS schedulables_new (
  uuid            TEXT NOT NULL PRIMARY KEY,
  kind            TEXT NOT NULL DEFAULT 'pomodoro',
  pid             INTEGER,
  duration        INTEGER NOT NULL DEFAULT 25,
  started_at      INTEGER NOT NULL,
  finished_at     INTEGER,
  cancelled_at    INTEGER,
  interruptions   INTEGER NOT NULL DEFAULT 0,
  CHECK ( pid >= 0 ),
  CHECK ( kind == 'pomodoro' OR kind == 'break' ),
  CHECK (
         -- active
             (finished_at IS NULL AND cancelled_at IS NULL)
         -- finished
         OR  (      started_at IS NOT NULL
                AND finished_at IS NOT NULL
                AND cancelled_at IS NULL
                AND finished_at >= started_at
             )
         -- cancelled
         OR  (      started_at IS NOT NULL
                AND finished_at IS NULL
                AND cancelled_at IS NOT NULL
                AND cancelled_at >= started_at
             )
        ),
  CHECK ( duration > 0 ),
  CHECK ( duration <= 480 ),
  CHECK ( interruptions >= 0 )
);

INSERT INTO schedulables_new SELECT * FROM schedulables;
DROP TABLE schedulables;
ALTER TABLE schedulables_new RENAME TO schedulables;

-- Recreate triggers and indexes that were on the old table

CREATE TRIGGER IF NOT EXISTS
  singularity_pid
BEFORE INSERT ON
  schedulables
BEGIN
  SELECT CASE WHEN
    (SELECT COUNT(*) FROM schedulables WHERE PID IS NOT NULL) > 0
  THEN
    RAISE(FAIL, "Cannot have two PIDs running at the same time")
  END;
END;

CREATE UNIQUE INDEX IF NOT EXISTS
  singularity_state
ON
  schedulables(started_at)
WHERE
    started_at IS NOT NULL
  AND
    finished_at IS NULL
  AND
    cancelled_at IS NULL
;

CREATE TRIGGER IF NOT EXISTS check_no_overlap
BEFORE INSERT ON schedulables
BEGIN
  SELECT CASE WHEN
    EXISTS (
      SELECT 1 FROM schedulables
      WHERE started_at < COALESCE(NEW.finished_at, NEW.cancelled_at, 9223372036854775807)
        AND COALESCE(NEW.started_at, 0) < COALESCE(finished_at, cancelled_at, 9223372036854775807)
    )
  THEN
    RAISE(FAIL, 'Time range overlaps with an existing entry (Rule #1)')
  END;
END;
