use rustomato::persistence::{Repository, PersistenceError};
use rustomato::{Kind, Schedulable};

#[test]
fn no_active() {
    let repo = Repository::new("file::memory:");
    let active = repo.active().expect("querying active");
    assert_eq!(active.is_none(), true);
}

#[test]
fn save_new() {
    let repo = Repository::new("file::memory:");
    let result = repo.save(&Schedulable::new(4711, Kind::Pomodoro, 25));
    assert_eq!(result.is_err(), true);
}

#[test]
fn save_active() {
    let repo = Repository::new("file::memory:");
    let mut pom = Schedulable::new(4711, Kind::Pomodoro, 25);
    pom.started_at = 12;
    repo.save(&pom).expect("saving active pomodoro");

    let active = repo.active().expect("querying active");
    assert_eq!(active.is_some(), true);
}

#[test]
fn save_finished() {
    let repo = Repository::new("file::memory:");
    let mut pom = Schedulable::new(4711, Kind::Pomodoro, 25);
    pom.started_at = 12;

    let result = repo.save(&pom);
    assert_eq!(result.is_ok(), true);

    // finish
    pom.finished_at = 13;
    let result = repo.save(&pom);
    assert_eq!(result.is_ok(), true);

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
    assert_eq!(result.is_ok(), true);

    // cancel
    pom.cancelled_at = 14;
    let result = repo.save(&pom);
    assert_eq!(result.is_ok(), true);

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
    assert_eq!(result.is_ok(), true);
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
    assert_eq!(result.is_err(), true);

    match result {
        Ok(_) => panic!("Should have been covered above"),
        Err(e) => assert_eq!(e, PersistenceError::AlreadyRunning(42)),
    }
}
