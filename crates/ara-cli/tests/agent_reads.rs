//! Actual-command contracts: discovery, precise relations, source fidelity and errors.
use assert_cmd::Command;
use serde_json::{Value, json};
use std::path::Path;
use tempfile::TempDir;

fn ara() -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command.env_remove("ARA_DIR");
    command
}
fn artifact(root: &Path, title: &str) {
    std::fs::create_dir_all(root.join("trace")).unwrap();
    std::fs::create_dir_all(root.join("logic")).unwrap();
    std::fs::write(root.join("trace/exploration_tree.yaml"),format!("tree:\n  - id: N01\n    type: question\n    title: {title}\n    timestamp: '2026-10-01'\n    children:\n      - id: N02\n        type: experiment\n        title: Boundary\n        status: planned\n        provenance: ai-executed\n        timestamp: '2026-10-02'\n        evidence: [C01]\n        result: |-\n          exact first line\n          exact second line\n        unfamiliar:\n          nested: retained\n  - id: N03\n    type: question\n    title: Independent\n    also_depends_on: [N02]\n")).unwrap();
    std::fs::write(root.join("logic/claims.md"),"# Claims\n\n## C01: Mechanism\n- **Statement**: Known mechanism.\n- **Conditions**: Boundary.\n- **Status**: hypothesis\n- **Proof**: E02 and exact evidence prose.\n- **Dependencies**: []\n- **Unfamiliar**: preserved prose\n").unwrap();
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
    assert!(output.stderr.is_empty(), "{:?}", output);
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn nesting_filters_paths_and_relations_do_not_follow_cross_edges() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Parent");
    let listed = run(
        dir.path(),
        &[
            "ls",
            "--under",
            "N01",
            "--type",
            "experiment",
            "--status",
            "planned",
            "--provenance",
            "ai-executed",
            "--since",
            "2026-10-02",
        ],
    );
    assert_eq!(listed["entries"][0]["id"], "N02");
    assert_eq!(listed["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        run(dir.path(), &["ls", "--under", "N03"])["entries"],
        json!([])
    );
    let path = run(dir.path(), &["path", "N02"]);
    assert_eq!(
        path["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["N01", "N02"]
    );
    let shown = run(
        dir.path(),
        &[
            "show",
            "N02",
            "N01",
            "--with",
            "parents,children,claims,sessions",
            "--full",
        ],
    );
    assert_eq!(shown["entries"][0]["relations"]["parents"], json!(["N01"]));
    assert_eq!(shown["entries"][0]["relations"]["claims"], json!(["C01"]));
    assert_eq!(shown["entries"][1]["relations"]["children"], json!(["N02"]));
    assert_eq!(
        shown["entries"][0]["source_fields"]["result"],
        "exact first line\nexact second line"
    );
    assert_eq!(
        shown["entries"][0]["source_fields"]["unfamiliar"]["nested"],
        "retained"
    );
    let projected = run(
        dir.path(),
        &[
            "show",
            "C01",
            "--fields",
            "statement,proof_content",
            "--full",
        ],
    );
    assert_eq!(projected["format"], "ara.show/v1");
    assert_eq!(projected["entries"][0]["id"], "C01");
    assert_eq!(
        projected["entries"][0]["proof_content"],
        "E02 and exact evidence prose."
    );
    assert!(projected["entries"][0].get("status").is_none());
    assert!(!dir.path().join(".ara").exists());
}
#[test]
fn discovery_explicit_environment_and_nearest_self_precedence() {
    let parent = TempDir::new().unwrap();
    let outer = parent.path().join("outer");
    let inner = outer.join("work");
    let child = inner.join("ara");
    artifact(&outer, "Outer");
    artifact(&inner, "Inner");
    artifact(&child, "Child");
    std::fs::create_dir_all(inner.join("nested")).unwrap();
    let output = ara()
        .current_dir(inner.join("nested"))
        .args(["show", "N01", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap()["entries"][0]["title"],
        "Inner"
    );
    let output = ara()
        .current_dir(&inner)
        .env("ARA_DIR", "ara")
        .args(["show", "N01", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap()["entries"][0]["title"],
        "Child"
    );
    let output = ara()
        .env("ARA_DIR", &child)
        .arg("-C")
        .arg(&outer)
        .args(["show", "N01", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&output).unwrap()["entries"][0]["title"],
        "Outer"
    );
    ara()
        .current_dir(&inner)
        .env("ARA_DIR", &outer)
        .args(["-C", "missing", "status", "--json"])
        .assert()
        .code(2)
        .stdout("");
}
#[test]
fn missing_selector_argument_and_invalid_artifact_errors_are_separate() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Root");
    let unknown = ara()
        .arg("-C")
        .arg(dir.path())
        .args(["show", "N01", "N999", "--json"])
        .assert()
        .code(1)
        .stdout("")
        .get_output()
        .stderr
        .clone();
    let unknown: Value = serde_json::from_slice(&unknown).unwrap();
    assert_eq!(unknown["error"]["code"], "unknown_id");
    assert_eq!(unknown["error"]["id"], "N999");
    let argument = ara()
        .arg("-C")
        .arg(dir.path())
        .args(["path", "--json"])
        .assert()
        .code(2)
        .stdout("")
        .get_output()
        .stderr
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&argument).unwrap()["format"],
        "ara.path/v1"
    );
    std::fs::write(
        dir.path().join("trace/exploration_tree.yaml"),
        "tree:\n  - id: N01\n    type: question\n  - id: N01\n    type: insight\n",
    )
    .unwrap();
    let status = ara()
        .arg("-C")
        .arg(dir.path())
        .args(["status", "--json"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let status: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(status["complete"], false);
    assert_eq!(status["counts"], Value::Null);
    assert_eq!(status["diagnostics"]["errors"], 1);
    ara()
        .arg("-C")
        .arg(dir.path())
        .args(["ls", "--json"])
        .assert()
        .code(1)
        .stdout("");
}
#[test]
fn source_read_recovers_nested_heading_exact_prose_and_digest() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Root");
    let source = "# Method\n\n## Boundary\n\n### Inner\n\n雪 exact prose\n\n```md\n## not a selector\n```\n\n### Other\nretained\n";
    std::fs::create_dir_all(dir.path().join("logic/solution")).unwrap();
    std::fs::write(dir.path().join("logic/solution/method.md"), source).unwrap();
    let whole = run(
        dir.path(),
        &[
            "show",
            "--document",
            "logic/solution/method.md",
            "--source",
            "--full",
        ],
    );
    assert_eq!(whole["entries"][0]["content"], source);
    let inner = run(
        dir.path(),
        &[
            "show",
            "--document",
            "logic/solution/method.md",
            "--heading",
            "Boundary",
            "--heading",
            "Inner",
            "--source",
            "--full",
        ],
    );
    assert_eq!(
        inner["entries"][0]["content"],
        "\n雪 exact prose\n\n```md\n## not a selector\n```\n\n"
    );
    use sha2::{Digest, Sha256};
    assert_eq!(
        inner["entries"][0]["digest"],
        format!(
            "sha256:{:x}",
            Sha256::digest(inner["entries"][0]["content"].as_str().unwrap().as_bytes())
        )
    );
    ara()
        .arg("-C")
        .arg(dir.path())
        .args(["show", "--document", "../outside.md", "--source", "--json"])
        .assert()
        .code(1)
        .stdout("");
}
#[test]
fn search_matches_native_identity_and_reports_no_matches_and_bad_limits() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Root");
    let result = run(
        dir.path(),
        &["find", "mechanism", "--type", "claim", "--full"],
    );
    assert_eq!(result["results"][0]["id"], "C01");
    assert_eq!(
        result["results"][0]["entry"]["statement"],
        "Known mechanism."
    );
    assert_eq!(
        run(dir.path(), &["find", "zzzzuniqueterm"])["results"],
        json!([])
    );
    ara()
        .arg("-C")
        .arg(dir.path())
        .args(["find", "mechanism", "--limit", "0", "--json"])
        .assert()
        .code(1)
        .stdout("");
    ara()
        .arg("-C")
        .arg(dir.path())
        .args(["ls", "--since", "éé-01-01", "--json"])
        .assert()
        .code(1)
        .stdout("");
}
#[test]
fn prepared_transaction_blocks_reads_but_does_not_create_lock() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Root");
    std::fs::create_dir_all(dir.path().join(".ara/transactions/active.preimages")).unwrap();
    std::fs::write(
        dir.path().join(".ara/transactions/active.json.prepared"),
        r#"{"format":"ara.transaction/v1","entries":[],"created_dirs":["staging"]}"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for relative in [".ara/transactions", ".ara/transactions/active.preimages"] {
            std::fs::set_permissions(
                dir.path().join(relative),
                std::fs::Permissions::from_mode(0o700),
            )
            .unwrap();
        }
    }
    let stderr = ara()
        .arg("-C")
        .arg(dir.path())
        .args(["show", "N01", "--json"])
        .assert()
        .code(2)
        .stdout("")
        .get_output()
        .stderr
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&stderr).unwrap()["error"]["code"],
        "pending_transaction"
    );
    assert!(!dir.path().join(".ara/lock").exists());
}

#[test]
fn structured_refs_have_exact_source_spans_without_duplicate_prose() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Root");
    std::fs::write(
        dir.path().join("trace/taste_log.yaml"),
        "entries:\n  - id: T01\n    target: C01\n    comment: human boundary preference\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("logic/concepts.md"),
        "# Concepts\n\n## 範囲\n- **Definition**: Native Unicode concept.\n- **Related**: C01\n",
    )
    .unwrap();
    let refs = run(dir.path(), &["refs", "C01"]);
    let certain = refs["structured"].as_array().unwrap();
    assert!(
        certain
            .iter()
            .any(|row| row["id"] == "N02" && row["field"] == "evidence")
    );
    assert!(
        certain
            .iter()
            .any(|row| row["id"] == "T01" && row["field"] == "target")
    );
    assert!(
        certain
            .iter()
            .any(|row| row["source"] == "logic/concepts.md" && row["field"] == "Related")
    );
    for row in certain {
        let source =
            std::fs::read_to_string(dir.path().join(row["source"].as_str().unwrap())).unwrap();
        let start = row["range"]["start"].as_u64().unwrap() as usize;
        let end = row["range"]["end"].as_u64().unwrap() as usize;
        assert!(source[start..end].contains("C01"));
        assert!(
            !refs["prose"]
                .as_array()
                .unwrap()
                .iter()
                .any(|other| other["source"] == row["source"]
                    && other["range"]["start"].as_u64().unwrap() as usize >= start
                    && other["range"]["end"].as_u64().unwrap() as usize <= end)
        );
    }
    assert!(
        run(dir.path(), &["refs", "logic/concepts.md:範囲"])["structured"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn native_annotation_status_filters_preserve_nonexperiment_metadata() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("trace")).unwrap();
    std::fs::write(dir.path().join("trace/exploration_tree.yaml"),"tree:\n  - id: N10\n    type: question\n    title: Closed boundary\n    description: Caller resolved this boundary.\n    status: resolved\n  - id: N20\n    type: decision\n    title: Conflicting decision\n    choice: Preserve both hypotheses.\n    alternatives: [A, B]\n    status: unresolved\n").unwrap();
    let filtered = run(dir.path(), &["ls", "--status", "unresolved"]);
    assert_eq!(
        filtered["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["N20"]
    );
    let shown = run(dir.path(), &["show", "N10", "N20", "--fields", "status"]);
    assert_eq!(shown["entries"][0]["status"], "resolved");
    assert_eq!(shown["entries"][1]["status"], "unresolved");
    assert!(!dir.path().join(".ara").exists());
}
#[test]
fn source_read_accepts_one_positional_document_path() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Root");
    std::fs::create_dir_all(dir.path().join("logic/solution")).unwrap();
    std::fs::write(
        dir.path().join("logic/solution/method.md"),
        "# Method\n\n## Boundary\nexact prose\n",
    )
    .unwrap();
    for heading in [&[][..], &["--heading", "Boundary"][..]] {
        let named = run(
            dir.path(),
            &[
                &["show", "--document", "logic/solution/method.md", "--source"],
                heading,
            ]
            .concat(),
        );
        let positional = run(
            dir.path(),
            &[&["show", "logic/solution/method.md", "--source"], heading].concat(),
        );
        assert_eq!(positional["entries"], named["entries"], "{heading:?}");
    }
    for (args, code) in [
        (
            &["show", "rubric/requirements.md", "--source"][..],
            "invalid_document",
        ),
        (&["show", "C01", "--source"][..], "invalid_selector"),
        (
            &[
                "show",
                "logic/claims.md",
                "logic/solution/method.md",
                "--source",
            ][..],
            "invalid_selector",
        ),
    ] {
        let output = ara()
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .arg("--json")
            .assert()
            .code(1)
            .get_output()
            .clone();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(text.contains(code), "{args:?}: {text}");
    }
}
