mod hooks_integration {
    use assert_matches::assert_matches;
    use rustomato::persistence::Repository;
    use rustomato::scheduling::{Scheduler, SchedulingError};
    use rustomato::{Kind, Schedulable};
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process;
    use tempfile::tempdir;

    /// Write an executable hook script at `root/hooks/<name>`.
    fn setup_hook(root: &Path, name: &str, content: &str) {
        let path = root.join("hooks").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Convenience: a scheduler backed by an in-memory database and the given
    /// root directory, with verbose off and hooks enabled.
    fn scheduler(root: &Path) -> Scheduler {
        let repo = Repository::new("file::memory:");
        Scheduler::new(repo, root.to_path_buf(), false, false)
    }

    // --- before-start -------------------------------------------------------

    #[test]
    fn before_start_pomodoro_hook_aborts_on_nonzero_exit() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "before-start-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let result = scheduler(dir.path()).run(Schedulable::new(
            process::id(),
            Kind::Pomodoro,
            0, // duration 0 → waiter returns immediately
        ));
        assert_matches!(result, Err(SchedulingError::HookRejected));
    }

    #[test]
    fn before_start_break_hook_aborts_on_nonzero_exit() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "before-start-break",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let result = scheduler(dir.path()).run(Schedulable::new(process::id(), Kind::Break, 0));
        assert_matches!(result, Err(SchedulingError::HookRejected));
    }

    #[test]
    fn before_start_pomodoro_hook_does_not_abort_on_zero_exit() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "before-start-pomodoro",
            "#!/usr/bin/env sh\nexit 0\n",
        );

        let result = scheduler(dir.path()).run(Schedulable::new(process::id(), Kind::Pomodoro, 0));
        assert!(result.is_ok());
    }

    // --- after-start (failure should NOT abort) -----------------------------

    #[test]
    fn after_start_pomodoro_hook_failure_is_not_fatal() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "after-start-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let result = scheduler(dir.path()).run(Schedulable::new(process::id(), Kind::Pomodoro, 0));
        assert!(result.is_ok());
    }

    #[test]
    fn after_start_break_hook_failure_is_not_fatal() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "after-start-break",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let result = scheduler(dir.path()).run(Schedulable::new(process::id(), Kind::Break, 0));
        assert!(result.is_ok());
    }

    // --- before-finish (pomodoro timer expiry) ------------------------------

    #[test]
    fn before_finish_pomodoro_hook_aborts_on_nonzero_exit() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "before-finish-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let result = scheduler(dir.path()).run(Schedulable::new(process::id(), Kind::Pomodoro, 0));
        assert_matches!(result, Err(SchedulingError::HookRejected));
    }

    #[test]
    fn before_finish_break_hook_aborts_on_nonzero_exit() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "before-finish-break",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let result = scheduler(dir.path()).run(Schedulable::new(process::id(), Kind::Break, 0));
        assert_matches!(result, Err(SchedulingError::HookRejected));
    }

    // --- after-finish (failure should NOT abort) ----------------------------

    #[test]
    fn after_finish_pomodoro_hook_failure_is_not_fatal() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "after-finish-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let result = scheduler(dir.path()).run(Schedulable::new(process::id(), Kind::Pomodoro, 0));
        assert!(result.is_ok());
    }

    #[test]
    fn after_finish_break_hook_failure_is_not_fatal() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "after-finish-break",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let result = scheduler(dir.path()).run(Schedulable::new(process::id(), Kind::Break, 0));
        assert!(result.is_ok());
    }

    // --- --no-hooks skips all hooks -----------------------------------------

    #[test]
    fn no_hooks_lets_operation_proceed_despite_failing_hook() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "before-start-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );
        setup_hook(
            dir.path(),
            "before-finish-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let repo = Repository::new("file::memory:");
        let scheduler = Scheduler::new(repo, dir.path().to_path_buf(), false, true); // no_hooks = true
        let result = scheduler.run(Schedulable::new(process::id(), Kind::Pomodoro, 0));
        assert!(result.is_ok());
    }

    // --- no hook files → normal operation -----------------------------------

    #[test]
    fn no_hook_files_does_not_hinder_operation() {
        let dir = tempdir().unwrap();
        // deliberately no hooks directory at all

        let result = scheduler(dir.path()).run(Schedulable::new(process::id(), Kind::Pomodoro, 0));
        assert!(result.is_ok());
    }

    // --- hooks can observe the finished state via side effects --------------

    #[test]
    fn after_finish_hook_can_observe_finished_state() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("result");

        setup_hook(
            dir.path(),
            "after-finish-pomodoro",
            &format!(
                "#!/usr/bin/env sh\n\
                 echo \"$RUSTOMATO_KIND:$RUSTOMATO_FINISHED_AT\" > {}\n",
                out.display()
            ),
        );

        let result = scheduler(dir.path()).run(Schedulable::new(process::id(), Kind::Pomodoro, 0));
        assert!(result.is_ok());

        let got = std::fs::read_to_string(&out).unwrap();
        let parts: Vec<&str> = got.trim().split(':').collect();
        assert_eq!(parts[0], "pomodoro");
        // finished_at should be a non-empty Unix timestamp
        let ts: i64 = parts[1].parse().expect("expected a numeric timestamp");
        assert!(ts > 0);
    }
}
