//! Consumer-visible regressions for PR 115's reference-history review.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;

const OBSERVATIONS: &str = "staging/observations.yaml";
const RECENT_SESSION: &str = "trace/sessions/2026-10-04_001.yaml";
const REASONING: &str = "trace/pm_reasoning_log.yaml";

fn ara(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ara").unwrap();
    command
        .arg("-C")
        .arg(root)
        .env_remove("ARA_DIR")
        .env_remove("ARA_NO_DUPLICATE_CHECK");
    command
}

fn write(root: &Path, path: &str, text: impl AsRef<[u8]>) {
    let target = root.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, text).unwrap();
}

fn fixture(root: &Path, tree: &str) {
    write(
        root,
        "PAPER.md",
        "---\ntitle: History review\n---\n# History review\n",
    );
    write(root, "trace/exploration_tree.yaml", tree);
}

fn observation(root: &Path, id: &str, bound_to: &[&str]) {
    write(
        root,
        OBSERVATIONS,
        ara_core::write::source::render_yaml(
            &json!({"observations":[{"id":id,"timestamp":"2026-10-01T10:00:00Z","content":"Investigate the boundary","potential_type":"claim","provenance":"user","promoted":false,"stale":false,"bound_to":bound_to}]}),
            0,
            "\n",
        ),
    );
}

fn ndjson(operations: &[Value]) -> String {
    operations.iter().map(|op| format!("{op}\n")).collect()
}

fn apply(root: &Path, operations: &[Value]) -> Value {
    let output = ara(root)
        .args(["apply", "-", "--json", "--no-duplicate-check"])
        .write_stdin(ndjson(operations))
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty(), "{output:?}");
    serde_json::from_slice(&output.stdout).unwrap()
}

fn log(root: &Path, timestamp: &str, summary: &str) {
    apply(
        root,
        &[json!({"op":"session.log","timestamp":timestamp,"summary":summary})],
    );
}

fn open_row(root: &Path, id: &str) -> Value {
    let output = ara(root)
        .args(["open", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty(), "{output:?}");
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["format"], "ara.open/v1");
    report["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == id)
        .unwrap_or_else(|| panic!("{report}"))
        .clone()
}

fn artifact_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, relative: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(root.join(relative)).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == ".ara" {
                continue;
            }
            let path = relative.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(path.clone(), fs::read(root.join(path)).unwrap());
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, Path::new(""), &mut files);
    files
}

fn stale_operations() -> [Value; 2] {
    [
        json!({"op":"session.log","timestamp":"2026-10-05T10:00:00Z","summary":"Caller stale decision"}),
        json!({"op":"observation.mark_stale","observation":"O95","reason":"Caller considers the topic abandoned","audit":{"signal":"user-directive","provenance":"user"}}),
    ]
}

fn assert_stale_refused(root: &Path, codes: &[&str]) {
    let before = artifact_bytes(root);
    let output = ara(root)
        .args(["apply", "-", "--json", "--no-duplicate-check"])
        .write_stdin(ndjson(&stale_operations()))
        .assert()
        .code(1)
        .stdout("")
        .get_output()
        .clone();
    let failure: Value = serde_json::from_slice(&output.stderr).unwrap();
    let error = &failure["error"];
    assert!(
        codes.contains(&error["code"].as_str().unwrap()),
        "{failure}"
    );
    if matches!(
        error["code"].as_str(),
        Some("write.observation" | "write.stale_history_unknown")
    ) {
        assert_eq!(error["line"], 2, "{failure}");
        assert_eq!(error["details"]["field"], "session_days", "{failure}");
    }
    assert_eq!(
        artifact_bytes(root),
        before,
        "refusal must preserve all native bytes"
    );
}

fn inactivity_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    fixture(
        dir.path(),
        "tree:\n  - id: N01\n    type: question\n    title: Local boundary\n    provenance: user\n",
    );
    observation(dir.path(), "O95", &[]);
    for date in ["2026-10-02", "2026-10-03", "2026-10-04"] {
        log(dir.path(), &format!("{date}T10:00:00Z"), "Unrelated work");
    }
    dir
}

fn alias_fixture(anchor_value: &str, excerpt: &str) -> TempDir {
    let generated = inactivity_fixture();
    let dir = TempDir::new().unwrap();
    // Seed native source without a pending journal tied to the generated bytes.
    for (path, bytes) in artifact_bytes(generated.path()) {
        write(dir.path(), path.to_str().unwrap(), bytes);
    }
    let source = fs::read_to_string(dir.path().join(RECENT_SESSION)).unwrap();
    let source = source.replace(
        "key_context: []",
        &format!("key_context:\n  - turn: 1\n    excerpt: {excerpt}"),
    );
    assert!(
        source.contains("excerpt:"),
        "session fixture must add a real turn row"
    );
    write(
        dir.path(),
        RECENT_SESSION,
        format!("recent: &recent {anchor_value}\n{source}"),
    );
    dir
}

fn assert_recent_reference_or_unknown(row: &Value) {
    if row["history_status"] == "complete" {
        assert_eq!(row["last_reference_turn"], "2026-10-04_001#1", "{row}");
        assert_eq!(row["last_reference_date"], "2026-10-04", "{row}");
        assert_eq!(row["reference_basis"], "literal", "{row}");
        assert_eq!(row["turns_since_reference"], 0, "{row}");
        assert_eq!(row["session_days_since_reference"], 0, "{row}");
        assert!(
            row["evidence_sources"].as_array().unwrap().iter().any(|e| {
                e["source"] == RECENT_SESSION
                    && e["field"] == "key_context[0].excerpt"
                    && e["target"] == "O95"
            }),
            "{row}"
        );
    } else {
        assert_unknown_history(row);
    }
}

fn assert_unknown_history(row: &Value) {
    assert_ne!(row["history_status"], "complete", "{row}");
    assert_eq!(row["turns_since_reference"], Value::Null, "{row}");
    assert_eq!(row["session_days_since_reference"], Value::Null, "{row}");
    assert!(
        !row["history_diagnostics"].as_array().unwrap().is_empty(),
        "{row}"
    );
}

#[test]
fn yaml_alias_reference_is_counted_or_reported_unknown_in_open() {
    let dir = alias_fixture("\"Revisited O95\"", "*recent");
    assert_recent_reference_or_unknown(&open_row(dir.path(), "O95"));
}

#[test]
fn yaml_alias_recent_reference_refuses_next_day_stale_without_writing() {
    let dir = alias_fixture("\"Revisited O95\"", "*recent");
    assert_stale_refused(
        dir.path(),
        &["write.observation", "write.stale_history_unknown"],
    );
}

#[test]
fn scoped_and_path_tokens_do_not_reset_local_observation_inactivity() {
    for text in ["Revisited peer:O95", "Read results/O95.csv"] {
        let dir = inactivity_fixture();
        log(dir.path(), "2026-10-04T12:00:00Z", text);
        let row = open_row(dir.path(), "O95");
        assert_eq!(row["history_status"], "complete", "{text}: {row}");
        assert_eq!(row["turns_since_reference"], 4, "{text}: {row}");
        assert_eq!(row["session_days_since_reference"], 3, "{text}: {row}");
        assert_eq!(row["last_reference_date"], "2026-10-01", "{text}: {row}");
        assert_eq!(row["reference_basis"], "staging_timestamp", "{text}: {row}");
        assert_eq!(row["evidence_sources"], json!([]), "{text}: {row}");
        apply(dir.path(), &stale_operations());
        assert_eq!(open_row(dir.path(), "O95")["stale"], true);
    }
}

#[test]
fn ordinary_local_tokens_reset_inactivity_and_refuse_stale() {
    let dir = inactivity_fixture();
    log(
        dir.path(),
        "2026-10-04T12:00:00Z",
        "Revisited (O95). Next check is pending.",
    );
    let row = open_row(dir.path(), "O95");
    assert_eq!(row["history_status"], "complete", "{row}");
    assert_eq!(row["turns_since_reference"], 0, "{row}");
    assert_eq!(row["session_days_since_reference"], 0, "{row}");
    assert_eq!(row["last_reference_turn"], "2026-10-04_001#2", "{row}");
    assert_stale_refused(dir.path(), &["write.observation"]);
}

struct ImportedFixture {
    _parent: TempDir,
    ours: PathBuf,
}

fn imported_fixture() -> ImportedFixture {
    let parent = TempDir::new().unwrap();
    let [base, ours, theirs] = ["base", "ours", "theirs"].map(|name| parent.path().join(name));
    fixture(&base, "tree: []\n");
    fixture(
        &ours,
        "tree:\n  - id: N01\n    type: question\n    title: Independent local node\n    provenance: user\n",
    );
    fixture(
        &theirs,
        "tree:\n  - id: N01\n    type: question\n    title: Independent peer node\n    provenance: user\n",
    );
    observation(&ours, "O95", &["N01"]);
    observation(&theirs, "O94", &["N01"]);
    for date in ["2026-10-02", "2026-10-03"] {
        log(&ours, &format!("{date}T10:00:00Z"), "Unrelated local work");
    }
    apply(
        &theirs,
        &[
            json!({"op":"session.log","timestamp":"2026-10-04T10:00:00Z","summary":"Peer revisited N01","key_context":[{"excerpt":"Revisited N01"}]}),
        ],
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
            "history-peer",
            "--json",
            "--no-duplicate-check",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty(), "{output:?}");
    let resolved = ara(&ours)
        .args(["resolve", "peer:N01", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let resolved: Value = serde_json::from_slice(&resolved.stdout).unwrap();
    assert_eq!(resolved["format"], "ara.resolve/v1");
    assert_eq!(resolved["id"], "N02", "{resolved}");
    let imported = yaml(&ours, RECENT_SESSION);
    assert_eq!(imported["key_context"][0]["excerpt"], "Revisited N02");
    let reasoning = yaml(&ours, REASONING);
    assert!(
        reasoning["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| {
                entry["turn"] == "2026-10-04_001#1"
                    && entry["session_metadata"]["after"]["summary"] == "Peer revisited N01"
            }),
        "imported archived summary must keep its historical spelling: {reasoning}"
    );
    ImportedFixture {
        _parent: parent,
        ours,
    }
}

fn yaml(root: &Path, path: &str) -> Value {
    let text = fs::read_to_string(root.join(path)).unwrap();
    ara_core::write::positions::YamlDocument::parse(&text)
        .unwrap()
        .root
        .to_json()
        .unwrap()
}

fn peer_observation(root: &Path) -> String {
    let output = ara(root)
        .args(["resolve", "peer:O94", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let resolved: Value = serde_json::from_slice(&output.stdout).unwrap();
    resolved["id"].as_str().unwrap().to_owned()
}

#[test]
fn imported_relocated_literal_and_preserved_archive_keep_peer_identity() {
    let fixture = imported_fixture();
    let local = open_row(&fixture.ours, "O95");
    assert_eq!(local["session_days_since_reference"], 3, "{local}");
    assert_eq!(local["last_reference_date"], "2026-10-01", "{local}");
    let peer = open_row(&fixture.ours, &peer_observation(&fixture.ours));
    assert_eq!(peer["history_status"], "complete", "{peer}");
    assert_eq!(peer["last_reference_turn"], "2026-10-04_001#1", "{peer}");
    assert_eq!(peer["session_days_since_reference"], 0, "{peer}");
    assert!(
        peer["evidence_sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| {
                e["literal"] == "N01"
                    && e["target"] == "N02"
                    && e["source"] == REASONING
                    && e["resolved_via"] == "alias:history-peer:N01"
            }),
        "preserved imported archive must still follow authenticated aliases: {peer}"
    );
    assert!(
        peer["evidence_sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| {
                e["literal"] == "N02"
                    && e["target"] == "N02"
                    && e["field"] == "key_context[0].excerpt"
                    && e["status"] == "attributed"
            }),
        "already relocated literal must not be remapped a second time: {peer}"
    );
}

fn native_continuation(root: &Path) {
    apply(
        root,
        &[
            json!({"op":"session.log","session":"2026-10-04_001","timestamp":"2026-10-04T12:00:00Z","summary":"Revisited N01","key_context":[{"excerpt":"Native checked N01"}]}),
        ],
    );
}

#[test]
fn native_continuation_of_imported_session_uses_local_literal_identity_in_open() {
    let fixture = imported_fixture();
    native_continuation(&fixture.ours);
    let local = open_row(&fixture.ours, "O95");
    assert_eq!(local["history_status"], "complete", "{local}");
    assert_eq!(local["last_reference_turn"], "2026-10-04_001#2", "{local}");
    assert_eq!(local["turns_since_reference"], 0, "{local}");
    assert_eq!(local["session_days_since_reference"], 0, "{local}");
    assert!(
        local["evidence_sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| {
                e["literal"] == "N01"
                    && e["target"] == "N01"
                    && e["turn"] == 2
                    && e.get("resolved_via").is_none()
            }),
        "native occurrence has no peer redirect: {local}"
    );
    let peer = open_row(&fixture.ours, &peer_observation(&fixture.ours));
    assert_eq!(peer["last_reference_turn"], "2026-10-04_001#1", "{peer}");
    assert_eq!(peer["turns_since_reference"], 1, "{peer}");
    assert!(
        peer["evidence_sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["turn"] != 2),
        "native N01 must not count as imported N02 activity: {peer}"
    );
}

#[test]
fn native_continuation_of_imported_session_refuses_local_stale_without_writing() {
    let fixture = imported_fixture();
    native_continuation(&fixture.ours);
    assert_stale_refused(&fixture.ours, &["write.observation"]);
}

#[test]
fn aliased_reasoning_containers_refuse_stale_without_writing() {
    for kind in ["entry", "entries", "metadata", "after"] {
        let generated = inactivity_fixture();
        let dir = TempDir::new().unwrap();
        for (path, bytes) in artifact_bytes(generated.path()) {
            write(dir.path(), path.to_str().unwrap(), bytes);
        }
        let mut reasoning = yaml(dir.path(), REASONING);
        let recent = json!({"turn":"2026-10-04_001#1","notes":["Revisited O95"]});
        let entries = reasoning["entries"].as_array_mut().unwrap();
        let anchor = match kind {
            "entry" => {
                entries.push(json!("REFERENCE_ALIAS"));
                recent
            }
            "entries" => {
                entries.push(recent);
                let anchor = json!(entries);
                reasoning["entries"] = json!("REFERENCE_ALIAS");
                anchor
            }
            "metadata" | "after" => {
                let metadata = &mut entries.last_mut().unwrap()["session_metadata"];
                metadata["after"]["summary"] = json!("Revisited O95");
                let selected = if kind == "metadata" {
                    metadata
                } else {
                    &mut metadata["after"]
                };
                let anchor = selected.clone();
                *selected = json!("REFERENCE_ALIAS");
                anchor
            }
            _ => unreachable!(),
        };
        let source = format!(
            "recent: &recent\n  {}\n{}\n",
            ara_core::write::source::render_yaml(&anchor, 2, "\n"),
            ara_core::write::source::render_yaml(&reasoning, 0, "\n")
                .replace("\"REFERENCE_ALIAS\"", "*recent")
                .replace("REFERENCE_ALIAS", "*recent")
        );
        write(dir.path(), REASONING, source);
        let result = open_row(dir.path(), "O95");
        assert_unknown_history(&result);
        assert!(
            result["history_diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["code"] == "history.reference_alias"),
            "{kind}: {result}"
        );
        assert_stale_refused(
            dir.path(),
            &["write.stale_history_unknown", "write.unsupported_source"],
        );
    }
}

#[test]
fn imported_occurrence_mapping_must_match_layer_and_document() {
    for (field, replacement) in [
        ("layer", "annotation_occurrence"),
        ("path", "logic/claims.md"),
    ] {
        let fixture = imported_fixture();
        let mut ledger = yaml(&fixture.ours, "trace/merge_log.yaml");
        let revision = ledger["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["kind"] == "revision")
            .unwrap();
        let mapping = revision["mappings"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|m| m["layer"] == "reasoning" && m["path"] == REASONING)
            .unwrap();
        mapping[field] = json!(replacement);
        write(
            &fixture.ours,
            "trace/merge_log.yaml",
            ara_core::write::source::render_yaml(&ledger, 0, "\n"),
        );
        let local = open_row(&fixture.ours, "O95");
        assert_unknown_history(&local);
        assert!(
            local["history_diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["code"] == "history.origin_unknown"),
            "{field}: {local}"
        );
    }
}

#[test]
fn imported_literal_with_colliding_session_reference_keeps_complete_history() {
    let parent = TempDir::new().unwrap();
    let [base, ours, theirs] = ["base", "ours", "theirs"].map(|name| parent.path().join(name));
    fixture(&base, "tree: []\n");
    fixture(
        &ours,
        "tree:\n  - id: N01\n    type: question\n    title: Independent local node\n    provenance: user\n",
    );
    fixture(
        &theirs,
        "tree:\n  - id: N01\n    type: question\n    title: Independent peer node\n    provenance: user\n",
    );
    observation(&ours, "O95", &["N01"]);
    observation(&theirs, "O94", &["N01"]);
    for date in ["2026-10-02", "2026-10-03", "2026-10-04"] {
        log(&ours, &format!("{date}T08:00:00Z"), "Unrelated local work");
    }
    apply(
        &theirs,
        &[
            json!({"op":"session.log","timestamp":"2026-10-04T10:00:00Z","summary":"Peer revisited N01","key_context":[{"excerpt":"Revisited N01 during 2026-10-04_001#1"}]}),
        ],
    );
    ara(&ours)
        .args(["merge", "--base"])
        .arg(&base)
        .arg("--theirs")
        .arg(&theirs)
        .args([
            "--as",
            "peer",
            "--source-key",
            "history-peer",
            "--json",
            "--no-duplicate-check",
        ])
        .assert()
        .success();
    let output = ara(&ours)
        .args(["resolve", "peer:2026-10-04_001", "--json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let resolved: Value = serde_json::from_slice(&output.stdout).unwrap();
    let imported_session = resolved["id"].as_str().unwrap();
    assert_ne!(imported_session, "2026-10-04_001");
    let imported = yaml(&ours, &format!("trace/sessions/{imported_session}.yaml"));
    assert_eq!(
        imported["key_context"][0]["excerpt"],
        format!("Revisited N02 during {imported_session}#1")
    );
    let peer = open_row(&ours, &peer_observation(&ours));
    assert_eq!(peer["history_status"], "complete", "{peer}");
    assert_eq!(peer["last_reference_turn"], format!("{imported_session}#1"));
    assert_eq!(peer["session_days_since_reference"], 0);
    let local = open_row(&ours, "O95");
    assert_eq!(local["history_status"], "complete", "{local}");
    assert_eq!(local["session_days_since_reference"], 3);
}
