//! Plan 19 A0-A3: one locked batch clock, explicit-owner defaulting, and
//! session selection or creation for an omitted-`session` log. Exact-time
//! cases run through `execute_at` with a fixed clock.
#![cfg(all(feature = "native", unix))]

use ara_core::write::{
    self, ApplyMode, ArtifactLock, ArtifactSnapshot, WorkingArtifact, WriteError, WriteReport,
    journal::{self, DurableJournal},
    positions::YamlDocument,
    source::{digest, render_yaml},
    transaction::{self, TransactionBoundary, TransactionHooks, TransactionObserver},
};
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const MIDNIGHT: &str = "2026-10-05T00:00:00Z";
const TREE: &str = "tree:\n  - id: N01\n    type: experiment\n    title: Existing experiment\n    provenance: ai-executed\n    result: Recorded result\n    timestamp: '2026-09-01T10:00:00Z'\n";
const CLAIMS: &str = "# Claims\n\n## C01: Existing\n- **Statement**: Existing statement\n- **Conditions**: Existing conditions\n- **Status**: hypothesis\n- **Provenance**: user\n- **Falsification**: Counterexample\n";

#[derive(Clone, Copy)]
struct Session<'a> {
    id: &'a str,
    last: &'a str,
    turns: u64,
    closed: bool,
}
fn open(id: &str, turns: u64) -> Session<'_> {
    Session {
        id,
        last: "",
        turns,
        closed: false,
    }
}

fn fixture(sessions: &[Session<'_>]) -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    for (path, text) in [
        (
            "PAPER.md",
            "---\ntitle: Owner fixture\n---\n# Owner fixture\n",
        ),
        ("trace/exploration_tree.yaml", TREE),
        ("logic/claims.md", CLAIMS),
    ] {
        put(root, path, text);
    }
    let mut rows = Vec::new();
    for session in sessions {
        let date = &session.id[..10];
        let started = format!("{date}T08:00:00Z");
        let last = if session.last.is_empty() {
            started.clone()
        } else {
            session.last.to_owned()
        };
        let mut metadata = json!({"id":session.id,"date":date,"started":started,"last_turn":last,"turn_count":session.turns,"summary":format!("Summary {}", session.id)});
        if session.closed {
            metadata["closed"] = json!(true);
        }
        let value = json!({"session":metadata,"events_logged":[],"ai_actions":[],"claims_touched":[],"logic_revisions":[],"key_context":[],"open_threads":[],"ai_suggestions_pending":[]});
        put(
            root,
            &format!("trace/sessions/{}.yaml", session.id),
            &(render_yaml(&value, 0, "\n") + "\n"),
        );
        rows.push(json!({"id":session.id,"date":date,"summary":format!("Summary {}", session.id),"turn_count":session.turns,"events_count":0,"claims_touched":[],"open_threads":0}));
    }
    if !sessions.is_empty() {
        put(
            root,
            "trace/sessions/session_index.yaml",
            &(render_yaml(&json!({ "sessions": rows }), 0, "\n") + "\n"),
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
fn run_at(
    root: &Path,
    lines: &[Value],
    mode: ApplyMode,
    time: &str,
) -> Result<WriteReport, WriteError> {
    write::execute_at(root, &ops(lines), mode, || Ok(time.to_owned()), |_, _| ()).map(|(r, ())| r)
}
fn commit(root: &Path, lines: &[Value], time: &str) -> WriteReport {
    run_at(root, lines, ApplyMode::Commit, time).unwrap()
}
fn reject(root: &Path, lines: &[Value], time: &str) -> WriteError {
    let before = tree(root);
    let error = run_at(root, lines, ApplyMode::Commit, time).unwrap_err();
    assert_eq!(
        tree(root),
        before,
        "a rejected batch leaves every source byte"
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
/// Complete knowledge-source bytes and directory existence (not `.ara/`).
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
fn log(summary: &str) -> Value {
    json!({"op":"session.log","summary":summary})
}
fn revise(statement: &str) -> Value {
    json!({"op":"logic.revise","target":{"id":"C01"},"set":{"Statement":statement},"signal":"user-directive","provenance":"user"})
}

#[test]
fn every_omitted_value_in_one_batch_uses_the_single_locked_clock_read() {
    let dir = fixture(&[]);
    let root = dir.path();
    let reads = Cell::new(0);
    let lines = [
        log("Caller summary of this turn"),
        json!({"op":"node.add","id":"$q","type":"question","parent":"root","title":"New question","fields":{"description":"Asked","provenance":"ai-executed"}}),
        json!({"op":"observation.stage","id":"$o","content":"Seen","potential_type":"unknown","provenance":"user"}),
        json!({"op":"observation.stage","id":"$dead","content":"Failed","potential_type":"unknown","provenance":"user"}),
        json!({"op":"observation.promote","observation":"$dead","to":"dead_end","title":"Dead end","fields":{"hypothesis":"H","failure_mode":"F","lesson":"L"},"signal":"empirical-resolution"}),
        json!({"op":"record.append","document":"trace/taste_log.yaml","record":{"target":"N01","tag":"endorse","object":"claim","comment":"Caller taste"}}),
        json!({"op":"entry.taste_append","target":{"id":"C01"},"record":{"tag":"endorse","object":"claim","comment":"Caller confirmed"}}),
        revise("Revised statement"),
        json!({"op":"record.append","document":"trace/pm_reasoning_log.yaml","record":{"notes":["Caller reasoning"]}}),
    ];
    let report = write::execute_at(
        root,
        &ops(&lines),
        ApplyMode::Commit,
        || {
            reads.set(reads.get() + 1);
            Ok(MIDNIGHT.to_owned())
        },
        |_, _| (),
    )
    .unwrap()
    .0;
    assert_eq!(reads.get(), 1, "the clock is read exactly once per batch");
    let anchor = &report.operations[0];
    assert_eq!(anchor.id.as_deref(), Some("2026-10-05_001"));
    assert_eq!(anchor.turn, Some(1));
    assert!(anchor.session_created);
    assert!(report.open_sessions.is_empty());
    let session = yaml(root, "trace/sessions/2026-10-05_001.yaml");
    assert_eq!(session["session"]["started"], MIDNIGHT);
    assert_eq!(session["session"]["last_turn"], MIDNIGHT);
    assert_eq!(session["session"]["date"], "2026-10-05");
    assert_eq!(session["session"]["summary"], "Caller summary of this turn");
    assert_eq!(session["logic_revisions"][0]["turn"], 1);
    assert_eq!(session["logic_revisions"][0]["after"], "Revised statement");
    let nodes = yaml(root, "trace/exploration_tree.yaml")["tree"].clone();
    assert_eq!(
        nodes[0]["timestamp"], "2026-09-01T10:00:00Z",
        "explicit stays exact"
    );
    assert_eq!(nodes[1]["timestamp"], MIDNIGHT);
    assert_eq!(nodes[2]["type"], "dead_end");
    assert_eq!(nodes[2]["timestamp"], MIDNIGHT, "promotion-created node");
    let observations = yaml(root, "staging/observations.yaml")["observations"].clone();
    assert_eq!(observations[0]["timestamp"], MIDNIGHT);
    assert_eq!(observations[1]["timestamp"], MIDNIGHT);
    assert_eq!(
        yaml(root, "trace/taste_log.yaml")["entries"][0]["timestamp"],
        MIDNIGHT
    );
    let claims = fs::read_to_string(root.join("logic/claims.md")).unwrap();
    assert!(claims.contains("[2026-10-05] `endorse` on `claim` — Caller confirmed"));
    assert!(claims.contains("2026-10-05 (2026-10-05_001#1)"), "{claims}");
    let reasoning = yaml(root, "trace/pm_reasoning_log.yaml")["entries"].clone();
    assert!(
        reasoning
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["turn"] == "2026-10-05_001#1"
                && entry["notes"] == json!(["Caller reasoning"]))
    );
    let revise_result = &report.operations[7];
    assert_eq!(revise_result.session.as_deref(), Some("2026-10-05_001"));
    assert_eq!(revise_result.turn, Some(1));
}

#[test]
fn explicit_values_and_offsets_stay_exact_and_select_by_written_date() {
    let dir = fixture(&[open("2026-10-04_001", 1)]);
    let root = dir.path();
    // 23:30 at UTC-5 is 04:30Z on 10-05; the written date is still 10-04.
    let report = commit(
        root,
        &[
            json!({"op":"session.log","timestamp":"2026-10-04T23:30:00-05:00","summary":"Late local work"}),
            json!({"op":"node.add","type":"question","parent":"root","title":"Explicit","fields":{"description":"d","provenance":"user","timestamp":"2026-10-04T23:31:00-05:00"}}),
            revise("Offset revision"),
        ],
        "2026-10-05T04:30:00Z",
    );
    assert_eq!(report.operations[0].id.as_deref(), Some("2026-10-04_001"));
    assert_eq!(report.operations[0].turn, Some(2));
    assert!(!report.operations[0].session_created);
    let session = yaml(root, "trace/sessions/2026-10-04_001.yaml");
    assert_eq!(session["session"]["last_turn"], "2026-10-04T23:30:00-05:00");
    assert_eq!(
        yaml(root, "trace/exploration_tree.yaml")["tree"][1]["timestamp"],
        "2026-10-04T23:31:00-05:00"
    );
    assert!(!root.join("trace/sessions/2026-10-05_001.yaml").exists());
    // Monotonicity compares instants: 23:59:59Z precedes 23:30-05:00.
    let error = reject(root, &[log("Earlier instant")], "2026-10-04T23:59:59Z");
    assert!(
        error.message.contains("precedes the previous turn"),
        "{}",
        error.message
    );
    // One second before midnight UTC selects that day's open session.
    let dir = fixture(&[open("2026-10-04_001", 1)]);
    let report = commit(
        dir.path(),
        &[log("Before midnight")],
        "2026-10-04T23:59:59Z",
    );
    assert_eq!(report.operations[0].id.as_deref(), Some("2026-10-04_001"));
    assert_eq!(report.operations[0].turn, Some(2));
}

#[test]
fn past_midnight_creates_today_and_reports_yesterday_still_open() {
    let dir = fixture(&[open("2026-10-04_001", 2)]);
    let root = dir.path();
    let yesterday = fs::read(root.join("trace/sessions/2026-10-04_001.yaml")).unwrap();
    let report = commit(
        root,
        &[log("Work after midnight"), revise("After midnight")],
        MIDNIGHT,
    );
    assert_eq!(report.operations[0].id.as_deref(), Some("2026-10-05_001"));
    assert!(report.operations[0].session_created);
    assert_eq!(report.open_sessions, vec!["2026-10-04_001".to_owned()]);
    assert_eq!(
        fs::read(root.join("trace/sessions/2026-10-04_001.yaml")).unwrap(),
        yesterday,
        "yesterday's session is neither closed, merged nor selected"
    );
    let index = yaml(root, "trace/sessions/session_index.yaml")["sessions"].clone();
    assert_eq!(index.as_array().unwrap().len(), 2);
    let serialized = serde_json::to_value(&report).unwrap();
    assert_eq!(serialized["open_sessions"], json!(["2026-10-04_001"]));
}

#[test]
fn explicit_past_session_with_omitted_timestamp_explains_the_date_mismatch() {
    let dir = fixture(&[open("2026-10-04_001", 1)]);
    let error = reject(
        dir.path(),
        &[json!({"op":"session.log","session":"2026-10-04_001","summary":"Continue"})],
        MIDNIGHT,
    );
    assert_eq!(error.code, "write.session");
    assert_eq!(error.line, Some(1));
    assert_eq!(error.field.as_deref(), Some("timestamp"));
    assert!(error.message.contains("2026-10-05"), "{}", error.message);
    assert!(
        error.message.contains("2026-10-04_001 is dated 2026-10-04"),
        "{}",
        error.message
    );
    // Naming the session with a date-compatible timestamp continues it.
    let report = commit(
        dir.path(),
        &[
            json!({"op":"session.log","session":"2026-10-04_001","timestamp":"2026-10-04T23:00:00Z"}),
        ],
        MIDNIGHT,
    );
    assert_eq!(report.operations[0].turn, Some(2));
    // An explicit timestamp earlier than the last turn is refused, not backdated.
    let error = reject(
        dir.path(),
        &[
            json!({"op":"session.log","session":"2026-10-04_001","timestamp":"2026-10-04T22:00:00Z"}),
        ],
        MIDNIGHT,
    );
    assert!(error.message.contains("precedes the previous turn"));
}

#[test]
fn zero_one_and_several_open_candidates_and_closed_sessions() {
    // One open candidate is selected.
    let dir = fixture(&[open("2026-10-05_001", 3)]);
    let report = commit(dir.path(), &[log("Continue")], "2026-10-05T09:00:00Z");
    assert_eq!(report.operations[0].id.as_deref(), Some("2026-10-05_001"));
    assert_eq!(report.operations[0].turn, Some(4));
    assert_eq!(
        report.operations[0].session.as_deref(),
        Some("2026-10-05_001")
    );
    // Several open candidates: refuse and list them.
    let dir = fixture(&[open("2026-10-05_001", 1), open("2026-10-05_002", 1)]);
    let error = reject(dir.path(), &[log("Which one")], "2026-10-05T09:00:00Z");
    assert_eq!(error.code, "write.session_ambiguous");
    assert_eq!(error.field.as_deref(), Some("session"));
    assert_eq!(error.line, Some(1));
    assert!(error.message.contains("2026-10-05_001, 2026-10-05_002"));
    // A closed session on the date is never selected or reopened.
    let closed = Session {
        id: "2026-10-05_001",
        last: "",
        turns: 2,
        closed: true,
    };
    let dir = fixture(&[closed]);
    let before = fs::read(dir.path().join("trace/sessions/2026-10-05_001.yaml")).unwrap();
    let report = commit(dir.path(), &[log("Fresh")], "2026-10-05T09:00:00Z");
    assert_eq!(report.operations[0].id.as_deref(), Some("2026-10-05_002"));
    assert!(report.operations[0].session_created);
    assert!(report.open_sessions.is_empty());
    assert_eq!(
        fs::read(dir.path().join("trace/sessions/2026-10-05_001.yaml")).unwrap(),
        before
    );
    let error = reject(
        dir.path(),
        &[json!({"op":"session.log","session":"2026-10-05_001","summary":"Reopen?"})],
        "2026-10-05T09:00:00Z",
    );
    assert_eq!(error.message, "closed sessions are immutable");
}

#[test]
fn turn_and_sequence_overflow_fail_closed() {
    let dir = fixture(&[open("2026-10-05_001", u64::MAX)]);
    let error = reject(dir.path(), &[log("Overflow")], "2026-10-05T09:00:00Z");
    assert_eq!(error.message, "turn count overflow");
    let exhausted = Session {
        id: "2026-10-05_999",
        last: "",
        turns: 1,
        closed: true,
    };
    let dir = fixture(&[exhausted]);
    let error = reject(dir.path(), &[log("No sequence left")], MIDNIGHT);
    assert_eq!(error.message, "session sequence exhausted");
    assert!(
        !dir.path()
            .join("trace/sessions/2026-10-05_1000.yaml")
            .exists()
    );
}

#[test]
fn omitted_context_requires_exactly_one_summarized_preceding_anchor() {
    let dir = fixture(&[open("2026-10-05_001", 1)]);
    let root = dir.path();
    let time = "2026-10-05T09:00:00Z";
    let error = reject(root, &[revise("No anchor")], time);
    assert_eq!(
        (error.code.as_str(), error.line),
        ("write.owner_required", Some(1))
    );
    let error = reject(
        root,
        &[
            json!({"op":"session.log","session":"2026-10-05_001","summary":"One"}),
            json!({"op":"session.log","session":"2026-10-05_001","summary":"Two"}),
            revise("Ambiguous"),
        ],
        time,
    );
    assert_eq!(
        (error.code.as_str(), error.line),
        ("write.owner_ambiguous", Some(3))
    );
    let error = reject(root, &[log("One"), log("Two")], time);
    assert_eq!(
        (error.code.as_str(), error.line),
        ("write.owner_ambiguous", Some(1))
    );
    let error = reject(root, &[json!({"op":"session.log"}), revise("x")], time);
    assert_eq!(
        (error.code.as_str(), error.line, error.field.as_deref()),
        ("write.owner_summary", Some(1), Some("summary"))
    );
    let error = reject(
        root,
        &[
            json!({"op":"session.log","session":"2026-10-05_001","summary":"  "}),
            revise("x"),
        ],
        time,
    );
    assert_eq!(error.code, "write.owner_summary");
    let error = reject(root, &[revise("Before anchor"), log("Late anchor")], time);
    assert_eq!(
        (error.code.as_str(), error.line),
        ("write.owner_order", Some(1))
    );
    let mut mismatched = revise("Wrong turn");
    mismatched["turn"] = json!(7);
    let error = reject(root, &[log("Anchor"), mismatched], time);
    assert_eq!(
        (error.code.as_str(), error.line, error.field.as_deref()),
        ("write.owner_mismatch", Some(2), Some("turn"))
    );
    let mut historical = revise("Historical turn");
    historical["turn"] = json!(1);
    let error = reject(root, &[log("Anchor"), historical], time);
    assert_eq!(error.code, "write.owner_mismatch");
    let mut matching = revise("Matching explicit session");
    matching["session"] = json!("2026-10-05_001");
    let report = commit(root, &[log("Anchor"), matching], time);
    assert_eq!(report.operations[1].turn, Some(2));
}

#[test]
fn bindings_keep_their_line_and_kind_and_an_earlier_start_can_supply_the_anchor() {
    let dir = fixture(&[]);
    let root = dir.path();
    let error = reject(
        root,
        &[
            json!({"op":"session.log","session":"$s","summary":"Forward"}),
            revise("x"),
            json!({"op":"session.start","id":"$s","summary":"Later"}),
        ],
        MIDNIGHT,
    );
    assert_eq!(
        (error.code.as_str(), error.line, error.field.as_deref()),
        ("write.binding_unknown", Some(1), Some("session"))
    );
    let error = reject(
        root,
        &[
            json!({"op":"node.add","id":"$n","type":"question","parent":"root","title":"q","fields":{"description":"d"}}),
            json!({"op":"session.log","session":"$n","summary":"Wrong kind"}),
            revise("x"),
        ],
        MIDNIGHT,
    );
    assert_eq!(
        (error.code.as_str(), error.line),
        ("write.binding_kind", Some(2))
    );
    let report = commit(
        root,
        &[
            json!({"op":"session.start","id":"$s","summary":"Explicit start"}),
            json!({"op":"session.log","session":"$s","summary":"Anchor"}),
            revise("Anchored to bound session"),
        ],
        MIDNIGHT,
    );
    assert_eq!(report.bindings["$s"], "2026-10-05_001");
    assert_eq!(report.operations[2].turn, Some(1));
    let session = yaml(root, "trace/sessions/2026-10-05_001.yaml");
    assert_eq!(session["session"]["started"], MIDNIGHT);
    assert_eq!(session["session"]["summary"], "Anchor");
}

#[test]
fn explicit_multi_log_batches_keep_order_flexibility() {
    let dir = fixture(&[open("2026-10-05_001", 1), open("2026-10-05_002", 1)]);
    let mut first = revise("Explicit before its log");
    first["session"] = json!("2026-10-05_002");
    first["turn"] = json!(2);
    let report = commit(
        dir.path(),
        &[
            first,
            json!({"op":"session.log","session":"2026-10-05_001","timestamp":"2026-10-05T09:00:00Z"}),
            json!({"op":"session.log","session":"2026-10-05_002","timestamp":"2026-10-05T09:01:00Z"}),
        ],
        MIDNIGHT,
    );
    assert_eq!(report.operations[0].turn, Some(2));
    assert!(report.open_sessions.is_empty());
    let second = yaml(dir.path(), "trace/sessions/2026-10-05_002.yaml");
    assert_eq!(second["logic_revisions"][0]["turn"], 2);
    // Without explicit ownership a revision cannot pick a log by proximity.
    let error = reject(
        dir.path(),
        &[
            json!({"op":"session.log","session":"2026-10-05_001","timestamp":"2026-10-05T10:00:00Z"}),
            revise("Which log?"),
            json!({"op":"session.log","session":"2026-10-05_002","timestamp":"2026-10-05T10:01:00Z"}),
        ],
        MIDNIGHT,
    );
    assert_eq!(
        (error.code.as_str(), error.line),
        ("write.owner_ambiguous", Some(2))
    );
}

#[test]
fn every_audited_omission_scope_attaches_to_the_anchor() {
    let concepts = "# Concepts\n\n## Original concept\n- **Definition**: Synthetic definition\n\n## Obsolete concept\n- **Definition**: Unused definition\n";
    let closed = |id| Session {
        id,
        last: "",
        turns: 1,
        closed: true,
    };
    let dir = fixture(&[
        closed("2026-10-02_001"),
        closed("2026-10-03_001"),
        closed("2026-10-04_001"),
    ]);
    let root = dir.path();
    put(root, "logic/concepts.md", concepts);
    put(
        root,
        "staging/observations.yaml",
        &(render_yaml(
            &json!({"observations":[{"id":"O01","timestamp":"2026-10-01T09:00:00Z","provenance":"user","content":"Old","potential_type":"unknown","bound_to":[],"promoted":false,"promoted_to":null,"crystallized_via":null,"stale":false}]}),
            0,
            "\n",
        ) + "\n"),
    );
    let snapshot = WorkingArtifact::new(ArtifactSnapshot::load(root).unwrap());
    let section_digest = |heading: &str| {
        let target: write::EntrySelector = serde_json::from_value(
            json!({"document":"logic/concepts.md","heading":["Concepts",heading]}),
        )
        .unwrap();
        let entry = write::logic::resolve(&snapshot, &target).unwrap();
        digest(snapshot.text("logic/concepts.md").unwrap()[entry.range].as_bytes())
    };
    let report = commit(
        root,
        &[
            log("Audited omissions"),
            json!({"op":"entry.rename","target":{"document":"logic/concepts.md","heading":["Concepts","Original concept"]},"name":"Renamed concept","expected":section_digest("Original concept"),"signal":"terminology-drift","provenance":"user"}),
            json!({"op":"entry.remove","target":{"document":"logic/concepts.md","heading":["Concepts","Obsolete concept"]},"expected":section_digest("Obsolete concept"),"signal":"user-directive","provenance":"user"}),
            json!({"op":"paper.edit","frontmatter":{"title":"Renamed fixture"},"audit":{"signal":"user-directive","provenance":"user"}}),
            json!({"op":"observation.mark_stale","observation":"O01","session_days":["2026-10-02","2026-10-03","2026-10-04"],"reason":"Caller abandoned the topic","audit":{"signal":"user-directive","provenance":"user"}}),
        ],
        MIDNIGHT,
    );
    for result in &report.operations[1..] {
        assert_eq!(
            result.session.as_deref(),
            Some("2026-10-05_001"),
            "{result:?}"
        );
        assert_eq!(result.turn, Some(1), "{result:?}");
    }
    let session = yaml(root, "trace/sessions/2026-10-05_001.yaml");
    let fields: Vec<&str> = session["logic_revisions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            assert_eq!(row["turn"], 1);
            row["field"].as_str().unwrap()
        })
        .collect();
    assert!(fields.contains(&"document"), "{fields:?}");
    assert!(
        fs::read_to_string(root.join("logic/concepts.md"))
            .unwrap()
            .contains("## Renamed concept")
    );
    assert!(
        !fs::read_to_string(root.join("logic/concepts.md"))
            .unwrap()
            .contains("Obsolete concept")
    );
    assert_eq!(
        yaml(root, "staging/observations.yaml")["observations"][0]["stale"],
        true
    );
    let reasoning = yaml(root, "trace/pm_reasoning_log.yaml")["entries"].clone();
    assert!(
        reasoning
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["turn"] == "2026-10-05_001#1"
                && entry["notes"][0] == "Caller abandoned the topic")
    );
    // The same audits without an anchor still fail closed.
    let error = reject(
        root,
        &[
            json!({"op":"paper.edit","frontmatter":{"title":"Again"},"audit":{"signal":"user-directive","provenance":"user"}}),
        ],
        MIDNIGHT,
    );
    assert_eq!(error.code, "write.owner_required");
}

#[test]
fn empty_and_no_op_batches_write_nothing_but_an_explicit_log_is_a_turn() {
    let dir = fixture(&[open("2026-10-05_001", 1)]);
    let root = dir.path();
    let before = tree(root);
    let report = commit(root, &[], MIDNIGHT);
    assert!(report.changed_paths.is_empty());
    assert_eq!(tree(root), before);
    let report = commit(
        root,
        &[json!({"op":"entry.edit","target":{"id":"C01"},"set":{"Status":"hypothesis"}})],
        MIDNIGHT,
    );
    assert!(report.operations[0].no_op);
    assert!(report.changed_paths.is_empty());
    assert_eq!(tree(root), before);
    let report = commit(
        root,
        &[log("Requested turn"), revise("Existing statement")],
        "2026-10-05T09:00:00Z",
    );
    assert!(report.operations[1].no_op);
    let session = yaml(root, "trace/sessions/2026-10-05_001.yaml");
    assert_eq!(
        session["session"]["turn_count"], 2,
        "explicit log is never suppressed"
    );
    assert_eq!(
        session["logic_revisions"],
        json!([]),
        "no-op makes no revision row"
    );
    assert!(
        !fs::read_to_string(root.join("logic/claims.md"))
            .unwrap()
            .contains("Last revised")
    );
}

#[test]
fn dry_run_reserves_no_time_turn_or_session() {
    let dir = fixture(&[]);
    let root = dir.path();
    let before = tree(root);
    let lines = [log("Tentative"), revise("Tentative revision")];
    let dry = run_at(root, &lines, ApplyMode::DryRun, MIDNIGHT).unwrap();
    assert!(dry.dry_run);
    assert_eq!(dry.operations[0].id.as_deref(), Some("2026-10-05_001"));
    assert_eq!(tree(root), before);
    assert!(!root.join(".ara").exists(), "dry run takes no lock");
    let later = "2026-10-05T10:00:00Z";
    commit(root, &lines, later);
    let session = yaml(root, "trace/sessions/2026-10-05_001.yaml");
    assert_eq!(
        session["session"]["started"], later,
        "dry-run time was not reserved"
    );
}

#[test]
fn failure_after_owner_allocation_rolls_back_bytes_and_created_directories() {
    let dir = fixture(&[]);
    let root = dir.path();
    let error = reject(
        root,
        &[
            log("Allocated then failed"),
            revise("Tentative"),
            json!({"op":"node.add","type":"question","parent":"N999","title":"Bad parent","fields":{"description":"d"}}),
        ],
        MIDNIGHT,
    );
    assert_eq!(error.line, Some(3));
    assert!(!root.join("trace/sessions").exists());
    assert!(!root.join("trace/pm_reasoning_log.yaml").exists());

    // A commit fault after planning restores originals and directory absence.
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
            if kind == TransactionBoundary::AfterRename && path.starts_with("trace/sessions/") {
                return Err(WriteError::io(format!("injected {path}")));
            }
            Ok(())
        }
    }
    let before = tree(root);
    let _lock = ArtifactLock::acquire(root).unwrap();
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(root).unwrap());
    working.batch_time = Some(MIDNIGHT.into());
    write::batch::plan_batch(
        &mut working,
        &ops(&[log("Hooked"), revise("Hooked revision")]),
    )
    .unwrap();
    assert_eq!(working.owner.as_ref().unwrap().session, "2026-10-05_001");
    working.validate().unwrap();
    working.plan_missing_directories().unwrap();
    let error = transaction::commit_with_hooks(&working, &mut Observer, &mut Fault).unwrap_err();
    assert!(error.message.contains("injected"));
    assert_eq!(tree(root), before);
    assert!(!root.join("trace/sessions").exists());
}

#[test]
fn cooperating_writers_select_one_session_and_allocate_distinct_turns() {
    let dir = fixture(&[]);
    let root: PathBuf = dir.path().to_path_buf();
    std::thread::scope(|scope| {
        for writer in 0..4 {
            let root = root.clone();
            scope.spawn(move || {
                commit(
                    &root,
                    &[log(&format!("Writer {writer}"))],
                    "2026-10-05T09:00:00Z",
                );
            });
        }
    });
    let session = yaml(&root, "trace/sessions/2026-10-05_001.yaml");
    assert_eq!(session["session"]["turn_count"], 4);
    assert!(!root.join("trace/sessions/2026-10-05_002.yaml").exists());
    assert_eq!(
        yaml(&root, "trace/sessions/session_index.yaml")["sessions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn clock_is_read_after_lock_and_recovery() {
    let dir = fixture(&[]);
    let root = dir.path();
    let original = fs::read(root.join("logic/claims.md")).unwrap();
    {
        // Leave a prepared transaction behind, as after a crash.
        let _lock = ArtifactLock::acquire(root).unwrap();
        let mut working = WorkingArtifact::new(ArtifactSnapshot::load(root).unwrap());
        let start = working
            .text("logic/claims.md")
            .unwrap()
            .find("Existing statement")
            .unwrap();
        working
            .edit(
                "logic/claims.md",
                start..start + 8,
                "Crashed!",
                "crash fixture",
            )
            .unwrap();
        let mut observer = DurableJournal::new(&working).unwrap();
        // Same staging layout the durable journal expects for candidate 0.
        let temporary = root.join("logic/.ara-write-900001-0");
        fs::write(&temporary, &working.files["logic/claims.md"]).unwrap();
        fs::File::open(&temporary).unwrap().sync_all().unwrap();
        observer
            .staged(&[("logic/claims.md".into(), temporary.clone())])
            .unwrap();
        observer.prepared(&working).unwrap();
        fs::rename(&temporary, root.join("logic/claims.md")).unwrap();
        drop(observer);
        assert!(journal::pending_prepared(root).unwrap());
    }
    let error = run_at(root, &[log("Dry")], ApplyMode::DryRun, MIDNIGHT).unwrap_err();
    assert_eq!(
        error.exit_code(),
        2,
        "dry run refuses before reading mixed state"
    );
    let reads = Cell::new(0);
    write::execute_at(
        root,
        &ops(&[log("After recovery")]),
        ApplyMode::Commit,
        || {
            assert!(root.join(".ara/lock").is_file(), "lock precedes the clock");
            assert!(
                !journal::pending_prepared(root).unwrap(),
                "recovery precedes the clock"
            );
            assert_eq!(fs::read(root.join("logic/claims.md")).unwrap(), original);
            reads.set(reads.get() + 1);
            Ok(MIDNIGHT.to_owned())
        },
        |_, _| (),
    )
    .unwrap();
    assert_eq!(reads.get(), 1);
    // A malformed clock value fails closed as an I/O-class error.
    let error = run_at(
        root,
        &[log("Bad clock")],
        ApplyMode::Commit,
        "2026-10-05T00:00Z",
    )
    .unwrap_err();
    assert_eq!(error.exit_code(), 2);
}

#[test]
fn planners_without_a_captured_clock_fail_closed() {
    let dir = fixture(&[]);
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(dir.path()).unwrap());
    let error = write::plan_operation(
        &mut working,
        &ops(&[json!({"op":"observation.stage","content":"c","potential_type":"unknown","provenance":"user"})])[0],
    )
    .unwrap_err();
    assert_eq!(error.code, "write.clock_required");
    let error = write::plan_operation(&mut working, &ops(&[revise("x")])[0]).unwrap_err();
    assert_eq!(error.code, "write.revision_required");
}

#[test]
fn explicit_audit_session_or_turn_must_match_the_anchor() {
    let dir = fixture(&[open("2026-10-05_001", 1), open("2026-10-04_001", 1)]);
    let root = dir.path();
    let time = "2026-10-05T09:00:00Z";
    for (audit, field) in [
        (
            json!({"turn":9,"signal":"user-directive","provenance":"user"}),
            "audit.turn",
        ),
        (
            json!({"session":"2026-10-04_001","signal":"user-directive","provenance":"user"}),
            "audit.session",
        ),
    ] {
        let error = reject(
            root,
            &[
                log("Anchor"),
                json!({"op":"paper.edit","frontmatter":{"title":"Mismatch"},"audit":audit}),
            ],
            time,
        );
        assert_eq!(
            (error.code.as_str(), error.line, error.field.as_deref()),
            ("write.owner_mismatch", Some(2), Some(field))
        );
    }
    // Omitted-field errors name the field the caller left out.
    let error = reject(
        root,
        &[
            json!({"op":"paper.edit","frontmatter":{"title":"x"},"audit":{"session":"2026-10-05_001","signal":"user-directive","provenance":"user"}}),
        ],
        time,
    );
    assert_eq!(
        (error.code.as_str(), error.field.as_deref()),
        ("write.owner_required", Some("audit.turn"))
    );
    let error = reject(
        root,
        &[
            json!({"op":"record.append","document":"trace/pm_reasoning_log.yaml","record":{"notes":["n"]}}),
            log("Late"),
        ],
        time,
    );
    assert_eq!(
        (error.code.as_str(), error.field.as_deref()),
        ("write.owner_order", Some("record.turn"))
    );
}
