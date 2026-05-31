# Hooks

# Quick start

```sh
rustomato init
```

This creates the `hooks/` directory (inside `$RUSTOMATO_ROOT`) with executable sample scripts for every hook. Each script exits `0` and does nothing by default.

# Available hooks

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

# How hooks are invoked

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

# Exit code semantics

- **`before-*` hooks**: exit `0` to allow the operation to proceed. Any non-zero exit **aborts** the operation, and rustomato exits non-zero itself.
- **`after-*` hooks**: the operation has already completed. A non-zero exit is logged as a warning (in `--verbose` mode) but has no effect on the operation.

# What hooks receive

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

# Timeout

A hook that runs longer than **3 seconds** is killed (`SIGKILL`). This prevents a misbehaving or hanging hook from blocking the timer.

The timeout can be changed via the `RUSTOMATO_HOOK_TIMEOUT` environment variable (value in milliseconds):

```sh
# Give hooks 10 seconds instead of 3
export RUSTOMATO_HOOK_TIMEOUT=10000
```

# Security

- Only files **inside** `$RUSTOMATO_ROOT/hooks/` are ever executed.
- Only files with the **executable bit** (`+x`) are invoked; stray files are silently ignored.
- Hooks run with the same privileges as the `rustomato` process (typically the current user).
- Use the `--no-hooks` flag to disable all hooks in case a broken hook prevents normal operation:

  ```sh
  rustomato --no-hooks pomodoro start
  ```

# Examples

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
