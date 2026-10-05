//! Private journals and alias records written by the binary that still treated
//! `rubric/` as native knowledge stay usable after the cutover.
//!
//! `tests/fixtures/rubric-aliases` was produced by that binary (`ara` 0.1.23
//! built from commit 4b0da70): `ours` is the
//! result of merging `theirs` (`base` plus rubric `R02` and claim `C02`) as
//! `peer`. `ours-ara` is the retained private journal of that merge commit; it
//! names `rubric/requirements.md` as a transaction target.
#![cfg(unix)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

const PREPARED: &str = ".ara/transactions/active.json.prepared";
const COMMITTED: &str = ".ara/transactions/active.json.committed";

fn ara(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command
        .arg("-C")
        .arg(root)
        .env_remove("ARA_DIR")
        .env_remove("ARA_NO_DUPLICATE_CHECK");
    command
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/rubric-aliases")
        .join(name)
}
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}
fn copied(name: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    copy_dir(&fixture(name), dir.path());
    dir
}
/// Git does not keep the journal's private modes; restore them.
fn private(path: &Path) {
    let mode = if path.is_dir() { 0o700 } else { 0o600 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    if path.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            private(&entry.unwrap().path());
        }
    }
}
/// `ours` with the merge's retained committed journal.
fn ours_with_journal() -> TempDir {
    let dir = copied("ours");
    copy_dir(&fixture("ours-ara"), &dir.path().join(".ara"));
    private(&dir.path().join(".ara/transactions"));
    assert!(dir.path().join(COMMITTED).exists());
    dir
}
fn read(root: &Path, path: &str) -> Vec<u8> {
    fs::read(root.join(path)).unwrap()
}
fn run(root: &Path, args: &[&str]) -> Value {
    let output = ara(root).args(args).arg("--json").output().unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn merge(ours: &Path, base: &Path, theirs: &Path, label: &str) -> (Option<i32>, Value) {
    let output = ara(ours)
        .arg("merge")
        .arg("--base")
        .arg(base)
        .arg("--theirs")
        .arg(theirs)
        .args(["--as", label, "--source-key", &format!("{label}-fork")])
        .arg("--json")
        .output()
        .unwrap();
    let bytes = if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    };
    (
        output.status.code(),
        serde_json::from_slice(&bytes).unwrap(),
    )
}
/// Merge reports carry full byte evidence; failure messages keep the shape.
fn brief(report: &Value) -> String {
    let conflicts = report["conflicts"].as_array().map(|rows| {
        rows.iter()
            .map(|row| format!("{}:{}", row["kind"], row["path"]))
            .collect::<Vec<_>>()
    });
    format!(
        "{} {} {:?}",
        report["error"]["code"], report["error"]["message"], conflicts
    )
}
fn set_conditions(root: &Path) {
    run(
        root,
        &["claim", "set", "C01", "--set", "Conditions=Later boundary."],
    );
    let claims = String::from_utf8(read(root, "logic/claims.md")).unwrap();
    assert!(claims.contains("Later boundary."));
}

#[test]
fn retained_committed_rubric_journal_does_not_block_writes_or_merges() {
    let dir = ours_with_journal();
    let rubric = read(dir.path(), "rubric/requirements.md");
    set_conditions(dir.path());
    assert_eq!(read(dir.path(), "rubric/requirements.md"), rubric);
    let journal = String::from_utf8(read(dir.path(), COMMITTED)).unwrap();
    assert!(!journal.contains("rubric/"), "{journal}");

    let dir = ours_with_journal();
    let theirs = copied("base");
    let (code, report) = merge(dir.path(), &fixture("base"), theirs.path(), "dave");
    assert_eq!(code, Some(0), "{}", brief(&report));
    assert_eq!(read(dir.path(), "rubric/requirements.md"), rubric);
}

#[test]
fn prepared_rubric_journal_rolls_back_to_the_preimage() {
    // The merge transaction interrupted just before its commit rename.
    let dir = ours_with_journal();
    fs::rename(dir.path().join(COMMITTED), dir.path().join(PREPARED)).unwrap();
    set_conditions(dir.path());
    assert!(!dir.path().join(PREPARED).exists());
    assert_eq!(
        read(dir.path(), "rubric/requirements.md"),
        read(&fixture("base"), "rubric/requirements.md")
    );
    assert!(!dir.path().join("trace/aliases.yaml").exists());
}

#[test]
fn rubric_aliases_stay_exact_history_and_scoped_reads_resolve() {
    let dir = copied("ours");
    let aliases = read(dir.path(), "trace/aliases.yaml");
    for address in ["peer:C01", "peer:C02"] {
        let shown = run(dir.path(), &["show", address]);
        assert_eq!(shown["entries"].as_array().unwrap().len(), 1, "{shown}");
    }
    assert_eq!(run(dir.path(), &["resolve", "peer:C02"])["id"], "C02");
    let output = ara(dir.path())
        .args([
            "resolve",
            "peer:rubric/requirements.md#Requirements/R02: Added",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    set_conditions(dir.path());
    assert_eq!(read(dir.path(), "trace/aliases.yaml"), aliases);
    let theirs = copied("base");
    let (code, report) = merge(dir.path(), &fixture("base"), theirs.path(), "dave");
    assert_eq!(code, Some(0), "{}", brief(&report));
}

#[test]
fn incoming_peer_aliases_that_name_rubric_entries_merge_without_dangling() {
    let destination = copied("base");
    let rubric = read(destination.path(), "rubric/requirements.md");
    // Keep the report to the rubric decision; `.gitignore` is unrelated.
    let theirs = copied("ours");
    fs::remove_file(theirs.path().join(".gitignore")).unwrap();
    let (code, report) = merge(destination.path(), &fixture("base"), theirs.path(), "carol");
    assert_eq!(code, Some(1), "{}", brief(&report));
    let conflicts = report["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{}", brief(&report));
    assert_eq!(conflicts[0]["kind"], "external_read_only");
    assert_eq!(conflicts[0]["path"], "rubric/requirements.md");
    assert_eq!(read(destination.path(), "rubric/requirements.md"), rubric);
    let shown = run(destination.path(), &["show", "carol:C02"]);
    assert_eq!(shown["entries"].as_array().unwrap().len(), 1, "{shown}");
}
