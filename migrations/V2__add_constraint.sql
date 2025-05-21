CREATE TABLE schedulables_new (
  uuid            TEXT NOT NULL PRIMARY KEY,
  kind            TEXT NOT NULL DEFAULT 'pomodoro',
  pid             INTEGER,
  duration        INTEGER,
  started_at      INTEGER NOT NULL,
  finished_at     INTEGER,
  cancelled_at    INTEGER,
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
        )
);

INSERT INTO schedulables_new SELECT * FROM schedulables;
DROP TABLE schedulables;
ALTER TABLE schedulables_new RENAME TO schedulables;

-- Only one row may have a non-NULL pid
CREATE TRIGGER
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

-- Only one schedulable may be in active state
CREATE UNIQUE INDEX
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
