//! CLI integration tests for `ara validate`.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn ara() -> Command {
    Command::cargo_bin("ara").expect("binary builds")
}

fn official(name: &str) -> PathBuf {
    // ara-cli/tests -> ara-cli -> crates -> repo root, then into ara-core fixtures.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ara-core/tests/fixtures/official")
        .join(name)
}

/// Builds a temp ARA artifact with the given tree YAML and optional claims.
fn artifact(tree_yaml: &str, claims_md: Option<&str>) -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("trace")).unwrap();
    std::fs::write(dir.path().join("trace/exploration_tree.yaml"), tree_yaml).unwrap();
    if let Some(claims) = claims_md {
        std::fs::create_dir_all(dir.path().join("logic")).unwrap();
        std::fs::write(dir.path().join("logic/claims.md"), claims).unwrap();
    }
    dir
}

#[test]
fn validate_official_exits_zero() {
    ara()
        .arg("validate")
        .arg(official("minimal-artifact"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));

    ara()
        .arg("validate")
        .arg(official("resnet-ara-example"))
        .assert()
        .success();
}

#[test]
fn validate_broken_exits_nonzero() {
    let dir = artifact(
        "tree:\n  - id: N01\n    type: question\n  - id: N01\n    type: insight\n",
        None,
    );
    ara()
        .arg("validate")
        .arg(dir.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("duplicate node id"));
}

#[test]
fn json_output_is_valid_json() {
    let output = ara()
        .arg("validate")
        .arg(official("minimal-artifact"))
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert!(parsed.get("errors").is_some());
    assert!(parsed.get("warnings").is_some());
}

#[test]
fn strict_promotes_warnings_to_failure() {
    // Unknown field -> warning, no error. Exit 0 normally, non-zero with --strict.
    let dir = artifact(
        "tree:\n  - id: N01\n    type: question\n    title: q\n    bogus_field: 1\n",
        None,
    );

    ara()
        .arg("validate")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("unknown field"));

    ara()
        .arg("validate")
        .arg(dir.path())
        .arg("--strict")
        .assert()
        .failure();
}

#[test]
fn missing_dir_is_clean_error_not_panic() {
    ara()
        .arg("validate")
        .arg("/no/such/ara/dir")
        .assert()
        .failure()
        .stdout(predicate::str::contains("cannot read"));
}

#[test]
fn missing_tree_file_is_clean_error() {
    let dir = TempDir::new().unwrap(); // empty, no trace/exploration_tree.yaml
    ara()
        .arg("validate")
        .arg(dir.path())
        .assert()
        .failure()
        .stdout(predicate::str::contains("cannot read"));
}
