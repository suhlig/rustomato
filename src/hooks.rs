use super::{Kind, Schedulable, SqlUuid};
use std::fmt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[cfg(test)]
thread_local! {
    static TEST_HOOK_TIMEOUT: std::cell::RefCell<Option<Duration>> = const { std::cell::RefCell::new(None) };
}

/// Read `RUSTOMATO_HOOK_TIMEOUT` (milliseconds) or default to 3 seconds.
fn hook_timeout() -> Duration {
    #[cfg(test)]
    {
        if let Some(d) = TEST_HOOK_TIMEOUT.with(|t| *t.borrow()) {
            return d;
        }
    }
    std::env::var("RUSTOMATO_HOOK_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(3))
}

/// All hook events that rustomato can fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    BeforeStartPomodoro,
    AfterStartPomodoro,
    BeforeFinishPomodoro,
    AfterFinishPomodoro,
    BeforeCancelPomodoro,
    AfterCancelPomodoro,
    BeforeInterruptPomodoro,
    AfterInterruptPomodoro,
    BeforeLogPomodoro,
    AfterLogPomodoro,
    BeforeAnnotatePomodoro,
    AfterAnnotatePomodoro,
    BeforeAnnotateBreak,
    AfterAnnotateBreak,
    BeforeStartBreak,
    AfterStartBreak,
    BeforeFinishBreak,
    AfterFinishBreak,
    BeforeLogBreak,
    AfterLogBreak,
}

impl HookEvent {
    /// The filename used to look up the hook script in `$RUSTOMATO_ROOT/hooks/`.
    pub fn filename(&self) -> &'static str {
        match self {
            HookEvent::BeforeStartPomodoro => "before-start-pomodoro",
            HookEvent::AfterStartPomodoro => "after-start-pomodoro",
            HookEvent::BeforeFinishPomodoro => "before-finish-pomodoro",
            HookEvent::AfterFinishPomodoro => "after-finish-pomodoro",
            HookEvent::BeforeCancelPomodoro => "before-cancel-pomodoro",
            HookEvent::AfterCancelPomodoro => "after-cancel-pomodoro",
            HookEvent::BeforeInterruptPomodoro => "before-interrupt-pomodoro",
            HookEvent::AfterInterruptPomodoro => "after-interrupt-pomodoro",
            HookEvent::BeforeLogPomodoro => "before-log-pomodoro",
            HookEvent::AfterLogPomodoro => "after-log-pomodoro",
            HookEvent::BeforeAnnotatePomodoro => "before-annotate-pomodoro",
            HookEvent::AfterAnnotatePomodoro => "after-annotate-pomodoro",
            HookEvent::BeforeAnnotateBreak => "before-annotate-break",
            HookEvent::AfterAnnotateBreak => "after-annotate-break",
            HookEvent::BeforeStartBreak => "before-start-break",
            HookEvent::AfterStartBreak => "after-start-break",
            HookEvent::BeforeFinishBreak => "before-finish-break",
            HookEvent::AfterFinishBreak => "after-finish-break",
            HookEvent::BeforeLogBreak => "before-log-break",
            HookEvent::AfterLogBreak => "after-log-break",
        }
    }

    /// All known hook filenames, used by `init` to create sample scripts.
    pub const ALL: &'static [&'static str] = &[
        "before-start-pomodoro",
        "after-start-pomodoro",
        "before-finish-pomodoro",
        "after-finish-pomodoro",
        "before-cancel-pomodoro",
        "after-cancel-pomodoro",
        "before-interrupt-pomodoro",
        "after-interrupt-pomodoro",
        "before-log-pomodoro",
        "after-log-pomodoro",
        "before-annotate-pomodoro",
        "after-annotate-pomodoro",
        "before-annotate-break",
        "after-annotate-break",
        "before-start-break",
        "after-start-break",
        "before-finish-break",
        "after-finish-break",
        "before-log-break",
        "after-log-break",
    ];
}

impl fmt::Display for HookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.filename())
    }
}

/// Contextual information about the schedulable that is passed to hook scripts.
pub struct HookContext {
    pub root: PathBuf,
    pub kind: Kind,
    pub uuid: SqlUuid,
    pub duration: i64,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub cancelled_at: Option<i64>,
    pub interruptions: i64,
    pub interrupt_kind: Option<String>,
    pub annotation: Option<String>,
    pub verbose: bool,
}

impl HookContext {
    pub fn from_schedulable(root: &Path, s: &Schedulable, verbose: bool) -> Self {
        Self {
            root: root.to_path_buf(),
            kind: s.kind,
            uuid: s.uuid,
            duration: s.duration,
            started_at: s.started_at,
            finished_at: if s.finished_at != 0 {
                Some(s.finished_at)
            } else {
                None
            },
            cancelled_at: if s.cancelled_at != 0 {
                Some(s.cancelled_at)
            } else {
                None
            },
            interruptions: s.interruptions,
            interrupt_kind: None,
            annotation: None,
            verbose,
        }
    }
}

/// Errors that can occur when running a hook.
#[derive(Debug)]
pub enum HookError {
    SpawnFailed(std::io::Error),
    ExecutionFailed(std::io::Error),
    /// The hook did not complete within the configured timeout.
    TimedOut(Duration),
    NonZeroExit(i32),
    TerminatedBySignal(i32),
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookError::SpawnFailed(e) => write!(f, "failed to spawn hook: {}", e),
            HookError::ExecutionFailed(e) => write!(f, "hook execution failed: {}", e),
            HookError::TimedOut(limit) => {
                write!(f, "hook timed out (limit {:?})", limit)
            }
            HookError::NonZeroExit(code) => write!(f, "hook exited with code {}", code),
            HookError::TerminatedBySignal(sig) => {
                write!(f, "hook was terminated by signal {}", sig)
            }
        }
    }
}

/// Look up and execute a hook. Returns `Ok(())` when the hook does not exist, is not
/// executable, or exits successfully. Before-hooks that exit non-zero cause an error,
/// which the caller should treat as an abort.
pub fn run_hook(event: HookEvent, context: &HookContext, no_hooks: bool) -> Result<(), HookError> {
    if no_hooks {
        return Ok(());
    }

    let hook_path = context.root.join("hooks").join(event.filename());

    if !hook_path.exists() {
        if context.verbose {
            eprintln!(
                "  Hook {} not found at {:?}, skipping",
                event.filename(),
                hook_path
            );
        }
        return Ok(());
    }

    if !is_executable(&hook_path) {
        if context.verbose {
            eprintln!(
                "  Hook {} exists but is not executable, skipping",
                event.filename()
            );
        }
        return Ok(());
    }

    if context.verbose {
        eprintln!("  Running hook {}...", event.filename());
    } else {
        println!("\u{2502} [hook:{}]", event.filename());
    }

    execute_hook(&hook_path, event, context, hook_timeout())
}

fn is_executable(path: &Path) -> bool {
    if let Ok(metadata) = path.metadata() {
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    } else {
        false
    }
}

fn execute_hook(
    hook_path: &Path,
    event: HookEvent,
    context: &HookContext,
    timeout: Duration,
) -> Result<(), HookError> {
    let mut cmd = Command::new(hook_path);

    cmd.env("RUSTOMATO_ROOT", &context.root)
        .env("RUSTOMATO_HOOK", event.filename())
        .env("RUSTOMATO_KIND", context.kind.to_string())
        .env("RUSTOMATO_UUID", context.uuid.to_string())
        .env("RUSTOMATO_DURATION", context.duration.to_string())
        .env("RUSTOMATO_STARTED_AT", context.started_at.to_string());

    if let Some(finished_at) = context.finished_at {
        cmd.env("RUSTOMATO_FINISHED_AT", finished_at.to_string());
    }
    if let Some(cancelled_at) = context.cancelled_at {
        cmd.env("RUSTOMATO_CANCELLED_AT", cancelled_at.to_string());
    }
    if let Some(ref interrupt_kind) = context.interrupt_kind {
        cmd.env("RUSTOMATO_INTERRUPT_KIND", interrupt_kind);
        cmd.env("RUSTOMATO_INTERRUPTIONS", context.interruptions.to_string());
    }

    if let Some(ref annotation) = context.annotation {
        cmd.env("RUSTOMATO_ANNOTATION", annotation);
    }

    // Pass the hook name as the first argument ($1).
    cmd.arg(event.filename());

    let mut child = cmd.spawn().map_err(HookError::SpawnFailed)?;

    let start = Instant::now();

    loop {
        match child.try_wait().map_err(HookError::ExecutionFailed)? {
            Some(status) => {
                if status.success() {
                    return Ok(());
                } else {
                    match status.code() {
                        Some(code) => return Err(HookError::NonZeroExit(code)),
                        None => {
                            return Err(HookError::TerminatedBySignal(
                                status.signal().unwrap_or(0),
                            ));
                        }
                    }
                }
            }
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait(); // reap to avoid zombie
                    return Err(HookError::TimedOut(timeout));
                }
                std::thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

/// Create the hooks directory under `root` and populate it with sample
/// (non-executable) hook scripts that just exit 0.
///
/// Hooks are created without the executable bit by design — only hooks you
/// explicitly `chmod +x` will run. This keeps the default noise-free.
pub fn init(root: &Path) -> std::io::Result<()> {
    let hooks_dir = root.join("hooks");
    std::fs::create_dir_all(&hooks_dir)?;

    for hook_name in HookEvent::ALL {
        let hook_path = hooks_dir.join(hook_name);
        if !hook_path.exists() {
            let content = sample_hook_content(hook_name);
            std::fs::write(&hook_path, content)?;
        }
    }

    Ok(())
}

fn sample_hook_content(hook_name: &str) -> String {
    format!(
        r#"#!/usr/bin/env sh
# rustomato hook: {}
#
# This hook is created without the executable bit. To enable it:
#
#   chmod +x "${{RUSTOMATO_ROOT:=\$HOME/.rustomato}}/hooks/{}"
#
# Arguments:
#   $1 - hook name (always '{}')
#
# Environment variables:
#   RUSTOMATO_ROOT       - the root directory
#   RUSTOMATO_HOOK       - the hook name
#   RUSTOMATO_KIND       - "pomodoro" or "break"
#   RUSTOMATO_UUID       - unique identifier of this unit
#   RUSTOMATO_DURATION   - duration in minutes
#   RUSTOMATO_STARTED_AT - Unix timestamp of start
#
# Exit 0 to allow the operation to proceed.
# Exit non-zero to abort (only meaningful for before-* hooks).

exit 0
"#,
        hook_name, hook_name, hook_name
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use tempfile::tempdir;

    /// Helper: write an executable hook script inside `root/hooks/<name>`.
    fn create_hook(root: &Path, name: &str, content: &str) {
        let path = root.join("hooks").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Helper: write a non-executable hook script.
    fn create_non_executable_hook(root: &Path, name: &str, content: &str) {
        let path = root.join("hooks").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        // deliberately no chmod +x
    }

    /// Build a minimal `HookContext` pointing at `root`.
    fn ctx(root: &Path) -> HookContext {
        HookContext {
            root: root.to_path_buf(),
            kind: Kind::Pomodoro,
            uuid: SqlUuid::default(),
            duration: 25,
            started_at: 1000,
            finished_at: None,
            cancelled_at: None,
            interruptions: 0,
            interrupt_kind: None,
            annotation: None,
            verbose: false,
        }
    }

    // --- hook file resolution ------------------------------------------------

    #[test]
    fn missing_hook_is_ok() {
        let dir = tempdir().unwrap();
        let result = run_hook(HookEvent::BeforeStartPomodoro, &ctx(dir.path()), false);
        assert!(result.is_ok());
    }

    #[test]
    fn non_executable_hook_is_ok() {
        let dir = tempdir().unwrap();
        create_non_executable_hook(
            dir.path(),
            "before-start-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );
        let result = run_hook(HookEvent::BeforeStartPomodoro, &ctx(dir.path()), false);
        assert!(result.is_ok());
    }

    // --- exit code handling -------------------------------------------------

    #[test]
    fn exit_zero_is_ok() {
        let dir = tempdir().unwrap();
        create_hook(
            dir.path(),
            "before-start-pomodoro",
            "#!/usr/bin/env sh\nexit 0\n",
        );
        let result = run_hook(HookEvent::BeforeStartPomodoro, &ctx(dir.path()), false);
        assert!(result.is_ok());
    }

    #[test]
    fn exit_nonzero_is_error() {
        let dir = tempdir().unwrap();
        create_hook(
            dir.path(),
            "before-start-pomodoro",
            "#!/usr/bin/env sh\nexit 42\n",
        );
        let result = run_hook(HookEvent::BeforeStartPomodoro, &ctx(dir.path()), false);
        assert_matches!(result, Err(HookError::NonZeroExit(42)));
    }

    // --- --no-hooks flag ----------------------------------------------------

    #[test]
    fn no_hooks_skips_even_failing_hook() {
        let dir = tempdir().unwrap();
        create_hook(
            dir.path(),
            "before-start-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );
        let result = run_hook(HookEvent::BeforeStartPomodoro, &ctx(dir.path()), true);
        assert!(result.is_ok());
    }

    // --- environment variables ----------------------------------------------

    #[test]
    fn hook_receives_environment() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("env_out");

        create_hook(
            dir.path(),
            "after-finish-pomodoro",
            &format!(
                "#!/usr/bin/env sh\n\
                 echo \"$RUSTOMATO_HOOK:$RUSTOMATO_KIND:$RUSTOMATO_DURATION:$RUSTOMATO_STARTED_AT\" > {}\n",
                out.display()
            ),
        );

        let result = run_hook(HookEvent::AfterFinishPomodoro, &ctx(dir.path()), false);
        assert!(result.is_ok());

        let got = std::fs::read_to_string(&out).unwrap();
        assert_eq!(got.trim(), "after-finish-pomodoro:pomodoro:25:1000");
    }

    #[test]
    fn hook_receives_finished_at() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("finish_out");

        create_hook(
            dir.path(),
            "after-finish-pomodoro",
            &format!(
                "#!/usr/bin/env sh\n\
                 echo \"$RUSTOMATO_FINISHED_AT\" > {}\n",
                out.display()
            ),
        );

        let mut c = ctx(dir.path());
        c.finished_at = Some(2000);

        let result = run_hook(HookEvent::AfterFinishPomodoro, &c, false);
        assert!(result.is_ok());

        let got = std::fs::read_to_string(&out).unwrap();
        assert_eq!(got.trim(), "2000");
    }

    #[test]
    fn hook_receives_cancelled_at() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("cancel_out");

        create_hook(
            dir.path(),
            "after-cancel-pomodoro",
            &format!(
                "#!/usr/bin/env sh\n\
                 echo \"$RUSTOMATO_CANCELLED_AT\" > {}\n",
                out.display()
            ),
        );

        let mut c = ctx(dir.path());
        c.cancelled_at = Some(3000);

        let result = run_hook(HookEvent::AfterCancelPomodoro, &c, false);
        assert!(result.is_ok());

        let got = std::fs::read_to_string(&out).unwrap();
        assert_eq!(got.trim(), "3000");
    }

    #[test]
    fn hook_receives_uuid() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("uuid_out");

        create_hook(
            dir.path(),
            "before-start-pomodoro",
            &format!(
                "#!/usr/bin/env sh\n\
                 echo \"$RUSTOMATO_UUID\" > {}\n",
                out.display()
            ),
        );

        let c = ctx(dir.path());

        let result = run_hook(HookEvent::BeforeStartPomodoro, &c, false);
        assert!(result.is_ok());

        let got = std::fs::read_to_string(&out).unwrap();
        // UUID is a 32-char hex string (no dashes)
        let trimmed = got.trim();
        assert_eq!(trimmed.len(), 32);
        assert!(trimmed.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    // --- first argument ($1) ------------------------------------------------

    #[test]
    fn hook_receives_name_as_first_argument() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("arg_out");

        create_hook(
            dir.path(),
            "before-start-pomodoro",
            &format!(
                "#!/usr/bin/env sh\n\
                 echo \"$1\" > {}\n",
                out.display()
            ),
        );

        let result = run_hook(HookEvent::BeforeStartPomodoro, &ctx(dir.path()), false);
        assert!(result.is_ok());

        let got = std::fs::read_to_string(&out).unwrap();
        assert_eq!(got.trim(), "before-start-pomodoro");
    }

    // --- timeout ------------------------------------------------------------

    #[test]
    fn hook_that_sleeps_is_killed() {
        let dir = tempdir().unwrap();
        create_hook(
            dir.path(),
            "before-start-pomodoro",
            "#!/usr/bin/env sh\nsleep 10\n",
        );

        // Use a very short timeout via the test override so the test doesn't take 3 s.
        TEST_HOOK_TIMEOUT.with(|t| *t.borrow_mut() = Some(Duration::from_millis(100)));
        let result = run_hook(HookEvent::BeforeStartPomodoro, &ctx(dir.path()), false);
        TEST_HOOK_TIMEOUT.with(|t| *t.borrow_mut() = None);

        assert_matches!(result, Err(HookError::TimedOut(_)));
    }
}
