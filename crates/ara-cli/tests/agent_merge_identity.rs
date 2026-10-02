//! Binary-level identity consumers for accepted native writer archives.
use ara_core::write::{self, ArtifactSnapshot, EntrySelector, WorkingArtifact, WriteOperation};
use assert_cmd::Command;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::TempDir;

const DOCUMENT: &str = "logic/solution/architecture.md";
const ORIGINAL: &str = "# Architecture\n\n## Parent\nContainer body.\n### A/B\nLiteral child body.\n### A\nNested parent body.\n#### B\nNested child body.\n";
fn put(root: &Path, path: &str, bytes: &[u8]) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, bytes).unwrap();
}
fn seed(root: &Path) {
    put(
        root,
        "trace/exploration_tree.yaml",
        b"tree: [{id: N01, type: question, title: Unrelated}]\n",
    );
    put(root, DOCUMENT, ORIGINAL.as_bytes());
}
fn ara(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command
        .env_remove("ARA_DIR")
        .env("ARA_NO_DUPLICATE_CHECK", "1")
        .arg("-C")
        .arg(root);
    command
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
fn writer_rename(root: &Path) {
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(root).unwrap());
    for value in [
        json!({"op":"session.start","id":"2026-10-01_001","date":"2026-10-01","started":"2026-10-01T10:00","summary":"Accepted writer fixture"}),
        json!({"op":"session.log","session":"2026-10-01_001","timestamp":"2026-10-01T10:01"}),
    ] {
        write::plan_operation(&mut working, &serde_json::from_value(value).unwrap()).unwrap();
    }
    let target = EntrySelector::Document {
        document: DOCUMENT.into(),
        heading: vec!["Architecture".into(), "Parent".into()],
        entry: None,
    };
    let range = write::logic::resolve(&working, &target).unwrap().range;
    let expected = write::source::digest(working.text(DOCUMENT).unwrap()[range].as_bytes());
    write::plan_operation(
        &mut working,
        &WriteOperation::EntryRename {
            target,
            name: "Renamed parent".into(),
            expected,
            references: vec![],
            session: Some("2026-10-01_001".into()),
            turn: Some(1),
            signal: Some("user-directive".into()),
            provenance: Some("user".into()),
        },
    )
    .unwrap();
    for revision in std::mem::take(&mut working.revisions) {
        write::sessions::append_revision(
            &mut working,
            &revision.session,
            revision.turn,
            &revision.record,
        )
        .unwrap();
    }
    write::logic::validate_references(&working).unwrap();
    write::sessions::validate_authored(&working).unwrap();
    let ledger = working
        .yaml("trace/logic_mutations.yaml")
        .unwrap()
        .root
        .to_json()
        .unwrap();
    let colliding = ledger["mutations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["from"] == "logic/solution/architecture.md:Architecture/Parent/A/B")
        .collect::<Vec<_>>();
    assert_eq!(colliding.len(), 2);
    assert_ne!(colliding[0]["from_selector"], colliding[1]["from_selector"]);
    for (path, bytes) in &working.files {
        put(root, path, bytes);
    }
}
fn show_vectors(root: &Path) {
    let whole = run(root, &["show", "--document", DOCUMENT, "--full"]);
    assert!(whole["entries"][0]["content"].as_str().unwrap().contains(
        "### A/B\nLiteral child body.\n### A\nNested parent body.\n#### B\nNested child body."
    ));
    for ancestor in ["Parent", "Renamed parent"] {
        let parent = run(
            root,
            &[
                "show",
                "--document",
                DOCUMENT,
                "--heading",
                "Architecture",
                "--heading",
                ancestor,
                "--full",
            ],
        );
        assert_eq!(
            parent["entries"][0]["heading"],
            json!(["Architecture", "Renamed parent"])
        );
        assert!(
            parent["entries"][0]["content"]
                .as_str()
                .unwrap()
                .contains("Literal child body.")
        );
        assert!(
            parent["entries"][0]["content"]
                .as_str()
                .unwrap()
                .contains("Nested child body.")
        );
    }
    assert_eq!(run(root, &["show", "N01"])["entries"][0]["id"], "N01");
    assert!(
        run(root, &["ls"])["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["id"] == "N01")
    );
    for ancestor in ["Parent", "Renamed parent"] {
        for (tail, body) in [
            (vec!["A/B"], "Literal child body.\n"),
            (vec!["A", "B"], "Nested child body.\n"),
        ] {
            let mut args = vec![
                "show",
                "--document",
                DOCUMENT,
                "--heading",
                "Architecture",
                "--heading",
                ancestor,
            ];
            for part in &tail {
                args.extend(["--heading", *part]);
            }
            args.push("--full");
            let actual = run(root, &args);
            assert_eq!(actual["entries"][0]["content"], body);
            let mut current = vec!["Architecture", "Renamed parent"];
            current.extend(tail);
            assert_eq!(actual["entries"][0]["heading"], json!(current));
        }
    }
    // Reject only the requested ambiguous legacy display locator.
    for address in [
        "logic/solution/architecture.md:Architecture/Parent/A/B",
        "logic/solution/architecture.md#Architecture/Renamed parent/A/B",
    ] {
        let output = ara(root)
            .args(["resolve", address, "--json"])
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("merge.redirect_ambiguous")
                || String::from_utf8_lossy(&output.stdout).contains("merge.redirect_ambiguous")
        );
    }
    assert_eq!(run(root, &["show", "N01"])["entries"][0]["id"], "N01");
}
fn frozen(root: &Path) -> BTreeMap<String, Vec<u8>> {
    ArtifactSnapshot::load_complete(root)
        .unwrap()
        .files
        .into_iter()
        .filter(|(_, file)| file.existed)
        .map(|(path, file)| (path, file.bytes))
        .collect()
}
#[test]
fn accepted_colliding_writer_vectors_survive_actual_reads_merge_and_identical_replay() {
    let owner = TempDir::new().unwrap();
    let base = owner.path().join("base");
    let ours = owner.path().join("ours");
    let source = owner.path().join("source");
    for root in [&base, &ours, &source] {
        seed(root);
    }
    writer_rename(&source);
    show_vectors(&source);
    let source_before = frozen(&source);
    let base_before = frozen(&base);
    let merge = || {
        run(
            &ours,
            &[
                "merge",
                "--base",
                base.to_str().unwrap(),
                "--theirs",
                source.to_str().unwrap(),
                "--as",
                "bob",
                "--source-key",
                "literal-vector-source",
                "--no-duplicate-check",
            ],
        )
    };
    let first = merge();
    assert_eq!(first["unresolved_count"], 0);
    assert_eq!(first["committed"], true);
    show_vectors(&ours);
    assert_eq!(
        fs::read(ours.join(DOCUMENT)).unwrap(),
        source_before[DOCUMENT]
    );
    let destination_before = frozen(&ours);
    let replay = merge();
    assert_eq!(replay["unresolved_count"], 0);
    assert_eq!(replay["changed_paths"], json!([]));
    assert_eq!(frozen(&ours), destination_before);
    assert_eq!(frozen(&source), source_before);
    assert_eq!(frozen(&base), base_before);
    show_vectors(&ours);
}
