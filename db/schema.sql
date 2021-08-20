DROP TABLE IF EXISTS schedulables;

CREATE TABLE schedulables (
  uuid            TEXT NOT NULL PRIMARY KEY,
  started_at      INTEGER NOT NULL,
  finished_at     INTEGER,
  cancelled_at    INTEGER,
  CHECK (
         -- active
             (finished_at IS NULL AND cancelled_at IS NULL)
         -- finished
         OR  (finished_at IS NOT NULL AND cancelled_at IS NULL)
         -- cancelled
         OR  (finished_at IS NULL AND cancelled_at IS NOT NULL)
        )
);

-- At any given time, only one may be in active state
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
    uuid,
    started_at,
    cancelled_at
  FROM
    schedulables
  WHERE
    started_at IS NOT NULL AND finished_at IS NULL AND cancelled_at IS NOT NULL
;

DROP VIEW IF EXISTS human;
CREATE VIEW
  human
AS
  SELECT
    uuid,
    datetime(started_at, 'unixepoch', 'localtime') as started_at,
    datetime(finished_at, 'unixepoch', 'localtime') as finished_at,
    datetime(cancelled_at, 'unixepoch', 'localtime') as cancelled_at
  FROM
    schedulables
;
