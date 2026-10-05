//! Structural reads of complete-but-invalid artifacts, retained refusals,
//! stray-fence claims recovery and dash claim separators.
use assert_cmd::Command;
use serde_json::{Value, json};
use std::path::Path;
use tempfile::TempDir;

const CLAIM_FIELDS: [&str; 7] = [
    "statement=x",
    "conditions=y",
    "proof=E01",
    "falsification=z",
    "status=hypothesis",
    "dependencies=[]",
    "provenance=ai-suggested",
];

/// Artifact files as (relative path, contents).
type Files = Vec<(&'static str, &'static str)>;

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
fn fail(root: &Path, args: &[&str], code: i32) -> Value {
    let output = ara()
        .arg("-C")
        .arg(root)
        .args(args)
        .arg("--json")
        .assert()
        .code(code)
        .get_output()
        .clone();
    serde_json::from_slice(&output.stderr).unwrap()
}
fn codes(diagnostics: &Value, bucket: &str) -> Vec<String> {
    diagnostics[bucket]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap().to_owned())
        .collect()
}
fn ids(rows: &Value) -> Vec<String> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap_or_default().to_owned())
        .collect()
}
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push((
                    path.strip_prefix(root).unwrap().display().to_string(),
                    std::fs::read(&path).unwrap(),
                ));
            }
        }
    }
    files.sort();
    files
}

const DANGLING_TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: Root\n    children:\n      - id: N02\n        type: experiment\n        title: Mechanism probe\n        evidence: [C01, C99]\n        also_depends_on: [N99]\n  - id: N03\n    type: question\n    title: Other\n";
const DANGLING_CLAIMS: &str = "# Claims\n\n## C01: Mechanism\n- **Statement**: Known mechanism.\n- **Status**: hypothesis\n- **Dependencies**: [C98]\n";

#[test]
fn complete_but_invalid_artifact_reads_with_original_diagnostics() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", DANGLING_TREE),
        ("logic/claims.md", DANGLING_CLAIMS),
    ]);
    let root = dir.path();
    let before = snapshot(root);
    let expected = ["ARA107", "ARA108", "ARA109"];

    let listed = run(root, &["ls"]);
    assert_eq!(ids(&listed["entries"])[..4], ["N01", "N02", "N03", "C01"]);
    let mut errors = codes(&listed["diagnostics"], "errors");
    errors.sort();
    assert_eq!(errors, expected);
    assert!(
        listed["diagnostics"]["errors"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["severity"] == "error")
    );

    // Relationship reads never invent the missing claim or node.
    let shown = run(root, &["show", "N02", "--with", "claims,depends_on"]);
    assert_eq!(shown["entries"][0]["relations"]["claims"], json!(["C01"]));
    assert_eq!(shown["entries"][0]["relations"]["depends_on"], json!([]));
    let path = run(root, &["path", "N02"]);
    assert_eq!(ids(&path["steps"]), ["N01", "N02"]);
    assert_eq!(codes(&path["diagnostics"], "errors").len(), 3);
    let refs = run(root, &["refs", "C01"]);
    assert!(ids(&refs["structured"]).contains(&"N02".to_owned()));
    assert_eq!(
        fail(root, &["refs", "C99"], 1)["error"]["code"],
        "unknown_id"
    );
    assert_eq!(
        fail(root, &["show", "N99"], 1)["error"]["code"],
        "unknown_id"
    );
    let under = run(root, &["ls", "--under", "N01"]);
    assert_eq!(ids(&under["entries"]), ["N02"]);
    let found = run(root, &["find", "mechanism"]);
    assert!(!found["results"].as_array().unwrap().is_empty());
    assert_eq!(codes(&found["diagnostics"], "errors").len(), 3);
    let open = run(root, &["open"]);
    assert_eq!(codes(&open["diagnostics"], "errors").len(), 3);

    // Validity is unchanged: status, check and validate still report invalid.
    let status = ara()
        .arg("-C")
        .arg(root)
        .args(["status", "--json"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let status: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(status["complete"], false);
    assert_eq!(status["counts"], Value::Null);
    assert_eq!(status["next_ids"], Value::Null);
    assert_eq!(status["diagnostics"]["errors"], 3);
    ara().arg("check").arg(root).assert().code(1);
    ara().arg("validate").arg(root).assert().failure();

    // A write command's own artifact load stays strict.
    let refused = fail(root, &["session", "log", "--summary", "x"], 1);
    assert_eq!(refused["error"]["code"], "invalid_artifact");
    assert_eq!(
        snapshot(root),
        before,
        "reads and refusals leave sources unchanged"
    );
}

#[test]
fn retained_refusals_name_their_blocking_codes() {
    let q = "tree:\n  - id: N01\n    type: question\n";
    let cases: Vec<(&str, &str, Files)> = vec![
        (
            "invalid_artifact",
            "ARA104",
            vec![("trace/exploration_tree.yaml", "tree:\n  - type: question\n")],
        ),
        (
            "invalid_artifact",
            "ARA105",
            vec![(
                "trace/exploration_tree.yaml",
                "tree:\n  - id: N01\n    type: question\n  - id: N01\n    type: insight\n    evidence: [C99]\n",
            )],
        ),
        (
            "invalid_artifact",
            "ARA106",
            vec![
                ("trace/exploration_tree.yaml", q),
                ("logic/claims.md", "## C01: A\n## C01 \u{2014} B\n"),
            ],
        ),
        (
            "invalid_artifact",
            "ARA110",
            vec![(
                "trace/exploration_tree.yaml",
                "tree:\n  - id: N01\n    type: question\n    also_depends_on: [N02]\n  - id: N02\n    type: question\n    also_depends_on: [N01]\n",
            )],
        ),
        (
            "invalid_artifact",
            "ARA111",
            vec![(
                "trace/exploration_tree.yaml",
                "tree:\n  - id: N01\n    type: question\n    parent: N99\n",
            )],
        ),
        (
            "invalid_artifact",
            "ARA100",
            vec![("trace/exploration_tree.yaml", "tree: not-a-list\n")],
        ),
        (
            "incomplete_artifact",
            "ARA217",
            vec![
                ("trace/exploration_tree.yaml", q),
                ("staging/observations.yaml", "observations: wrong-shape\n"),
            ],
        ),
    ];
    for (error, code, files) in cases {
        let dir = artifact(&files);
        for read in [
            &["ls"][..],
            &["show", "N01"],
            &["path", "N01"],
            &["refs", "N01"],
            &["open"],
            &["find", "question"],
        ] {
            let refused = fail(dir.path(), read, 1);
            assert_eq!(refused["error"]["code"], error, "{code} {read:?}");
            let blocking = &refused["error"]["details"]["blocking"];
            assert_eq!(blocking, &json!([code]), "{code} {read:?}");
            assert!(
                refused["error"]["details"]["hint"]
                    .as_str()
                    .unwrap()
                    .contains("ara check"),
                "{code}"
            );
        }
    }
}

#[test]
fn explicit_source_reads_do_not_claim_validation() {
    let dir = artifact(&[
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: question\n  - id: N01\n    type: question\n",
        ),
        ("logic/claims.md", DANGLING_CLAIMS),
    ]);
    let shown = run(
        dir.path(),
        &[
            "show",
            "--document",
            "logic/claims.md",
            "--source",
            "--heading",
            "C01",
        ],
    );
    assert_eq!(shown["artifact_validation"], "not_run");
    assert_eq!(
        shown["entries"][0]["heading_path"],
        json!(["Claims", "C01: Mechanism"])
    );
}

#[test]
fn stray_fence_claims_read_natively_and_writes_stay_strict() {
    let claims = "---\n# Claims\n\n## C01: First\n- **Statement**: a\n\n## C02: Second\n- **Statement**: b\n";
    let dir = artifact(&[
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: experiment\n    evidence: [C01, C02]\n",
        ),
        ("logic/claims.md", claims),
    ]);
    let root = dir.path();
    let shown = run(root, &["show", "C02"]);
    assert_eq!(shown["entries"][0]["title"], "Second");
    assert_eq!(codes(&shown["diagnostics"], "errors"), Vec::<String>::new());
    assert_eq!(codes(&shown["diagnostics"], "warnings"), ["ARA228"]);
    assert_eq!(
        shown["diagnostics"]["warnings"][0]["path"],
        "logic/claims.md:1"
    );
    let section = run(
        root,
        &["show", "--document", "logic/claims.md", "--heading", "C02"],
    );
    assert_eq!(
        section["entries"][0]["heading_path"],
        json!(["Claims", "C02: Second"])
    );
    let source = run(
        root,
        &[
            "show",
            "--document",
            "logic/claims.md",
            "--source",
            "--heading",
            "C01",
        ],
    );
    assert_eq!(source["artifact_validation"], "not_run");
    assert!(ids(&run(root, &["refs", "C01"])["structured"]).contains(&"N01".to_owned()));
    let status = run(root, &["status"]);
    assert_eq!(status["complete"], true);
    assert_eq!(status["counts"]["claim"], 2);

    // A warning-only stray fence follows check's warning exit policy.
    ara().arg("check").arg(root).assert().code(0);
    ara()
        .arg("check")
        .arg(root)
        .arg("--strict")
        .assert()
        .code(1);

    let before = snapshot(root);
    let mut add = vec!["claim", "add", "--title", "Third"];
    for field in CLAIM_FIELDS {
        add.extend(["--set", field]);
    }
    let refused = fail(root, &add, 1);
    assert_eq!(refused["error"]["code"], "write.frontmatter");
    let request = root.join("../request.jsonl");
    std::fs::write(
        &request,
        r#"{"op":"entry.edit","target":{"id":"C01"},"set":{"Statement":"changed"}}"#,
    )
    .unwrap();
    for mode in [&["--dry-run"][..], &[]] {
        let mut args = vec!["apply", request.to_str().unwrap()];
        args.extend(mode);
        assert_eq!(fail(root, &args, 1)["error"]["code"], "write.frontmatter");
    }
    std::fs::remove_file(&request).unwrap();
    assert_eq!(
        snapshot(root)
            .into_iter()
            .filter(|(path, _)| !path.starts_with(".ara"))
            .collect::<Vec<_>>(),
        before
    );
}

#[test]
fn dangling_references_refuse_when_a_typed_document_lost_entries() {
    let tree = "tree:\n  - id: N01\n    type: experiment\n    evidence: [C01]\n";
    let claim = "## C01: A\n- **Statement**: a\n";
    let cases = [
        // A protected fence hides every claim.
        (
            vec![(
                "logic/claims.md",
                "---\ntitle: Paper\n# Claims\n\n## C01: A\n- **Statement**: x\n",
            )],
            "logic/claims.md:1",
        ),
        // An unspaced dash drops the claim heading.
        (
            vec![(
                "logic/claims.md",
                "# Claims\n\n## C01\u{2014}Speedup\n- **Statement**: x\n",
            )],
            "logic/claims.md:3",
        ),
        // An unclosed code fence swallows a later claim.
        (
            vec![(
                "logic/claims.md",
                "## C00: Other\n- **Statement**: y\n\n```\n## C01: A\n- **Statement**: x\n",
            )],
            "logic/claims.md:5",
        ),
        // Lowercase, bold, level-three and spaced IDs are not claim headings.
        (
            vec![("logic/claims.md", "## c01 \u{2014} A\n- **Statement**: x\n")],
            "logic/claims.md:1",
        ),
        (
            vec![("logic/claims.md", "## **C01**: A\n- **Statement**: x\n")],
            "logic/claims.md:1",
        ),
        (
            vec![(
                "logic/claims.md",
                "# Claims\n### C01: A\n- **Statement**: x\n",
            )],
            "logic/claims.md:2",
        ),
        (
            vec![("logic/claims.md", "## C 01: A\n- **Statement**: x\n")],
            "logic/claims.md:1",
        ),
        // A protected fence on another typed document hides its entries.
        (
            vec![
                ("logic/claims.md", "# Claims\n## C02: B\n"),
                (
                    "logic/solution/heuristics.md",
                    "---\ntitle: x\n## H01: Hidden\n",
                ),
            ],
            "logic/solution/heuristics.md:1",
        ),
    ];
    for (documents, location) in cases {
        let mut files = vec![("trace/exploration_tree.yaml", tree)];
        files.extend(documents);
        let dir = artifact(&files);
        for read in [&["ls"][..], &["show", "N01"], &["find", "speedup"]] {
            let refused = fail(dir.path(), read, 1);
            assert_eq!(refused["error"]["code"], "invalid_artifact", "{location}");
            let details = &refused["error"]["details"];
            assert_eq!(details["blocking"], json!(["ARA107"]), "{location}");
            assert!(
                details["unrepresented"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|reason| reason.as_str().unwrap().starts_with(location)),
                "{details}"
            );
        }
    }

    // A genuinely absent claim still reads through: prose mentions, a
    // `Dependencies` value and an unrelated `## Notes` heading are not headings
    // naming it.
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", tree),
        (
            "logic/claims.md",
            "# Claims\n## Notes\nC01 was withdrawn.\n## C02: B\n- **Statement**: b\n- **Dependencies**: [C01]\n## C10: Ten\n",
        ),
    ]);
    let listed = run(dir.path(), &["ls"]);
    let mut errors = codes(&listed["diagnostics"], "errors");
    errors.sort();
    assert_eq!(errors, ["ARA107", "ARA109"]);

    // An untyped document behind a fence has no entries to lose.
    let dir = artifact(&[
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: experiment\n    evidence: [C01, C99]\n",
        ),
        ("logic/claims.md", claim),
        ("logic/problem.md", "---\ntitle: Problem\n## Hidden\nbody\n"),
    ]);
    let listed = run(dir.path(), &["ls"]);
    assert_eq!(codes(&listed["diagnostics"], "errors"), ["ARA107"]);
    assert_eq!(codes(&listed["diagnostics"], "warnings"), ["ARA229"]);
    let miss = fail(
        dir.path(),
        &[
            "show",
            "--document",
            "logic/problem.md",
            "--heading",
            "Hidden",
        ],
        1,
    );
    assert_eq!(miss["error"]["code"], "unknown_id");
}

#[test]
fn dash_claim_headings_resolve_without_rewriting_source() {
    let claims = "# Claims\n\n## C01: Colon\n- **Statement**: a\n\n## C02 - Hyphen\n- **Statement**: b\n\n## C03 \u{2013} En dash\n- **Statement**: c\n\n## C04 \u{2014} Em dash\n- **Statement**: d\n- **Dependencies**: [C02]\n";
    let dir = artifact(&[
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: experiment\n    evidence: [C02, C03, C04]\n",
        ),
        ("logic/claims.md", claims),
    ]);
    let root = dir.path();
    let status = run(root, &["status"]);
    assert_eq!(status["complete"], true);
    assert_eq!(status["counts"]["claim"], 4);
    assert_eq!(status["next_ids"]["C"], "C05");
    assert_eq!(
        run(root, &["show", "C04"])["entries"][0]["title"],
        "Em dash"
    );
    let section = run(
        root,
        &["show", "--document", "logic/claims.md", "--heading", "C03"],
    );
    assert_eq!(
        section["entries"][0]["heading_path"],
        json!(["Claims", "C03 \u{2013} En dash"])
    );
    let scoped = run(root, &["show", "logic/claims.md#C02", "--full"]);
    assert_eq!(scoped["entries"][0]["id"], "C02");
    assert!(
        scoped["entries"][0]["body"]
            .as_str()
            .unwrap()
            .starts_with("## C02 - Hyphen")
    );
    let refs = run(root, &["refs", "C02"]);
    let owners = ids(&refs["structured"]);
    assert!(owners.contains(&"N01".to_owned()), "{refs}");
    assert!(owners.contains(&"C04".to_owned()), "{refs}");
    assert_eq!(
        std::fs::read_to_string(root.join("logic/claims.md")).unwrap(),
        claims
    );
    // Native dash spellings are neither drift nor missing claims, so check
    // passes and --fix leaves the source alone.
    let check = ara()
        .arg("check")
        .arg(root)
        .arg("--fix")
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let check = String::from_utf8(check).unwrap();
    assert!(!check.contains("ARA004"), "{check}");
    assert!(!check.contains("ARA107"), "{check}");
    assert_eq!(
        std::fs::read_to_string(root.join("logic/claims.md")).unwrap(),
        claims
    );
    // An unspaced dash is still drift that check repairs.
    std::fs::write(
        root.join("logic/claims.md"),
        claims.replace("## C04 \u{2014} Em dash", "## C04\u{2014}Em dash"),
    )
    .unwrap();
    let check = ara()
        .arg("check")
        .arg(root)
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let check = String::from_utf8(check).unwrap();
    assert_eq!(check.matches("ARA004").count(), 1, "{check}");
}

#[test]
fn vendored_em_dash_speedrun_reads_claims() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ara-core/tests/fixtures/corpus/speedrun/nanogpt-speedrun");
    let dir = TempDir::new().unwrap();
    for file in ["trace/exploration_tree.yaml", "logic/claims.md"] {
        let target = dir.path().join(file);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(source.join(file), target).unwrap();
    }
    let status = run(dir.path(), &["status"]);
    assert_eq!(status["complete"], true);
    assert_eq!(status["counts"]["claim"], 10);
    let shown = run(dir.path(), &["show", "C04", "--with", "sessions"]);
    assert_eq!(
        shown["entries"][0]["title"],
        "FlexAttention Enables Document-Aware 64K Context"
    );
    let found = run(dir.path(), &["find", "muon optimizer", "--type", "claim"]);
    assert_eq!(found["results"][0]["id"], "C03");
}

#[test]
fn whole_document_replace_repairs_unclosed_fences() {
    let dir = artifact(&[
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: experiment\n    evidence: [C01]\n",
        ),
        (
            "logic/claims.md",
            "---\n# Claims\n\n## C01: First\n- **Statement**: a\n",
        ),
        (
            "logic/problem.md",
            "---\ntitle: Problem\n## Gap\nOpen gap.\n",
        ),
    ]);
    let root = dir.path();
    let request = root.join("../repair.jsonl");
    for (document, content) in [
        (
            "logic/claims.md",
            "# Claims\n\n## C01: First\n- **Statement**: a\n",
        ),
        ("logic/problem.md", "# Problem\n\n## Gap\nOpen gap.\n"),
    ] {
        let digest =
            run(root, &["show", "--document", document, "--source"])["entries"][0]["digest"]
                .clone();
        let operation = json!({"op":"document.replace","document":document,"expected":digest,"content":content});
        std::fs::write(&request, operation.to_string()).unwrap();
        run(root, &["apply", request.to_str().unwrap(), "--dry-run"]);
        run(root, &["apply", request.to_str().unwrap()]);
        assert_eq!(
            std::fs::read_to_string(root.join(document)).unwrap(),
            content
        );
    }
    std::fs::remove_file(&request).unwrap();
    let status = run(root, &["status"]);
    assert_eq!(status["complete"], true);
    assert_eq!(status["diagnostics"]["warnings"], 0);
    let gap = run(
        root,
        &["show", "--document", "logic/problem.md", "--heading", "Gap"],
    );
    assert_eq!(gap["entries"][0]["heading_path"], json!(["Problem", "Gap"]));
}

/// A whole-document fence repair cannot drop a claim that reads recover
/// behind the stray opener; one that keeps every claim still succeeds.
#[test]
fn fence_repair_retains_recovered_claims() {
    const CLAIMS: &str = "---\n# Claims\n\n## C01: First\n- **Statement**: a\n\n## C02: Second\n- **Statement**: b\n";
    let dir = artifact(&[
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: experiment\n    evidence: [C01]\n",
        ),
        ("logic/claims.md", CLAIMS),
    ]);
    let root = dir.path();
    assert_eq!(
        ids(&run(root, &["ls", "--type", "claim"])["entries"]),
        ["C01", "C02"]
    );
    // A private request directory: sibling tests share the temp root.
    let requests = TempDir::new().unwrap();
    let request = requests.path().join("repair.jsonl");
    let replace = |content: &str| {
        let digest = run(root, &["show", "--document", "logic/claims.md", "--source"])["entries"]
            [0]["digest"]
            .clone();
        let operation = json!({"op":"document.replace","document":"logic/claims.md","expected":digest,"content":content});
        std::fs::write(&request, operation.to_string()).unwrap();
    };

    // Removing the opener and C02 retires a recovered canonical claim.
    replace("# Claims\n\n## C01: First\n- **Statement**: a\n");
    let before = snapshot(root);
    for mode in [&["--dry-run"][..], &[]] {
        let mut args = vec!["apply", request.to_str().unwrap()];
        args.extend(mode);
        assert_eq!(
            fail(root, &args, 1)["error"]["code"],
            "write.claim_retention"
        );
    }
    assert_eq!(
        std::fs::read_to_string(root.join("logic/claims.md")).unwrap(),
        CLAIMS
    );
    assert_eq!(
        snapshot(root)
            .into_iter()
            .filter(|(path, _)| !path.starts_with(".ara"))
            .collect::<Vec<_>>(),
        before
            .into_iter()
            .filter(|(path, _)| !path.starts_with(".ara"))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        ids(&run(root, &["ls", "--type", "claim"])["entries"]),
        ["C01", "C02"]
    );

    // Removing only the stray opener keeps both claims and succeeds.
    let repaired = &CLAIMS["---\n".len()..];
    replace(repaired);
    run(root, &["apply", request.to_str().unwrap()]);
    std::fs::remove_file(&request).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("logic/claims.md")).unwrap(),
        repaired
    );
    assert_eq!(
        ids(&run(root, &["ls", "--type", "claim"])["entries"]),
        ["C01", "C02"]
    );
    assert_eq!(run(root, &["status"])["diagnostics"]["warnings"], 0);
}

#[test]
fn selector_misses_name_a_fence_only_when_it_hides_the_target() {
    let dir = artifact(&[
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: question\n",
        ),
        (
            "logic/claims.md",
            "---\n# Claims\n\n## C01: First\n- **Statement**: a\n",
        ),
    ]);
    let root = dir.path();
    let request = root.join("../edit.jsonl");
    for (target, code) in [
        (json!({"id":"C01"}), "write.frontmatter"),
        (json!({"id":"C99"}), "write.selector"),
        (
            json!({"document":"logic/claims.md","heading":["Claims","C01: First"]}),
            "write.frontmatter",
        ),
        (
            json!({"document":"logic/claims.md","heading":["Missing"]}),
            "write.selector",
        ),
    ] {
        let operation = json!({"op":"entry.edit","target":target,"set":{"Statement":"b"}});
        std::fs::write(&request, operation.to_string()).unwrap();
        let refused = fail(root, &["apply", request.to_str().unwrap(), "--dry-run"], 1);
        assert_eq!(refused["error"]["code"], code, "{target}");
    }
    std::fs::remove_file(&request).unwrap();
}

#[test]
fn dangling_dependency_refuses_when_its_claim_was_swallowed() {
    let dir = artifact(&[
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: question\n",
        ),
        (
            "logic/claims.md",
            "## C02: B\n- **Statement**: b\n- **Dependencies**: [C01]\n\n```\n## C01: A\n- **Statement**: a\n",
        ),
    ]);
    let refused = fail(dir.path(), &["ls"], 1);
    assert_eq!(refused["error"]["code"], "invalid_artifact");
    let details = &refused["error"]["details"];
    assert_eq!(details["blocking"], json!(["ARA109"]));
    assert!(
        details["unrepresented"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason.as_str().unwrap().starts_with("logic/claims.md:6")),
        "{details}"
    );
}

#[test]
fn unclosed_code_fence_hiding_a_claim_refuses_independently_of_dangling_ids() {
    // C99 dangles, so reads take the read-through path; the swallowed C02 is
    // referenced nowhere, so no dangling ID points at it.
    let tree = "tree:\n  - id: N01\n    type: experiment\n    evidence: [C01, C99]\n";
    // Every claim-like spelling counts, including a space-separated title.
    let hidden = [
        "## C02: Hidden",
        "## C02 Hidden",
        "## **C02** Hidden",
        "### c02 - Hidden",
    ];
    let cases = hidden.iter().flat_map(|heading| {
        let body = format!("## C01: A\n- **Statement**: a\n\n```\n{heading}\n- **Statement**: b\n");
        let stray = format!("---\n# Claims\n\n{body}");
        [
            (body, "logic/claims.md:5"),
            // A leading stray opener must not be declared recovered (`ARA228`).
            (stray, "logic/claims.md:1"),
        ]
    });
    for (claims, location) in cases {
        let dir = artifact(&[
            ("trace/exploration_tree.yaml", tree),
            ("logic/claims.md", &claims),
        ]);
        for read in [
            &["ls"][..],
            &["find", "hidden"],
            &["show", "C01"],
            &["show", "N01"],
        ] {
            let refused = fail(dir.path(), read, 1);
            assert_eq!(refused["error"]["code"], "invalid_artifact", "{claims:?}");
            let details = &refused["error"]["details"];
            assert_eq!(details["blocking"], json!(["ARA107"]), "{claims:?}");
            assert!(
                details["unrepresented"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|reason| reason.as_str().unwrap().starts_with(location)),
                "{details}"
            );
        }
        let check = ara()
            .arg("check")
            .arg(dir.path())
            .arg("--json")
            .output()
            .unwrap();
        let check = String::from_utf8_lossy(&check.stdout);
        assert!(
            !check.contains("ARA228"),
            "stray opener declared recovered: {claims:?}"
        );
    }

    // Control: a closed fence holding an example heading stays readable.
    let closed = "---\n# Claims\n\n## C01: A\n- **Statement**: a\n\n```\n## C02: Example\n```\n";
    for claims in [&closed[4..], closed] {
        let dir = artifact(&[
            ("trace/exploration_tree.yaml", tree),
            ("logic/claims.md", claims),
        ]);
        let listed = run(dir.path(), &["ls"]);
        assert_eq!(codes(&listed["diagnostics"], "errors"), ["ARA107"]);
        assert!(ids(&listed["entries"]).contains(&"C01".to_owned()));
        assert_eq!(run(dir.path(), &["show", "C01"])["entries"][0]["id"], "C01");
        assert_eq!(
            fail(dir.path(), &["show", "C02"], 1)["error"]["code"],
            "unknown_id"
        );
    }
}
