use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

// Basic check that the binary is invocable and Clap wiring works
#[test]
fn prints_version() {
    let mut cmd = Command::cargo_bin("i2pd-exporter").unwrap();
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("i2pd-exporter"));
}
