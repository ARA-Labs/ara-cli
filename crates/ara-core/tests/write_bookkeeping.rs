//! Plan 19 A4/A5: operation-derived `events_logged` and `claims_touched`
//! rows. Facts come from successful operations; caller rows keep their
//! summaries and judgments; conflicts reject the whole batch.
#![cfg(all(feature = "native", unix))]

use ara_core::write::{
    self, ApplyMode, ArtifactLock, ArtifactSnapshot, WorkingArtifact, WriteError, WriteReport,
    positions::YamlDocument,
    source::render_yaml,
    transaction::{self, TransactionBoundary, TransactionHooks, TransactionObserver},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const TIME: &str = "2026-10-05T09:00:00Z";
const SESSION: &str = "2026-10-05_001";
const SESSION_PATH: &str = "trace/sessions/2026-10-05_001.yaml";
const TREE: &str = "tree:\n  - id: N01\n    type: experiment\n    title: Existing experiment\n    provenance: ai-executed\n    result: Recorded result\n    timestamp: '2026-09-01T10:00:00Z'\n";
const CLAIMS: &str = "# Claims\n\n## C01: Existing\n- **Statement**: Existing statement\n- **Conditions**: Existing conditions\n- **Status**: hypothesis\n- **Provenance**: user\n- **Falsification**: Counterexample\n\n## C02: Survivor\n- **Statement**: Survivor statement\n- **Conditions**: Survivor conditions\n- **Status**: supported\n- **Provenance**: user\n- **Falsification**: Counterexample\n";

/// An artifact with one open session (`2026-10-05_001`, one turn at 08:00Z)
/// unless `session` is false.
fn fixture(session: bool) -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    put(
        root,
        "PAPER.md",
        "---\ntitle: Bookkeeping\n---\n# Bookkeeping\n",
    );
    put(root, "trace/exploration_tree.yaml", TREE);
    put(root, "logic/claims.md", CLAIMS);
    if session {
        let value = json!({"session":{"id":SESSION,"date":"2026-10-05","started":"2026-10-05T08:00:00Z","last_turn":"2026-10-05T08:00:00Z","turn_count":1,"summary":"Morning"},"events_logged":[],"ai_actions":[],"claims_touched":[],"logic_revisions":[],"key_context":[],"open_threads":[],"ai_suggestions_pending":[]});
        put(root, SESSION_PATH, &(render_yaml(&value, 0, "\n") + "\n"));
        let index = json!({"sessions":[{"id":SESSION,"date":"2026-10-05","summary":"Morning","turn_count":1,"events_count":0,"claims_touched":[],"open_threads":0}]});
        put(
            root,
            "trace/sessions/session_index.yaml",
            &(render_yaml(&index, 0, "\n") + "\n"),
        );
    }
    dir
}
fn put(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn ops(lines: &[Value]) -> Vec<write::WriteOperation> {
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    write::batch::parse_batch(text.as_bytes()).unwrap()
}
fn run(root: &Path, lines: &[Value], mode: ApplyMode) -> Result<WriteReport, WriteError> {
    write::execute_at(root, &ops(lines), mode, || Ok(TIME.to_owned()), |_, _| ()).map(|(r, ())| r)
}
fn commit(root: &Path, lines: &[Value]) -> WriteReport {
    run(root, lines, ApplyMode::Commit).unwrap()
}
/// Reject, and prove the rejected batch left every source byte alone.
fn reject(root: &Path, lines: &[Value]) -> WriteError {
    let before = tree(root);
    let error = run(root, lines, ApplyMode::Commit).unwrap_err();
    assert_eq!(tree(root), before, "a rejected batch leaves every byte");
    let dry = run(root, lines, ApplyMode::DryRun).unwrap_err();
    assert_eq!(
        (&dry.code, dry.line, &dry.field),
        (&error.code, error.line, &error.field),
        "dry run plans the same rejection"
    );
    error
}
fn yaml(root: &Path, path: &str) -> Value {
    YamlDocument::parse(&fs::read_to_string(root.join(path)).unwrap())
        .unwrap()
        .root
        .to_json()
        .unwrap()
}
fn session(root: &Path) -> Value {
    yaml(root, SESSION_PATH)
}
fn tree(root: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
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
                files.insert(format!("{relative}/"), None);
                pending.push(path);
            } else {
                files.insert(relative, Some(fs::read(&path).unwrap()));
            }
        }
    }
    files
}
fn log(extra: Value) -> Value {
    let mut value = json!({"op":"session.log","summary":"Caller turn summary"});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    value
}
fn node(provenance: Option<&str>) -> Value {
    let mut fields = json!({"description":"Asked"});
    if let Some(provenance) = provenance {
        fields["provenance"] = json!(provenance);
    }
    json!({"op":"node.add","type":"question","parent":"N01","title":"New question","fields":fields})
}
fn stage(content: &str) -> Value {
    json!({"op":"observation.stage","content":content,"potential_type":"unknown","provenance":"user"})
}
fn claim() -> Value {
    json!({"op":"claim.add","title":"Fresh claim","fields":{"Statement":"S","Conditions":"C","Status":"hypothesis","Provenance":"ai-suggested","Falsification":"F"}})
}
fn revise(claim: &str, set: Value) -> Value {
    json!({"op":"logic.revise","target":{"id":claim},"set":set,"signal":"empirical-resolution","provenance":"user"})
}
fn rows(value: &Value, key: &str) -> Vec<Value> {
    value[key].as_array().cloned().unwrap_or_default()
}

#[test]
fn sole_log_derives_events_and_claim_touches_from_operation_facts() {
    let dir = fixture(true);
    let root = dir.path();
    let content = "Complete observation text\n  with its own indentation, never truncated";
    commit(
        root,
        &[
            log(json!({})),
            node(Some("ai-executed")),
            stage(content),
            claim(),
            json!({"op":"heuristic.add","title":"Fresh heuristic","fields":{"Rationale":"R","Sensitivity":"low","Code ref":["x"],"Provenance":"user"}}),
            revise("C01", json!({"Statement":"Revised statement"})),
        ],
    );
    let record = session(root);
    assert_eq!(
        rows(&record, "events_logged"),
        vec![
            json!({"type":"question","id":"N02","routing":"direct","provenance":"ai-executed","summary":"New question","turn":2}),
            json!({"type":"observation","id":"O01","routing":"staged","provenance":"user","summary":content,"turn":2}),
            json!({"type":"claim","id":"C03","routing":"direct","provenance":"ai-suggested","summary":"Fresh claim","turn":2}),
            json!({"type":"heuristic","id":"H01","routing":"direct","provenance":"user","summary":"Fresh heuristic","turn":2}),
        ]
    );
    assert_eq!(
        rows(&record, "claims_touched"),
        vec![
            json!({"id":"C03","action":"created","turn":2}),
            json!({"id":"C01","action":"revised","turn":2}),
        ]
    );
    let index = &yaml(root, "trace/sessions/session_index.yaml")["sessions"][0];
    assert_eq!(index["events_count"], 4);
    assert_eq!(index["claims_touched"], json!(["C01", "C03"]));
}

#[test]
fn every_promotion_destination_derives_one_crystallized_event() {
    let cases = [
        (
            "claim",
            json!({"Statement":"S","Conditions":"C","Status":"hypothesis","Falsification":"F"}),
            "C03",
            None,
        ),
        (
            "heuristic",
            json!({"Rationale":"R","Sensitivity":"low","Code ref":["x"]}),
            "H01",
            None,
        ),
        (
            "dead_end",
            json!({"hypothesis":"H","failure_mode":"F","lesson":"L"}),
            "N02",
            None,
        ),
        (
            "concept",
            json!({"Definition":"D"}),
            "O01",
            Some("logic/concepts.md"),
        ),
        (
            "constraint",
            json!({"Constraint":"C"}),
            "O01",
            Some("logic/solution/constraints.md"),
        ),
        (
            "architecture",
            json!({"Architecture":"A"}),
            "O01",
            Some("logic/solution/architecture.md"),
        ),
    ];
    for (to, fields, id, document) in cases {
        let dir = fixture(true);
        let root = dir.path();
        // Staged earlier without a turn: a standalone write creates no session row.
        commit(root, &[stage("Seen earlier")]);
        assert!(rows(&session(root), "events_logged").is_empty());
        commit(
            root,
            &[
                log(json!({})),
                json!({"op":"observation.promote","observation":"O01","to":to,"title":"Finding","fields":fields,"signal":"empirical-resolution"}),
            ],
        );
        let record = session(root);
        let mut expected = json!({"type":to,"id":id,"routing":"crystallized","provenance":"user","summary":"Finding","turn":2});
        if let Some(document) = document {
            expected["target"] = json!({"document":document,"heading":["Finding"]});
        }
        assert_eq!(rows(&record, "events_logged"), vec![expected], "{to}");
        let touched = if to == "claim" {
            vec![json!({"id":"C03","action":"crystallized","turn":2})]
        } else {
            Vec::new()
        };
        assert_eq!(rows(&record, "claims_touched"), touched, "{to}");
    }
}

#[test]
fn explicit_rows_keep_caller_summaries_and_suppress_matching_derived_rows() {
    let dir = fixture(true);
    let root = dir.path();
    commit(
        root,
        &[
            log(json!({
                "events":[
                    {"type":"question","id":"N02","routing":"direct","provenance":"ai-executed","summary":"Caller-written summary"},
                    {"type":"question","id":"N02","routing":"direct","provenance":"ai-executed","summary":"Caller-written summary"},
                    {"type":"experiment","id":"N01","routing":"direct","provenance":"ai-executed","summary":"Unrelated existing node"}
                ],
                "claims_touched":[{"id":"C03","action":"created"},{"id":"C03","action":"created"},{"id":"C01","action":"advanced"}],
                "key_context":[{"excerpt":"kept"}],
                "open_threads":["kept thread"]
            })),
            node(Some("ai-executed")),
            stage("Missing from the caller rows"),
            claim(),
            revise("C01", json!({"Status":"testing"})),
        ],
    );
    let record = session(root);
    assert_eq!(
        rows(&record, "events_logged"),
        vec![
            json!({"type":"question","id":"N02","routing":"direct","provenance":"ai-executed","summary":"Caller-written summary","turn":2}),
            json!({"type":"experiment","id":"N01","routing":"direct","provenance":"ai-executed","summary":"Unrelated existing node","turn":2}),
            json!({"type":"observation","id":"O01","routing":"staged","provenance":"user","summary":"Missing from the caller rows","turn":2}),
            json!({"type":"claim","id":"C03","routing":"direct","provenance":"ai-suggested","summary":"Fresh claim","turn":2}),
        ],
        "explicit rows first and verbatim, one copy; remaining facts follow in operation order"
    );
    assert_eq!(
        rows(&record, "claims_touched"),
        vec![
            json!({"id":"C03","action":"created","turn":2}),
            json!({"id":"C01","action":"advanced","turn":2}),
        ],
        "the caller's judgment replaces the generic revised row"
    );
    assert_eq!(record["key_context"][0]["excerpt"], "kept");
    assert_eq!(record["open_threads"], json!(["kept thread"]));
}

#[test]
fn explicit_and_omitted_requests_choose_equivalent_rows() {
    let omitted = fixture(true);
    commit(
        omitted.path(),
        &[
            log(json!({})),
            node(Some("user")),
            claim(),
            revise("C01", json!({"Statement":"Same"})),
        ],
    );
    let explicit = fixture(true);
    commit(
        explicit.path(),
        &[
            log(json!({
                "session":SESSION,
                "timestamp":TIME,
                "events":[
                    {"type":"question","id":"N02","routing":"direct","provenance":"user","summary":"New question"},
                    {"type":"claim","id":"C03","routing":"direct","provenance":"ai-suggested","summary":"Fresh claim"}
                ],
                "claims_touched":[{"id":"C03","action":"created"},{"id":"C01","action":"revised"}]
            })),
            node(Some("user")),
            claim(),
            json!({"op":"logic.revise","target":{"id":"C01"},"set":{"Statement":"Same"},"session":SESSION,"turn":2,"signal":"empirical-resolution","provenance":"user"}),
        ],
    );
    assert_eq!(
        tree(omitted.path()),
        tree(explicit.path()),
        "omitted defaults and an explicit request naming the same values write the same bytes"
    );
}

#[test]
fn derivation_needs_a_session_log_and_skips_no_ops() {
    let dir = fixture(true);
    let root = dir.path();
    let before = fs::read(root.join(SESSION_PATH)).unwrap();
    // Standalone writes never create or append a turn.
    commit(root, &[node(None), claim()]);
    assert_eq!(fs::read(root.join(SESSION_PATH)).unwrap(), before);
    // A logged turn whose only mutation is a no-op gets no derived rows.
    commit(
        root,
        &[
            log(json!({})),
            revise("C01", json!({"Statement":"Existing statement"})),
        ],
    );
    let record = session(root);
    assert_eq!(record["session"]["turn_count"], 2);
    assert!(rows(&record, "events_logged").is_empty());
    assert!(rows(&record, "claims_touched").is_empty());
    assert!(rows(&record, "logic_revisions").is_empty());
}

#[test]
fn missing_provenance_requires_a_value_or_an_explicit_row() {
    let dir = fixture(true);
    let root = dir.path();
    let error = reject(root, &[log(json!({})), node(None)]);
    assert_eq!(error.code, "write.event_provenance");
    assert_eq!(error.line, Some(2));
    assert_eq!(error.field.as_deref(), Some("fields.provenance"));
    assert_eq!(error.exit_code(), 1);
    let error = reject(
        root,
        &[
            log(json!({})),
            json!({"op":"heuristic.add","title":"H","fields":{"Rationale":"R","Sensitivity":"low","Code ref":["x"]}}),
        ],
    );
    assert_eq!(error.code, "write.event_provenance");
    assert_eq!(error.field.as_deref(), Some("fields.Provenance"));
    // An explicit row supplies the provenance the operation lacks.
    commit(
        root,
        &[
            log(
                json!({"events":[{"type":"question","id":"N02","routing":"direct","provenance":"user","summary":"Logged by caller"}]}),
            ),
            node(None),
        ],
    );
    assert_eq!(
        rows(&session(root), "events_logged"),
        vec![
            json!({"type":"question","id":"N02","routing":"direct","provenance":"user","summary":"Logged by caller","turn":2})
        ]
    );
}

#[test]
fn conflicting_rows_reject_with_both_input_locations() {
    let dir = fixture(true);
    let root = dir.path();
    let row = |summary: &str| json!({"type":"question","id":"N02","routing":"direct","provenance":"user","summary":summary});
    // Two different explicit rows for one identity.
    let error = reject(
        root,
        &[
            log(json!({"events":[row("First"), row("Second")]})),
            node(Some("user")),
        ],
    );
    assert_eq!(error.code, "write.event_conflict");
    assert_eq!(
        (error.line, error.field.as_deref()),
        (Some(1), Some("events[1]"))
    );
    assert_eq!(
        (error.related_line(), error.related_field()),
        (Some(1), Some("events[0]"))
    );
    // A row for the new node that relabels its routing.
    let error = reject(
        root,
        &[
            node(Some("user")),
            log(
                json!({"events":[{"type":"question","id":"N02","routing":"staged","provenance":"user","summary":"x"}]}),
            ),
        ],
    );
    assert_eq!(error.code, "write.event_conflict");
    assert_eq!(
        (error.line, error.field.as_deref()),
        (Some(2), Some("events[0]"))
    );
    assert_eq!(
        error.related_line(),
        Some(1),
        "names the creating operation"
    );
    // Matching identity with different provenance or type.
    for row in [
        json!({"type":"question","id":"N02","routing":"direct","provenance":"ai-executed","summary":"x"}),
        json!({"type":"decision","id":"N02","routing":"direct","provenance":"user","summary":"x"}),
    ] {
        let error = reject(root, &[log(json!({"events":[row]})), node(Some("user"))]);
        assert_eq!(error.code, "write.event_conflict");
        assert_eq!(error.related_line(), Some(2));
    }
    // A named-section promotion row without its target cannot stand in for it.
    commit(root, &[stage("Seen")]);
    let error = reject(
        root,
        &[
            log(
                json!({"events":[{"type":"concept","id":"O01","routing":"crystallized","provenance":"user","summary":"x"}]}),
            ),
            json!({"op":"observation.promote","observation":"O01","to":"concept","title":"Idea","fields":{"Definition":"D"},"signal":"empirical-resolution"}),
        ],
    );
    assert_eq!(error.code, "write.event_conflict");
    assert!(error.message.contains("target"), "{}", error.message);
    // Staged and promoted in one batch: the error names the promotion.
    let error = reject(
        root,
        &[
            log(
                json!({"events":[{"type":"concept","id":"O02","routing":"crystallized","provenance":"user","summary":"x"}]}),
            ),
            stage("Seen now"),
            json!({"op":"observation.promote","observation":"O02","to":"concept","title":"Now","fields":{"Definition":"D"},"signal":"empirical-resolution"}),
        ],
    );
    assert_eq!(error.code, "write.event_conflict");
    assert_eq!(error.related_line(), Some(3), "{}", error.message);
    assert!(error.message.contains("crystallized"), "{}", error.message);
    // With the exact target the row matches and its summary is kept.
    commit(
        root,
        &[
            log(
                json!({"events":[{"type":"concept","id":"O01","routing":"crystallized","provenance":"user","summary":"Caller words","target":{"document":"logic/concepts.md","heading":["Idea"]}}]}),
            ),
            json!({"op":"observation.promote","observation":"O01","to":"concept","title":"Idea","fields":{"Definition":"D"},"signal":"empirical-resolution"}),
        ],
    );
    let events = rows(&session(root), "events_logged");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["summary"], "Caller words");
}

#[test]
fn claim_touch_labels_must_match_their_operation() {
    let dir = fixture(true);
    let root = dir.path();
    let error = reject(
        root,
        &[
            log(json!({"claims_touched":[{"id":"C03","action":"crystallized"}]})),
            claim(),
        ],
    );
    assert_eq!(error.code, "write.claim_touch_conflict");
    assert_eq!(
        (error.line, error.field.as_deref(), error.related_line()),
        (Some(1), Some("claims_touched[0]"), Some(2))
    );
    let error = reject(
        root,
        &[
            log(json!({"claims_touched":[{"id":"C01","action":"created"}]})),
            revise("C01", json!({"Statement":"Changed"})),
        ],
    );
    assert_eq!(error.code, "write.claim_touch_conflict");
    commit(root, &[stage("Promote me")]);
    let error = reject(
        root,
        &[
            log(json!({"claims_touched":[{"id":"C03","action":"created"}]})),
            json!({"op":"observation.promote","observation":"O01","to":"claim","title":"P","fields":{"Statement":"S","Conditions":"C","Status":"hypothesis","Falsification":"F"},"signal":"empirical-resolution"}),
        ],
    );
    assert_eq!(error.code, "write.claim_touch_conflict");
}

#[test]
fn judgments_replace_revised_after_status_consistency_checks() {
    let status = |value: &str| revise("C01", json!({"Status":value}));
    // No judgment: a Status change to supported is only `revised`.
    let dir = fixture(true);
    commit(dir.path(), &[log(json!({})), status("supported")]);
    assert_eq!(
        rows(&session(dir.path()), "claims_touched"),
        vec![json!({"id":"C01","action":"revised","turn":2})]
    );
    // A matching judgment replaces it.
    let dir = fixture(true);
    commit(
        dir.path(),
        &[
            log(json!({"claims_touched":[{"id":"C01","action":"confirmed"}]})),
            status("supported"),
        ],
    );
    assert_eq!(
        rows(&session(dir.path()), "claims_touched"),
        vec![json!({"id":"C01","action":"confirmed","turn":2})]
    );
    // A judgment contradicting the explicit Status change rejects.
    let dir = fixture(true);
    let error = reject(
        dir.path(),
        &[
            log(json!({"claims_touched":[{"id":"C01","action":"refuted"}]})),
            status("supported"),
        ],
    );
    assert_eq!(error.code, "write.claim_touch_conflict");
    assert_eq!(error.related_line(), Some(2));
    // Contradictory judgments need distinct supporting transitions.
    let error = reject(
        dir.path(),
        &[
            log(
                json!({"claims_touched":[{"id":"C01","action":"confirmed"},{"id":"C01","action":"refuted"}]}),
            ),
            status("supported"),
        ],
    );
    assert_eq!(error.code, "write.claim_touch_conflict");
    commit(
        dir.path(),
        &[
            log(
                json!({"claims_touched":[{"id":"C01","action":"confirmed"},{"id":"C01","action":"refuted"}]}),
            ),
            status("supported"),
            status("refuted"),
        ],
    );
    assert_eq!(rows(&session(dir.path()), "claims_touched").len(), 2);
    // Without a Status change only the vocabulary applies; the stored Status
    // (hypothesis) is never consulted.
    let dir = fixture(true);
    commit(
        dir.path(),
        &[
            log(json!({"claims_touched":[{"id":"C01","action":"withdrawn"}]})),
            revise("C01", json!({"Statement":"Changed"})),
        ],
    );
    assert_eq!(
        rows(&session(dir.path()), "claims_touched"),
        vec![json!({"id":"C01","action":"withdrawn","turn":2})]
    );
    // `advanced` and `weakened` are never compared with Status values.
    let dir = fixture(true);
    commit(
        dir.path(),
        &[
            log(
                json!({"claims_touched":[{"id":"C01","action":"weakened"},{"id":"C01","action":"advanced"}]}),
            ),
            status("supported"),
        ],
    );
    assert_eq!(rows(&session(dir.path()), "claims_touched").len(), 2);
    // `withdrawn` against an explicit change to another Status rejects.
    let dir = fixture(true);
    let error = reject(
        dir.path(),
        &[
            log(json!({"claims_touched":[{"id":"C01","action":"withdrawn"}]})),
            status("testing"),
        ],
    );
    assert_eq!(error.code, "write.claim_touch_conflict");
    // confirmed + refuted without any Status change still rejects.
    let error = reject(
        dir.path(),
        &[
            log(
                json!({"claims_touched":[{"id":"C01","action":"confirmed"},{"id":"C01","action":"refuted"}]}),
            ),
            revise("C01", json!({"Statement":"Changed"})),
        ],
    );
    assert_eq!(error.code, "write.claim_touch_conflict");
    assert_eq!(
        (error.field.as_deref(), error.related_field()),
        (Some("claims_touched[1]"), Some("claims_touched[0]"))
    );
    // Judgments for claims this batch does not change keep existing checks only.
    commit(
        dir.path(),
        &[log(
            json!({"claims_touched":[{"id":"C02","action":"weakened"}]}),
        )],
    );
}

#[test]
fn a_merge_revision_derives_revised_and_accepts_a_caller_merged_judgment() {
    let dir = fixture(true);
    let root = dir.path();
    let merge = revise("C01", json!({"Status":"withdrawn","Merged into":"C02"}));
    commit(root, &[log(json!({})), merge.clone()]);
    assert_eq!(
        rows(&session(root), "claims_touched"),
        vec![json!({"id":"C01","action":"revised","turn":2})],
        "merged and split belong to explicit merge/split operations"
    );
    let dir = fixture(true);
    let root = dir.path();
    commit(
        root,
        &[
            log(json!({"claims_touched":[{"id":"C01","action":"merged"}]})),
            merge,
        ],
    );
    assert_eq!(
        rows(&session(root), "claims_touched"),
        vec![json!({"id":"C01","action":"merged","turn":2})],
        "the caller judgment replaces the generic revised row"
    );
}

#[test]
fn several_logs_need_explicit_attribution() {
    let dir = fixture(true);
    let root = dir.path();
    let first = json!({"op":"session.log","session":SESSION,"timestamp":TIME,"summary":"Turn two"});
    let second =
        json!({"op":"session.log","session":SESSION,"timestamp":TIME,"summary":"Turn three"});
    let error = reject(root, &[first.clone(), second.clone(), node(Some("user"))]);
    assert_eq!(error.code, "write.owner_ambiguous");
    assert_eq!(error.line, Some(3));
    // A row in both logs is ambiguous too, naming both.
    let mut a = first.clone();
    let mut b = second.clone();
    let row = json!([{"type":"question","id":"N02","routing":"direct","provenance":"user","summary":"x"}]);
    a["events"] = row.clone();
    b["events"] = row;
    let error = reject(root, &[a, b, node(Some("user"))]);
    assert_eq!(error.code, "write.owner_ambiguous");
    assert_eq!((error.line, error.related_line()), (Some(2), Some(1)));
}

#[test]
fn several_logs_attribute_each_row_to_its_own_turn() {
    let dir = fixture(true);
    let root = dir.path();
    commit(
        root,
        &[
            json!({"op":"session.log","session":SESSION,"timestamp":TIME,"summary":"Turn two","events":[{"type":"question","id":"N02","routing":"direct","provenance":"user","summary":"Turn two node"}]}),
            json!({"op":"session.log","session":SESSION,"timestamp":TIME,"summary":"Turn three","events":[{"type":"observation","id":"O01","routing":"staged","provenance":"user","summary":"Turn three observation"}]}),
            node(Some("user")),
            stage("Staged"),
            json!({"op":"logic.revise","target":{"id":"C01"},"set":{"Statement":"Turn two change"},"session":SESSION,"turn":2,"signal":"empirical-resolution","provenance":"user"}),
        ],
    );
    let record = session(root);
    let events = rows(&record, "events_logged");
    assert_eq!(events.len(), 2);
    assert_eq!(
        (events[0]["id"].clone(), events[0]["turn"].clone()),
        (json!("N02"), json!(2))
    );
    assert_eq!(
        (events[1]["id"].clone(), events[1]["turn"].clone()),
        (json!("O01"), json!(3))
    );
    assert_eq!(
        rows(&record, "claims_touched"),
        vec![json!({"id":"C01","action":"revised","turn":2})],
        "the derived row is inserted in its own turn's place"
    );
}

#[test]
fn explicit_event_targets_must_resolve_the_promotion_tuple() {
    let dir = fixture(true);
    let root = dir.path();
    commit(
        root,
        &[
            stage("Seen"),
            json!({"op":"observation.promote","observation":"O01","to":"concept","title":"Idea","fields":{"Definition":"D"},"signal":"empirical-resolution"}),
            stage("Seen too"),
            json!({"op":"observation.promote","observation":"O02","to":"concept","title":"Other","fields":{"Definition":"D"},"signal":"empirical-resolution"}),
        ],
    );
    let row = |heading: &str| {
        log(
            json!({"events":[{"type":"concept","id":"O01","routing":"crystallized","provenance":"user","summary":"Earlier promotion","target":{"document":"logic/concepts.md","heading":[heading]}}]}),
        )
    };
    let error = reject(root, &[row("Other")]);
    assert_eq!(error.code, "write.event_target");
    assert_eq!(
        (error.line, error.field.as_deref()),
        (Some(1), Some("events[0].target"))
    );
    let error = reject(
        root,
        &[log(
            json!({"events":[{"type":"concept","id":"O01","routing":"staged","provenance":"user","summary":"x","target":{"document":"logic/concepts.md","heading":["Idea"]}}]}),
        )],
    );
    assert_eq!(error.code, "write.event_target");
    commit(root, &[row("Idea")]);
    assert_eq!(
        rows(&session(root), "events_logged")[0]["target"],
        json!({"document":"logic/concepts.md","heading":["Idea"]})
    );
}

#[test]
fn derived_rows_roll_back_with_the_batch() {
    let dir = fixture(true);
    let root = dir.path();
    let lines = [log(json!({})), node(Some("user")), claim()];
    // Dry run plans the rows but persists nothing and takes no lock.
    let before = tree(root);
    run(root, &lines, ApplyMode::DryRun).unwrap();
    assert_eq!(tree(root), before);
    assert!(!root.join(".ara").exists());
    // A later failure after derivation would have run leaves every byte.
    let error = reject(
        root,
        &[
            log(json!({})),
            node(Some("user")),
            json!({"op":"node.add","type":"question","parent":"N999","title":"Bad","fields":{"description":"d","provenance":"user"}}),
        ],
    );
    assert_eq!(error.line, Some(3));
    // A commit fault after the derived rows are planned restores the originals.
    struct Observer;
    impl TransactionObserver for Observer {
        fn prepared(&mut self, _: &WorkingArtifact) -> Result<(), WriteError> {
            Ok(())
        }
        fn completed(&mut self) -> Result<(), WriteError> {
            Ok(())
        }
        fn rolled_back(&mut self) -> Result<(), WriteError> {
            Ok(())
        }
    }
    struct Fault;
    impl TransactionHooks for Fault {
        fn boundary(&mut self, kind: TransactionBoundary, path: &str) -> Result<(), WriteError> {
            if kind == TransactionBoundary::AfterRename
                && path == "trace/sessions/session_index.yaml"
            {
                return Err(WriteError::io(format!("injected {path}")));
            }
            Ok(())
        }
    }
    let _lock = ArtifactLock::acquire(root).unwrap();
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(root).unwrap());
    working.batch_time = Some(TIME.into());
    write::batch::plan_batch(&mut working, &ops(&lines)).unwrap();
    let candidate = working.text(SESSION_PATH).unwrap().to_owned();
    assert!(candidate.contains("New question") && candidate.contains("created"));
    working.validate().unwrap();
    working.plan_missing_directories().unwrap();
    let error = transaction::commit_with_hooks(&working, &mut Observer, &mut Fault).unwrap_err();
    assert!(error.message.contains("injected"));
    assert_eq!(tree(root), before);
}
