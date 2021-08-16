I am learning Rust by implementing a simple [Pomodoro](https://en.wikipedia.org/wiki/Pomodoro_Technique) timer.

[![.github/workflows/build.yml](https://github.com/suhlig/rustomato/actions/workflows/build.yml/badge.svg)](https://github.com/suhlig/rustomato/actions/workflows/build.yml)

# Usage

* `rustomato break start` blocks until the time for a break is over. If the command is interrupted with Control-C (`SIGINT`), the break is finished immediately.
* Until we have hooks, here is how to use notifications:
  ```command
  $ rustomato break start -d 1 && terminal-notifier -message "Break is over" -title rustomato || terminal-notifier -message "Break cancelled" -title rustomato
  ```

# Notes

* Install and update rust with `rustup`
* Run: `cargo run -- pomodoro`
* Build a release: `cargo build --release` (binary found in `target/release/`)
