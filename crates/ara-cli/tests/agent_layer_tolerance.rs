//! Optional-layer tolerance (#128): typed entry headings accept dash
//! separators and a bracketed status, and an `ARA217`/`ARA218` warning about
//! one optional-layer file no longer hides the rest of the artifact. Such a
//! read prints its result, names the file under `incomplete`, and exits 1.
use assert_cmd::Command;
use serde_json::{Value, json};
use std::path::Path;
use tempfile::TempDir;

const TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: Q\n";
const DUPLICATE_OBSERVATIONS: &str =
    "observations:\n  - id: O01\n    content: a\n  - id: O01\n    content: b\n";

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
/// A JSON read's stdout, asserting its exit code.
fn read(root: &Path, args: &[&str], code: i32) -> Value {
    let output = ara()
        .arg("-C")
        .arg(root)
        .args(args)
        .arg("--json")
        .assert()
        .code(code)
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap()
}
fn run(root: &Path, args: &[&str]) -> Value {
    read(root, args, 0)
}
/// A read that went through with an incomplete optional layer.
fn partial(root: &Path, args: &[&str]) -> Value {
    let value = read(root, args, 1);
    assert_eq!(value["complete"], false, "{value}");
    value
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
fn incomplete_paths(result: &Value) -> Vec<&str> {
    result["incomplete"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect()
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
    assert!(listed.get("complete").is_none() && listed.get("incomplete").is_none());
    for (id, title) in [
        ("E01", "Baseline vs. seed (upper bound)"),
        ("E02", "Hyphen separated"),
        ("H03", "Track the sanity metric"),
    ] {
        assert_eq!(entry(&listed, id)["title"], title, "{id}");
    }
    // Bare-ID lookup resolves through the shared heading parser too.
    let shown = run(dir.path(), &["show", "H03"]);
    assert!(
        entry(&shown, "H03")["body"]
            .as_str()
            .unwrap()
            .starts_with("## H03 \u{2013} Track")
    );
}

#[test]
fn hyphenated_title_word_is_not_a_separator() {
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
fn bracketed_status_is_kept_unless_a_status_field_exists() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        (
            "logic/experiments.md",
            "# Experiments\n\n\
             ## E02 [in progress]: Reception analysis\n- **Setup**: y\n\n\
             ## E03 [planned] \u{2014} Overridden\n- **Status**: done\n",
        ),
    ]);
    let shown = run(dir.path(), &["show", "E02", "E03"]);
    let e02 = entry(&shown, "E02");
    assert_eq!(e02["title"], "Reception analysis");
    assert_eq!(e02["status"], "in progress");
    assert!(
        e02["body"]
            .as_str()
            .unwrap()
            .starts_with("## E02 [in progress]: Reception analysis")
    );
    let e03 = entry(&shown, "E03");
    assert_eq!(e03["title"], "Overridden");
    assert_eq!(e03["status"], "done");
}

#[test]
fn malformed_entry_heading_reads_through_with_its_line() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        (
            "logic/experiments.md",
            "# Experiments\n\n## E01: Kept\n- **Setup**: x\n\n## E2x: Dropped\n",
        ),
    ]);
    let listed = partial(dir.path(), &["ls"]);
    assert_eq!(entry(&listed, "E01")["title"], "Kept");
    assert_eq!(entry(&listed, "N01")["title"], "Q");
    assert_eq!(
        listed["incomplete"],
        json!([{
            "path": "logic/experiments.md",
            "codes": ["ARA217"],
            "reason": "malformed entry heading `E2x: Dropped`",
        }])
    );
    let warning = &listed["diagnostics"]["warnings"][0];
    assert_eq!(warning["path"], "logic/experiments.md:6");
}

#[test]
fn duplicate_observation_id_does_not_hide_the_tree() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        ("staging/observations.yaml", DUPLICATE_OBSERVATIONS),
    ]);
    let shown = partial(dir.path(), &["show", "N01"]);
    assert_eq!(entry(&shown, "N01")["title"], "Q");
    assert!(
        warning_codes(&shown).contains(&"ARA218".to_owned()),
        "{shown}"
    );
    assert_eq!(incomplete_paths(&shown), ["staging/observations.yaml"]);
    // Both records stay readable.
    let listed = partial(dir.path(), &["ls", "--type", "observation"]);
    let contents: Vec<&str> = listed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["id"] == "O01")
        .map(|row| row["content"].as_str().unwrap())
        .collect();
    assert_eq!(contents, ["a", "b"]);
    let found = partial(dir.path(), &["find", "Q"]);
    assert_eq!(incomplete_paths(&found), ["staging/observations.yaml"]);
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
    let listed = partial(dir.path(), &["ls"]);
    assert_eq!(entry(&listed, "N01")["title"], "Q");
    assert!(
        warning_codes(&listed).contains(&"ARA217".to_owned()),
        "{listed}"
    );
    assert_eq!(
        incomplete_paths(&listed),
        ["trace/sessions/2026-10-16_001.yaml"]
    );
}

#[test]
fn non_text_promoted_to_does_not_hide_the_tree() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        (
            "staging/observations.yaml",
            "observations:\n  - id: O01\n    content: a\n    promoted_to: 3\n",
        ),
    ]);
    let listed = partial(dir.path(), &["ls"]);
    assert_eq!(entry(&listed, "O01")["content"], "a");
    assert_eq!(incomplete_paths(&listed), ["staging/observations.yaml"]);
}

#[test]
fn brief_reads_name_the_incomplete_file() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        ("staging/observations.yaml", DUPLICATE_OBSERVATIONS),
    ]);
    let output = ara()
        .arg("-C")
        .arg(dir.path())
        .arg("ls")
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("trace/exploration_tree.yaml\tquestion=1")
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("incomplete: staging/observations.yaml (ARA218)"),
        "{stderr}"
    );
}

#[test]
fn status_keeps_counts_and_withholds_only_the_affected_next_id() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        (
            "logic/experiments.md",
            "# Experiments\n\n## E01: Kept\n- **Setup**: x\n\n## E2x: Dropped\n",
        ),
    ]);
    let status = read(dir.path(), &["status"], 1);
    assert_eq!(status["complete"], false);
    assert_eq!(status["counts"]["experiment_plan"], 1);
    assert_eq!(status["next_ids"]["N"], "N02");
    assert_eq!(status["next_ids"]["E"], Value::Null);
    assert_eq!(
        status["next_id_errors"]["E"]["code"],
        "next_id.incomplete_layer"
    );
    assert_eq!(incomplete_paths(&status), ["logic/experiments.md"]);
}

#[test]
fn node_annotation_warnings_still_refuse() {
    let dir = artifact(&[(
        "trace/exploration_tree.yaml",
        "tree:\n  - id: N01\n    type: question\n    title: Q\n    same_as: 7\n",
    )]);
    let output = ara()
        .arg("-C")
        .arg(dir.path())
        .args(["ls", "--json"])
        .assert()
        .code(1)
        .get_output()
        .clone();
    assert!(output.stdout.is_empty());
    let refused: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(refused["error"]["code"], "incomplete_artifact");
    assert_eq!(refused["error"]["details"]["blocking"], json!(["ARA222"]));
}
