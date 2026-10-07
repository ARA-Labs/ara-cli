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
    let path = run(dir.path(), &["show", "N02", "--with", "path"]);
    assert_eq!(
        path["entries"][0]["relations"]["path"]
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
        .args(["show", "--identity", "--json"])
        .assert()
        .code(2)
        .stdout("")
        .get_output()
        .stderr
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&argument).unwrap()["format"],
        "ara.show/v1"
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
    let refs =
        run(dir.path(), &["show", "C01", "--with", "refs"])["entries"][0]["relations"]["refs"]
            .clone();
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
        run(
            dir.path(),
            &["show", "logic/concepts.md:範囲", "--with", "refs"]
        )["entries"][0]["relations"]["refs"]["structured"]
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

#[test]
fn combined_relations_preserve_order_documents_and_projection() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Parent");
    let value = run(
        dir.path(),
        &[
            "show",
            "N02",
            "N01",
            "--with",
            "path,refs",
            "--with",
            "parents,path",
        ],
    );
    assert_eq!(value["entries"][0]["id"], "N02");
    assert_eq!(value["entries"][1]["id"], "N01");
    assert_eq!(value["entries"][0]["relations"]["path"][0]["id"], "N01");
    assert_eq!(value["entries"][0]["relations"]["path"][1]["id"], "N02");
    assert_eq!(value["entries"][0]["relations"]["parents"], json!(["N01"]));
    assert_eq!(
        value["entries"][0]["relations"].as_object().unwrap().len(),
        3
    );
    let positional = run(
        dir.path(),
        &["show", "trace/exploration_tree.yaml", "--with", "refs"],
    );
    let document = run(
        dir.path(),
        &[
            "show",
            "--document",
            "trace/exploration_tree.yaml",
            "--with",
            "refs",
        ],
    );
    assert_eq!(
        positional["entries"][0]["relations"],
        document["entries"][0]["relations"]
    );
    let projected = run(
        dir.path(),
        &["show", "C01", "--with", "refs", "--fields", "relations"],
    );
    assert!(
        projected["entries"][0]["relations"]["refs"]["structured"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == "N02")
    );
}

#[test]
fn incompatible_relations_reject_the_entire_selection() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Parent");
    for args in [
        vec!["show", "N02", "C01", "--with", "path"],
        vec![
            "show",
            "--document",
            "logic/claims.md",
            "--heading",
            "C01",
            "--with",
            "refs",
        ],
        vec!["show", "--document", "logic/claims.md", "--with", "parents"],
        vec!["show", "logic/claims.md", "--source", "--with", "refs"],
    ] {
        let output = ara()
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .arg("--json")
            .assert()
            .code(1)
            .stdout("")
            .get_output()
            .clone();
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stderr).unwrap()["error"]["code"],
            "invalid_selector"
        );
    }
}

#[test]
fn unfinished_filters_intersect_without_document_fallback() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Parent");
    let rows = run(
        dir.path(),
        &[
            "ls",
            "--unfinished",
            "--type",
            "question",
            "--fields",
            "reasons,source_refs",
        ],
    );
    assert_eq!(rows["entries"].as_array().unwrap().len(), 1);
    assert_eq!(rows["entries"][0]["id"], "N03");
    assert_eq!(rows["entries"][0]["reasons"], json!(["childless_question"]));
    assert_eq!(rows["entries"][0]["source_refs"], json!([]));
    for args in [
        vec!["ls", "--unfinished", "--under", "N01"],
        vec![
            "ls",
            "logic/claims.md",
            "--unfinished",
            "--status",
            "confirmed",
            "--fields",
            "reasons,history_status,evidence_sources",
        ],
        vec![
            "ls",
            "--unfinished",
            "--since",
            "2026-10-02",
            "--type",
            "question",
            "--fields",
            "reasons,source_refs",
        ],
    ] {
        assert_eq!(run(dir.path(), &args)["entries"], json!([]));
    }
}

#[test]
fn unfinished_solution_survives_type_and_document_filters() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Parent");
    std::fs::create_dir_all(dir.path().join("logic/solution")).unwrap();
    std::fs::write(
        dir.path().join("logic/solution/method.md"),
        "# Method\n\nImplementation: [pending]\n",
    )
    .unwrap();
    let unfinished = run(dir.path(), &["ls", "--unfinished"]);
    let recipe = unfinished["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "solution")
        .unwrap();
    assert_eq!(recipe["key"], "logic/solution/method.md");
    assert_eq!(recipe["source"], "logic/solution/method.md");
    assert_eq!(recipe["reasons"], json!(["pending_binding"]));
    let filtered = run(dir.path(), &["ls", "--unfinished", "--type", "solution"]);
    assert_eq!(filtered["entries"], json!([recipe]));
    let scoped = run(
        dir.path(),
        &[
            "ls",
            "logic/solution/method.md",
            "--unfinished",
            "--type",
            "solution",
        ],
    );
    assert_eq!(scoped["entries"], filtered["entries"]);
}

#[test]
fn relation_budget_rejects_complete_metadata_and_never_drops_relations() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Parent");
    let expected =
        run(dir.path(), &["show", "C01", "--with", "refs"])["entries"][0]["relations"].clone();
    let bounded = run(
        dir.path(),
        &["show", "C01", "--with", "refs", "--max-bytes", "100000"],
    );
    assert_eq!(bounded["entries"][0]["relations"], expected);
    let output = ara()
        .arg("-C")
        .arg(dir.path())
        .args([
            "show",
            "C01",
            "--with",
            "refs",
            "--max-bytes",
            "1",
            "--json",
        ])
        .assert()
        .code(1)
        .stdout("")
        .get_output()
        .clone();
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["error"]["code"], "output_limit_too_small");
    assert!(
        error["error"]["details"]["hint"]
            .as_str()
            .unwrap()
            .contains("--max-bytes")
    );
}

#[test]
fn identity_address_count_rejects_before_artifact_discovery() {
    let dir = TempDir::new().unwrap();
    let empty = dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let pending = dir.path().join("pending");
    std::fs::create_dir_all(&pending).unwrap();
    artifact(&pending, "Pending");
    let marker = pending.join(".ara/transactions/active.json.prepared");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, r#"{"format":"ara.transaction/v1","entries":[]}"#).unwrap();
    for root in [empty, dir.path().join("missing"), pending] {
        for args in [
            vec!["show", "--identity", "--json"],
            vec!["show", "--identity", "trace:N01", "trace:N02", "--json"],
        ] {
            let output = ara()
                .arg("-C")
                .arg(&root)
                .args(args)
                .assert()
                .code(2)
                .stdout("")
                .get_output()
                .clone();
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stderr).unwrap()["error"]["code"],
                "argument_error",
            );
        }
        assert!(!root.join(".ara/lock").exists());
    }
    assert_eq!(
        std::fs::read_to_string(marker).unwrap(),
        r#"{"format":"ara.transaction/v1","entries":[]}"#
    );
}

#[test]
fn identity_lookup_does_not_require_a_representable_current_body() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Parent");
    std::fs::write(
        dir.path().join("trace/exploration_tree.yaml"),
        "tree:\n  - id: N01\n    type: question\n    title: Retained identity\n    description: {opaque: invalid-shape}\n",
    )
    .unwrap();
    let before = std::fs::read(dir.path().join("trace/exploration_tree.yaml")).unwrap();
    ara()
        .arg("-C")
        .arg(dir.path())
        .args(["show", "N01", "--json"])
        .assert()
        .code(1)
        .stdout("");
    let row = run(
        dir.path(),
        &[
            "show",
            "--identity",
            "trace:N01",
            "--fields",
            "kind",
            "--full",
        ],
    )["entries"][0]
        .clone();
    assert_eq!(
        row,
        json!({"kind":"identity","requested_address":"trace:N01","resolved_target":"N01"})
    );
    let unknown = ara()
        .arg("-C")
        .arg(dir.path())
        .args(["show", "--identity", "trace:N99", "--json"])
        .assert()
        .code(1)
        .stdout("")
        .get_output()
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&unknown.stderr).unwrap()["error"]["code"],
        "merge.unknown_identity"
    );
    for args in [
        vec!["show", "--identity"],
        vec!["show", "--identity", "trace:N01", "trace:N02"],
        vec![
            "show",
            "--identity",
            "trace:N01",
            "--document",
            "logic/claims.md",
        ],
        vec!["show", "--identity", "trace:N01", "--with", "refs"],
    ] {
        ara()
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .arg("--json")
            .assert()
            .code(2)
            .stdout("");
    }
    let bounded = ara()
        .arg("-C")
        .arg(dir.path())
        .args([
            "show",
            "--identity",
            "trace:N01",
            "--max-bytes",
            "1",
            "--json",
        ])
        .assert()
        .code(1)
        .stdout("")
        .get_output()
        .clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&bounded.stderr).unwrap()["error"]["code"],
        "output_limit_too_small"
    );
    assert_eq!(
        std::fs::read(dir.path().join("trace/exploration_tree.yaml")).unwrap(),
        before
    );
}

#[test]
fn unfinished_unknown_inactivity_survives_filters_and_reads_write_nothing() {
    let dir = TempDir::new().unwrap();
    artifact(dir.path(), "Parent");
    std::fs::create_dir_all(dir.path().join("staging")).unwrap();
    let staging = dir.path().join("staging/observations.yaml");
    std::fs::write(&staging, "observations:\n  - id: O01\n    content: Missing timestamp and history\n    provenance: user\n    bound_to: [N02]\n    promoted: false\n    stale: true\n  - id: O02\n    content: Other work\n    provenance: ai-suggested\n    bound_to: [N03]\n    promoted: false\n").unwrap();
    let paths = [
        "trace/exploration_tree.yaml",
        "logic/claims.md",
        "staging/observations.yaml",
    ];
    let before: Vec<_> = paths
        .iter()
        .map(|path| std::fs::read(dir.path().join(path)).unwrap())
        .collect();
    let full = run(dir.path(), &["ls", "--unfinished", "--type", "observation"]);
    let filtered = run(
        dir.path(),
        &[
            "ls",
            "staging/observations.yaml",
            "--unfinished",
            "--type",
            "observation",
            "--provenance",
            "user",
        ],
    );
    assert_eq!(filtered["entries"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["entries"][0], full["entries"][0]);
    let row = &filtered["entries"][0];
    assert_eq!(row["id"], "O01");
    assert_eq!(row["turns_since_reference"], Value::Null);
    assert_eq!(row["session_days_since_reference"], Value::Null);
    assert_ne!(row["history_status"], "complete");
    assert!(
        row["history_diagnostics"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
    assert!(
        row["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("stale_observation"))
    );
    assert_eq!(
        run(dir.path(), &["ls", "--unfinished", "--under", "N02"])["entries"],
        json!([])
    );
    let after: Vec<_> = paths
        .iter()
        .map(|path| std::fs::read(dir.path().join(path)).unwrap())
        .collect();
    assert_eq!(before, after);
    assert!(!dir.path().join(".ara").exists());
}
