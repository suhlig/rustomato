use rustomato::persistence::Repository;

#[test]
fn test_no_active() {
    let repo = Repository::from_str("file::memory:");
    let active = repo.active().expect("querying active");
    assert_eq!(active.is_none(), true);
}
