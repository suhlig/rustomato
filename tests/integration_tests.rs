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

    // --- nth_most_recently_finished_pomodoro ----------------------------------

    #[test]
    fn nth_most_recently_finished_pomodoro_none_when_empty() {
        let repo = Repository::new("file::memory:");
        let result = repo
            .nth_most_recently_finished_pomodoro(1)
            .expect("querying");
        assert!(result.is_none());
    }

    #[test]
    fn nth_most_recently_finished_pomodoro_returns_second_most_recent() {
        let repo = Repository::new("file::memory:");

        // First pomodoro (older, finished_at = 2000)
        {
            let mut pom = Schedulable::new(1, Kind::Pomodoro, 25);
            pom.started_at = 1000;
            repo.save(&pom).expect("saving first");
            pom.finished_at = 2000;
            repo.save(&pom).expect("finishing first");
        }

        // Second pomodoro (newer, finished_at = 4000)
        {
            let mut pom = Schedulable::new(2, Kind::Pomodoro, 25);
            pom.started_at = 3000;
            repo.save(&pom).expect("saving second");
            pom.finished_at = 4000;
            repo.save(&pom).expect("finishing second");
        }

        // Third pomodoro (most recent, finished_at = 6000)
        {
            let mut pom = Schedulable::new(3, Kind::Pomodoro, 25);
            pom.started_at = 5000;
            repo.save(&pom).expect("saving third");
            pom.finished_at = 6000;
            repo.save(&pom).expect("finishing third");
        }

        // index 2 = second most recent = finished_at 4000
        let result = repo
            .nth_most_recently_finished_pomodoro(2)
            .expect("querying")
            .expect("should find one");
        assert_eq!(result.finished_at, 4000);
    }

    // --- nth_most_recently_started ------------------------------------------

    #[test]
    fn nth_most_recently_started_none_when_empty() {
        let repo = Repository::new("file::memory:");
        let result = repo
            .nth_most_recently_started(1, None, None)
            .expect("querying");
        assert!(result.is_none());
    }

    #[test]
    fn nth_most_recently_started_returns_by_started_at() {
        let repo = Repository::new("file::memory:");

        // First pomodoro (oldest, started_at = 1000)
        let mut pom1 = Schedulable::new(1, Kind::Pomodoro, 25);
        pom1.started_at = 1000;
        repo.save(&pom1).expect("saving pom1");
        pom1.finished_at = 2000;
        repo.save(&pom1).expect("finishing pom1");

        // Second entry — a break (started_at = 3000)
        let mut brk = Schedulable::new(2, Kind::Break, 5);
        brk.started_at = 3000;
        repo.save(&brk).expect("saving break");
        brk.finished_at = 3300;
        repo.save(&brk).expect("finishing break");

        // Third pomodoro (most recently started, started_at = 5000)
        let mut pom3 = Schedulable::new(3, Kind::Pomodoro, 25);
        pom3.started_at = 5000;
        repo.save(&pom3).expect("saving pom3");
        // leave pom3 active (no finished_at)

        // no kind filter: n=1 → most recently started = pom3 (active, started_at=5000)
        let result = repo
            .nth_most_recently_started(1, None, None)
            .expect("querying")
            .expect("should find one");
        assert_eq!(result.started_at, 5000);
        assert_eq!(result.kind, Kind::Pomodoro);

        // no kind filter: n=2 → second most recently started = break (started_at=3000)
        let result = repo
            .nth_most_recently_started(2, None, None)
            .expect("querying")
            .expect("should find one");
        assert_eq!(result.started_at, 3000);
        assert_eq!(result.kind, Kind::Break);

        // no kind filter: n=3 → third most recently started = pom1 (started_at=1000)
        let result = repo
            .nth_most_recently_started(3, None, None)
            .expect("querying")
            .expect("should find one");
        assert_eq!(result.started_at, 1000);
        assert_eq!(result.kind, Kind::Pomodoro);
    }

    #[test]
    fn nth_most_recently_started_filters_by_kind() {
        let repo = Repository::new("file::memory:");

        // Pomodoro (started_at = 1000)
        let mut pom = Schedulable::new(1, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving pom");
        pom.finished_at = 2000;
        repo.save(&pom).expect("finishing pom");

        // Break (started_at = 3000) — more recent, but different kind
        let mut brk = Schedulable::new(2, Kind::Break, 5);
        brk.started_at = 3000;
        repo.save(&brk).expect("saving break");
        brk.finished_at = 3300;
        repo.save(&brk).expect("finishing break");

        // Filtered to break: n=1 → the break
        let result = repo
            .nth_most_recently_started(1, Some(Kind::Break), None)
            .expect("querying")
            .expect("should find one");
        assert_eq!(result.started_at, 3000);
        assert_eq!(result.kind, Kind::Break);

        // Filtered to pomodoro: n=1 → the pomodoro (skips the break)
        let result = repo
            .nth_most_recently_started(1, Some(Kind::Pomodoro), None)
            .expect("querying")
            .expect("should find one");
        assert_eq!(result.started_at, 1000);
        assert_eq!(result.kind, Kind::Pomodoro);

        // Filtered to pomodoro: n=2 → no more pomodori
        let result = repo
            .nth_most_recently_started(2, Some(Kind::Pomodoro), None)
            .expect("querying");
        assert!(result.is_none());
    }

    #[test]
    fn nth_most_recently_started_includes_active() {
        let repo = Repository::new("file::memory:");

        // Finished pomodoro (started_at = 1000)
        let mut pom1 = Schedulable::new(1, Kind::Pomodoro, 25);
        pom1.started_at = 1000;
        repo.save(&pom1).expect("saving pom1");
        pom1.finished_at = 2000;
        repo.save(&pom1).expect("finishing pom1");

        // Active pomodoro (started_at = 3000) — more recent, still running
        let mut pom2 = Schedulable::new(2, Kind::Pomodoro, 25);
        pom2.started_at = 3000;
        repo.save(&pom2).expect("saving pom2");
        // no finished_at → still active

        // n=1 should return the active one (most recently started)
        let result = repo
            .nth_most_recently_started(1, Some(Kind::Pomodoro), None)
            .expect("querying")
            .expect("should find one");
        assert_eq!(result.started_at, 3000);
        assert_eq!(result.finished_at, 0); // still active

        // n=2 should return the finished one
        let result = repo
            .nth_most_recently_started(2, Some(Kind::Pomodoro), None)
            .expect("querying")
            .expect("should find one");
        assert_eq!(result.started_at, 1000);
        assert_eq!(result.finished_at, 2000);
    }

    // --- find_by_uuid_prefix --------------------------------------------------

    #[test]
    fn find_by_uuid_prefix_empty_prefix_fails() {
        let repo = Repository::new("file::memory:");
        let result = repo.find_by_uuid_prefix("deadbeef");
        assert!(result.is_err());
        assert_matches!(result, Err(PersistenceError::CannotFind(_)));
    }

    #[test]
    fn find_by_uuid_prefix_matches_abbreviated() {
        let repo = Repository::new("file::memory:");

        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active");
        pom.finished_at = 2000;
        repo.save(&pom).expect("finishing");

        let uuid_str = pom.uuid.to_string();
        let prefix = &uuid_str[..10];

        let found = repo
            .find_by_uuid_prefix(prefix)
            .expect("should find by prefix");
        assert_eq!(found.uuid.to_string(), uuid_str);
    }

    // --- find_by_timestamp ----------------------------------------------------

    #[test]
    fn find_by_timestamp_within_finished_range() {
        let repo = Repository::new("file::memory:");

        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active");
        pom.finished_at = 2000;
        repo.save(&pom).expect("finishing");

        // Timestamp in the middle of the range
        let found = repo.find_by_timestamp(1500).expect("querying");
        assert!(found.is_some());
        assert_eq!(found.unwrap().started_at, 1000);
    }

    #[test]
    fn find_by_timestamp_outside_range_returns_none() {
        let repo = Repository::new("file::memory:");

        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active");
        pom.finished_at = 2000;
        repo.save(&pom).expect("finishing");

        // Before started_at
        assert!(repo.find_by_timestamp(500).expect("querying").is_none());
        // After finished_at
        assert!(repo.find_by_timestamp(2500).expect("querying").is_none());
    }

    #[test]
    fn find_by_timestamp_matches_active_entry() {
        let repo = Repository::new("file::memory:");

        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active");

        // Active entry has no finished_at, so any timestamp >= started_at should match
        let found = repo.find_by_timestamp(1000).expect("querying");
        assert!(found.is_some());
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

    // --- save_external_finished --------------------------------------------

    #[test]
    fn save_external_finished_pomodoro() {
        let repo = Repository::new("file::memory:");

        let mut pom = Schedulable::new(0, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        pom.finished_at = 1000 + 25 * 60;

        let saved = repo
            .save_external_finished(&pom)
            .expect("saving external finished pomodoro");

        assert_eq!(saved.finished_at, 1000 + 25 * 60);
        assert_eq!(saved.kind, Kind::Pomodoro);
        assert_eq!(saved.interruptions, 0);
    }

    #[test]
    fn save_external_finished_break() {
        let repo = Repository::new("file::memory:");

        let mut brk = Schedulable::new(0, Kind::Break, 5);
        brk.started_at = 2000;
        brk.finished_at = 2000 + 5 * 60;

        let saved = repo
            .save_external_finished(&brk)
            .expect("saving external finished break");

        assert_eq!(saved.finished_at, 2000 + 5 * 60);
        assert_eq!(saved.kind, Kind::Break);
        assert_eq!(saved.interruptions, 0);
    }

    #[test]
    fn save_external_finished_overlaps_existing_returns_error() {
        let repo = Repository::new("file::memory:");

        // Insert a finished pomodoro [1000, 2500]
        let mut existing = Schedulable::new(0, Kind::Pomodoro, 25);
        existing.started_at = 1000;
        existing.finished_at = 2500;
        repo.save_external_finished(&existing)
            .expect("saving existing pomodoro");

        // Try to insert an overlapping pomodoro [2000, 3500]
        let mut overlapping = Schedulable::new(0, Kind::Pomodoro, 25);
        overlapping.started_at = 2000;
        overlapping.finished_at = 3500;

        let result = repo.save_external_finished(&overlapping);
        assert_matches!(result, Err(PersistenceError::OverlappingTimeRange));
    }

    #[test]
    fn save_external_finished_adjacent_non_overlapping_succeeds() {
        let repo = Repository::new("file::memory:");

        // Insert a finished pomodoro [1000, 2500]
        let mut existing = Schedulable::new(0, Kind::Pomodoro, 25);
        existing.started_at = 1000;
        existing.finished_at = 2500;
        repo.save_external_finished(&existing)
            .expect("saving existing pomodoro");

        // Insert an adjacent non-overlapping pomodoro [2500, 4000]
        let mut adjacent = Schedulable::new(0, Kind::Pomodoro, 25);
        adjacent.started_at = 2500;
        adjacent.finished_at = 4000;

        let result = repo.save_external_finished(&adjacent);
        assert!(result.is_ok());
    }

    // --- entries_between -----------------------------------------------------

    #[test]
    fn entries_between_empty() {
        let repo = Repository::new("file::memory:");
        let entries = repo.entries_between(0, 1000).expect("querying entries");
        assert!(entries.is_empty());
    }

    #[test]
    fn entries_between_returns_entries_in_range() {
        let repo = Repository::new("file::memory:");

        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active");
        pom.finished_at = 2000;
        repo.save(&pom).expect("finishing");

        let mut brk = Schedulable::new(43, Kind::Break, 5);
        brk.started_at = 3000;
        repo.save(&brk).expect("saving break");
        brk.finished_at = 3300;
        repo.save(&brk).expect("finishing break");

        // Range covering both entries
        let all = repo.entries_between(0, 4000).expect("querying all");
        assert_eq!(all.len(), 2);

        // Range covering only the first
        let first = repo.entries_between(0, 2500).expect("querying first");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].started_at, 1000);

        // Range covering only the second
        let second = repo.entries_between(2501, 4000).expect("querying second");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].started_at, 3000);

        // Range before both
        let none = repo.entries_between(0, 999).expect("querying none");
        assert!(none.is_empty());
    }

    // --- save_interrupt / interrupts_between ---------------------------------

    #[test]
    fn save_interrupt_and_query() {
        use rustomato::{InterruptLog, InterruptionKind};

        let repo = Repository::new("file::memory:");

        // First create a finished pomodoro to reference
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active");
        pom.finished_at = 2000;
        let saved = repo.save(&pom).expect("finishing");

        // Save two interrupt logs referencing it
        let log1 = InterruptLog {
            uuid: SqlUuid::default(),
            schedulable_uuid: saved.uuid,
            kind: InterruptionKind::Internal,
            created_at: 1100,
        };
        repo.save_interrupt(&log1).expect("saving interrupt log 1");

        let log2 = InterruptLog {
            uuid: SqlUuid::default(),
            schedulable_uuid: saved.uuid,
            kind: InterruptionKind::External,
            created_at: 1200,
        };
        repo.save_interrupt(&log2).expect("saving interrupt log 2");

        // Query all interrupts in range
        let logs = repo
            .interrupts_between(1000, 2000)
            .expect("querying interrupts");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].kind, InterruptionKind::Internal);
        assert_eq!(logs[1].kind, InterruptionKind::External);

        // Query a sub-range
        let sub = repo
            .interrupts_between(1150, 1250)
            .expect("querying subrange");
        assert_eq!(sub.len(), 1);
        assert_eq!(sub[0].kind, InterruptionKind::External);

        // Query outside range
        let none = repo.interrupts_between(0, 1000).expect("querying none");
        assert!(none.is_empty());
    }

    #[test]
    fn save_interrupt_updates_counter_on_schedulable() {
        use rustomato::{InterruptLog, InterruptionKind};

        let repo = Repository::new("file::memory:");

        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active");
        pom.finished_at = 2000;
        let saved = repo.save(&pom).expect("finishing");

        // Record interrupt through the scheduler-like sequence:
        // increment counter + save log
        repo.record_interrupt(saved.uuid)
            .expect("recording interrupt");
        let log = InterruptLog {
            uuid: SqlUuid::default(),
            schedulable_uuid: saved.uuid,
            kind: InterruptionKind::Internal,
            created_at: 1100,
        };
        repo.save_interrupt(&log).expect("saving interrupt log");

        // Counter and log should agree
        let schedulable = repo.find_by_uuid(saved.uuid).expect("finding");
        assert_eq!(schedulable.interruptions, 1);

        let logs = repo
            .interrupts_between(0, 9999)
            .expect("querying interrupts");
        assert_eq!(logs.len(), 1);
    }

    // --- consecutive_pomodoro_count ------------------------------------------

    #[test]
    fn consecutive_pomodoro_count_none() {
        let repo = Repository::new("file::memory:");
        let count = repo
            .consecutive_pomodoro_count_at(10000)
            .expect("querying count");
        assert_eq!(count, 0);
    }

    #[test]
    fn consecutive_pomodoro_count_single_finished() {
        let repo = Repository::new("file::memory:");

        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active");
        pom.finished_at = 2000;
        repo.save(&pom).expect("finishing");

        let count = repo
            .consecutive_pomodoro_count_at(10000)
            .expect("querying count");
        assert_eq!(count, 1);
    }

    #[test]
    fn consecutive_pomodoro_count_ignores_cancelled() {
        let repo = Repository::new("file::memory:");

        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 1000;
        repo.save(&pom).expect("saving active");
        pom.cancelled_at = 1500;
        repo.save(&pom).expect("cancelling");

        let count = repo
            .consecutive_pomodoro_count_at(10000)
            .expect("querying count");
        assert_eq!(count, 0);
    }

    #[test]
    fn consecutive_pomodoro_count_four_finished() {
        let repo = Repository::new("file::memory:");

        for i in 0..4 {
            let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
            pom.started_at = 1000 + i * 2000;
            repo.save(&pom).expect("saving active");
            pom.finished_at = 2000 + i * 2000;
            repo.save(&pom).expect("finishing");
        }

        let count = repo
            .consecutive_pomodoro_count_at(10000)
            .expect("querying count");
        assert_eq!(count, 4);
    }

    #[test]
    fn consecutive_pomodoro_count_resets_after_long_break() {
        let repo = Repository::new("file::memory:");

        // Three finished pomodori
        for i in 0..3 {
            let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
            pom.started_at = 1000 + i * 2000;
            repo.save(&pom).expect("saving active");
            pom.finished_at = 2000 + i * 2000;
            repo.save(&pom).expect("finishing");
        }

        // A long break (duration = 15 >= threshold of 10)
        let mut brk = Schedulable::new(0, Kind::Break, 15);
        brk.started_at = 7000;
        brk.finished_at = 8000;
        repo.save_external_finished(&brk)
            .expect("saving long break");

        // Another finished pomodoro after the long break
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 9000;
        repo.save(&pom).expect("saving active");
        pom.finished_at = 10000;
        repo.save(&pom).expect("finishing");

        let count = repo
            .consecutive_pomodoro_count_at(10000)
            .expect("querying count");
        assert_eq!(count, 1);
    }

    #[test]
    fn consecutive_pomodoro_count_short_break_does_not_reset() {
        let repo = Repository::new("file::memory:");

        // Three finished pomodori
        for i in 0..3 {
            let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
            pom.started_at = 1000 + i * 2000;
            repo.save(&pom).expect("saving active");
            pom.finished_at = 2000 + i * 2000;
            repo.save(&pom).expect("finishing");
        }

        // A short break (duration = 5 < threshold of 10) — should NOT reset
        let mut brk = Schedulable::new(0, Kind::Break, 5);
        brk.started_at = 7000;
        brk.finished_at = 7300;
        repo.save_external_finished(&brk)
            .expect("saving short break");

        // Another finished pomodoro after the short break
        let mut pom = Schedulable::new(42, Kind::Pomodoro, 25);
        pom.started_at = 8000;
        repo.save(&pom).expect("saving active");
        pom.finished_at = 9000;
        repo.save(&pom).expect("finishing");

        // Count should be 4 (3 before + 1 after short break)
        let count = repo
            .consecutive_pomodoro_count_at(10000)
            .expect("querying count");
        assert_eq!(count, 4);
    }
}

// --- parse_timestamp ---------------------------------------------------------

mod parse_timestamp_tests {
    use rustomato::parse_timestamp;

    #[test]
    fn parse_rfc3339_with_zulu() {
        let ts = parse_timestamp("2026-05-29T14:30:00Z").expect("parsing RFC 3339 Z");
        assert_eq!(ts, 1780065000);
    }

    #[test]
    fn parse_rfc3339_with_offset() {
        // 2026-05-29T14:30:00+02:00 = 2026-05-29T12:30:00Z
        let ts =
            parse_timestamp("2026-05-29T14:30:00+02:00").expect("parsing RFC 3339 with offset");
        assert_eq!(ts, 1780057800);
    }

    #[test]
    fn parse_unix_timestamp() {
        let ts = parse_timestamp("1716994200").expect("parsing Unix timestamp");
        assert_eq!(ts, 1716994200);
    }

    #[test]
    fn parse_invalid_returns_error() {
        let result = parse_timestamp("not-a-timestamp");
        assert!(result.is_err());
    }
}
