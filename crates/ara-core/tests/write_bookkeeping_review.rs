//! PR #115: Body Status transitions and distinct same-turn structural actions.
#![cfg(all(feature = "native", unix))]

use ara_core::write::{self, ApplyMode, WriteError, positions::YamlDocument, source};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::TempDir;

const TIME: &str = "2026-10-05T09:00:00Z";
const SESSION: &str = "2026-10-05_001";
const SESSION_PATH: &str = "trace/sessions/2026-10-05_001.yaml";

fn put(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn body(statement: &str, status: &str) -> String {
    format!(
        "- **Statement**: {statement}\n- **Conditions**: Applicable conditions\n- **Status**: {status}\n- **Provenance**: user\n- **Falsification**: Counterexample\n\n"
    )
}

fn fixture(status: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    put(
        root,
        "PAPER.md",
        "---\ntitle: Review bookkeeping\n---\n# Review bookkeeping\n",
    );
    put(root, "trace/exploration_tree.yaml", "tree: []\n");
    let mut claims = format!(
        "# Claims\n\n## C01: Primary\n{}",
        body("Original statement", status)
    );
    for (id, title) in [("C17", "Source"), ("C18", "Spin-off"), ("C19", "Survivor")] {
        claims.push_str(&format!("## {id}: {title}\n{}", body(title, "hypothesis")));
    }
    put(root, "logic/claims.md", &claims);
    let session = json!({"session":{"id":SESSION,"date":"2026-10-05","started":"2026-10-05T08:00:00Z","last_turn":"2026-10-05T08:00:00Z","turn_count":1,"summary":"Morning"},"events_logged":[],"ai_actions":[],"claims_touched":[],"logic_revisions":[],"key_context":[],"open_threads":[],"ai_suggestions_pending":[]});
    put(
        root,
        SESSION_PATH,
        &(source::render_yaml(&session, 0, "\n") + "\n"),
    );
    let index = json!({"sessions":[{"id":SESSION,"date":"2026-10-05","summary":"Morning","turn_count":1,"events_count":0,"claims_touched":[],"open_threads":0}]});
    put(
        root,
        "trace/sessions/session_index.yaml",
        &(source::render_yaml(&index, 0, "\n") + "\n"),
    );
    dir
}

fn run(root: &Path, lines: &[Value], mode: ApplyMode) -> Result<(), WriteError> {
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    let ops = write::batch::parse_batch(text.as_bytes()).unwrap();
    write::execute_at(root, &ops, mode, || Ok(TIME.to_owned()), |_, _| ()).map(|_| ())
}

fn commit(root: &Path, lines: &[Value]) {
    run(root, lines, ApplyMode::Commit).unwrap();
}

fn bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if relative == ".ara" || relative == ".gitignore" {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                result.insert(relative, fs::read(path).unwrap());
            }
        }
    }
    result
}

fn reject(root: &Path, lines: &[Value]) -> WriteError {
    let before = bytes(root);
    let error = run(root, lines, ApplyMode::Commit).unwrap_err();
    assert_eq!(bytes(root), before, "judgment conflict is atomic");
    let dry = run(root, lines, ApplyMode::DryRun).unwrap_err();
    assert_eq!(
        (&dry.code, dry.line, &dry.field),
        (&error.code, error.line, &error.field)
    );
    error
}

fn session(root: &Path) -> Value {
    YamlDocument::parse(&fs::read_to_string(root.join(SESSION_PATH)).unwrap())
        .unwrap()
        .root
        .to_json()
        .unwrap()
}

fn log(touches: Value) -> Value {
    json!({"op":"session.log","summary":"Review regression turn","claims_touched":touches})
}

fn revise(id: &str, set: Value) -> Value {
    json!({"op":"logic.revise","target":{"id":id},"set":set,"signal":"empirical-resolution","provenance":"user"})
}

fn body_revision(root: &Path, replacement: &str) -> Value {
    let text = fs::read_to_string(root.join("logic/claims.md")).unwrap();
    let range = write::documents::body_range(&text, &["C01: Primary".into()]).unwrap();
    json!({"op":"logic.revise","target":{"document":"logic/claims.md","entry":"C01"},"set":{"Body":replacement},"expected":source::digest(text[range].as_bytes()),"signal":"empirical-resolution","provenance":"user"})
}

fn split(statement: &str) -> Value {
    let mut op = revise("C17", json!({"Statement":statement}));
    op["action"] = json!("split");
    op["split_into"] = json!([{"id":"C18"}]);
    op["references"] = json!([]);
    op
}

fn merge(statement: &str) -> Value {
    let mut op = revise(
        "C17",
        json!({"Statement":statement,"Status":"withdrawn","Merged into":"C19"}),
    );
    op["rewrite_references"] = json!(true);
    op
}

fn touches(actions: &[&str]) -> Value {
    Value::Array(
        actions
            .iter()
            .map(|action| json!({"id":"C17","action":action,"turn":2}))
            .collect(),
    )
}

#[test]
fn selected_claim_body_status_change_rejects_contradictory_judgment() {
    for (before, after, judgment) in [
        ("supported", "\n  refuted", "confirmed"),
        ("\n  refuted", "supported", "refuted"),
    ] {
        let dir = fixture(before);
        let error = reject(
            dir.path(),
            &[
                log(json!([{"id":"C01","action":judgment}])),
                body_revision(dir.path(), &body("Changed statement", after)),
            ],
        );
        assert_eq!(error.code, "write.claim_touch_conflict");
        assert_eq!(
            (error.line, error.field.as_deref(), error.related_line()),
            (Some(1), Some("claims_touched[0]"), Some(2))
        );
    }
}

#[test]
fn selected_claim_body_status_change_accepts_matching_decoded_judgment() {
    for (before, after, judgment) in [
        ("supported", "\n  refuted", "refuted"),
        ("\n  refuted", "supported", "confirmed"),
    ] {
        let dir = fixture(before);
        let replacement = body("Changed statement", after);
        commit(
            dir.path(),
            &[
                log(json!([{"id":"C01","action":judgment}])),
                body_revision(dir.path(), &replacement),
            ],
        );
        let record = session(dir.path());
        assert_eq!(
            record["claims_touched"],
            json!([{"id":"C01","action":judgment,"turn":2}])
        );
        assert_eq!(record["logic_revisions"][0]["field"], "Body");
        assert!(
            record["logic_revisions"][0]["after"]
                .as_str()
                .unwrap()
                .contains("Changed statement")
        );
    }
}

#[test]
fn selected_claim_body_unchanged_decoded_status_does_not_constrain_judgment() {
    for (before, after) in [
        ("supported", "\n  supported"),
        ("\n  supported", "supported"),
    ] {
        let dir = fixture(before);
        commit(
            dir.path(),
            &[
                log(json!([{"id":"C01","action":"refuted"}])),
                body_revision(dir.path(), &body("Changed statement", after)),
            ],
        );
        assert_eq!(
            session(dir.path())["claims_touched"],
            json!([{"id":"C01","action":"refuted","turn":2}])
        );
    }
}

#[test]
fn selected_claim_body_status_change_without_judgment_derives_only_revised() {
    let dir = fixture("supported");
    commit(
        dir.path(),
        &[
            log(json!([])),
            body_revision(dir.path(), &body("Changed statement", "refuted")),
        ],
    );
    assert_eq!(
        session(dir.path())["claims_touched"],
        json!([{"id":"C01","action":"revised","turn":2}])
    );
}

#[test]
fn same_turn_split_then_merge_preserves_both_structural_actions() {
    let dir = fixture("supported");
    commit(
        dir.path(),
        &[
            log(json!([])),
            split("Split source"),
            merge("Merged source"),
        ],
    );
    assert_eq!(
        session(dir.path())["claims_touched"],
        touches(&["split", "merged"])
    );
}

#[test]
fn same_turn_merge_then_split_preserves_structural_operation_order() {
    let dir = fixture("supported");
    commit(
        dir.path(),
        &[
            log(json!([])),
            merge("Merged source"),
            split("Split source after merge"),
        ],
    );
    assert_eq!(
        session(dir.path())["claims_touched"],
        touches(&["merged", "split"])
    );
}

#[test]
fn repeated_structural_action_deduplicates_and_supersedes_generic_revised() {
    let dir = fixture("supported");
    commit(
        dir.path(),
        &[
            log(json!([])),
            revise("C17", json!({"Conditions":"Before structural actions"})),
            split("First split"),
            revise("C17", json!({"Conditions":"Between structural actions"})),
            split("Second split"),
            merge("First merge"),
            merge("Second merge"),
        ],
    );
    assert_eq!(
        session(dir.path())["claims_touched"],
        touches(&["split", "merged"])
    );
}

#[test]
fn explicit_structural_actions_keep_distinct_actions_and_deduplicate_repeats() {
    let dir = fixture("supported");
    commit(
        dir.path(),
        &[
            log(
                json!([{"id":"C17","action":"split"},{"id":"C17","action":"merged"},{"id":"C17","action":"split"}]),
            ),
            split("Split source"),
            merge("Merged source"),
        ],
    );
    assert_eq!(
        session(dir.path())["claims_touched"],
        touches(&["split", "merged"])
    );
}

#[test]
fn selected_claim_body_descendant_status_does_not_constrain_judgment() {
    let dir = fixture("supported");
    let replacement = format!(
        "{}### Supporting detail\n- **Status**: refuted\n",
        body("Changed statement", "supported"),
    );
    commit(
        dir.path(),
        &[
            log(json!([{"id":"C01","action":"refuted"}])),
            body_revision(dir.path(), &replacement),
        ],
    );
    assert_eq!(
        session(dir.path())["claims_touched"],
        json!([{"id":"C01","action":"refuted","turn":2}])
    );
}

#[test]
fn structural_actions_use_their_first_operation_order_across_claims() {
    let dir = fixture("supported");
    commit(
        dir.path(),
        &[
            log(json!([])),
            revise("C17", json!({"Conditions":"Before structural actions"})),
            revise("C01", json!({"Statement":"Intervening revision"})),
            split("First split"),
            merge("First merge"),
            split("Repeated split"),
        ],
    );
    assert_eq!(
        session(dir.path())["claims_touched"],
        json!([
            {"id":"C01","action":"revised","turn":2},
            {"id":"C17","action":"split","turn":2},
            {"id":"C17","action":"merged","turn":2},
        ])
    );
}
