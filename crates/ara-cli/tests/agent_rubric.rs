//! `rubric/` is grading material read with file tools, not native knowledge.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;

const TREE: &str =
    "tree:\n  - id: N01\n    type: question\n    title: Boundary mechanism\n    provenance: user\n";
const CLAIMS: &str = "# Claims\n\n## C01: Existing mechanism\n- **Statement**: Original mechanism.\n- **Status**: hypothesis\n- **Provenance**: user\n";
const RUBRIC: &str = "# Requirements\n\n## R01: Source grounding\n- **Rubric ID**: uuid-1\n- **Requirement**: Quokka grading text.\n\n## R84: Long requirement shortened to sixty characters plus a literal ...\n- **Rubric ID**: uuid-84\n- **Requirement**: Quokka final text.\n";

fn ara(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command
        .arg("-C")
        .arg(root)
        .env_remove("ARA_DIR")
        .env_remove("ARA_NO_DUPLICATE_CHECK");
    command
}
fn put(root: &Path, path: &str, content: impl AsRef<[u8]>) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}
fn fixture(paper: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    put(dir.path(), "trace/exploration_tree.yaml", TREE);
    put(dir.path(), "logic/claims.md", CLAIMS);
    put(dir.path(), "PAPER.md", paper);
    put(dir.path(), "rubric/requirements.md", RUBRIC);
    put(
        dir.path(),
        "evidence/tables/result.md",
        "# Result\nQuokka evidence.\n",
    );
    dir
}
fn plain() -> TempDir {
    fixture("---\ntitle: Rubric fixture\n---\n# Rubric fixture\n")
}
fn run(root: &Path, args: &[&str]) -> Value {
    let output = ara(root)
        .args(args)
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .clone();
    serde_json::from_slice(&output.stdout).unwrap()
}
fn failure(root: &Path, args: &[&str]) -> Value {
    let output = ara(root)
        .args(args)
        .arg("--json")
        .assert()
        .code(1)
        .stdout("")
        .get_output()
        .clone();
    serde_json::from_slice::<Value>(&output.stderr).unwrap()["error"].clone()
}
fn apply_failure(root: &Path, operations: &[Value]) -> Value {
    let text = operations
        .iter()
        .map(|op| format!("{op}\n"))
        .collect::<String>();
    let output = ara(root)
        .args(["apply", "-", "--json", "--no-duplicate-check"])
        .write_stdin(text)
        .assert()
        .code(1)
        .get_output()
        .clone();
    serde_json::from_slice::<Value>(&output.stderr).unwrap()["error"].clone()
}
fn artifact_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|name| name == ".ara") {
                continue;
            }
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}
fn copy_dir(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_dir(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}
/// Records written by the pre-removal binary (`ara` 0.1.23 built from commit
/// 4b0da70): a rubric document, a revised `R01`, `R02` renamed to `R04`, and
/// `R03` removed.
fn history() -> TempDir {
    let dir = TempDir::new().unwrap();
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rubric-history"),
        dir.path(),
    );
    dir
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
fn assert_file_access_hint(error: &Value) {
    assert_eq!(error["code"], "invalid_document", "{error}");
    let roots = error["details"]["file_access"].as_array().unwrap();
    for root in ["rubric/", "evidence/", "src/"] {
        assert!(roots.contains(&json!(root)), "{error}");
    }
    let native = error["details"]["native_documents"].as_array().unwrap();
    assert!(native.contains(&json!("logic/**/*.md")), "{error}");
    assert!(!error["details"]["hint"].as_str().unwrap().is_empty());
}

#[test]
fn rubric_document_reads_return_invalid_document_with_file_access_hint() {
    let dir = plain();
    for args in [
        &["show", "--document", "rubric/requirements.md"][..],
        &["show", "--document", "rubric/requirements.md", "--full"],
        &["show", "--document", "rubric/requirements.md", "--source"],
        &[
            "show",
            "--document",
            "rubric/requirements.md",
            "--heading",
            "R84",
        ],
        &["show", "--document", "evidence/tables/result.md"],
        &["show", "--document", "src/train.py", "--source"],
        &["show", "--document", "../outside.md"],
        &["show", "--document", "rubric"],
        &["show", "--document", "evidence", "--source"],
        &["show", "--document", "src"],
    ] {
        assert_file_access_hint(&failure(dir.path(), args));
    }
    assert_eq!(failure(dir.path(), &["show", "R84"])["code"], "unknown_id");
    assert_eq!(failure(dir.path(), &["show", "R01"])["code"], "unknown_id");
    // The probe word is in the rubric, an evidence body and one native document.
    put(
        dir.path(),
        "logic/problem.md",
        "# Problem\n\n## Gap\nQuokka gap.\n",
    );
    let found = run(dir.path(), &["find", "quokka"]);
    let sources = found["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["source"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(sources, ["logic/problem.md"], "{found}");
    let listed = run(dir.path(), &["ls"]);
    let entries = listed["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
    // Evidence tables stay typed exhibits; no file-access body is a document.
    for entry in entries {
        let source = entry["source"].as_str().unwrap_or("");
        assert!(!source.starts_with("rubric/"), "{entry}");
        assert!(
            !ara_core::file_access_path(source) || entry["kind"] == "exhibit",
            "{entry}"
        );
    }
    assert!(
        entries.iter().any(|entry| entry["kind"] == "exhibit"),
        "{listed}"
    );
}

#[test]
fn registered_rubric_path_does_not_bypass_the_native_boundary() {
    let dir = fixture(
        "---\ntitle: Rubric fixture\nknowledge_paths: [rubric/requirements.md]\n---\n# Rubric fixture\n",
    );
    for args in [
        &["show", "--document", "rubric/requirements.md", "--source"][..],
        &["show", "--document", "rubric/requirements.md"],
    ] {
        assert_file_access_hint(&failure(dir.path(), args));
    }
}

#[test]
fn removed_native_rubric_writes_reject_without_changing_bytes() {
    let empty = plain();
    fs::remove_dir_all(empty.path().join("rubric")).unwrap();
    let before = artifact_bytes(empty.path());
    let error = apply_failure(
        empty.path(),
        &[json!({"op":"document.create","document":"rubric/requirements.md","content":RUBRIC})],
    );
    assert_eq!(error["code"], "write.document", "{error}");
    assert_eq!(artifact_bytes(empty.path()), before);

    let dir = plain();
    let before = artifact_bytes(dir.path());
    let error = apply_failure(
        dir.path(),
        &[
            json!({"op":"document.replace","document":"rubric/requirements.md","expected":"sha256:00","content":"# Replaced\n"}),
        ],
    );
    assert_eq!(error["code"], "write.document", "{error}");
    let error = apply_failure(
        dir.path(),
        &[
            json!({"op":"entry.edit","target":{"document":"rubric/requirements.md","entry":"R01"},"set":{"Requirement":"edited"}}),
        ],
    );
    assert_eq!(error["code"], "write.selector", "{error}");
    let error = apply_failure(
        dir.path(),
        &[json!({"op":"entry.edit","target":{"id":"R01"},"set":{"Requirement":"edited"}})],
    );
    assert_eq!(error["code"], "write.namespace", "{error}");
    let error = apply_failure(
        dir.path(),
        &[json!({"op":"paper.edit","frontmatter":{"knowledge_paths":["rubric/notes.md"]}})],
    );
    assert_eq!(error["code"], "write.knowledge_paths", "{error}");
    assert_eq!(artifact_bytes(dir.path()), before);
}

#[test]
fn rubric_history_from_earlier_binaries_stays_exact_and_inspectable() {
    let dir = history();
    let before = artifact_bytes(dir.path());
    let status = run(dir.path(), &["status"]);
    assert_eq!(status["diagnostics"]["errors"], 0, "{status}");
    run(dir.path(), &["ls"]);
    run(dir.path(), &["open"]);
    let session = run(dir.path(), &["show", "2026-10-01_001", "--full"]);
    assert_eq!(
        session["entries"][0]["logic_revisions"][2]["entry"]["id"], "R03",
        "{session}"
    );
    let never_existed = failure(dir.path(), &["show", "C99"])["code"].clone();
    for id in ["R01", "R02", "R03", "R04"] {
        assert_eq!(failure(dir.path(), &["show", id])["code"], never_existed);
    }
    assert_file_access_hint(&failure(
        dir.path(),
        &["show", "--document", "rubric/requirements.md"],
    ));
    ara(dir.path())
        .args(["check", "."])
        .current_dir(dir.path())
        .assert()
        .success();
    assert_eq!(artifact_bytes(dir.path()), before);

    run(
        dir.path(),
        &[
            "claim",
            "set",
            "C01",
            "--set",
            "Conditions=Within boundary.",
        ],
    );
    let after = artifact_bytes(dir.path());
    for path in [
        "rubric/requirements.md",
        "trace/logic_mutations.yaml",
        "trace/sessions/2026-10-01_001.yaml",
    ] {
        assert_eq!(after[Path::new(path)], before[Path::new(path)], "{path}");
    }
}

#[test]
fn rubric_history_merges_rubric_as_external_read_only() {
    let ours = history();
    let base = history();
    let theirs = history();
    let original = fs::read(ours.path().join("rubric/requirements.md")).unwrap();
    let merge = |theirs: &Path| {
        let output = ara(ours.path())
            .arg("merge")
            .arg("--base")
            .arg(base.path())
            .arg("--theirs")
            .arg(theirs)
            .args(["--as", "peer", "--source-key", "peer-fork", "--json"])
            .output()
            .unwrap();
        (
            output.status.code(),
            serde_json::from_slice::<Value>(if output.stdout.is_empty() {
                &output.stderr
            } else {
                &output.stdout
            })
            .unwrap(),
        )
    };
    let (code, report) = merge(theirs.path());
    assert_eq!(code, Some(0), "{}", brief(&report));
    assert!(report["conflicts"].as_array().unwrap().is_empty());

    let mut incoming = original.clone();
    incoming.extend_from_slice(b"\n## R09: Incoming requirement\n- **Requirement**: incoming\n");
    put(theirs.path(), "rubric/requirements.md", &incoming);
    let (code, report) = merge(theirs.path());
    assert_eq!(code, Some(1), "{}", brief(&report));
    let conflicts = report["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{}", brief(&report));
    assert_eq!(conflicts[0]["kind"], "external_read_only");
    assert_eq!(conflicts[0]["path"], "rubric/requirements.md");
    assert_eq!(conflicts[0]["allowed"], json!(["ours"]));
    assert!(
        report["imports"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| !row["original"].as_str().unwrap().contains("R09")),
        "{}",
        brief(&report)
    );
    assert_eq!(
        fs::read(ours.path().join("rubric/requirements.md")).unwrap(),
        original
    );
}
