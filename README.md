I am learning Rust by implementing a simple [Pomodoro](https://en.wikipedia.org/wiki/Pomodoro_Technique) timer.

# Usage

* `rustomato break start` blocks until the time for a break is over. If the command is interrupted with Control-C (`SIGINT`), the break is finished immediately.

# Notes

* Install and update rust with `rustup`
* Run: `cargo run -- pomodoro`
* Build a release: `cargo build --release` (binary found in `target/release/`)
