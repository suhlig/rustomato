Simple [Pomodoro](https://en.wikipedia.org/wiki/Pomodoro_Technique) timer written in Rust

# Usage

```command
$ rustomato pomodoro start           # Starts a new Pomodoro. Auto-finishes the currently active break if there is one.
$ rustomato pomodoro interrupt       # Records an interruption on the active (or most recently finished) Pomodoro.
$ rustomato pomodoro annotate <text> # Annotates the active, or the most recently completed, Pomodoro with the given text.
$ rustomato pomodoro log             # Log an externally completed pomodoro.
$ rustomato break start              # Starts a break. Auto-finishes the currently active Pomodoro if there is one.
```

`pomodoro` and `break` will block until the time is over. If the command is interrupted with Control-C (`SIGINT`), the currently running Pomodoro is cancelled immediately. If a break is currently running, it is finished.

# Rule #1

There must never be more than one pomodoro [XOR](http://en.wikipedia.org/wiki/Xor) break at any given time.

This is scoped to an instance of the database (as pointed to by `$RUSTOMATO_DATABASE_URL`). The enforcement happens at the database level via a trigger that rejects overlapping time ranges, and at the application level in the scheduler.

# State Transitions

```mermaid
stateDiagram-v2
    [*] --> New
    New --> Active : pomodoro / break start
    Active --> Finished : timer expired
    Active --> Cancelled : SIGINT (pomodoro)
    Active --> Finished : SIGINT (break)
    Active --> Stale : process dies
    Cancelled --> [*]
    Finished --> [*]
    Stale --> [*]
```

The possible application states are valid for an instance of the database (as pointed to by `$RUSTOMATO_DATABASE_URL`, which defaults to `$RUSTOMATO_ROOT/data.db`). The default for `$RUSTOMATO_ROOT` is `$HOME/.rustomato`.

The key difference between a pomodoro and a break is how they respond to interruptions and cancellation:

* a pomodoro can be interrupted (keeping it running) or cancelled (via SIGINT), whereas
* a break is simply finished — it does not accept interruptions and SIGINT finishes it rather than cancelling it.

# Breaks

Following the classic Pomodoro Technique (Cirillo), `break start` automatically picks a duration based on how many finished pomodori have been completed consecutively:

| Pomodori since last reset | Break duration |
|---|---|
| 0–3 | 5 min (short break) |
| 4, 8, 12, … | 15 min (long break) |

The counter resets after a long break (`duration >= 10`) or at midnight. Short breaks (`duration < 10`) do not reset the counter — they extend the current set. Only finished pomodori count toward the total; cancelled and stale entries are ignored.

Pass `--duration` explicitly to override the auto-calculated duration.

# Target Selection

Many commands accept a **target** to determine which pomodoro or break to act on. The same resolution logic is used whether the target comes from a positional argument, `--target`, or a shortcut like `-1`.

## Relative targets

| Target | Resolves to |
|---|---|
| `0` | The entry with a PID — the currently running (or stale) one. Error if none. |
| `-1` | The most recently started entry. Skips the active entry if present. |
| `-2` … `-9` | The second, third, … ninth most recently started entry. |

When a command has an implicit **kind context** (`pomodoro` or `break`), `-N` only counts entries of that kind. For example, `rustomato pomodoro annotate -1` finds the most recently started pomodoro, skipping any breaks.

### Default (no explicit target)

| Condition | Behaviour |
|---|---|
| Something is running (or stale) | Targets the active entry, as if `0` were given. |
| Nothing is running and no stale entry | Targets the most recent entry, as if `-1` were given. |

Commands with kind-specific constraints may override this default (for example, `interrupt` targets the active pomodoro first, falling back to the most recent pomodoro, but never targets a break).

## Other target formats

These are also accepted by `--target` and `rustomato show`:

| Format | Example | Description |
|---|---|---|
| UUID prefix | `a1b2c3` | An abbreviated or full UUID (minimum 6 chars). |
| Today's time | `14:30` | The entry running at that time today. |
| RFC 3339 | `2026-05-30T14:30:00` | The entry running at that absolute time. |

## Examples

```
rustomato pomodoro annotate "review PR"      # annotate the active entry (or most recent pomodoro)
rustomato pomodoro annotate 0 "review PR"    # same, explicitly
rustomato pomodoro annotate -1 "review PR"    # most recently started pomodoro (skips active)
rustomato show 0                              # details of the active entry
rustomato show -1                              # details of the most recent entry (any kind)
rustomato break annotate -1 "good break"      # most recently started break
rustomato pomodoro interrupt -1               # record interrupt on most recent pomodoro

# Interrupts

When you call `rustomato pomodoro interrupt`, the current pomodoro's interruption counter is incremented by one. The pomodoro **continues running** -- an interrupt does not cancel or finish it.

`interrupt` uses the unified target resolution: tries the active pomodoro first (`0`), then falls back to the most recent pomodoro (`-1`). If neither exists, it errors.

Interrupt hooks receive two additional environment variables:

| Variable | Example | Description |
|---|---|---|
| `RUSTOMATO_INTERRUPT_KIND` | `internal` | `internal` or `external` |
| `RUSTOMATO_INTERRUPTIONS` | `3` | Total interruption count on this pomodoro |

Use `--kind internal` (default) or `--kind external` to classify the interruption. Internal interruptions are self-inflicted (e.g. checking your phone); external ones are caused by the environment (e.g. a colleague knocking).

# Annotations

Annotations let you attach arbitrary text to a pomodoro or break. This is useful for noting what you worked on, capturing thoughts mid-session, or tagging entries for later review.

```sh
rustomato pomodoro annotate "Reviewed PR #42"
rustomato break annotate "Coffee break"
```

If no annotation text is given on the command line, rustomato reads from stdin, which lets you pipe in content:

```sh
echo "Fixed the flaky test" | rustomato pomodoro annotate
```

## Interactive annotation with `sk`

For users who want to select a pomodoro interactively with a fuzzy-finder preview before annotating, install [skim](https://github.com/skim-rs/skim) and add this shell function:

```sh
rustomato-annotate() {
  local target
  target=$(
    rustomato list --no-header \
      | sk --delimiter ' ' --with-nth 1 \
           --preview 'rustomato show {1}' \
           --layout=reverse \
      | cut -d' ' -f1
  ) && rustomato pomodoro annotate --target "$target" "$@"
}
```

What this does:

| Step | Description |
|---|---|
| `rustomato list --no-header` | List recent entries, one per line, no header |
| `sk --delimiter ' ' --with-nth 1` | Show only the UUID column; `{1}` in preview refers to the UUID |
| `sk --preview 'rustomato show {1}'` | Show full details of the highlighted entry |
| `cut -d' ' -f1` | Extract the UUID from the selected line |
| `rustomato annotate --target "$target"` | Annotate with the chosen target |

The preview window shows the full details of each entry as you arrow through the list. When you press enter, the annotation is applied to the selected pomodoro.

# Hooks

Rustomato can run user-provided scripts — **hooks** — at key state transitions. Hooks live in `$RUSTOMATO_ROOT/hooks/` and are looked up by exact filename.

## Quick start

```sh
rustomato init
```

This creates the `hooks/` directory (inside `$RUSTOMATO_ROOT`) with executable sample scripts for every hook. Each script exits `0` and does nothing by default.

## Available hooks

| Hook | Fires | Can abort? |
|---|---|---|
| `before-start-pomodoro` | Before a pomodoro starts | yes |
| `after-start-pomodoro` | After a pomodoro started | no |
| `before-finish-pomodoro` | Pomodoro timer expired | yes |
| `after-finish-pomodoro` | Pomodoro finished | no |
| `before-cancel-pomodoro` | Ctrl-C during a pomodoro | yes |
| `after-cancel-pomodoro` | Pomodoro cancelled | no |
| `before-interrupt-pomodoro` | Before an interrupt is recorded | yes |
| `after-interrupt-pomodoro` | Interrupt recorded | no |
| `before-annotate-pomodoro` | Before an annotation is added to a pomodoro | yes |
| `after-annotate-pomodoro` | Annotation added to a pomodoro | no |
| `before-annotate-break` | Before an annotation is added to a break | yes |
| `after-annotate-break` | Annotation added to a break | no |
| `before-log-pomodoro` | Before an external pomodoro is logged | yes |
| `after-log-pomodoro` | External pomodoro logged | no |
| `before-start-break` | Before a break starts | yes |
| `after-start-break` | After a break started | no |
| `before-finish-break` | Break timer expired or Ctrl-C | yes |
| `after-finish-break` | Break finished | no |

## How hooks are invoked

Rustomato executes the hook file directly using the OS `execve`-equivalent, which means the file must be an executable with a valid shebang line or a binary. Any language works:

```sh
#!/usr/bin/env bash
# …
exit 0
```

```python
#!/usr/bin/env python3
import sys
sys.exit(0)
```

Hooks that do not have the executable bit (`+x`) set are silently skipped (shown in `--verbose` mode).

## Exit code semantics

- **`before-*` hooks**: exit `0` to allow the operation to proceed. Any non-zero exit **aborts** the operation, and rustomato exits non-zero itself.
- **`after-*` hooks**: the operation has already completed. A non-zero exit is logged as a warning (in `--verbose` mode) but has no effect on the operation.

## What hooks receive

**First argument (`$1`):** the hook name, e.g. `before-start-pomodoro`. This lets a single script dispatch on the hook name if desired.

**Environment variables** (set for every hook):

| Variable | Example | Description |
|---|---|---|
| `RUSTOMATO_ROOT` | `/Users/me/.rustomato` | Rustomato data directory |
| `RUSTOMATO_HOOK` | `before-start-pomodoro` | The hook being run |
| `RUSTOMATO_KIND` | `pomodoro` | `pomodoro` or `break` |
| `RUSTOMATO_UUID` | `967a14ee45da44a49049794aeea7c292` | Unique identifier |
| `RUSTOMATO_DURATION` | `25` | Duration in minutes |
| `RUSTOMATO_STARTED_AT` | `1748464846` | Unix timestamp of start |
| `RUSTOMATO_FINISHED_AT` | `1748464864` | Unix timestamp (after-* only) |
| `RUSTOMATO_CANCELLED_AT` | `1748464864` | Unix timestamp (after-* only) |
| `RUSTOMATO_INTERRUPT_KIND` | `internal` | Kind of interrupt (`internal` or `external`; interrupt hooks only) |
| `RUSTOMATO_INTERRUPTIONS` | `2` | Total interruption count on this pomodoro or break (interrupt hooks only) |
| `RUSTOMATO_ANNOTATION` | `Reviewed PR #42` | Annotation body (annotate hooks only) |

## Timeout

A hook that runs longer than **3 seconds** is killed (`SIGKILL`). This prevents a misbehaving or hanging hook from blocking the timer.

The timeout can be changed via the `RUSTOMATO_HOOK_TIMEOUT` environment variable (value in milliseconds):

```sh
# Give hooks 10 seconds instead of 3
export RUSTOMATO_HOOK_TIMEOUT=10000
```

## Security

- Only files **inside** `$RUSTOMATO_ROOT/hooks/` are ever executed.
- Only files with the **executable bit** (`+x`) are invoked; stray files are silently ignored.
- Hooks run with the same privileges as the `rustomato` process (typically the current user).
- Use the `--no-hooks` flag to disable all hooks in case a broken hook prevents normal operation:

  ```sh
  rustomato --no-hooks pomodoro start
  ```

## Examples

**Desktop notification when a pomodoro finishes** (`after-finish-pomodoro`):

```sh
#!/usr/bin/env bash
terminal-notifier -message "Pomodoro is over" \
                  -title rustomato    \
                  -sound glass        \
                  -group rustomato
exit 0
```

**Prevent starting a pomodoro after a certain hour** (`before-start-pomodoro`):

```sh
#!/usr/bin/env bash
if [ "$(date +%H)" -ge 22 ]; then
  echo "No pomodori after 10 PM!"
  exit 1
fi
exit 0
```

**Log cancelled pomodori to a file** (`after-cancel-pomodoro`):

```sh
#!/usr/bin/env bash
echo "$RUSTOMATO_STARTED_AT Cancelled $RUSTOMATO_KIND $RUSTOMATO_UUID" \
  >> "$HOME/.rustomato_cancellations"
exit 0
```

# Installation

## Homebrew

```sh
brew install suhlig/tap/rustomato
```

This formula automatically installs shell completions for bash, zsh, and fish, as well as the man page.

## Manual

| File | Architecture | Typical Hardware |
|---|---|---|
| `rustomato-linux-amd64.tar.gz` | x86_64 | Desktop PCs, laptops |
| `rustomato-linux-arm64.tar.gz` | ARM 64-bit | Raspberry Pi 3/4/5 (64-bit OS) |
| `rustomato-linux-armv7.tar.gz` | ARM 32-bit | Raspberry Pi 2/3/4/5 (32-bit OS) |
| `rustomato-darwin-amd64.tar.gz` | x86_64 | Intel Macs |
| `rustomato-darwin-arm64.tar.gz` | ARM 64-bit | Apple Silicon Macs |

**Linux**

```sh
arch=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/;s/armv7l/armv7/') && curl -sL "https://github.com/suhlig/rustomato/releases/latest/download/rustomato-linux-${arch}.tar.gz" | tar xz && sudo mv rustomato /usr/local/bin
```

## Man page

Rustomato ships a man page that you can view in several ways:

**From the binary** (no installation needed):

```sh
rustomato man | man --local-file -
```

Or with `--help` output directed to a pager:

```sh
rustomato man | less
```

**Install system-wide** (Linux/macOS):

```sh
rustomato man > /usr/local/share/man/man1/rustomato.1
mandb  # on Linux only
```

**With Homebrew**: the man page is installed automatically and is available via `man rustomato`.

# Releasing

> Requires [git-cliff](https://git-cliff.org) (e.g. `brew install git-cliff`) and [cargo-release](https://github.com/crate-ci/cargo-release) (`cargo install cargo-release`)

Cut a new release with a single command:

```sh
cargo release patch --execute # or `minor`, or `major`
```

This will:
- Choose the next version by bumping the current one (`patch` → `0.0.11`, `minor` → `0.1.0`, …)
- Run `git cliff` to prepend the new changelog entry to `CHANGELOG.md`
- Bump the version in `Cargo.toml`
- Commit both files together
- Create a git tag (e.g. `v0.0.11`)
- Push the commit and tag to GitHub, where `release.yml` builds and publishes the artifacts

To preview without making changes, do not specify `--execute`:

```sh
cargo release patch
```

# Development

* Install and update rust with `rustup`
* Run tests with `cargo test`
  - use `cargo watch -x test` for fast iteration
  - install the plugin with `cargo install cargo-watch`
* Run the app: `cargo run -- pomodoro`
* Run `pre-commit install` to install the pre-commit hook

# TODO

* Allow `--target` for the cancel command, including shortcuts, in order to cancel a specific pomodoro or break retroactively (e.g. we let the pomodoro elapse, thus it is recorded as completed, but acutally we were away from the computer for a while and chatted with a colleague)
* Is that statement even true?

  > Auto-finishes the currently active Pomodoro if there is one.

* Check and fix consistency of writing pomodoro vs. Pomodoro. Same for break.
* Show progress bar only when attached to a terminal
  - When resizing due to SIGWINCH, shall we clear the progress bar and redraw it? Right now we add another line with the new size.
* Does a CSV export of pomodori and breaks make sense for externally created reports?
