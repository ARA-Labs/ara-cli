//! Consumer-visible authoring contracts exercised through the real binary.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;

const TREE: &str = "# retained tree comment\ntree:\n  - id: N01\n    type: question\n    title: Boundary mechanism\n    provenance: user\n    opaque_extension: {nested: [one, two], enabled: true}\n";
const CLAIMS: &str = "# Claims\n\n## C01: Existing mechanism\n- **Statement**: Original mechanism.\n- **Conditions**: Within boundary.\n- **Status**: hypothesis\n- **Provenance**: user\n- **Falsification**: Contrary result.\n- **Dependencies**: []\n- **Unfamiliar**: retain exact prose = 雪\n\n## C02: Untouched\n- **Statement**: Independent statement.\n- **Status**: hypothesis\n";

fn ara(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command
        .arg("-C")
        .arg(root)
        .env_remove("ARA_DIR")
        .env_remove("ARA_NO_DUPLICATE_CHECK");
    command
}
fn write(root: &Path, path: &str, content: impl AsRef<[u8]>) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}
fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "trace/exploration_tree.yaml", TREE);
    write(dir.path(), "logic/claims.md", CLAIMS);
    write(
        dir.path(),
        "PAPER.md",
        "---\ntitle: Native writer fixture\nunknown_extension: {retained: true}\n---\n# Native writer fixture\n",
    );
    dir
}
fn run(root: &Path, args: &[&str]) -> Value {
    let output = ara(root)
        .args(args)
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty(), "{output:?}");
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
    serde_json::from_slice(&output.stderr).unwrap()
}
fn apply(root: &Path, operations: &[Value], dry_run: bool) -> Value {
    let text = operations
        .iter()
        .map(|op| format!("{op}\n"))
        .collect::<String>();
    let mut command = ara(root);
    command
        .args(["apply", "-", "--json", "--no-duplicate-check"])
        .write_stdin(text);
    if dry_run {
        command.arg("--dry-run");
    }
    let output = command.assert().success().get_output().clone();
    assert!(output.stderr.is_empty(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}
fn yaml(root: &Path, path: &str) -> Value {
    let text = fs::read_to_string(root.join(path)).unwrap();
    ara_core::write::positions::YamlDocument::parse(&text)
        .unwrap()
        .root
        .to_json()
        .unwrap()
}
fn artifact_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, relative: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(root.join(relative)).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == ".ara" {
                continue;
            }
            let path = relative.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                visit(root, &path, result);
            } else {
                result.insert(path.clone(), fs::read(root.join(path)).unwrap());
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, Path::new(""), &mut result);
    result
}

#[test]
fn single_add_replay_preserves_opaque_source_and_native_parent() {
    let dir = fixture();
    let args = [
        "add",
        "node",
        "--type",
        "question",
        "--parent",
        "N01",
        "--id",
        "N03",
        "--title",
        "New boundary",
        "--set",
        "description=Caller detail = 雪",
        "--provenance",
        "user",
        "--no-duplicate-check",
    ];
    let added = run(dir.path(), &args);
    assert_eq!(added["id"], "N03");
    assert_eq!(added["committed"], true);
    let tree = fs::read_to_string(dir.path().join("trace/exploration_tree.yaml")).unwrap();
    assert!(tree.starts_with(TREE));
    assert_eq!(
        fs::read(dir.path().join("logic/claims.md")).unwrap(),
        CLAIMS.as_bytes()
    );
    let shown = run(dir.path(), &["show", "N03", "--with", "parents", "--full"]);
    assert_eq!(shown["entries"][0]["relations"]["parents"], json!(["N01"]));
    assert_eq!(
        shown["entries"][0]["source_fields"]["description"],
        "Caller detail = 雪"
    );
    let before = artifact_bytes(dir.path());
    assert_eq!(
        failure(dir.path(), &args)["error"]["code"],
        "write.id_collision"
    );
    assert_eq!(artifact_bytes(dir.path()), before);
    assert_eq!(
        run(dir.path(), &["show", "N01", "--full"])["entries"][0]["source_fields"]["opaque_extension"],
        json!({"nested":["one","two"],"enabled":true})
    );
}

#[test]
fn ordered_batch_bindings_preview_then_commit_exact_native_graph() {
    let dir = fixture();
    let operations = [
        json!({"op":"node.add","id":"$branch","type":"question","parent":"N01","title":"Batch branch","fields":{"description":"Caller-authored branch question","provenance":"user"}}),
        json!({"op":"node.add","id":"$leaf","type":"question","parent":"$branch","title":"Batch leaf","fields":{"description":"Literal @file and $unbound remain prose","provenance":"user"}}),
    ];
    let before = artifact_bytes(dir.path());
    let preview = apply(dir.path(), &operations, true);
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["committed"], false);
    assert_eq!(artifact_bytes(dir.path()), before);
    assert!(!dir.path().join(".ara").exists());
    let committed = apply(dir.path(), &operations, false);
    assert_eq!(committed["bindings"], preview["bindings"]);
    let branch = committed["bindings"]["$branch"].as_str().unwrap();
    let leaf = committed["bindings"]["$leaf"].as_str().unwrap();
    assert_ne!(branch, leaf);
    let path = run(dir.path(), &["path", leaf]);
    assert_eq!(
        path["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["N01", branch, leaf]
    );
    assert_eq!(
        run(dir.path(), &["show", leaf, "--full"])["entries"][0]["source_fields"]["description"],
        "Literal @file and $unbound remain prose"
    );
}

#[cfg(unix)]
#[test]
fn dry_run_does_not_open_readonly_lock_or_create_operational_state() {
    use std::os::unix::fs::PermissionsExt;
    let dir = fixture();
    let operations = [
        json!({"op":"node.add","type":"question","parent":"N01","title":"Readonly preview","fields":{"description":"Caller-authored readonly question","provenance":"user"}}),
    ];
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let preview = apply(dir.path(), &operations, true);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(preview["committed"], false);
    assert!(!dir.path().join(".ara").exists());
    write(dir.path(), ".ara/lock", b"persistent lock inode bytes\n");
    fs::set_permissions(
        dir.path().join(".ara/lock"),
        fs::Permissions::from_mode(0o444),
    )
    .unwrap();
    fs::set_permissions(dir.path().join(".ara"), fs::Permissions::from_mode(0o555)).unwrap();
    let before = artifact_bytes(dir.path());
    apply(dir.path(), &operations, true);
    fs::set_permissions(dir.path().join(".ara"), fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(
        dir.path().join(".ara/lock"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert_eq!(
        fs::read(dir.path().join(".ara/lock")).unwrap(),
        b"persistent lock inode bytes\n"
    );
    assert_eq!(artifact_bytes(dir.path()), before);
    assert!(!dir.path().join(".ara/transactions").exists());
}

#[test]
fn guarded_failure_and_malformed_jsonl_report_physical_line_without_partial_write() {
    let dir = fixture();
    let before = artifact_bytes(dir.path());
    let first = json!({"op":"node.add","id":"$new","type":"question","parent":"N01","title":"Must not commit","fields":{"description":"Caller-authored rollback question","provenance":"user"}});
    for (last, code) in [
        (
            "{\"op\":\"entry.edit\",\"target\":{\"id\":\"N01\"},\"set\":{\"title\":\"Changed\"}}",
            "write.immutable",
        ),
        (
            "{\"op\":\"edge.add\",\"node\":\"N01\",\"depends_on\":\"N01\",\"unexpected\":true}",
            "write.batch_operation",
        ),
        (
            "{\"op\":\"node.add\",\"op\":\"edge.add\"}",
            "write.batch_json",
        ),
        ("{", "write.batch_json"),
    ] {
        let output = ara(dir.path())
            .args(["apply", "-", "--json", "--no-duplicate-check"])
            .write_stdin(format!("\n{first}\n\n{last}\n"))
            .assert()
            .code(1)
            .stdout("")
            .get_output()
            .clone();
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["error"]["code"], code);
        assert_eq!(error["error"]["line"], 4);
        assert_eq!(artifact_bytes(dir.path()), before);
    }
}

#[test]
fn mutable_claim_file_input_changes_only_selected_field() {
    let dir = fixture();
    let input = TempDir::new().unwrap();
    let prose = "New mechanism = 雪\n\n  exact indentation\n";
    write(input.path(), "statement.txt", prose);
    let assignment = format!(
        "Statement=@{}",
        input.path().join("statement.txt").display()
    );
    run(dir.path(), &["claim", "set", "C01", "--set", &assignment]);
    let shown = run(dir.path(), &["show", "C01", "--full"]);
    assert_eq!(shown["entries"][0]["statement"], prose);
    let bytes = fs::read_to_string(dir.path().join("logic/claims.md")).unwrap();
    assert!(bytes.contains("- **Unfamiliar**: retain exact prose = 雪\n"));
    assert!(
        bytes.ends_with(
            CLAIMS
                .split("## C02:")
                .nth(1)
                .map(|suffix| format!("## C02:{suffix}"))
                .unwrap()
                .as_str()
        )
    );
    assert_eq!(
        fs::read(dir.path().join("trace/exploration_tree.yaml")).unwrap(),
        TREE.as_bytes()
    );
    let before = artifact_bytes(dir.path());
    assert_eq!(
        failure(
            dir.path(),
            &["claim", "set", "C01", "--set", "Last revised=forged"]
        )["error"]["code"],
        "write.revision_required"
    );
    assert_eq!(artifact_bytes(dir.path()), before);
}

#[test]
fn session_next_turn_and_revision_are_one_atomic_authored_history() {
    let dir = fixture();
    let session = run(
        dir.path(),
        &[
            "session",
            "start",
            "--date",
            "2026-10-01",
            "--started",
            "2026-10-01T10:00",
            "--summary",
            "Explicit session",
        ],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let first = run(
        dir.path(),
        &[
            "session",
            "log",
            "--session",
            &session,
            "--node",
            "N01",
            "--timestamp",
            "2026-10-01T10:01",
            "--summary",
            "First event",
        ],
    );
    assert_eq!(first["turn"], 1);
    let session_path = format!("trace/sessions/{session}.yaml");
    let old_event = yaml(dir.path(), &session_path)["events_logged"][0].clone();
    let old_source = fs::read_to_string(dir.path().join(&session_path)).unwrap();
    let old_document = ara_core::write::positions::YamlDocument::parse(&old_source).unwrap();
    let old_node = &old_document
        .root
        .get("events_logged")
        .unwrap()
        .unwrap()
        .sequence()
        .unwrap()[0];
    let old_event_bytes = old_source[old_node.start..old_node.end].to_owned();
    let operations = [
        json!({"op":"session.log","session":session,"timestamp":"2026-10-01T10:02","summary":"Second turn","events":[{"type":"question","id":"N01","routing":"direct","provenance":"user","summary":"Second event"}]}),
        json!({"op":"logic.revise","target":{"id":"C01"},"set":{"Statement":"Revised mechanism."},"session":session,"turn":2,"signal":"empirical-resolution","provenance":"user","note":"Caller supplied contrary evidence"}),
    ];
    let before = artifact_bytes(dir.path());
    let dry = apply(dir.path(), &operations, true);
    assert_eq!(dry["operations"][0]["turn"], 2);
    assert_eq!(artifact_bytes(dir.path()), before);
    apply(dir.path(), &operations, false);
    let record = yaml(dir.path(), &session_path);
    assert_eq!(record["session"]["turn_count"], 2);
    assert_eq!(record["events_logged"][0], old_event);
    assert!(
        fs::read_to_string(dir.path().join(&session_path))
            .unwrap()
            .contains(&old_event_bytes)
    );
    assert_eq!(record["events_logged"][1]["turn"], 2);
    let revision = &record["logic_revisions"][0];
    assert_eq!(revision["before"], "Original mechanism.");
    assert_eq!(revision["after"], "Revised mechanism.");
    assert_eq!(revision["turn"], 2);
    assert_eq!(revision["signal"], "empirical-resolution");
    let claim = run(dir.path(), &["show", "C01", "--full"]);
    assert_eq!(claim["entries"][0]["statement"], "Revised mechanism.");
    assert_eq!(
        claim["entries"][0]["last_revised"],
        format!("2026-10-01 ({session}#2)")
    );
    assert_eq!(
        yaml(dir.path(), "trace/sessions/session_index.yaml")["sessions"][0]["turn_count"],
        2
    );
    let archive = yaml(dir.path(), "trace/pm_reasoning_log.yaml");
    assert_eq!(
        archive["entries"][1]["session_metadata"]["before"]["turn_count"],
        1
    );
    assert_eq!(
        archive["entries"][1]["session_metadata"]["after"]["turn_count"],
        2
    );
    let third = run(
        dir.path(),
        &[
            "session",
            "log",
            "--session",
            &session,
            "--node",
            "N01",
            "--timestamp",
            "2026-10-01T10:03",
            "--summary",
            "Third event",
        ],
    );
    assert_eq!(third["turn"], 3);
}

#[test]
fn promotion_keeps_observation_and_commits_claim_with_final_audit_tuple() {
    let dir = fixture();
    let content = "Original observation = 雪\n  exact indentation\n";
    let observation = run(
        dir.path(),
        &[
            "stage",
            "--content",
            content,
            "--potential-type",
            "claim",
            "--context",
            "Boundary context",
            "--provenance",
            "ai-executed",
            "--timestamp",
            "2026-10-01T10:00",
            "--bound-to",
            "N01",
        ],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let original = yaml(dir.path(), "staging/observations.yaml")["observations"][0].clone();
    let args = [
        "promote",
        observation.as_str(),
        "--to",
        "claim",
        "--title",
        "Crystallized finding",
        "--signal",
        "empirical-resolution",
        "--set",
        "Statement=Caller mechanism",
        "--set",
        "Conditions=Boundary context",
        "--set",
        "Status=hypothesis",
        "--set",
        "Falsification=Contrary evidence",
    ];
    let promoted = run(dir.path(), &args);
    let claim = promoted["id"].as_str().unwrap();
    let row = yaml(dir.path(), "staging/observations.yaml")["observations"][0].clone();
    for key in [
        "content",
        "context",
        "timestamp",
        "provenance",
        "bound_to",
        "potential_type",
    ] {
        assert_eq!(row[key], original[key]);
    }
    assert_eq!(row["content"], content);
    assert_eq!(row["promoted"], true);
    assert_eq!(row["promoted_to"], format!("logic/claims.md:{claim}"));
    assert_eq!(row["crystallized_via"], "empirical-resolution");
    let shown = run(dir.path(), &["show", claim, "--full"]);
    assert_eq!(shown["entries"][0]["statement"], "Caller mechanism");
    assert_eq!(shown["entries"][0]["provenance"], "ai-executed");
    let before = artifact_bytes(dir.path());
    failure(dir.path(), &args);
    assert_eq!(artifact_bytes(dir.path()), before);
}

#[test]
fn missing_only_initialization_preserves_existing_unknown_fields_and_rejects_conflict() {
    let dir = fixture();
    let paper = fs::read_to_string(dir.path().join("PAPER.md")).unwrap();
    let before = artifact_bytes(dir.path());
    let operation = json!({"op":"artifact.init","profile":"research-manager","paper":paper,"missing_only":true});
    apply(dir.path(), std::slice::from_ref(&operation), true);
    assert_eq!(artifact_bytes(dir.path()), before);
    assert!(!dir.path().join(".ara").exists());
    apply(dir.path(), std::slice::from_ref(&operation), false);
    for (path, bytes) in before {
        assert_eq!(fs::read(dir.path().join(path)).unwrap(), bytes);
    }
    assert_eq!(
        yaml(dir.path(), "staging/observations.yaml")["observations"],
        json!([])
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("logic/solution/heuristics.md")).unwrap(),
        "# Heuristics\n"
    );
    let complete = artifact_bytes(dir.path());
    assert_eq!(apply(dir.path(), &[operation], false)["no_op"], true);
    assert_eq!(artifact_bytes(dir.path()), complete);
    let output = ara(dir.path()).args(["apply", "-", "--json"]).write_stdin(format!("{}\n", json!({"op":"artifact.init","profile":"research-manager","paper":"# Different paper\n","missing_only":true}))).assert().code(1).stdout("").get_output().clone();
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap()["error"]["code"],
        "write.init_conflict"
    );
    assert_eq!(artifact_bytes(dir.path()), complete);
}

#[test]
fn duplicate_warning_retains_both_identities_and_search_finds_new_native_entry() {
    let dir = fixture();
    write(
        dir.path(),
        "trace/exploration_tree.yaml",
        TREE.replace(
            "    provenance: user",
            "    description: Caller-authored repeated question\n    provenance: user",
        ),
    );
    let added = run(
        dir.path(),
        &[
            "add",
            "node",
            "--type",
            "question",
            "--parent",
            "root",
            "--title",
            "Boundary mechanism",
            "--set",
            "description=Caller-authored repeated question",
            "--provenance",
            "user",
        ],
    );
    let id = added["id"].as_str().unwrap();
    assert_ne!(id, "N01");
    let pairs = added["duplicate_candidates"].as_array().unwrap();
    assert!(
        pairs
            .iter()
            .any(|pair| (pair["left"] == "N01" && pair["right"] == id)
                || (pair["right"] == "N01" && pair["left"] == id))
    );
    let shown = run(
        dir.path(),
        &["show", "N01", id, "--with", "same_as", "--full"],
    );
    assert_eq!(shown["entries"][0]["id"], "N01");
    assert_eq!(shown["entries"][1]["id"], id);
    assert_eq!(
        shown["entries"][1]["relations"]["same_as"],
        json!({"incoming":[],"outgoing":[]})
    );
    let found = run(
        dir.path(),
        &["find", "Boundary mechanism", "--type", "question", "--full"],
    );
    let identities = found["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(identities.contains(&"N01"));
    assert!(identities.contains(&id));
    assert_eq!(
        run(dir.path(), &["show", "N01", "--full"])["entries"][0]["source_fields"]["opaque_extension"]
            ["enabled"],
        true
    );
}

#[test]
fn scalar_evidence_and_compiler_heuristic_file_values_are_not_split_or_normalized() {
    let dir = fixture();
    let input = TempDir::new().unwrap();
    let evidence = "source.csv:1 = 雪; [literal], not JSON\n  exact continuation\n";
    write(input.path(), "evidence.txt", evidence);
    let evidence_assignment = format!("evidence=@{}", input.path().join("evidence.txt").display());
    let added = run(
        dir.path(),
        &[
            "add",
            "node",
            "--type",
            "experiment",
            "--parent",
            "root",
            "--title",
            "Caller experiment",
            "--set",
            "result=Caller-observed result",
            "--set",
            &evidence_assignment,
            "--no-duplicate-check",
        ],
    );
    let shown = run(
        dir.path(),
        &["show", added["id"].as_str().unwrap(), "--full"],
    );
    assert_eq!(shown["entries"][0]["source_fields"]["evidence"], evidence);
    let decision = run(
        dir.path(),
        &[
            "add",
            "node",
            "--type",
            "decision",
            "--parent",
            "root",
            "--title",
            "Caller decision",
            "--set",
            "choice=Caller selection",
            "--set",
            "alternatives=[\"Keep source\",\"Revise source\"]",
            "--set",
            "evidence=source:1 = 雪; punctuation, [literal]",
            "--no-duplicate-check",
        ],
    );
    let shown = run(
        dir.path(),
        &["show", decision["id"].as_str().unwrap(), "--full"],
    );
    assert_eq!(
        shown["entries"][0]["source_fields"]["evidence"],
        "source:1 = 雪; punctuation, [literal]"
    );
    assert_eq!(
        shown["entries"][0]["source_fields"]["alternatives"],
        json!(["Keep source", "Revise source"])
    );
    let listed = run(
        dir.path(),
        &[
            "add",
            "node",
            "--type",
            "experiment",
            "--parent",
            "root",
            "--title",
            "Caller explicit evidence list",
            "--set",
            "result=Caller list result",
            "--set",
            "evidence=[\"C01\",\"literal = 雪; punctuation, preserved\"]",
            "--no-duplicate-check",
        ],
    );
    let shown = run(
        dir.path(),
        &["show", listed["id"].as_str().unwrap(), "--full"],
    );
    assert_eq!(
        shown["entries"][0]["source_fields"]["evidence"],
        json!(["C01", "literal = 雪; punctuation, preserved"])
    );

    let code = "src/model.rs:12 = 雪; [literal], not JSON\n  exact code continuation\n";
    let source = "paper.pdf:p3 «a = b; [input], unchanged»\n  source continuation\n";
    let bounds = "Only the supplied synthetic context.\n  exact bound = 雪\n";
    write(input.path(), "code.txt", code);
    write(input.path(), "source.txt", source);
    write(input.path(), "bounds.txt", bounds);
    let code_assignment = format!("Code ref=@{}", input.path().join("code.txt").display());
    let source_assignment = format!("Source=@{}", input.path().join("source.txt").display());
    let bounds_assignment = format!("Bounds=@{}", input.path().join("bounds.txt").display());
    let heuristic = run(
        dir.path(),
        &[
            "heuristic",
            "add",
            "--title",
            "Caller compiler heuristic",
            "--set",
            "Rationale=Caller explanation",
            "--set",
            "Sensitivity=Not specified in paper",
            "--set",
            &code_assignment,
            "--set",
            &source_assignment,
            "--set",
            &bounds_assignment,
        ],
    );
    let shown = run(
        dir.path(),
        &["show", heuristic["id"].as_str().unwrap(), "--full"],
    );
    let fields = shown["entries"][0]["source_fields"].as_array().unwrap();
    for (name, value) in [("Source", source), ("Bounds", bounds), ("Code ref", code)] {
        assert_eq!(
            fields.iter().find(|field| field["name"] == name).unwrap()["value"],
            value
        );
    }
    assert!(!fields.iter().any(|field| matches!(
        field["name"].as_str(),
        Some("Status" | "Provenance" | "Sources")
    )));
}

#[test]
fn same_as_uses_authored_order_and_concepts_require_native_headings() {
    let dir = fixture();
    let operations = [
        json!({"op":"node.add","id":"N90","type":"question","parent":"root","title":"Earlier authoring","fields":{"description":"Caller earlier body"}}),
        json!({"op":"node.add","id":"N03","type":"question","parent":"root","title":"Later authoring","fields":{"description":"Caller later body","concepts":["Caller term"]}}),
        json!({"op":"document.create","document":"logic/concepts.md","content":"# Concepts\n\n## Caller term\n\n- **Definition**: Caller-authored definition.\n"}),
        json!({"op":"node.link_same_as","node":"N03","same_as":"N90"}),
    ];
    apply(dir.path(), &operations, false);
    let shown = run(dir.path(), &["show", "N03", "--with", "same_as", "--full"]);
    assert_eq!(
        shown["entries"][0]["source_fields"]["same_as"],
        json!(["N90"])
    );
    assert_eq!(
        shown["entries"][0]["source_fields"]["concepts"],
        json!(["Caller term"])
    );
    for operation in [
        json!({"op":"node.link_same_as","node":"N90","same_as":"N03"}),
        json!({"op":"node.link_same_as","node":"N03","same_as":"N03"}),
        json!({"op":"node.link_same_as","node":"N03","same_as":"N999"}),
        json!({"op":"node.add","type":"question","parent":"root","title":"Uncreated concept","fields":{"description":"Caller body","concepts":["Arbitrary missing term"]}}),
    ] {
        let before = artifact_bytes(dir.path());
        let output = ara(dir.path())
            .args(["apply", "-", "--json"])
            .write_stdin(format!("{operation}\n"))
            .assert()
            .code(1)
            .stdout("")
            .get_output()
            .clone();
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["error"]["code"], "write.node");
        assert_eq!(artifact_bytes(dir.path()), before);
    }
}

fn copied_agent_fixture() -> TempDir {
    fn copy_dir(src: &Path, dst: &Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    let dir = TempDir::new().unwrap();
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../ara-core/tests/fixtures/agent-cli"),
        dir.path(),
    );
    dir
}
fn shown_entry(root: &Path, id: &str) -> Value {
    run(root, &["show", id, "--full"])["entries"][0].clone()
}
fn source_field_names(entry: &Value) -> Vec<String> {
    entry["source_fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field["name"].as_str().unwrap().to_owned())
        .collect()
}
fn source_field(entry: &Value, name: &str) -> Value {
    entry["source_fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["name"] == name)
        .unwrap()["value"]
        .clone()
}

/// Reported case: the new C17 listed fields alphabetically, put every value on
/// a continuation line, and the creation order ignored the claim schema.
#[test]
fn reported_claim_add_renders_fixed_schema_order_with_inline_values() {
    let dir = copied_agent_fixture();
    let before = fs::read(dir.path().join("logic/claims.md")).unwrap();
    let before_all = artifact_bytes(dir.path());
    let added = run(
        dir.path(),
        &[
            "claim",
            "add",
            "--title",
            "Order probe",
            "--set",
            "Statement=S text",
            "--set",
            "Conditions=C text",
            "--set",
            "Status=supported",
            "--set",
            "Falsification criteria=F text",
            "--set",
            "Proof=[]",
            "--set",
            "Dependencies=[]",
            "--set",
            "Provenance=user",
            "--set",
            "Tags=[\"x\"]",
        ],
    );
    assert_eq!(added["id"], "C17");
    let after = fs::read(dir.path().join("logic/claims.md")).unwrap();
    assert_eq!(&after[..before.len()], &before[..]);
    assert_eq!(
        String::from_utf8(after[before.len()..].to_vec()).unwrap(),
        "\n## C17: Order probe\n- **Statement**: S text\n- **Conditions**: C text\n\
         - **Status**: supported\n- **Provenance**: user\n- **Falsification**: F text\n\
         - **Proof**: []\n- **Dependencies**: []\n- **Tags**: [\"x\"]\n"
    );
    // Outside the appended block only the operational ignore entry may change.
    let mut changed = artifact_bytes(dir.path());
    changed.retain(|path, bytes| {
        before_all.get(path) != Some(bytes) && path != Path::new(".gitignore")
    });
    assert_eq!(
        changed.keys().collect::<Vec<_>>(),
        [&PathBuf::from("logic/claims.md")]
    );

    let entry = shown_entry(dir.path(), "C17");
    assert_eq!(
        source_field_names(&entry),
        [
            "Statement",
            "Conditions",
            "Status",
            "Provenance",
            "Falsification",
            "Proof",
            "Dependencies",
            "Tags"
        ]
    );
    assert_eq!(entry["statement"], "S text");
    assert_eq!(entry["falsification"], "F text");
    assert_eq!(entry["deps"], json!([]));
    assert_eq!(entry["proof"], json!([]));
    assert_eq!(entry["proof_content"], "[]");
    assert_eq!(entry["tags"], "[\"x\"]");
    // The neighbouring hand-written claim keeps its alias label and list styles.
    let prior = shown_entry(dir.path(), "C16");
    assert!(source_field_names(&prior).contains(&"Falsification criteria".to_owned()));
    assert_eq!(prior["deps"], json!(["C03", "C04", "C05", "C06"]));
    assert_eq!(prior["tags"], "evaluation, experimental-design");
}

#[test]
fn claim_add_lists_scalars_and_multiline_values_round_trip_through_show() {
    let dir = fixture();
    let input = TempDir::new().unwrap();
    let statement = "First line 雪\r\n\n  indented tail\n";
    write(input.path(), "statement.txt", statement);
    let statement_assignment = format!(
        "Statement=@{}",
        input.path().join("statement.txt").display()
    );
    let before = fs::read_to_string(dir.path().join("logic/claims.md")).unwrap();
    let added = run(
        dir.path(),
        &[
            "claim",
            "add",
            "--title",
            "Lossless probe",
            "--set",
            "Tags=evaluation, experimental-design",
            "--set",
            "Dependencies=[\"C01\",\"C02\"]",
            "--set",
            "Sources=[\"a, b\",\"[x]\",\"\",\"none\",\"q\\\"uote\",\"back\\\\slash\",\"雪\"]",
            "--set",
            "Proof=none",
            "--set",
            "Falsification=F text",
            "--set",
            "Provenance=ai-suggested",
            "--set",
            "Status=hypothesis",
            "--set",
            "Conditions=[]",
            "--set",
            &statement_assignment,
        ],
    );
    let id = added["id"].as_str().unwrap();
    let after = fs::read_to_string(dir.path().join("logic/claims.md")).unwrap();
    assert!(after.starts_with(&before));
    assert_eq!(
        &after[before.len()..],
        format!(
            "\n## {id}: Lossless probe\n- **Statement**:\n  First line 雪\r\n  \n    indented tail\n  \n\
             - **Conditions**: []\n\
             - **Sources**: [\"a, b\",\"[x]\",\"\",\"none\",\"q\\\"uote\",\"back\\\\slash\",\"雪\"]\n\
             - **Status**: hypothesis\n- **Provenance**: ai-suggested\n- **Falsification**: F text\n\
             - **Proof**: none\n- **Dependencies**: [C01, C02]\n\
             - **Tags**: evaluation, experimental-design\n"
        )
    );
    let entry = shown_entry(dir.path(), id);
    assert_eq!(entry["statement"], statement);
    assert_eq!(entry["conditions"], "[]");
    assert_eq!(entry["deps"], json!(["C01", "C02"]));
    assert_eq!(entry["proof"], json!([]));
    assert_eq!(entry["proof_content"], "none");
    assert_eq!(entry["tags"], "evaluation, experimental-design");
    let sources: Value =
        serde_json::from_str(source_field(&entry, "Sources").as_str().unwrap()).unwrap();
    assert_eq!(
        sources,
        json!(["a, b", "[x]", "", "none", "q\"uote", "back\\slash", "雪"])
    );
    assert_eq!(source_field(&entry, "Dependencies"), "[C01, C02]");
}

#[test]
fn heuristic_add_and_promotions_create_fixed_schema_blocks() {
    let dir = fixture();
    let heuristic = run(
        dir.path(),
        &[
            "heuristic",
            "add",
            "--title",
            "Ordered heuristic",
            "--set",
            "Tags=a, b",
            "--set",
            "code_ref=[\"src/run.rs:12\",\"src/[x].rs\"]",
            "--set",
            "Bounds=Only synthetic data.\n  exact bound = 雪\n",
            "--set",
            "Sensitivity=unknown",
            "--set",
            "Provenance=user",
            "--set",
            "Status=active",
            "--set",
            "Sources=[\"doi:1\"]",
            "--set",
            "Source=paper.pdf p3 «a, b»",
            "--set",
            "Rationale=Reason",
        ],
    );
    let id = heuristic["id"].as_str().unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("logic/solution/heuristics.md")).unwrap(),
        format!(
            "# Heuristics\n\n## {id}: Ordered heuristic\n- **Rationale**: Reason\n\
             - **Source**: paper.pdf p3 «a, b»\n- **Sources**: [\"doi:1\"]\n\
             - **Status**: active\n- **Provenance**: user\n- **Sensitivity**: unknown\n\
             - **Bounds**:\n  Only synthetic data.\n    exact bound = 雪\n  \n\
             - **Code ref**: [\"src/run.rs:12\",\"src/[x].rs\"]\n- **Tags**: a, b\n"
        )
    );
    let entry = shown_entry(dir.path(), id);
    assert_eq!(entry["rationale"], "Reason");
    assert_eq!(entry["code_ref"], "[\"src/run.rs:12\",\"src/[x].rs\"]");
    assert_eq!(
        source_field(&entry, "Bounds"),
        "Only synthetic data.\n  exact bound = 雪\n"
    );
    assert_eq!(source_field(&entry, "Tags"), "a, b");

    for (to, title, sets) in [
        (
            "claim",
            "Promoted claim",
            vec![
                "Tags=[\"t\"]",
                "Falsification criteria=Contrary evidence",
                "Status=hypothesis",
                "Conditions=Boundary context",
                "Statement=Caller mechanism",
            ],
        ),
        (
            "heuristic",
            "Promoted heuristic",
            vec![
                "Code ref=src/run.rs",
                "Sensitivity=low",
                "Rationale=Observed twice",
            ],
        ),
    ] {
        let observation = run(
            dir.path(),
            &[
                "stage",
                "--content",
                "Observation text",
                "--potential-type",
                to,
                "--context",
                "Context",
                "--provenance",
                "ai-executed",
                "--timestamp",
                "2026-10-01T10:00",
            ],
        )["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let document = if to == "claim" {
            "logic/claims.md"
        } else {
            "logic/solution/heuristics.md"
        };
        let before = fs::read_to_string(dir.path().join(document)).unwrap();
        let mut args = vec![
            "promote",
            observation.as_str(),
            "--to",
            to,
            "--title",
            title,
            "--signal",
            "empirical-resolution",
        ];
        for set in &sets {
            args.extend(["--set", set]);
        }
        let promoted = run(dir.path(), &args);
        let id = promoted["id"].as_str().unwrap();
        let after = fs::read_to_string(dir.path().join(document)).unwrap();
        assert!(after.starts_with(&before));
        let expected = if to == "claim" {
            format!(
                "\n## {id}: {title}\n- **Statement**: Caller mechanism\n\
                 - **Conditions**: Boundary context\n- **Status**: hypothesis\n\
                 - **Provenance**: ai-executed\n- **Falsification**: Contrary evidence\n\
                 - **Tags**: [\"t\"]\n"
            )
        } else {
            format!(
                "\n## {id}: {title}\n- **Rationale**: Observed twice\n\
                 - **Provenance**: ai-executed\n- **Sensitivity**: low\n\
                 - **Code ref**: src/run.rs\n"
            )
        };
        assert_eq!(&after[before.len()..], expected);
        assert_eq!(shown_entry(dir.path(), id)["provenance"], "ai-executed");
    }
}

#[test]
fn crlf_claim_file_gets_crlf_structure_and_exact_values_through_show_and_check() {
    let dir = fixture();
    let original =
        "# Claims\r\n\r\n## C01: Prior\r\n- **Statement**: Kept\r\n- **Status**: hypothesis\r\n";
    write(dir.path(), "logic/claims.md", original);
    write(
        dir.path(),
        "logic/experiments.md",
        "## E01: A\n\n## E03: B\n",
    );
    let input = TempDir::new().unwrap();
    let statement = "Multi 雪\nline\r\nvalue\n";
    write(input.path(), "statement.txt", statement);
    let statement_assignment = format!(
        "Statement=@{}",
        input.path().join("statement.txt").display()
    );
    let added = run(
        dir.path(),
        &[
            "claim",
            "add",
            "--title",
            "CRLF probe",
            "--set",
            &statement_assignment,
            "--set",
            "Conditions=C text",
            "--set",
            "Status=supported",
            "--set",
            "Provenance=user",
            "--set",
            "Falsification=F text",
            "--set",
            "Proof=[\"E01\",\"E03\"]",
            "--set",
            "Sources=[\"E03 table\",\"C01\"]",
            "--set",
            "Dependencies=[\"C01\"]",
        ],
    );
    assert_eq!(added["id"], "C02");
    let after = fs::read_to_string(dir.path().join("logic/claims.md")).unwrap();
    assert!(after.starts_with(original));
    assert_eq!(
        &after[original.len()..],
        "\r\n## C02: CRLF probe\r\n- **Statement**:\n  Multi 雪\n  line\r\n  value\n  \n\
         - **Conditions**: C text\r\n- **Sources**: [\"E03 table\",\"C01\"]\r\n\
         - **Status**: supported\r\n- **Provenance**: user\r\n- **Falsification**: F text\r\n\
         - **Proof**: [\"E01\",\"E03\"]\r\n- **Dependencies**: [C01]\r\n"
    );
    let entry = shown_entry(dir.path(), "C02");
    assert_eq!(entry["title"], "CRLF probe");
    assert_eq!(entry["statement"], statement);
    assert_eq!(entry["conditions"], "C text");
    assert_eq!(entry["falsification"], "F text");
    assert_eq!(entry["proof"], json!(["E01", "E03"]));
    assert_eq!(entry["proof_content"], "[\"E01\",\"E03\"]");
    assert_eq!(entry["sources"], "[\"E03 table\",\"C01\"]");
    assert_eq!(entry["deps"], json!(["C01"]));
    let check = ara(dir.path())
        .args(["check", "."])
        .current_dir(dir.path())
        .arg("--json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let check: Value = serde_json::from_slice(&check).unwrap();
    assert_eq!(check["validate"]["errors"], json!([]));
}

#[test]
fn created_list_references_fail_closed_on_dangling_ids() {
    let dir = fixture();
    write(
        dir.path(),
        "logic/experiments.md",
        "## E01: A\n\n## E03: B\n",
    );
    for (field, list) in [
        ("Proof", "[\"E01\",\"E09\"]"),
        ("Sources", "[\"table\",\"C99\"]"),
    ] {
        let before = artifact_bytes(dir.path());
        let assignment = format!("{field}={list}");
        let error = failure(
            dir.path(),
            &[
                "claim",
                "add",
                "--title",
                "Dangling probe",
                "--set",
                "Statement=S",
                "--set",
                "Conditions=C",
                "--set",
                "Status=hypothesis",
                "--set",
                "Provenance=user",
                "--set",
                "Falsification=F",
                "--set",
                &assignment,
            ],
        );
        assert_eq!(error["error"]["code"], "write.reference", "{field}");
        assert_eq!(artifact_bytes(dir.path()), before, "{field}");
    }
}

fn utc_now() -> String {
    ara_core::write::clock::system_utc().unwrap()
}
fn assert_native_utc(value: &Value) -> String {
    let text = value.as_str().unwrap_or_else(|| panic!("{value:?}"));
    let bytes = text.as_bytes();
    assert_eq!(bytes.len(), 20, "{text}");
    assert!(
        text.chars().enumerate().all(|(i, c)| match i {
            4 | 7 => c == '-',
            10 => c == 'T',
            13 | 16 => c == ':',
            19 => c == 'Z',
            _ => c.is_ascii_digit(),
        }),
        "{text}"
    );
    text.to_owned()
}
fn apply_failure(root: &Path, text: &str) -> Value {
    let output = ara(root)
        .args(["apply", "-", "--json", "--no-duplicate-check"])
        .write_stdin(text.to_owned())
        .assert()
        .code(1)
        .stdout("")
        .get_output()
        .clone();
    serde_json::from_slice(&output.stderr).unwrap()
}

#[test]
fn apply_derives_owner_from_one_summarized_log_and_stamps_one_clock_value() {
    let dir = fixture();
    let before = utc_now();
    let report = apply(
        dir.path(),
        &[
            json!({"op":"session.log","summary":"Caller-written turn summary"}),
            json!({"op":"node.add","type":"question","parent":"N01","title":"Stamped","fields":{"description":"d"}}),
            json!({"op":"observation.stage","content":"Seen","potential_type":"unknown","provenance":"user"}),
            json!({"op":"logic.revise","target":{"id":"C01"},"set":{"Statement":"Revised by anchor"},"signal":"user-directive","provenance":"user"}),
        ],
        false,
    );
    let after = utc_now();
    let session = report["operations"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(report["operations"][0]["turn"], 1);
    assert_eq!(report["operations"][0]["session_created"], true);
    assert_eq!(report["operations"][3]["session"], session);
    assert_eq!(report["operations"][3]["turn"], 1);
    let record = yaml(dir.path(), &format!("trace/sessions/{session}.yaml"));
    let stamps = [
        record["session"]["started"].clone(),
        record["session"]["last_turn"].clone(),
        yaml(dir.path(), "trace/exploration_tree.yaml")["tree"][0]["children"][0]["timestamp"]
            .clone(),
        yaml(dir.path(), "staging/observations.yaml")["observations"][0]["timestamp"].clone(),
    ];
    let first = assert_native_utc(&stamps[0]);
    for stamp in &stamps {
        assert_eq!(assert_native_utc(stamp), first, "one clock value per batch");
    }
    assert!(
        before <= first && first <= after,
        "{before} <= {first} <= {after}"
    );
    assert_eq!(&session[..10], &first[..10]);
    assert_eq!(record["logic_revisions"][0]["turn"], 1);
}

#[test]
fn omitted_session_refuses_several_open_candidates_at_the_physical_line() {
    let dir = fixture();
    let day = utc_now();
    let first = run(dir.path(), &["session", "start", "--summary", "First"]);
    let second = run(dir.path(), &["session", "start", "--summary", "Second"]);
    if utc_now()[..10] != day[..10] {
        return; // The run crossed midnight UTC; the sessions are on two days.
    }
    let ids = [
        first["id"].as_str().unwrap(),
        second["id"].as_str().unwrap(),
    ];
    assert_eq!(&ids[0][..10], &day[..10]);
    let before = artifact_bytes(dir.path());
    // Explicit timestamps on the sessions' date keep the rest of this test
    // independent of the wall clock crossing midnight.
    let late = format!("{}T23:59:59Z", &day[..10]);
    let error = apply_failure(
        dir.path(),
        &format!(
            "\n{{\"op\":\"session.log\",\"summary\":\"Which session?\",\"timestamp\":\"{late}\"}}\n"
        ),
    );
    assert_eq!(error["error"]["code"], "write.session_ambiguous");
    assert_eq!(error["error"]["line"], 2);
    assert_eq!(error["error"]["details"]["field"], "session");
    let message = error["error"]["message"].as_str().unwrap();
    assert!(
        message.contains(&format!("{}, {}", ids[0], ids[1])),
        "{message}"
    );
    let error = failure(
        dir.path(),
        &[
            "session",
            "log",
            "--summary",
            "Which?",
            "--timestamp",
            &late,
            "--record",
            "{}",
        ],
    );
    assert_eq!(error["error"]["code"], "write.session_ambiguous");
    // Omitted context without a log anchor is refused, not guessed.
    let error = apply_failure(
        dir.path(),
        "{\"op\":\"logic.revise\",\"target\":{\"id\":\"C01\"},\"set\":{\"Statement\":\"x\"},\"signal\":\"user-directive\",\"provenance\":\"user\"}\n",
    );
    assert_eq!(error["error"]["code"], "write.owner_required");
    assert_eq!(error["error"]["line"], 1);
    assert_eq!(artifact_bytes(dir.path()), before);
}

#[test]
fn convenience_commands_default_inside_the_writer_lock() {
    let dir = fixture();
    let before = utc_now();
    let started = run(dir.path(), &["session", "start", "--summary", "Today"]);
    let staged = run(
        dir.path(),
        &[
            "stage",
            "--content",
            "Unstamped",
            "--potential-type",
            "unknown",
            "--provenance",
            "user",
        ],
    );
    assert_eq!(staged["id"], "O01");
    let logged = run(
        dir.path(),
        &["session", "log", "--summary", "Selected", "--record", "{}"],
    );
    let after = utc_now();
    if before[..10] != after[..10] {
        return; // Crossed midnight UTC between commands.
    }
    assert_eq!(logged["id"], started["id"]);
    assert_eq!(logged["turn"], 1);
    let session = started["id"].as_str().unwrap();
    let record = yaml(dir.path(), &format!("trace/sessions/{session}.yaml"));
    let stamp = assert_native_utc(&record["session"]["started"]);
    assert!(before <= stamp && stamp <= after);
    let observed = assert_native_utc(
        &yaml(dir.path(), "staging/observations.yaml")["observations"][0]["timestamp"],
    );
    assert!(before <= observed && observed <= after);
    // Omitting --session without --summary is refused; explicit logging keeps
    // its rolling-summary behavior.
    let error = failure(dir.path(), &["session", "log", "--record", "{}"]);
    assert_eq!(error["error"]["code"], "write.owner_summary");
    let late = format!("{}T23:59:59Z", &session[..10]);
    let explicit = run(
        dir.path(),
        &[
            "session",
            "log",
            "--session",
            session,
            "--timestamp",
            &late,
            "--record",
            "{}",
        ],
    );
    assert_eq!(explicit["turn"], 2);
    assert_eq!(
        yaml(dir.path(), &format!("trace/sessions/{session}.yaml"))["session"]["summary"],
        "Selected"
    );
}

fn merge_fixture(root: &Path, session: &str, threads: &str, open_threads: u64) {
    let date = &session[..10];
    write(
        root,
        "trace/exploration_tree.yaml",
        "tree:\n  - id: N01\n    type: question\n    title: Existing\n    provenance: user\n",
    );
    write(
        root,
        &format!("trace/sessions/{session}.yaml"),
        format!(
            "session:\n  id: {session}\n  date: {date}\n  started: '{date}T00:00:00Z'\n  last_turn: '{date}T00:00:00Z'\n  turn_count: 1\n  summary: base\nevents_logged: []\nai_actions: []\nclaims_touched: []\nlogic_revisions: []\nkey_context: []\nopen_threads: {threads}\nai_suggestions_pending: []\n"
        ),
    );
    write(
        root,
        "trace/sessions/session_index.yaml",
        format!(
            "sessions:\n  - id: {session}\n    date: {date}\n    summary: base\n    turn_count: 1\n    events_count: 0\n    claims_touched: []\n    open_threads: {open_threads}\n"
        ),
    );
}

#[test]
fn merge_resolve_derives_turn_and_locked_timestamp_without_turn_flag() {
    let parent = TempDir::new().unwrap();
    let today = utc_now();
    let session = format!("{}_001", &today[..10]);
    let [base, ours, theirs] = ["base", "ours", "theirs"].map(|name| parent.path().join(name));
    merge_fixture(&base, &session, "[]", 0);
    merge_fixture(&ours, &session, "[our thread]", 1);
    merge_fixture(&theirs, &session, "[their thread]", 1);
    let output = ara(&ours)
        .args(["merge", "--base"])
        .arg(&base)
        .arg("--theirs")
        .arg(&theirs)
        .args([
            "--as",
            "peer",
            "--source-key",
            "peer-fork",
            "--json",
            "--no-duplicate-check",
        ])
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let conflict = report["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["field"] == "open_threads")
        .unwrap_or_else(|| panic!("{report:#}"))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let audit = [
        "--take",
        "theirs",
        "--session",
        session.as_str(),
        "--signal",
        "user-directive",
        "--provenance",
        "user",
    ];
    let before = artifact_bytes(&ours);
    let error = failure(
        &ours,
        &[
            &["merge", "resolve", &conflict][..],
            &audit,
            &["--turn", "5"],
        ]
        .concat(),
    );
    assert_eq!(error["error"]["code"], "merge.resolution_session");
    assert_eq!(artifact_bytes(&ours), before);
    let started = utc_now();
    let output = ara(&ours)
        .args([&["merge", "resolve", &conflict][..], &audit, &["--json"]].concat())
        .output()
        .unwrap();
    let finished = utc_now();
    if started[..10] != today[..10] || finished[..10] != today[..10] {
        return; // Crossed midnight UTC; the session belongs to the previous day.
    }
    assert!(output.status.success(), "{output:?}");
    let resolved: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(resolved["session"], session);
    assert_eq!(resolved["turn"], 2);
    let record = yaml(&ours, &format!("trace/sessions/{session}.yaml"));
    assert_eq!(record["session"]["turn_count"], 2);
    assert_eq!(record["session"]["summary"], "base");
    assert_eq!(record["open_threads"], json!(["their thread"]));
    let stamp = assert_native_utc(&record["session"]["last_turn"]);
    assert!(started <= stamp && stamp <= finished);
    assert_eq!(record["logic_revisions"][0]["turn"], 2);
}

#[test]
fn merge_repair_derives_turn_without_turn_flag() {
    let parent = TempDir::new().unwrap();
    let today = utc_now();
    let session = format!("{}_001", &today[..10]);
    let original = "entries:\n  - summary: Existing\n    title: 'Opaque extension'\n";
    let [base, ours, theirs] = ["base", "ours", "theirs"].map(|name| parent.path().join(name));
    for root in [&base, &ours, &theirs] {
        merge_fixture(root, &session, "[]", 0);
        write(root, "trace/reasoning.yaml", original);
    }
    write(
        &theirs,
        "trace/reasoning.yaml",
        original.replace("'Opaque extension'", "\"Opaque extension\""),
    );
    let output = ara(&ours)
        .args(["merge", "--base"])
        .arg(&base)
        .arg("--theirs")
        .arg(&theirs)
        .args([
            "--as",
            "peer",
            "--source-key",
            "peer-fork",
            "--json",
            "--no-duplicate-check",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["error"]["code"], "merge.protected_content");
    let conflict = error["error"]["details"]["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["field"] == "title")
        .unwrap()
        .clone();
    let conflict_file = parent.path().join("conflict.json");
    fs::write(&conflict_file, conflict.to_string()).unwrap();
    // An explicit timestamp on the session's date keeps this independent of
    // the wall clock crossing midnight.
    let late = format!("{}T23:59:59Z", &today[..10]);
    let fingerprint = conflict["ours"]["fingerprint"].as_str().unwrap();
    let conflict_path = conflict_file.to_str().unwrap();
    let args = [
        "merge",
        "repair",
        "--conflict-file",
        conflict_path,
        "--decision",
        "reject_incoming",
        "--expected-current",
        fingerprint,
        "--session",
        session.as_str(),
        "--timestamp",
        late.as_str(),
        "--signal",
        "user-directive",
        "--provenance",
        "user",
        "--reason",
        "Keep local history",
    ];
    let before = artifact_bytes(&ours);
    let error = failure(&ours, &[&args[..], &["--turn", "1"]].concat());
    assert_eq!(error["error"]["code"], "merge.resolution_session");
    assert_eq!(artifact_bytes(&ours), before);
    let repaired = run(&ours, &args);
    assert_eq!(repaired["session"], session);
    assert_eq!(repaired["turn"], 2);
    let record = yaml(&ours, &format!("trace/sessions/{session}.yaml"));
    assert_eq!(record["session"]["last_turn"], late);
    assert_eq!(record["logic_revisions"][0]["turn"], 2);
    assert_eq!(
        fs::read_to_string(ours.join("trace/reasoning.yaml")).unwrap(),
        original
    );
}
