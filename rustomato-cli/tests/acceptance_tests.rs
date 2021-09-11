mod acceptance_tests {
  use assert_cmd::Command;
  use tempfile::tempdir;
  use predicates::prelude::*;

  #[test]
  fn plain() {
    let mut cmd = Command::cargo_bin("rustomato").unwrap();
    cmd.assert().success();
  }

  #[test]
  fn verbose() {
    let rustomato_root = tempdir().unwrap();

    let assert = Command::cargo_bin("rustomato")
      .unwrap()
      .env("RUSTOMATO_ROOT", rustomato_root.into_path())
      .arg("--verbose")
      .arg("status")
      .assert();

    assert.success().stdout(predicate::str::starts_with("Using root"));
  }
}
