-- Rule #1: No overlapping time ranges for any entry.
-- At any given time there must be at most one pomodoro or break.
--
-- The effective time range of an entry is:
--   [started_at, COALESCE(finished_at, cancelled_at, infinity))
--
-- Overlap exists when old.start < new.end AND new.start < old.end.
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
