I am learning Rust by implementing a simple [Pomodoro](https://en.wikipedia.org/wiki/Pomodoro_Technique) timer.

[![.github/workflows/build.yml](https://github.com/suhlig/rustomato/actions/workflows/build.yml/badge.svg)](https://github.com/suhlig/rustomato/actions/workflows/build.yml)

# Usage

```command
rustomato pomodoro [start]   # Starts a new Pomodoro. Auto-finishes the currently active break if there is one.
rustomato pomodoro annotate  # Annotates a¹ Pomodoro.
rustomato pomodoro interrupt # Mark a¹ Pomodoro as interrupted.
rustomato pomodoro log       # Log a previously finished pomodoro.
rustomato break [start]      # Starts a break. Auto-finishes the currently active Pomodoro if there is one.
```
[1] the running, if there is one, or the most recently completed, or the given

* `pomodoro` and `break` will block until the time is over. If the command is interrupted with Control-C (`SIGINT`), the Pomodoro or break is finished immediately.
* Until we have hooks, here is how to use notifications:
  ```command
  $ rustomato pomodoro start && terminal-notifier -message "Pomodoro is over" -title rustomato || terminal-notifier -message "Pomodoro cancelled" -title rustomato
  ```

  If you prefer tmux:

  ```command
  $ rustomato pomodoro start && tmux display-message "Pomodoro is over" || tmux display-message "Pomodoro cancelled"
  ```

  Or, on a Mac:

  ```command
  $ rustomato pomodoro start && say "Pomodoro is over" || say "Pomodoro cancelled"
  ```

# Notes

* Install and update rust with `rustup`
* Run: `cargo run -- pomodoro`
* Build a release: `cargo build --release` (binary found in `target/release/`)

# WIP Persistence

```command
$ sqlite3 ~/.rustomato.sqlite3
```

```sql
CREATE TABLE schedulables (
  uuid            TEXT NOT NULL,
  started_at      INTEGER,
  finished_at     INTEGER,
  cancelled_at    INTEGER
);

-- TODO Can we set up a constraint to have either finished_at or cancelled_at as zero?

INSERT INTO schedulables (uuid, started_at) VALUES ("3de9cc4b-f731-4c3d-9c93-1700c932f218", 1629318386);

SELECT
  *,
  datetime(started_at, 'unixepoch', 'localtime') as started_at_datetime,
  datetime(finished_at, 'unixepoch', 'localtime') as finished_at_datetime,
  datetime(cancelled_at, 'unixepoch', 'localtime') as cancelled_at_datetime
FROM
  schedulables
;
