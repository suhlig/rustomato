I am learning Rust by implementing a simple [Pomodoro](https://en.wikipedia.org/wiki/Pomodoro_Technique) timer.

[![.github/workflows/build.yml](https://github.com/suhlig/rustomato/actions/workflows/build.yml/badge.svg)](https://github.com/suhlig/rustomato/actions/workflows/build.yml)

# Usage

```command
$ rustomato pomodoro [start]   # Starts a new Pomodoro. Auto-finishes the currently active break if there is one.
$ rustomato pomodoro annotate  # Annotates a¹ Pomodoro.
$ rustomato pomodoro interrupt # Mark a¹ Pomodoro as interrupted.
$ rustomato pomodoro log       # Log a previously finished pomodoro.
$ rustomato break [start]      # Starts a break. Auto-finishes the currently active Pomodoro if there is one.
```
[1] the running, if there is one, or the most recently completed, or the given

* `pomodoro` and `break` will block until the time is over. If the command is interrupted with Control-C (`SIGINT`), the Pomodoro or break is finished immediately.
* Until we have hooks, here is how to use notifications:
  ```command
  $ rustomato pomodoro start && terminal-notifier -message "Pomodoro is over" -title rustomato -sound glass -group rustomato || terminal-notifier -message "Pomodoro cancelled" -title rustomato -sound glass -group rustomato
  ```

  If you prefer tmux:

  ```command
  $ rustomato pomodoro start && tmux display-message "Pomodoro is over" || tmux display-message "Pomodoro cancelled"
  ```

  Or, on a Mac:

  ```command
  $ rustomato pomodoro start && say "Pomodoro is over" || say "Pomodoro cancelled"
  ```

# Release a new version

There is a GitHub action that compiles the code, creates a new release and attaches the binaries:

```command
$ gh release create v0.0.3 --notes 'Enforce singularity (per `RUSTOMATO_ROOT`)'
```

# Notes

* Install and update rust with `rustup`
* Run: `cargo run -- pomodoro`
* Build a release manually with `cargo build --release` (binary will be found in `target/release/`)
* `rustomato pomodoro interrupt` sends `SIGUSR1` to the currently running `rustomato` process (use [signal-hook](https://crates.io/crates/signal-hook) for that)
* TODO Show progress bar only when attached to a terminal
* TODO Not sure if we need the views (beyond `db/test.sh`)
* TODO Write a test that proves that there are no two active (Pomororo XOR Break) at the same time
* TODO Annotations table, joined onto Pomodori
* TODO Interrupts table, joined onto Pomodori
