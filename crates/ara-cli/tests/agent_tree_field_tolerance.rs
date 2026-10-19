//! #130: one mistyped value in `trace/exploration_tree.yaml` no longer fails
//! the whole tree (`ARA100`). Reads go through and exit 0; a value read as
//! its source text warns `ARA230`.
use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn ara() -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command.env_remove("ARA_DIR");
    command
}
fn tree(node_fields: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("trace/exploration_tree.yaml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let source = format!("tree:\n  - id: N01\n    type: decision\n    title: Q\n{node_fields}");
    std::fs::write(path, source).unwrap();
    dir
}
/// `show N01 --full --json` stdout; panics with stderr on a refusal.
fn show(root: &Path) -> Value {
    let output = ara()
        .arg("-C")
        .arg(root)
        .args(["show", "N01", "--full", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn decision(shown: &Value) -> &Value {
    &shown["entries"][0]["fields"]["decision"]
}

#[test]
fn text_fields_already_keep_scalar_spelling() {
    // Guard: plain text fields take a numeric scalar's source spelling.
    let dir = tree("    rationale: 1e5\n    alternatives: [0x1F, 1.50, true]\n");
    let shown = show(dir.path());
    assert_eq!(decision(&shown)["rationale"], "1e5");
    assert_eq!(
        decision(&shown)["alternatives"],
        serde_json::json!(["0x1F", "1.50", "true"])
    );
}

#[test]
fn all_digit_commit_sha_in_evidence_keeps_its_spelling() {
    let dir = tree("    evidence:\n    - be13e96\n    - 7258910\n    - 0123456\n");
    let shown = show(dir.path());
    let notes = shown["entries"][0]["evidence_notes"].to_string();
    for sha in ["be13e96", "7258910", "0123456"] {
        assert!(notes.contains(sha), "{sha} missing from {notes}");
    }
}

#[test]
fn colon_in_plain_alternative_does_not_fail_the_tree() {
    let dir = tree(
        "    alternatives:\n    - Build on Debian 12 like the task images (rejected: its glibc is above the floor).\n    - Static linking\n",
    );
    let shown = show(dir.path());
    assert_eq!(
        decision(&shown)["alternatives"],
        serde_json::json!([
            "Build on Debian 12 like the task images (rejected: its glibc is above the floor).",
            "Static linking"
        ])
    );
    let warning = &shown["diagnostics"]["warnings"][0];
    assert_eq!(warning["code"], "ARA230");
    assert_eq!(warning["path"], "trace/exploration_tree.yaml:6");
    assert!(shown.get("incomplete").is_none(), "{shown}");
}
