mod integration_tests {
    use assert_matches::assert_matches;
    use rustomato::persistence::{PersistenceError, Repository};
    use rustomato::{Annotation, Kind, Schedulable, SqlUuid};

    #[test]
    fn no_active() {
        let repo = Repository::new("file::memory:");
        let active = repo.active().expect("querying active");
        assert!(active.is_none());
    }

    #[test]
    fn save_new() {
        let repo = Repository::new("file::memory:");
        let result = repo.save(&Schedulable::new(4711, Kind::Pomodoro, 25));
        assert!(result.is_err());
    }

    #[test]
    fn save_active() {
        let repo = Repository::new("file::memory:");
        let mut pom = Schedulable::new(4711, Kind::Pomodoro, 25);
        pom.started_at = 12;
        repo.save(&pom).expect("saving active pomodoro");

        let active = repo.active().expect("querying active");
        assert!(active.is_some());
    }

    #[test]
    fn save_finished() {
        let repo = Repository::new("file::memory:");
        let mut pom = Schedulable::new(4711, Kind::Pomodoro, 25);
        pom.started_at = 12;

        let result = repo.save(&pom);
        assert!(result.is_ok());

        // finish
        pom.finished_at = 13;
        let result = repo.save(&pom);
        assert!(result.is_ok());

        match result {
            Ok(finished) => {
                assert_eq!(finished.finished_at, 13);
            }
            Err(_) => panic!("Should have been covered above"),
        }
    }

    #[test]
    fn save_cancelled() {
        let repo = Repository::new("file::memory:");
        let mut pom = Schedulable::new(4711, Kind::Pomodoro, 25);
        pom.started_at = 12;

        let result = repo.save(&pom);
        assert!(result.is_ok());

        // cancel
        pom.cancelled_at = 14;
        let result = repo.save(&pom);
        assert!(result.is_ok());

        match result {
            Ok(finished) => assert_eq!(finished.cancelled_at, 14),
            Err(_) => panic!("Should have been covered above"),
        }
    }

    #[test]
    fn save_second_after_finish() {
        let repo = Repository::new("file::memory:");
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 12;
        repo.save(&pom).expect("saving active pomodoro");

        pom.finished_at = 13;
        repo.save(&pom).expect("saving finished pomodoro");

        let mut second = Schedulable::new(4711, Kind::Break, 25);
        second.started_at = 14;
        let result = repo.save(&second);
        assert!(result.is_ok());
    }

    #[test]
    fn save_second() {
        let repo = Repository::new("file::memory:");
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 12;
        repo.save(&pom).expect("saving active pomodoro");

        let mut second = Schedulable::new(4711, Kind::Break, 25);
        second.started_at = 13;
        let result = repo.save(&second);
        assert!(result.is_err());

        match result {
            Ok(_) => panic!("Should have been covered above"),
            Err(e) => assert_eq!(e, PersistenceError::AlreadyRunning(42)),
        }
    }

    // --- record_interrupt -----------------------------------------------------

    #[test]
    fn record_interrupt_increments_counter() {
        let repo = Repository::new("file::memory:");
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active pomodoro");

        // Fetch it back to get the generated UUID
        let active = repo.active().expect("querying active").unwrap();

        let updated = repo
            .record_interrupt(active.uuid)
            .expect("recording interrupt");
        assert_eq!(updated.interruptions, 1);

        // Second interrupt
        let updated = repo
            .record_interrupt(active.uuid)
            .expect("recording second interrupt");
        assert_eq!(updated.interruptions, 2);
    }

    #[test]
    fn record_interrupt_on_nonexistent_uuid() {
        let repo = Repository::new("file::memory:");
        let dummy = SqlUuid::default();
        let result = repo.record_interrupt(dummy);
        assert!(result.is_err());
        assert_matches!(result, Err(PersistenceError::CannotFind(_)));
    }

    // --- most_recently_finished_pomodoro -------------------------------------

    #[test]
    fn most_recently_finished_pomodoro_none() {
        let repo = Repository::new("file::memory:");
        let result = repo.most_recently_finished_pomodoro().expect("querying");
        assert!(result.is_none());
    }

    #[test]
    fn most_recently_finished_pomodoro_returns_most_recent() {
        let repo = Repository::new("file::memory:");
        let now = 1000;

        // First pomodoro
        let mut pom1 = Schedulable::new(42, Kind::Pomodoro, 25);
        pom1.started_at = now;
        repo.save(&pom1).expect("saving pom1");
        pom1.finished_at = now + 10;
        repo.save(&pom1).expect("finishing pom1");

        // Second pomodoro
        let mut pom2 = Schedulable::new(43, Kind::Pomodoro, 25);
        pom2.started_at = now + 20;
        repo.save(&pom2).expect("saving pom2");
        pom2.finished_at = now + 30;
        repo.save(&pom2).expect("finishing pom2");

        let result = repo
            .most_recently_finished_pomodoro()
            .expect("querying")
            .expect("should find one");
        assert_eq!(result.finished_at, now + 30);
    }

    #[test]
    fn most_recently_finished_pomodoro_ignores_active_pomodoro() {
        let repo = Repository::new("file::memory:");
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active pomodoro");

        // Active pomodoro should not appear as "finished"
        let result = repo.most_recently_finished_pomodoro().expect("querying");
        assert!(result.is_none());
    }

    #[test]
    fn pom_finished_before_started() {
        let repo = Repository::new("file::memory:");
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 13;
        repo.save(&pom).expect("saving active pomodoro");

        pom.finished_at = 12;
        let result = repo.save(&pom);
        assert!(result.is_err());

        match result {
            Ok(_) => panic!("Should have been covered above"),
            Err(e) => assert_matches!(e, PersistenceError::CannotUpdate(msg) => {
                assert!(msg.starts_with("CHECK constraint failed"));
            }),
        }
    }

    #[test]
    fn today_empty() {
        let repo = Repository::new("file::memory:");
        let entries = repo.today().expect("querying today");
        assert!(entries.is_empty());
    }

    #[test]
    fn today_with_entries() {
        use chrono::Local;

        let repo = Repository::new("file::memory:");
        let now = Local::now().timestamp();

        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = now;
        repo.save(&pom).expect("saving active pomodoro");

        pom.finished_at = now + 1;
        repo.save(&pom).expect("saving finished pomodoro");

        let entries = repo.today().expect("querying today");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].finished_at, now + 1);
    }

    #[test]
    fn today_multiple_kinds() {
        use chrono::Local;

        let repo = Repository::new("file::memory:");
        let now = Local::now().timestamp();

        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = now;
        repo.save(&pom).expect("saving pomodoro");
        pom.finished_at = now + 1;
        repo.save(&pom).expect("finishing pomodoro");

        let mut brk = Schedulable::new(43, Kind::Break, 5);
        brk.started_at = now + 2;
        repo.save(&brk).expect("saving break");
        brk.finished_at = now + 3;
        repo.save(&brk).expect("finishing break");

        let entries = repo.today().expect("querying today");
        assert_eq!(entries.len(), 2);
    }

    // --- save_annotation -------------------------------------------------------

    #[test]
    fn save_annotation() {
        let repo = Repository::new("file::memory:");
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active pomodoro");
        pom.finished_at = 1001;
        let saved = repo.save(&pom).expect("saving finished pomodoro");

        let annotation = Annotation {
            uuid: SqlUuid::default(),
            schedulable_uuid: saved.uuid,
            body: "test annotation".to_string(),
            created_at: 1002,
        };
        repo.save_annotation(&annotation)
            .expect("saving annotation");

        let found = repo
            .find_annotation_by_uuid(annotation.uuid)
            .expect("finding annotation");
        assert_eq!(found.body, "test annotation");
        assert_eq!(found.schedulable_uuid.to_string(), saved.uuid.to_string());
    }

    #[test]
    fn save_annotation_twice_for_same_schedulable() {
        let repo = Repository::new("file::memory:");
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active pomodoro");
        pom.finished_at = 1001;
        let saved = repo.save(&pom).expect("saving finished pomodoro");

        let annotation1 = Annotation {
            uuid: SqlUuid::default(),
            schedulable_uuid: saved.uuid,
            body: "first annotation".to_string(),
            created_at: 1002,
        };
        repo.save_annotation(&annotation1)
            .expect("saving first annotation");

        let annotation2 = Annotation {
            uuid: SqlUuid::default(),
            schedulable_uuid: saved.uuid,
            body: "second annotation".to_string(),
            created_at: 1003,
        };
        repo.save_annotation(&annotation2)
            .expect("saving second annotation");

        let annotations = repo
            .annotations_for(saved.uuid)
            .expect("querying annotations");
        assert_eq!(annotations.len(), 2);
        assert_eq!(annotations[0].body, "first annotation");
        assert_eq!(annotations[1].body, "second annotation");
    }

    #[test]
    fn annotations_for_nonexistent() {
        let repo = Repository::new("file::memory:");
        let dummy = SqlUuid::default();
        let annotations = repo.annotations_for(dummy).expect("querying annotations");
        assert!(annotations.is_empty());
    }

    // --- most_recently_ended ---------------------------------------------------

    #[test]
    fn most_recently_ended_none() {
        let repo = Repository::new("file::memory:");
        let result = repo.most_recently_ended().expect("querying");
        assert!(result.is_none());
    }

    #[test]
    fn most_recently_ended_finished() {
        let repo = Repository::new("file::memory:");

        let mut brk = Schedulable::new(42, Kind::Break, 5);
        brk.started_at = 10;
        repo.save(&brk).expect("saving break");
        brk.finished_at = 15;
        repo.save(&brk).expect("finishing break");

        let mut pom = Schedulable::new(43, Kind::Pomodoro, 25);
        pom.started_at = 20;
        repo.save(&pom).expect("saving pomodoro");
        pom.finished_at = 30;
        repo.save(&pom).expect("finishing pomodoro");

        let result = repo
            .most_recently_ended()
            .expect("querying")
            .expect("should find one");
        assert_eq!(result.finished_at, 30);
        assert_eq!(result.kind, Kind::Pomodoro);
    }

    #[test]
    fn most_recently_ended_cancelled() {
        let repo = Repository::new("file::memory:");

        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 100;
        repo.save(&pom).expect("saving pomodoro");
        pom.finished_at = 110;
        repo.save(&pom).expect("finishing pomodoro");

        let mut brk = Schedulable::new(43, Kind::Break, 5);
        brk.started_at = 120;
        repo.save(&brk).expect("saving break");
        brk.cancelled_at = 125;
        repo.save(&brk).expect("cancelling break");

        let result = repo
            .most_recently_ended()
            .expect("querying")
            .expect("should find one");
        // The cancelled break has cancelled_at=125, the finished pomodoro has finished_at=110
        // So the break should be returned since 125 > 110
        assert_eq!(result.cancelled_at, 125);
        assert_eq!(result.kind, Kind::Break);
    }

    #[test]
    fn most_recently_ended_ignores_active() {
        let repo = Repository::new("file::memory:");
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active pomodoro");

        let result = repo.most_recently_ended().expect("querying");
        assert!(result.is_none());
    }

    #[test]
    fn most_recently_ended_mixed_kinds() {
        let repo = Repository::new("file::memory:");

        // Finished pomodoro at t=10
        let mut pom1 = Schedulable::new(42, Kind::Pomodoro, 25);
        pom1.started_at = 5;
        repo.save(&pom1).expect("saving pom1");
        pom1.finished_at = 10;
        repo.save(&pom1).expect("finishing pom1");

        // Cancelled break at t=20
        let mut brk = Schedulable::new(43, Kind::Break, 5);
        brk.started_at = 15;
        repo.save(&brk).expect("saving break");
        brk.cancelled_at = 20;
        repo.save(&brk).expect("cancelling break");

        // Finished pomodoro at t=30
        let mut pom2 = Schedulable::new(44, Kind::Pomodoro, 25);
        pom2.started_at = 25;
        repo.save(&pom2).expect("saving pom2");
        pom2.finished_at = 30;
        repo.save(&pom2).expect("finishing pom2");

        let result = repo
            .most_recently_ended()
            .expect("querying")
            .expect("should find one");
        // The most recent is pom2 with finished_at=30
        assert_eq!(result.finished_at, 30);
        assert_eq!(result.kind, Kind::Pomodoro);
    }
}
