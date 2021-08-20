DROP TABLE IF EXISTS schedulables;

CREATE TABLE schedulables (
  uuid            TEXT NOT NULL PRIMARY KEY,
  kind            TEXT NOT NULL DEFAULT 'pomodoro',
  duration        INTEGER,
  started_at      INTEGER NOT NULL,
  finished_at     INTEGER,
  cancelled_at    INTEGER,
  CHECK ( kind == 'pomodoro' OR kind == 'break' ),
  CHECK (
         -- active
             (finished_at IS NULL AND cancelled_at IS NULL)
         -- finished
         OR  (finished_at IS NOT NULL AND cancelled_at IS NULL)
         -- cancelled
         OR  (finished_at IS NULL AND cancelled_at IS NOT NULL)
        )
);

-- At any given time, only one schedulable may be in active state
CREATE UNIQUE INDEX
  singularity
ON
  schedulables(started_at)
WHERE
    started_at IS NOT NULL
  AND
    finished_at IS NULL
  AND
    cancelled_at IS NULL
;

DROP VIEW IF EXISTS active;
CREATE VIEW
  active
AS
  SELECT
    kind,
    uuid,
    started_at
  FROM
    schedulables
  WHERE
    started_at IS NOT NULL AND finished_at IS NULL AND cancelled_at IS NULL
;

DROP VIEW IF EXISTS finished;
CREATE VIEW
  finished
AS
  SELECT
    kind,
    uuid,
    started_at,
    finished_at
  FROM
    schedulables
  WHERE
    started_at IS NOT NULL AND finished_at IS NOT NULL AND cancelled_at IS NULL
;

DROP VIEW IF EXISTS cancelled;
CREATE VIEW
  cancelled
AS
  SELECT
    kind,
    uuid,
    started_at,
    cancelled_at
  FROM
    schedulables
  WHERE
    kind == 'pomodoro' AND started_at IS NOT NULL AND finished_at IS NULL AND cancelled_at IS NOT NULL
;

DROP VIEW IF EXISTS human;
CREATE VIEW
  human
AS
  SELECT
    kind,
    uuid,
    duration,
    datetime(started_at, 'unixepoch', 'localtime') as started_at,
    datetime(finished_at, 'unixepoch', 'localtime') as finished_at,
    datetime(cancelled_at, 'unixepoch', 'localtime') as cancelled_at
  FROM
    schedulables
;
