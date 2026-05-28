mod acceptance_tests {
    use assert_cmd::Command;
    use predicates::prelude::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    /// Helper: create a `Command` that runs the `rustomato` binary with a clean
    /// environment (so `RUSTOMATO_DATABASE_URL` from the host shell does not leak).
    fn rustomato() -> Command {
        let mut cmd = Command::cargo_bin("rustomato").unwrap();
        cmd.env_remove("RUSTOMATO_DATABASE_URL");
        cmd
    }

    #[test]
    fn plain() {
        let mut cmd = Command::cargo_bin("rustomato").unwrap();
        cmd.assert().code(2);
    }

    #[test]
    fn verbose() {
        let rustomato_root = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", rustomato_root.keep())
            .arg("--verbose")
            .arg("status")
            .assert()
            .success()
            .stdout(predicate::str::starts_with("Using root"));
    }

    // --- init ---------------------------------------------------------------

    #[test]
    fn init_creates_hooks_directory_and_sample_hooks() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("init")
            .assert()
            .success()
            .stdout(predicate::str::contains("Initialized rustomato"));

        // The hooks directory exists
        assert!(dir.path().join("hooks").is_dir());

        // All sample hooks are present and executable
        for name in rustomato::hooks::HookEvent::ALL {
            let path = dir.path().join("hooks").join(name);
            assert!(path.is_file(), "missing hook: {}", name);

            let meta = path.metadata().unwrap();
            assert!(meta.is_file());
            assert!(
                meta.permissions().mode() & 0o111 != 0,
                "hook not executable: {}",
                name
            );
        }
    }

    #[test]
    fn init_is_idempotent() {
        let dir = tempdir().unwrap();

        // Run init twice
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("init")
            .assert()
            .success();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("init")
            .assert()
            .success();

        // Still exactly the expected set of hook files
        let entries: Vec<_> = std::fs::read_dir(dir.path().join("hooks"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), rustomato::hooks::HookEvent::ALL.len());
    }

    // --- --no-hooks ---------------------------------------------------------

    #[test]
    fn no_hooks_flag_skips_failing_hook_and_allows_pomodoro() {
        let dir = tempdir().unwrap();

        std::fs::create_dir_all(dir.path().join("hooks")).unwrap();
        std::fs::write(
            dir.path().join("hooks").join("before-start-pomodoro"),
            "#!/usr/bin/env sh\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(
            dir.path().join("hooks").join("before-start-pomodoro"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();

        // Without --no-hooks this would fail; with --no-hooks it succeeds
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("start")
            .arg("--duration")
            .arg("0")
            .assert()
            .success();
    }

    // --- failing before-hook CLI behaviour ----------------------------------

    #[test]
    fn failing_before_start_hook_exits_nonzero() {
        let dir = tempdir().unwrap();

        std::fs::create_dir_all(dir.path().join("hooks")).unwrap();
        std::fs::write(
            dir.path().join("hooks").join("before-start-pomodoro"),
            "#!/usr/bin/env sh\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(
            dir.path().join("hooks").join("before-start-pomodoro"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("pomodoro")
            .arg("start")
            .arg("--duration")
            .arg("0")
            .assert()
            .failure()
            .code(predicate::eq(1));
    }

    // --- break also runs hooks ----------------------------------------------

    #[test]
    fn no_hooks_break_succeeds_despite_failing_hook() {
        let dir = tempdir().unwrap();

        std::fs::create_dir_all(dir.path().join("hooks")).unwrap();
        std::fs::write(
            dir.path().join("hooks").join("before-start-break"),
            "#!/usr/bin/env sh\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(
            dir.path().join("hooks").join("before-start-break"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("start")
            .arg("--duration")
            .arg("0")
            .assert()
            .success();
    }

    #[test]
    fn failing_before_start_break_hook_exits_nonzero() {
        let dir = tempdir().unwrap();

        std::fs::create_dir_all(dir.path().join("hooks")).unwrap();
        std::fs::write(
            dir.path().join("hooks").join("before-start-break"),
            "#!/usr/bin/env sh\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(
            dir.path().join("hooks").join("before-start-break"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("break")
            .arg("start")
            .arg("--duration")
            .arg("0")
            .assert()
            .failure()
            .code(predicate::eq(1));
    }

    // --- non-executable hook does not block ---------------------------------

    #[test]
    fn non_executable_hook_is_ignored() {
        let dir = tempdir().unwrap();

        std::fs::create_dir_all(dir.path().join("hooks")).unwrap();
        std::fs::write(
            dir.path().join("hooks").join("before-start-pomodoro"),
            "#!/usr/bin/env sh\nexit 1\n",
        )
        .unwrap();
        // deliberately no chmod +x

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("pomodoro")
            .arg("start")
            .arg("--duration")
            .arg("0")
            .assert()
            .success();
    }

    // --- verbose shows hook activity ---------------------------------------

    #[test]
    fn verbose_output_mentions_hooks() {
        let dir = tempdir().unwrap();

        // Sample hook from init is fine (it exits 0)
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("init")
            .assert()
            .success();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--verbose")
            .arg("pomodoro")
            .arg("start")
            .arg("--duration")
            .arg("0")
            .assert()
            .success()
            .stderr(predicate::str::contains("Running hook"));
    }

    // --- interrupt ----------------------------------------------------------

    #[test]
    fn interrupt_nothing_active_fails() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("pomodoro")
            .arg("interrupt")
            .assert()
            .failure()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains("nothing active to interrupt"));
    }

    #[test]
    fn interrupt_invalid_kind_fails() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("pomodoro")
            .arg("interrupt")
            .arg("--kind")
            .arg("invalid")
            .assert()
            .failure()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains("unknown interruption kind"));
    }

    #[test]
    fn interrupt_active_pomodoro_increments_counter() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("data.db");

        // Seed an active pomodoro directly into the database
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            pom.started_at = 1000;
            repo.save(&pom).expect("saving active pomodoro");
        }

        // Run interrupt via CLI
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("interrupt")
            .assert()
            .success();

        // Verify via the CLI status command
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("status")
            .assert()
            .success()
            .stdout(predicate::str::contains("1 interruption"));
    }

    #[test]
    fn interrupt_with_external_flag_works() {
        let dir = tempdir().unwrap();

        // Seed an active pomodoro
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            pom.started_at = 1000;
            repo.save(&pom).expect("saving active pomodoro");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("interrupt")
            .arg("--kind")
            .arg("external")
            .assert()
            .success();
    }

    #[test]
    fn interrupt_verbose_shows_pomodoro_with_counter() {
        let dir = tempdir().unwrap();

        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            pom.started_at = 1000;
            repo.save(&pom).expect("saving active pomodoro");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("--verbose")
            .arg("pomodoro")
            .arg("interrupt")
            .assert()
            .success()
            .stdout(predicate::str::contains("1 interruption"));
    }
}
