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
        cmd.assert().success();
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

        // All sample hooks are present and not executable by default
        for name in rustomato::hooks::HookEvent::ALL {
            let path = dir.path().join("hooks").join(name);
            assert!(path.is_file(), "missing hook: {}", name);

            let meta = path.metadata().unwrap();
            assert!(meta.is_file());
            assert!(
                meta.permissions().mode() & 0o111 == 0,
                "hook should not be executable by default: {}",
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
            dir.path().join("hooks").join("before-log-pomodoro"),
            "#!/usr/bin/env sh\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(
            dir.path().join("hooks").join("before-log-pomodoro"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();

        // Without --no-hooks this would fail; with --no-hooks it succeeds
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
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

        // Seed a finished break to annotate
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            let db_path = dir.path().join("data.db");
            // Repository::new runs migrations, creates the DB
            let _repo = Repository::new(&db_path.to_string_lossy());
            let mut b = Schedulable::new(0, Kind::Break, 5);
            b.started_at = 1000;
            b.finished_at = 1300;
            _repo.save_external_finished(&b).expect("seeding break");
        }

        std::fs::create_dir_all(dir.path().join("hooks")).unwrap();
        std::fs::write(
            dir.path().join("hooks").join("before-annotate-break"),
            "#!/usr/bin/env sh\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(
            dir.path().join("hooks").join("before-annotate-break"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("annotate")
            .arg("test")
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
            dir.path().join("hooks").join("before-log-pomodoro"),
            "#!/usr/bin/env sh\nexit 1\n",
        )
        .unwrap();
        // deliberately no chmod +x

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
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

        // Make hooks executable so they actually run
        use std::os::unix::fs::PermissionsExt;
        for name in rustomato::hooks::HookEvent::ALL {
            let path = dir.path().join("hooks").join(name);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        // Use log instead of start to avoid blocking on timer
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--verbose")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
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
            .stderr(predicate::str::contains("no entry at position -1"));
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

    // --- interrupt with positional index ----------------------------------

    #[test]
    fn interrupt_positional_index() {
        let dir = tempdir().unwrap();

        // Seed two finished pomodori, then interrupt the most recent (-1)
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());

            let mut p1 = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            p1.started_at = 1000;
            let p1 = repo.save(&p1).expect("saving p1");
            let mut p1 = repo.find_by_uuid(p1.uuid).unwrap();
            p1.finished_at = 2000;
            repo.save(&p1).expect("finishing p1");

            let mut p2 = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            p2.started_at = 3000;
            let p2 = repo.save(&p2).expect("saving p2");
            let mut p2 = repo.find_by_uuid(p2.uuid).unwrap();
            p2.finished_at = 4000;
            repo.save(&p2).expect("finishing p2");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("interrupt")
            .arg("-1")
            .assert()
            .success();
    }

    #[test]
    fn interrupt_positional_and_target_conflict() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("interrupt")
            .arg("--target")
            .arg("-1")
            .arg("-1")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "cannot use both --target and a positional index",
            ));
    }

    // --- interrupt with --target -------------------------------------------

    #[test]
    fn interrupt_with_target_uuid_prefix() {
        let dir = tempdir().unwrap();

        // Seed a finished pomodoro
        let pom_uuid = {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            pom.started_at = 1000;
            let pom = repo.save(&pom).expect("saving active pomodoro");
            let mut pom = repo.find_by_uuid(pom.uuid).unwrap();
            pom.finished_at = 2000;
            let pom = repo.save(&pom).expect("finishing pomodoro");
            pom.uuid.to_string()
        };

        // Use an abbreviated prefix (first 8 chars)
        let prefix = &pom_uuid[..8];

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("interrupt")
            .arg("--target")
            .arg(prefix)
            .assert()
            .success();
    }

    #[test]
    fn interrupt_with_target_negative_index() {
        let dir = tempdir().unwrap();

        // Seed two finished pomodori, then interrupt the second-most-recent (-2)
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());

            // First pomodoro (older)
            let mut p1 = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            p1.started_at = 1000;
            let p1 = repo.save(&p1).expect("saving p1");
            let mut p1 = repo.find_by_uuid(p1.uuid).unwrap();
            p1.finished_at = 2000;
            repo.save(&p1).expect("finishing p1");

            // Second pomodoro (more recent)
            let mut p2 = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            p2.started_at = 3000;
            let p2 = repo.save(&p2).expect("saving p2");
            let mut p2 = repo.find_by_uuid(p2.uuid).unwrap();
            p2.finished_at = 4000;
            repo.save(&p2).expect("finishing p2");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("interrupt")
            .arg("--target")
            .arg("-2")
            .assert()
            .success();
    }

    #[test]
    fn interrupt_with_target_invalid_fails() {
        let dir = tempdir().unwrap();

        // Seed a finished pomodoro
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            pom.started_at = 1000;
            let pom = repo.save(&pom).expect("saving active pomodoro");
            let mut pom = repo.find_by_uuid(pom.uuid).unwrap();
            pom.finished_at = 2000;
            repo.save(&pom).expect("finishing pomodoro");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("interrupt")
            .arg("--target")
            .arg("nonexistent")
            .assert()
            .failure()
            .stderr(predicate::str::contains("cannot resolve"));
    }

    #[test]
    fn interrupt_with_target_break_fails() {
        let dir = tempdir().unwrap();

        // Seed a finished break
        let break_uuid = {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut brk = Schedulable::new(process::id(), Kind::Break, 5);
            brk.started_at = 1000;
            let brk = repo.save(&brk).expect("saving break");
            let mut brk = repo.find_by_uuid(brk.uuid).unwrap();
            brk.finished_at = 2000;
            let brk = repo.save(&brk).expect("finishing break");
            brk.uuid.to_string()
        };

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("interrupt")
            .arg("--target")
            .arg(&break_uuid[..8])
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "interruptions can only be recorded on pomodori",
            ));
    }

    // --- annotate ----------------------------------------------------------

    #[test]
    fn annotate_nothing_at_all_fails() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("pomodoro")
            .arg("annotate")
            .arg("some")
            .arg("words")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "nothing active or previously done to annotate",
            ));
    }

    #[test]
    fn annotate_active_pomodoro() {
        let dir = tempdir().unwrap();

        // Seed an active pomodoro directly into the database
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
            .arg("annotate")
            .arg("my")
            .arg("annotation")
            .assert()
            .success();
    }

    #[test]
    fn annotate_break() {
        let dir = tempdir().unwrap();

        // Seed an active break directly into the database
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut b = Schedulable::new(process::id(), Kind::Break, 5);
            b.started_at = 1000;
            repo.save(&b).expect("saving active break");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("annotate")
            .arg("break")
            .arg("note")
            .assert()
            .success();
    }

    #[test]
    fn annotate_fallback_to_most_recently_ended() {
        let dir = tempdir().unwrap();

        // Seed a finished pomodoro: first save as active, then finish it
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            pom.started_at = 1000;
            let pom = repo.save(&pom).expect("saving active pomodoro");
            let mut pom = repo.find_by_uuid(pom.uuid).unwrap();
            pom.finished_at = 2000;
            repo.save(&pom).expect("finishing pomodoro");
        }

        // No active pomodoro, so it should fall back to the most recently ended
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("annotate")
            .arg("note")
            .arg("on")
            .arg("finished")
            .assert()
            .success();
    }

    #[test]
    fn annotate_empty_fails() {
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
            .arg("annotate")
            .assert()
            .failure()
            .stderr(predicate::str::contains("annotation text is empty"));
    }

    #[test]
    fn annotate_break_empty_fails() {
        let dir = tempdir().unwrap();

        // Seed an active break
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut b = Schedulable::new(process::id(), Kind::Break, 5);
            b.started_at = 1000;
            repo.save(&b).expect("saving active break");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("annotate")
            .assert()
            .failure()
            .stderr(predicate::str::contains("annotation text is empty"));
    }

    // --- annotate with --target -------------------------------------------

    #[test]
    fn annotate_with_target_uuid_prefix() {
        let dir = tempdir().unwrap();

        // Seed a finished pomodoro
        let pom_uuid = {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            pom.started_at = 1000;
            let pom = repo.save(&pom).expect("saving active pomodoro");
            let mut pom = repo.find_by_uuid(pom.uuid).unwrap();
            pom.finished_at = 2000;
            let pom = repo.save(&pom).expect("finishing pomodoro");
            pom.uuid.to_string()
        };

        // Use an abbreviated prefix (first 8 chars)
        let prefix = &pom_uuid[..8];

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("annotate")
            .arg("--target")
            .arg(prefix)
            .arg("matched")
            .arg("by")
            .arg("prefix")
            .assert()
            .success();
    }

    #[test]
    fn annotate_with_target_negative_index() {
        let dir = tempdir().unwrap();

        // Seed two finished pomodori, then annotate the second-most-recent (-2)
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());

            // First pomodoro (older)
            let mut p1 = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            p1.started_at = 1000;
            let p1 = repo.save(&p1).expect("saving p1");
            let mut p1 = repo.find_by_uuid(p1.uuid).unwrap();
            p1.finished_at = 2000;
            repo.save(&p1).expect("finishing p1");

            // Second pomodoro (more recent)
            let mut p2 = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            p2.started_at = 3000;
            let p2 = repo.save(&p2).expect("saving p2");
            let mut p2 = repo.find_by_uuid(p2.uuid).unwrap();
            p2.finished_at = 4000;
            repo.save(&p2).expect("finishing p2");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("annotate")
            .arg("--target")
            .arg("-2")
            .arg("second")
            .arg("most")
            .arg("recent")
            .assert()
            .success();
    }

    #[test]
    fn annotate_with_target_invalid_fails() {
        let dir = tempdir().unwrap();

        // Seed a finished pomodoro
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            pom.started_at = 1000;
            let pom = repo.save(&pom).expect("saving active pomodoro");
            let mut pom = repo.find_by_uuid(pom.uuid).unwrap();
            pom.finished_at = 2000;
            repo.save(&pom).expect("finishing pomodoro");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("annotate")
            .arg("--target")
            .arg("nonexistent")
            .arg("text")
            .assert()
            .failure()
            .stderr(predicate::str::contains("cannot resolve"));
    }

    // --- annotate with positional index ---

    #[test]
    fn annotate_positional_index() {
        let dir = tempdir().unwrap();

        // Seed a finished pomodoro
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());
            let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            pom.started_at = 1000;
            let pom = repo.save(&pom).expect("saving active pomodoro");
            let mut pom = repo.find_by_uuid(pom.uuid).unwrap();
            pom.finished_at = 2000;
            repo.save(&pom).expect("finishing pomodoro");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("annotate")
            .arg("-1")
            .arg("positional")
            .arg("note")
            .assert()
            .success();
    }

    #[test]
    fn annotate_positional_and_target_conflict() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("annotate")
            .arg("--target")
            .arg("-1")
            .arg("-1")
            .arg("text")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "cannot use both --target and a positional index",
            ));
    }

    // --- cancel with --target and positional index ---

    #[test]
    fn cancel_no_target_no_active_fails() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("cancel")
            .assert()
            .failure()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains("nothing active"));
    }

    #[test]
    fn cancel_target_negative_index() {
        let dir = tempdir().unwrap();

        // Seed two finished pomodori, then cancel the second-most-recent (-2).
        // Pomodoro cancel always exits 1.
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());

            let mut p1 = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            p1.started_at = 1000;
            let p1 = repo.save(&p1).expect("saving p1");
            let mut p1 = repo.find_by_uuid(p1.uuid).unwrap();
            p1.finished_at = 2000;
            repo.save(&p1).expect("finishing p1");

            let mut p2 = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            p2.started_at = 3000;
            let p2 = repo.save(&p2).expect("saving p2");
            let mut p2 = repo.find_by_uuid(p2.uuid).unwrap();
            p2.finished_at = 4000;
            repo.save(&p2).expect("finishing p2");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("cancel")
            .arg("--target")
            .arg("-2")
            .assert()
            .code(predicate::eq(1));
    }

    #[test]
    fn cancel_break_target_uuid_prefix() {
        let dir = tempdir().unwrap();

        // Seed a non-finished break, then cancel (= finish) it via --target with UUID.
        // We cannot use -1 here because the seeded break has pid=0, making it "active"
        // (pid_is_alive(0) is true), so -N excludes it.
        let break_uuid = {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());

            let mut b = Schedulable::new(0, Kind::Break, 5);
            b.started_at = 1000;
            let b = repo.save(&b).expect("seeding break");
            b.uuid.to_string()
        };

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("cancel")
            .arg("--target")
            .arg(&break_uuid[..8])
            .assert()
            .code(predicate::eq(0));
    }

    // --- these next tests demonstrate the -1..-9 positional shortcut ---

    // --- cancel positional index (PROVES BUG: -1 is currently rejected) ---

    #[test]
    fn cancel_positional_index() {
        let dir = tempdir().unwrap();

        // Seed a finished pomodoro, then cancel it via -1 shorthand.
        // Pomodoro cancel exits 1.
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            use std::process;
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());

            let mut pom = Schedulable::new(process::id(), Kind::Pomodoro, 25);
            pom.started_at = 1000;
            let pom = repo.save(&pom).expect("saving active pomodoro");
            let mut pom = repo.find_by_uuid(pom.uuid).unwrap();
            pom.finished_at = 2000;
            repo.save(&pom).expect("finishing pomodoro");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("cancel")
            .arg("-1")
            .assert()
            .code(predicate::eq(1));
    }

    #[test]
    fn cancel_break_positional_is_not_rejected_by_clap() {
        let dir = tempdir().unwrap();

        // The break gets saved with pid=0, making it "active" to the repository,
        // so -1 has nothing to resolve to. But the test proves clap accepts the
        // positional -1 syntax (it doesn't say "unexpected argument").
        {
            use rustomato::persistence::Repository;
            use rustomato::{Kind, Schedulable};
            let db_path = dir.path().join("data.db");
            let repo = Repository::new(&db_path.to_string_lossy());

            let mut b = Schedulable::new(0, Kind::Break, 5);
            b.started_at = 1000;
            repo.save(&b).expect("seeding break");
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("cancel")
            .arg("-1")
            .assert()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains("no entry at position -1"));
    }

    #[test]
    fn cancel_positional_and_target_conflict() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("cancel")
            .arg("--target")
            .arg("-1")
            .arg("-1")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "cannot use both --target and a positional index",
            ));
    }

    // --- log ---

    #[test]
    fn log_needs_at_least_one_timestamp() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .assert()
            .failure()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains(
                "at least one of --started-at or --finished-at",
            ));
    }

    #[test]
    fn log_all_three_is_error() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .arg("--finished-at")
            .arg("2026-05-29T10:25:00Z")
            .arg("--duration")
            .arg("25")
            .assert()
            .failure()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains(
                "cannot specify --duration when both",
            ));
    }

    #[test]
    fn log_with_started_at_and_duration() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .arg("--duration")
            .arg("25")
            .assert()
            .success();
    }

    #[test]
    fn log_with_finished_at_and_duration() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--finished-at")
            .arg("2026-05-29T10:25:00Z")
            .arg("--duration")
            .arg("25")
            .assert()
            .success();
    }

    #[test]
    fn log_with_both_timestamps() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .arg("--finished-at")
            .arg("2026-05-29T10:30:00Z")
            .assert()
            .success();
    }

    #[test]
    fn log_finished_before_started_is_error() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:25:00Z")
            .arg("--finished-at")
            .arg("2026-05-29T10:00:00Z")
            .assert()
            .failure()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains(
                "--finished-at must be after --started-at",
            ));
    }

    #[test]
    fn log_with_default_duration() {
        let dir = tempdir().unwrap();

        // Only --started-at, no --duration → defaults to 25 minutes
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .assert()
            .success();

        // Verify via verbose that 25 min is recorded
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--verbose")
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T11:00:00Z")
            .assert()
            .success()
            .stdout(predicate::str::contains("25 min"));
    }

    #[test]
    fn log_overlapping_rule1_error() {
        let dir = tempdir().unwrap();

        // First log: 10:00 - 10:25
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .arg("--duration")
            .arg("25")
            .assert()
            .success();

        // Second log overlapping: 10:10 - 10:35
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:10:00Z")
            .arg("--duration")
            .arg("25")
            .assert()
            .failure()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains("Error"));
    }

    #[test]
    fn log_with_unix_timestamp() {
        let dir = tempdir().unwrap();

        // Unix timestamp for 2026-05-29T10:00:00Z
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("1780056000")
            .arg("--duration")
            .arg("25")
            .assert()
            .success();
    }

    // --- break log ------------------------------------------------------------

    #[test]
    fn break_log_needs_at_least_one_timestamp() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("log")
            .assert()
            .failure()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains(
                "at least one of --started-at or --finished-at",
            ));
    }

    #[test]
    fn break_log_all_three_is_error() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .arg("--finished-at")
            .arg("2026-05-29T10:05:00Z")
            .arg("--duration")
            .arg("5")
            .assert()
            .failure()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains(
                "cannot specify --duration when both",
            ));
    }

    #[test]
    fn break_log_with_started_at_and_duration() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .arg("--duration")
            .arg("5")
            .assert()
            .success();
    }

    #[test]
    fn break_log_with_finished_at_and_duration() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("log")
            .arg("--finished-at")
            .arg("2026-05-29T10:05:00Z")
            .arg("--duration")
            .arg("5")
            .assert()
            .success();
    }

    #[test]
    fn break_log_with_both_timestamps() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .arg("--finished-at")
            .arg("2026-05-29T10:10:00Z")
            .assert()
            .success();
    }

    #[test]
    fn break_log_finished_before_started_is_error() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:25:00Z")
            .arg("--finished-at")
            .arg("2026-05-29T10:00:00Z")
            .assert()
            .failure()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains(
                "--finished-at must be after --started-at",
            ));
    }

    #[test]
    fn break_log_with_default_duration() {
        let dir = tempdir().unwrap();

        // Only --started-at, no --duration → defaults to 5 minutes
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .assert()
            .success();

        // Verify via verbose that 5 min is recorded
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--verbose")
            .arg("--no-hooks")
            .arg("break")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T11:00:00Z")
            .assert()
            .success()
            .stdout(predicate::str::contains("5 min"));
    }

    #[test]
    fn break_log_overlapping_rule1_error() {
        let dir = tempdir().unwrap();

        // First log: 10:00 - 10:05
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .arg("--duration")
            .arg("5")
            .assert()
            .success();

        // Second log overlapping: 10:03 - 10:08
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:03:00Z")
            .arg("--duration")
            .arg("5")
            .assert()
            .failure()
            .code(predicate::eq(1))
            .stderr(predicate::str::contains("Error"));
    }

    #[test]
    fn break_log_with_unix_timestamp() {
        let dir = tempdir().unwrap();

        // Unix timestamp for 2026-05-29T10:00:00Z
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("break")
            .arg("log")
            .arg("--started-at")
            .arg("1780056000")
            .arg("--duration")
            .arg("5")
            .assert()
            .success();
    }

    // --- report day -----------------------------------------------------------

    #[test]
    fn report_day_empty() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("day")
            .arg("--date")
            .arg("2026-05-29")
            .assert()
            .success()
            .stdout(predicate::str::contains("Nothing recorded for this day."));
    }

    #[test]
    fn report_day_with_logged_pomodoro() {
        let dir = tempdir().unwrap();

        // Log a pomodoro
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .arg("--duration")
            .arg("25")
            .assert()
            .success();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("day")
            .arg("--date")
            .arg("2026-05-29")
            .assert()
            .success()
            .stdout(predicate::str::contains("Pomodori"))
            .stdout(predicate::str::contains("1 completed"))
            .stdout(predicate::str::contains("100% completion rate"));
    }

    #[test]
    fn report_day_with_multiple_entries() {
        let dir = tempdir().unwrap();

        // Log two pomodori
        for hour in [10, 11] {
            rustomato()
                .env("RUSTOMATO_ROOT", dir.path())
                .arg("--no-hooks")
                .arg("pomodoro")
                .arg("log")
                .arg("--started-at")
                .arg(format!("2026-05-29T{:02}:00:00Z", hour))
                .arg("--duration")
                .arg("25")
                .assert()
                .success();
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("day")
            .arg("--date")
            .arg("2026-05-29")
            .assert()
            .success()
            .stdout(predicate::str::contains("Pomodori"))
            .stdout(predicate::str::contains("2 completed"))
            .stdout(predicate::str::contains("0 cancelled"));
    }

    #[test]
    fn report_day_invalid_date() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("day")
            .arg("--date")
            .arg("not-a-date")
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid date"));
    }

    #[test]
    fn report_day_defaults_to_today() {
        use chrono::Local;

        let dir = tempdir().unwrap();

        // Log a pomodoro at a time that falls on today's date (local time)
        let today_midnight = Local::now()
            .date_naive()
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .earliest()
            .unwrap();
        let ts = today_midnight.format("%Y-%m-%dT%H:%M:%S").to_string();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg(&ts)
            .arg("--duration")
            .arg("25")
            .assert()
            .success();

        // Report without --date should pick up today's entries
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("day")
            .assert()
            .success()
            .stdout(predicate::str::contains("Pomodori"))
            .stdout(predicate::str::contains("1 completed"));
    }

    // ── Week report ────────────────────────────────────────

    #[test]
    fn report_week_empty() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("week")
            .arg("--date")
            .arg("2026-05-29")
            .assert()
            .success()
            .stdout(predicate::str::contains("nothing recorded this week"));
    }

    #[test]
    fn report_week_with_logged_pomodori() {
        let dir = tempdir().unwrap();

        // Log pomodori on Monday, Wednesday, and Friday of the ISO week
        // that contains 2026-05-29 (Friday).
        for (day, hour) in [("2026-05-25", 10), ("2026-05-27", 11), ("2026-05-29", 14)] {
            rustomato()
                .env("RUSTOMATO_ROOT", dir.path())
                .arg("--no-hooks")
                .arg("pomodoro")
                .arg("log")
                .arg("--started-at")
                .arg(format!("{}T{:02}:00:00Z", day, hour))
                .arg("--duration")
                .arg("25")
                .assert()
                .success();
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("week")
            .arg("--date")
            .arg("2026-05-29")
            .assert()
            .success()
            .stdout(predicate::str::contains("3 completed"))
            .stdout(predicate::str::contains("0 cancelled"));
    }

    #[test]
    fn report_week_invalid_date() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("week")
            .arg("--date")
            .arg("not-a-date")
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid date"));
    }

    #[test]
    fn report_week_defaults_to_today() {
        use chrono::Local;

        let dir = tempdir().unwrap();

        // Log a pomodoro that falls on today's date
        let today_midnight = Local::now()
            .date_naive()
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .earliest()
            .unwrap();
        let ts = today_midnight.format("%Y-%m-%dT%H:%M:%S").to_string();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg(&ts)
            .arg("--duration")
            .arg("25")
            .assert()
            .success();

        // Report without --date should pick up today's entries within this week
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("week")
            .assert()
            .success()
            .stdout(predicate::str::contains("completed"));
    }

    #[test]
    fn report_week_shows_day_by_day_breakdown() {
        let dir = tempdir().unwrap();

        // Log pomodori on Mon, Wed, Fri of the ISO week containing 2026-05-29
        for (day, hour) in [
            ("2026-05-25", 9),
            ("2026-05-26", 10),
            ("2026-05-27", 11),
            ("2026-05-29", 14),
        ] {
            rustomato()
                .env("RUSTOMATO_ROOT", dir.path())
                .arg("--no-hooks")
                .arg("pomodoro")
                .arg("log")
                .arg("--started-at")
                .arg(format!("{}T{:02}:00:00Z", day, hour))
                .arg("--duration")
                .arg("25")
                .assert()
                .success();
        }

        let assert = rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("week")
            .arg("--date")
            .arg("2026-05-29")
            .assert()
            .success();

        // Should show day headers for each day of the week
        assert
            .stdout(predicate::str::contains("Mon"))
            .stdout(predicate::str::contains("Tue"))
            .stdout(predicate::str::contains("Wed"))
            .stdout(predicate::str::contains("Thu"))
            .stdout(predicate::str::contains("Fri"))
            .stdout(predicate::str::contains("4 completed"));
    }

    // ── Interruption patterns report ────────────────────────

    #[test]
    fn report_interruptions_empty() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("interruptions")
            .arg("--date")
            .arg("2026-05-29")
            .arg("--days")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("No interruptions recorded"));
    }

    #[test]
    fn report_interruptions_invalid_date() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("interruptions")
            .arg("--date")
            .arg("not-a-date")
            .arg("--days")
            .arg("1")
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid date"));
    }

    #[test]
    fn report_interruptions_with_only_counter_data() {
        let dir = tempdir().unwrap();

        // Log a finished pomodoro that has interruptions recorded via counter
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .arg("--duration")
            .arg("25")
            .assert()
            .success();

        // The report should note that no interrupt log entries exist
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("interruptions")
            .arg("--date")
            .arg("2026-05-29")
            .arg("--days")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("No interruptions recorded"));
    }

    // ── Month report ─────────────────────────────────────────

    #[test]
    fn report_month_empty() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("month")
            .arg("--date")
            .arg("2025-01")
            .arg("--months")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("nothing recorded this month"));
    }

    #[test]
    fn report_month_with_logged_pomodori() {
        let dir = tempdir().unwrap();

        // Log pomodori on multiple days within May 2026
        for day in [
            "2026-05-04",
            "2026-05-05",
            "2026-05-11",
            "2026-05-12",
            "2026-05-13",
        ] {
            rustomato()
                .env("RUSTOMATO_ROOT", dir.path())
                .arg("--no-hooks")
                .arg("pomodoro")
                .arg("log")
                .arg("--started-at")
                .arg(format!("{}T10:00:00Z", day))
                .arg("--duration")
                .arg("25")
                .assert()
                .success();
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("month")
            .arg("--date")
            .arg("2026-05")
            .arg("--months")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("5 completed"))
            .stdout(predicate::str::contains("Monthly Report: May 2026"));
    }

    #[test]
    fn report_month_invalid_date() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("month")
            .arg("--date")
            .arg("not-a-date")
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid date"));
    }

    #[test]
    fn report_month_with_yyyymm_format() {
        let dir = tempdir().unwrap();

        // Log a pomodoro using YYYY-MM-DD format
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-06-15T09:00:00Z")
            .arg("--duration")
            .arg("25")
            .assert()
            .success();

        // Query with YYYY-MM format
        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("month")
            .arg("--date")
            .arg("2026-06")
            .arg("--months")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("1 completed"))
            .stdout(predicate::str::contains("June 2026"));
    }

    #[test]
    fn report_month_shows_week_breakdown() {
        let dir = tempdir().unwrap();

        // Log pomodori in two different weeks of May 2026
        for day in ["2026-05-04", "2026-05-11", "2026-05-25"] {
            rustomato()
                .env("RUSTOMATO_ROOT", dir.path())
                .arg("--no-hooks")
                .arg("pomodoro")
                .arg("log")
                .arg("--started-at")
                .arg(format!("{}T10:00:00Z", day))
                .arg("--duration")
                .arg("25")
                .assert()
                .success();
        }

        let assert = rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("month")
            .arg("--date")
            .arg("2026-05")
            .arg("--months")
            .arg("1")
            .assert()
            .success();

        assert
            .stdout(predicate::str::contains("Week-by-week"))
            .stdout(predicate::str::contains("3 completed"));
    }

    #[test]
    fn report_month_defaults_to_today() {
        use chrono::Local;

        let dir = tempdir().unwrap();

        // Log a pomodoro at today's time
        let today_midnight = Local::now()
            .date_naive()
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .earliest()
            .unwrap();
        let ts = today_midnight.format("%Y-%m-%dT%H:%M:%S").to_string();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg(&ts)
            .arg("--duration")
            .arg("25")
            .assert()
            .success();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("month")
            .arg("--months")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("Monthly Report"))
            .stdout(predicate::str::contains("completed"));
    }

    // ── Rolling window report ────────────────────────────────────

    #[test]
    fn report_last_empty() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("last")
            .arg("--date")
            .arg("2025-01-15")
            .arg("--days")
            .arg("7")
            .assert()
            .success()
            .stdout(predicate::str::contains("Nothing recorded"));
    }

    #[test]
    fn report_last_with_data() {
        let dir = tempdir().unwrap();

        // Log pomodori on consecutive days
        for day in ["2026-05-27", "2026-05-28", "2026-05-29"] {
            rustomato()
                .env("RUSTOMATO_ROOT", dir.path())
                .arg("--no-hooks")
                .arg("pomodoro")
                .arg("log")
                .arg("--started-at")
                .arg(format!("{}T10:00:00Z", day))
                .arg("--duration")
                .arg("25")
                .assert()
                .success();
        }

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("last")
            .arg("--date")
            .arg("2026-05-29")
            .arg("--days")
            .arg("3")
            .assert()
            .success()
            .stdout(predicate::str::contains("3 completed"))
            .stdout(predicate::str::contains("Day-by-day"));
    }

    #[test]
    fn report_last_single_day() {
        let dir = tempdir().unwrap();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg("2026-05-29T10:00:00Z")
            .arg("--duration")
            .arg("25")
            .assert()
            .success();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("last")
            .arg("--date")
            .arg("2026-05-29")
            .arg("--days")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("1 completed"))
            .stdout(predicate::str::contains("Last 1 day"));
    }

    #[test]
    fn report_last_defaults_to_today() {
        use chrono::Local;

        let dir = tempdir().unwrap();

        let today_midnight = Local::now()
            .date_naive()
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .earliest()
            .unwrap();
        let ts = today_midnight.format("%Y-%m-%dT%H:%M:%S").to_string();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("pomodoro")
            .arg("log")
            .arg("--started-at")
            .arg(&ts)
            .arg("--duration")
            .arg("25")
            .assert()
            .success();

        rustomato()
            .env("RUSTOMATO_ROOT", dir.path())
            .arg("--no-hooks")
            .arg("report")
            .arg("last")
            .arg("--days")
            .arg("1")
            .assert()
            .success()
            .stdout(predicate::str::contains("completed"));
    }

    // --- missing RUSTOMATO_ROOT directory -----------------------------------

    #[test]
    fn creates_root_directory_when_it_does_not_exist() {
        let dir = tempdir().unwrap();
        let missing_path = dir.path().join("nonexistent_subdir");
        assert!(
            !missing_path.exists(),
            "sanity check: path should not exist"
        );

        rustomato()
            .env("RUSTOMATO_ROOT", &missing_path)
            .arg("--no-hooks")
            .arg("status")
            .assert()
            .success();

        assert!(
            missing_path.is_dir(),
            "root directory should have been created"
        );
        assert!(
            missing_path.join("data.db").is_file(),
            "database should have been created"
        );
    }
}
