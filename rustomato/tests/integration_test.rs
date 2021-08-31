use rustomato::persistence::Repository;
use rustomato::{Schedulable, Kind};

#[test]
fn test_no_active() {
    let repo = Repository::from_str("file::memory:");
    let active = repo.active().expect("querying active");
    assert_eq!(active.is_none(), true);
}

#[test]
fn test_save_new() {
    let repo = Repository::from_str("file::memory:");
    let result = repo.save(&Schedulable::new(4711, Kind::Pomodoro, 25));
    assert_eq!(result.is_err(), true);
}

#[test]
fn test_one_active() {
    let repo = Repository::from_str("file::memory:");
    let mut pom = Schedulable::new(4711, Kind::Pomodoro, 25);
    pom.started_at = 12;
    repo.save(&pom).expect("saving new pomodoro");

    let active = repo.active().expect("querying active");
    assert_eq!(active.is_some(), true);
}
