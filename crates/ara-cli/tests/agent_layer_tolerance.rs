//! Reproductions for #128: real ARAs refused over optional agent layers.
//!
//! Typed entries in `logic/experiments.md` and `logic/solution/heuristics.md`
//! should accept the dash separators claims already accept, and a malformed or
//! duplicated optional-layer record should not hide the rest of the artifact
//! from structural reads.
use assert_cmd::Command;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

const TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: Q\n";

fn ara() -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command.env_remove("ARA_DIR");
    command
}
fn artifact(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (rel, body) in files {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    dir
}
fn run(root: &Path, args: &[&str]) -> Value {
    let output = ara()
        .arg("-C")
        .arg(root)
        .args(args)
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap()
}
fn warning_codes(result: &Value) -> Vec<String> {
    result["diagnostics"]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap().to_owned())
        .collect()
}
fn entry<'a>(result: &'a Value, id: &str) -> &'a Value {
    result["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("no entry {id} in {result}"))
}

#[test]
fn dash_separated_typed_entry_headings_parse() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        (
            "logic/experiments.md",
            "# Experiments\n\n\
             ## E01 \u{2014} Baseline vs. seed (upper bound)\n- **Setup**: x\n\n\
             ## E02 - Hyphen separated\n- **Setup**: y\n",
        ),
        (
            "logic/solution/heuristics.md",
            "# Heuristics\n\n## H03 \u{2013} Track the sanity metric\n- **Rationale**: z\n",
        ),
    ]);
    let listed = run(dir.path(), &["ls"]);
    assert_eq!(warning_codes(&listed), Vec::<String>::new(), "{listed}");
    for (id, title) in [
        ("E01", "Baseline vs. seed (upper bound)"),
        ("E02", "Hyphen separated"),
        ("H03", "Track the sanity metric"),
    ] {
        assert_eq!(entry(&listed, id)["title"], title, "{id}");
    }
}

#[test]
fn hyphenated_title_word_is_not_a_separator() {
    // `E01-ablation: Title` is not an entry ID; the dash split must need
    // whitespace on both sides, exactly as for claims.
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        (
            "logic/experiments.md",
            "# Experiments\n\n## E01: Two-stage - warmup\n- **Setup**: x\n",
        ),
    ]);
    let listed = run(dir.path(), &["ls"]);
    assert_eq!(entry(&listed, "E01")["title"], "Two-stage - warmup");
}

#[test]
fn duplicate_observation_id_does_not_hide_the_tree() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        (
            "staging/observations.yaml",
            "observations:\n  - id: O01\n    content: a\n  - id: O01\n    content: b\n",
        ),
    ]);
    let shown = run(dir.path(), &["show", "N01"]);
    assert_eq!(entry(&shown, "N01")["title"], "Q");
    assert!(warning_codes(&shown).contains(&"ARA218".to_owned()), "{shown}");
}

#[test]
fn malformed_session_file_does_not_hide_the_tree() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        (
            "trace/sessions/2026-10-16_001.yaml",
            "session:\n  id: s1\n  id: s1\n",
        ),
    ]);
    let listed = run(dir.path(), &["ls"]);
    assert_eq!(entry(&listed, "N01")["title"], "Q");
    assert!(warning_codes(&listed).contains(&"ARA217".to_owned()), "{listed}");
}
