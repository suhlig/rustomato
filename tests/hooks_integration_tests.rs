mod hooks_integration {
    use assert_matches::assert_matches;
    use rustomato::persistence::Repository;
    use rustomato::scheduling::{Scheduler, SchedulingError};
    use rustomato::{InterruptionKind, Kind, Schedulable};
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

    // --- interrupt hooks -----------------------------------------------------

    #[test]
    fn before_interrupt_pomodoro_hook_aborts_on_nonzero_exit() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "before-interrupt-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let sched = scheduler(dir.path());

        // Manually save an active pomodoro
        let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
        pom.started_at = 1000;
        sched.repo().save(&pom).expect("saving active pomodoro");

        let result = sched.interrupt(InterruptionKind::Internal);
        assert_matches!(result, Err(SchedulingError::HookRejected));
    }

    #[test]
    fn after_interrupt_pomodoro_hook_failure_is_not_fatal() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "after-interrupt-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let sched = scheduler(dir.path());

        let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
        pom.started_at = 1000;
        sched.repo().save(&pom).expect("saving active pomodoro");

        let result = sched.interrupt(InterruptionKind::Internal);
        assert!(result.is_ok());
    }

    #[test]
    fn interrupt_increments_counter() {
        let dir = tempdir().unwrap();
        let sched = scheduler(dir.path());

        let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
        pom.started_at = 1000;
        let pom = sched.repo().save(&pom).expect("saving active pomodoro");

        sched
            .interrupt(InterruptionKind::Internal)
            .expect("first interrupt");
        sched
            .interrupt(InterruptionKind::External)
            .expect("second interrupt");

        let updated = sched.repo().find_by_uuid(pom.uuid).expect("finding");
        assert_eq!(updated.interruptions, 2);
    }

    #[test]
    fn interrupt_no_active_returns_error() {
        let dir = tempdir().unwrap();
        let sched = scheduler(dir.path());

        let result = sched.interrupt(InterruptionKind::Internal);
        assert_matches!(result, Err(SchedulingError::NoActiveSchedulable));
    }

    #[test]
    fn interrupt_during_break_falls_back_to_finished_pomodoro() {
        let dir = tempdir().unwrap();
        let sched = scheduler(dir.path());

        // First, create and finish a pomodoro
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        sched.repo().save(&pom).expect("saving pomodoro");
        pom.finished_at = 2000;
        sched.repo().save(&pom).expect("finishing pomodoro");

        // Now start a break
        let mut brk = Schedulable::new(43, Kind::Break, 5);
        brk.started_at = 3000;
        let brk = sched.repo().save(&brk).expect("saving break");

        // Interrupt during break → should go to finished pomodoro
        let result = sched.interrupt(InterruptionKind::External);
        assert!(result.is_ok());

        // The finished pomodoro should have the interruption, not the break
        let finished = sched
            .repo()
            .most_recently_finished_pomodoro()
            .expect("querying")
            .expect("should exist");
        assert_eq!(finished.interruptions, 1);

        // The break should have 0 interruptions
        let active_break = sched.repo().find_by_uuid(brk.uuid).expect("finding break");
        assert_eq!(active_break.interruptions, 0);
    }

    #[test]
    fn interrupt_during_break_no_finished_pomodoro_returns_error() {
        let dir = tempdir().unwrap();
        let sched = scheduler(dir.path());

        // Start a break but never finish a pomodoro
        let mut brk = Schedulable::new(43, Kind::Break, 5);
        brk.started_at = 1000;
        sched.repo().save(&brk).expect("saving break");

        let result = sched.interrupt(InterruptionKind::Internal);
        assert_matches!(result, Err(SchedulingError::NoFinishedPomodoro));
    }

    #[test]
    fn interrupt_hook_receives_interrupt_kind_env() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("result");

        setup_hook(
            dir.path(),
            "after-interrupt-pomodoro",
            &format!(
                "#!/usr/bin/env sh\necho \"$RUSTOMATO_INTERRUPT_KIND:$RUSTOMATO_INTERRUPTIONS\" > {}\
",
                out.display()
            ),
        );

        let sched = scheduler(dir.path());

        let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
        pom.started_at = 1000;
        sched.repo().save(&pom).expect("saving active pomodoro");

        sched
            .interrupt(InterruptionKind::External)
            .expect("interrupt");

        let got = std::fs::read_to_string(&out).unwrap();
        let trimmed = got.trim();
        assert_eq!(trimmed, "external:1");
    }

    // --- annotate -----------------------------------------------------------

    #[test]
    fn annotate_active_pomodoro() {
        let dir = tempdir().unwrap();
        let sched = scheduler(dir.path());

        let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
        pom.started_at = 1000;
        sched.repo().save(&pom).expect("saving active pomodoro");

        let annotation = sched.annotate("hello world").unwrap();
        assert_eq!(annotation.body, "hello world");

        let found = sched
            .repo()
            .find_annotation_by_uuid(annotation.uuid)
            .unwrap();
        assert_eq!(found.body, "hello world");
    }

    #[test]
    fn annotate_most_recently_ended() {
        let dir = tempdir().unwrap();
        let sched = scheduler(dir.path());

        let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
        pom.started_at = 1000;
        sched.repo().save(&pom).expect("saving pomodoro");
        pom.finished_at = 2000;
        sched.repo().save(&pom).expect("finishing pomodoro");

        let annotation = sched.annotate("late note").unwrap();
        assert_eq!(annotation.body, "late note");
    }

    #[test]
    fn annotate_nothing_available_returns_error() {
        let dir = tempdir().unwrap();
        let sched = scheduler(dir.path());

        let result = sched.annotate("nothing");
        assert_matches!(result, Err(SchedulingError::NothingToAnnotate));
    }

    #[test]
    fn annotate_break() {
        let dir = tempdir().unwrap();
        let sched = scheduler(dir.path());

        let mut brk = Schedulable::new(process::id(), Kind::Break, 5);
        brk.started_at = 1000;
        sched.repo().save(&brk).expect("saving active break");

        let annotation = sched.annotate("break annotation").unwrap();
        assert_eq!(annotation.body, "break annotation");
    }

    #[test]
    fn before_annotate_pomodoro_hook_aborts_on_nonzero_exit() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "before-annotate-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let sched = scheduler(dir.path());

        let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
        pom.started_at = 1000;
        sched.repo().save(&pom).expect("saving active pomodoro");

        let result = sched.annotate("hello");
        assert_matches!(result, Err(SchedulingError::HookRejected));
    }

    #[test]
    fn after_annotate_pomodoro_hook_failure_is_not_fatal() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "after-annotate-pomodoro",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let sched = scheduler(dir.path());

        let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
        pom.started_at = 1000;
        sched.repo().save(&pom).expect("saving active pomodoro");

        let annotation = sched.annotate("persist despite failure").unwrap();
        let found = sched
            .repo()
            .find_annotation_by_uuid(annotation.uuid)
            .unwrap();
        assert_eq!(found.body, "persist despite failure");
    }

    #[test]
    fn annotate_receives_annotation_env() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("result");

        setup_hook(
            dir.path(),
            "after-annotate-pomodoro",
            &format!(
                "#!/usr/bin/env sh\necho \"$RUSTOMATO_ANNOTATION\" > {}\n",
                out.display()
            ),
        );

        let sched = scheduler(dir.path());

        let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
        pom.started_at = 1000;
        sched.repo().save(&pom).expect("saving active pomodoro");

        sched.annotate("test annotation").unwrap();

        let got = std::fs::read_to_string(&out).unwrap();
        assert_eq!(got.trim(), "test annotation");
    }

    #[test]
    fn before_annotate_break_hook_aborts_on_nonzero_exit() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "before-annotate-break",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let sched = scheduler(dir.path());

        let mut brk = Schedulable::new(process::id(), Kind::Break, 5);
        brk.started_at = 1000;
        sched.repo().save(&brk).expect("saving active break");

        let result = sched.annotate("hello");
        assert_matches!(result, Err(SchedulingError::HookRejected));
    }

    #[test]
    fn after_annotate_break_hook_failure_is_not_fatal() {
        let dir = tempdir().unwrap();
        setup_hook(
            dir.path(),
            "after-annotate-break",
            "#!/usr/bin/env sh\nexit 1\n",
        );

        let sched = scheduler(dir.path());

        let mut brk = Schedulable::new(process::id(), Kind::Break, 5);
        brk.started_at = 1000;
        sched.repo().save(&brk).expect("saving active break");

        let annotation = sched.annotate("still saved").unwrap();
        let found = sched
            .repo()
            .find_annotation_by_uuid(annotation.uuid)
            .unwrap();
        assert_eq!(found.body, "still saved");
    }
}
