#![cfg(feature = "native")]

use ara_core::write::{ArtifactSnapshot, Fields, WorkingArtifact, WriteOperation, plan_operation};
use ara_core::write::{positions::YamlDocument, records, sessions, staging};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn fixture() -> (tempfile::TempDir, WorkingArtifact) {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in [
        (
            "PAPER.md",
            "---\ntitle: Writer fixture\n---\n# Writer fixture\n",
        ),
        (
            "trace/exploration_tree.yaml",
            "tree:\n  - id: N01\n    type: experiment\n    title: Original experiment\n    provenance: ai-executed\n    result: Recorded result\n  - id: N02\n    type: question\n    title: Original question\n    provenance: user\n",
        ),
        (
            "logic/problem.md",
            "# Problem\n\n## O99: Problem namespace\n",
        ),
        (
            "logic/claims.md",
            "# Claims\n\n## C01: Existing\n- **Statement**: Existing statement\n- **Conditions**: Existing conditions\n- **Status**: hypothesis\n- **Provenance**: user\n- **Falsification**: Counterexample\n",
        ),
    ] {
        let target = dir.path().join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(dir.path()).unwrap());
    // Planner-level tests stand in for the writer's one locked clock read.
    working.batch_time = Some("2026-10-01T12:00:00Z".into());
    (dir, working)
}
fn op(value: Value) -> WriteOperation {
    serde_json::from_value(value).unwrap()
}
fn apply(working: &mut WorkingArtifact, value: Value) -> ara_core::write::OperationResult {
    plan_operation(working, &op(value)).unwrap()
}
fn yaml(working: &WorkingArtifact, path: &str) -> Value {
    working.yaml(path).unwrap().root.to_json().unwrap()
}
fn stage(working: &mut WorkingArtifact) -> String {
    apply(working,json!({"op":"observation.stage","content":"原文 = quoted\n\n  indentation\r\nending\n","potential_type":"claim","context":"literal @file\ncontext\n","provenance":"ai-executed","timestamp":"2026-10-01T10:00","bound_to":["N01"]})).id.unwrap()
}
fn start(working: &mut WorkingArtifact, date: &str) -> String {
    apply(working,json!({"op":"session.start","date":date,"started":format!("{date}T10:00"),"summary":"Started explicitly"})).id.unwrap()
}
fn log(session: &str, timestamp: &str, summary: &str) -> Value {
    json!({"op":"session.log","session":session,"timestamp":timestamp,"summary":summary,
        "events":[{"type":"experiment","id":"N01","routing":"direct","provenance":"ai-executed","summary":"Exact event\nsecond line\n"}],
        "ai_actions":[{"action":"Executed caller's command\n","provenance":"ai-executed","files_changed":["src/run.rs"]}],
        "claims_touched":[{"id":"C01","action":"revised"}],
        "logic_revisions":[{"entry":"C01","field":"Statement","before":"old\n\n","after":"new\n","signal":"user-directive","provenance":"user","note":"Exact note\n"}],
        "key_context":[{"excerpt":"Caller said: = preserve\n"}],"open_threads":["Continue exact thread"],"ai_suggestions_pending":["Pending suggestion"]})
}
fn stale(observation: &str, days: Value, owner_date: &str) -> Value {
    json!({"op":"observation.mark_stale","observation":observation,"session_days":days,
        "reason":"Caller decided the topic has been abandoned.\nKeep exact wording.\n",
        "audit":{"session":format!("{owner_date}_001"),"turn":1,"signal":"user-directive","provenance":"user","note":"Caller audit note"}})
}
fn claim_fields() -> Value {
    json!({"Statement":"Mechanism persists\n","Conditions":"Within caller boundary","Status":"hypothesis","Falsification":"A contrary observation"})
}
fn fields(value: Value) -> Fields {
    serde_json::from_value(value).unwrap()
}

#[test]
fn stage_preserves_exact_typed_strings_and_separate_namespace() {
    let (_dir, mut working) = fixture();
    let id = stage(&mut working);
    assert_eq!(id, "O01");
    let value = yaml(&working, staging::OBSERVATIONS);
    let row = &value["observations"][0];
    assert_eq!(row["content"], "原文 = quoted\n\n  indentation\r\nending\n");
    assert_eq!(row["context"], "literal @file\ncontext\n");
    assert_eq!(row["bound_to"], json!(["N01"]));
    assert_eq!(row["provenance"], "ai-executed");
    assert_eq!(row["promoted"], false);
    staging::validate_references(&working).unwrap();
    for timestamp in [
        "2026-02-30T10:00",
        "2026-10-01T25:00",
        "2026-10-01T10:00:60",
        "not-a-date",
    ] {
        let bad = op(
            json!({"op":"observation.stage","content":"literal","potential_type":"claim","provenance":"user","timestamp":timestamp}),
        );
        assert!(plan_operation(&mut working, &bad).is_err());
    }
    let operation = op(
        json!({"op":"observation.stage","content":"dangling","potential_type":"claim","provenance":"user","timestamp":"2026-10-01T10:00","bound_to":["N999"]}),
    );
    plan_operation(&mut working, &operation).unwrap();
    assert!(staging::validate_references(&working).is_err());
}

#[test]
fn promotion_creates_every_destination_and_guards_final_tuple() {
    let targets = [
        ("claim", "logic/claims.md:C02", claim_fields()),
        (
            "heuristic",
            "logic/solution/heuristics.md:H01",
            json!({"Rationale":"Caller rationale","Status":"active","Sensitivity":"unknown","Code ref":["pending"]}),
        ),
        (
            "concept",
            "logic/concepts.md#Finding",
            json!({"Definition":"Caller definition"}),
        ),
        (
            "constraint",
            "logic/solution/constraints.md#Finding",
            json!({"Constraint":"Caller constraint"}),
        ),
        (
            "architecture",
            "logic/solution/architecture.md#Finding",
            json!({"Architecture":"Caller architecture"}),
        ),
        (
            "dead_end",
            "trace:N03",
            json!({"hypothesis":"Caller hypothesis","failure_mode":"Caller failure","lesson":"Caller lesson"}),
        ),
    ];
    for (to, destination, input) in targets {
        let (_dir, mut working) = fixture();
        let id = stage(&mut working);
        let original = yaml(&working, staging::OBSERVATIONS)["observations"][0].clone();
        let promote = json!({"op":"observation.promote","observation":id,"to":to,"title":"Finding","fields":input,"signal":"empirical-resolution"});
        let result = apply(&mut working, promote.clone());
        assert_eq!(result.target.as_deref(), Some(destination));
        staging::validate_references(&working).unwrap();
        let row = yaml(&working, staging::OBSERVATIONS)["observations"][0].clone();
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
        assert_eq!(row["promoted"], true);
        assert_eq!(row["promoted_to"], destination);
        assert_eq!(row["crystallized_via"], "empirical-resolution");
        let target_path = if to == "dead_end" {
            "trace/exploration_tree.yaml"
        } else {
            destination.split([':', '#']).next().unwrap()
        };
        assert!(working.text(target_path).unwrap().contains("ai-executed"));
        let before: BTreeMap<_, _> = working
            .changed_paths()
            .into_iter()
            .map(|p| (p.clone(), working.text(&p).unwrap().to_owned()))
            .collect();
        assert!(plan_operation(&mut working, &op(promote)).is_err());
        for (path, bytes) in before {
            assert_eq!(working.text(&path).unwrap(), bytes);
        }
        assert!(
            staging::edit_pointer(
                &mut working,
                &id,
                &fields(json!({"crystallized_via":"verbal-affirmation"}))
            )
            .is_err()
        );
    }
}

#[test]
fn explicit_promotion_provenance_is_not_an_automatic_upgrade() {
    let (_dir, mut working) = fixture();
    let id = stage(&mut working);
    let mut input = claim_fields();
    input
        .as_object_mut()
        .unwrap()
        .insert("Provenance".into(), json!("user-revised"));
    apply(
        &mut working,
        json!({"op":"observation.promote","observation":id,"to":"claim","title":"Finding","fields":input,"signal":"verbal-affirmation"}),
    );
    let text = working.text("logic/claims.md").unwrap();
    let section = ara_core::markdown::sections(text)
        .into_iter()
        .find(|s| s.heading.starts_with("C02:"))
        .unwrap();
    let provenance = ara_core::markdown::fields(text, section.body_range)
        .into_iter()
        .find(|f| f.name == "Provenance")
        .unwrap();
    assert_eq!(
        ara_core::markdown::decode_field(&provenance),
        "user-revised"
    );
    assert_eq!(
        yaml(&working, staging::OBSERVATIONS)["observations"][0]["provenance"],
        "ai-executed"
    );
}

#[test]
fn full_turns_preserve_earlier_record_bytes_and_archive_exact_metadata() {
    let (_dir, mut working) = fixture();
    let id = start(&mut working, "2026-10-01");
    let path = format!("trace/sessions/{id}.yaml");
    let first = apply(
        &mut working,
        log(&id, "2026-10-01T10:01", "First exact summary\n"),
    );
    assert_eq!(first.turn, Some(1));
    let source = working.text(&path).unwrap();
    let document = YamlDocument::parse(source).unwrap();
    let old_records: Vec<String> = [
        "events_logged",
        "ai_actions",
        "claims_touched",
        "logic_revisions",
        "key_context",
    ]
    .iter()
    .map(|key| {
        let record = &document.root.get(key).unwrap().unwrap().sequence().unwrap()[0];
        source[record.start..record.end].to_owned()
    })
    .collect();
    let mut second = log(&id, "2026-10-01T10:02", "Second exact summary\n");
    second.as_object_mut().unwrap().remove("open_threads");
    second["ai_suggestions_pending"] = json!([]);
    let second = apply(&mut working, second);
    assert_eq!(second.turn, Some(2));
    let source = working.text(&path).unwrap();
    for bytes in old_records {
        assert_eq!(source.matches(&bytes).count(), 1);
    }
    let value = yaml(&working, &path);
    for key in [
        "events_logged",
        "ai_actions",
        "claims_touched",
        "logic_revisions",
        "key_context",
    ] {
        assert_eq!(value[key][0]["turn"], 1);
        assert_eq!(value[key][1]["turn"], 2);
    }
    assert_eq!(
        value["events_logged"][0]["summary"],
        "Exact event\nsecond line\n"
    );
    assert_eq!(value["open_threads"], json!(["Continue exact thread"]));
    assert_eq!(value["ai_suggestions_pending"], json!([]));
    let archive = yaml(&working, records::REASONING);
    let transition = &archive["entries"][1]["session_metadata"];
    assert_eq!(transition["before"]["summary"], "First exact summary\n");
    assert_eq!(transition["after"]["summary"], "Second exact summary\n");
    assert_eq!(transition["before"]["turn_count"], 1);
    assert_eq!(transition["after"]["turn_count"], 2);
    assert_eq!(
        transition["before"]["ai_suggestions_pending"],
        json!(["Pending suggestion"])
    );
    assert_eq!(transition["after"]["ai_suggestions_pending"], json!([]));
    let index = yaml(&working, sessions::INDEX);
    assert_eq!(index["sessions"][0]["events_count"], 2);
    assert_eq!(index["sessions"][0]["claims_touched"], json!(["C01"]));
    assert_eq!(index["sessions"][0]["open_threads"], 1);
    sessions::validate_index(&working).unwrap();
    records::validate_references(&working).unwrap();
}

#[test]
fn sessions_reject_chronology_unknown_records_closed_days_and_bad_indexes() {
    let (_dir, mut working) = fixture();
    let id = start(&mut working, "2026-10-01");
    assert_eq!(start(&mut working, "2026-10-01"), "2026-10-01_002");
    apply(&mut working, log(&id, "2026-10-01T10:01", "First"));
    for timestamp in ["2026-10-01T10:00", "2026-10-02T10:02"] {
        assert!(plan_operation(&mut working, &op(log(&id, timestamp, "Reject"))).is_err());
    }
    let mut wrong = log(&id, "2026-10-01T10:02", "Unknown field");
    wrong["events"][0]["unapproved"] = json!(true);
    assert!(plan_operation(&mut working, &op(wrong)).is_err());
    let path = format!("trace/sessions/{id}.yaml");
    working
        .replace_yaml_field(&path, &["session".into()], "closed", &json!(true))
        .unwrap();
    assert!(plan_operation(&mut working, &op(log(&id, "2026-10-01T10:02", "Closed"))).is_err());
    working
        .replace_yaml_field(
            sessions::INDEX,
            &[
                "sessions".into(),
                ara_core::write::positions::PathPart::Index(0),
            ],
            "events_count",
            &json!(999),
        )
        .unwrap();
    assert!(sessions::validate_index(&working).is_err());
}

#[test]
fn revisions_attach_only_to_exact_new_owned_turn_including_earlier_batch_turn() {
    let (_dir, mut working) = fixture();
    let id = start(&mut working, "2026-10-01");
    let revision = json!({"entry":"C01","field":"Statement","before":"Exact old\n","after":"Exact new\n","signal":"user-directive","provenance":"user"});
    assert!(sessions::append_revision(&mut working, &id, 1, &revision).is_err());
    apply(&mut working, log(&id, "2026-10-01T10:01", "First"));
    apply(&mut working, log(&id, "2026-10-01T10:02", "Second"));
    sessions::append_revision(&mut working, &id, 1, &revision).unwrap();
    let value = yaml(&working, &format!("trace/sessions/{id}.yaml"));
    assert_eq!(value["logic_revisions"][1]["turn"], 1);
    assert_eq!(value["logic_revisions"][1]["before"], "Exact old\n");
    assert_eq!(value["logic_revisions"][1]["after"], "Exact new\n");
    assert_eq!(value["logic_revisions"][2]["turn"], 2);
    assert!(sessions::append_revision(&mut working, &id, 3, &revision).is_err());
    working.owned_turns.clear();
    assert!(sessions::append_revision(&mut working, &id, 1, &revision).is_err());
    sessions::validate_index(&working).unwrap();
}

#[test]
fn taste_and_reasoning_append_exact_records_without_node_mutation() {
    let (_dir, mut working) = fixture();
    let tree = working
        .text("trace/exploration_tree.yaml")
        .unwrap()
        .to_owned();
    let first = apply(
        &mut working,
        json!({"op":"record.append","id":"T09","document":records::TASTE,"record":{"timestamp":"2026-10-01T10:00","target":"N01","tag":"uncertain","object":"framing","comment":"Exact\n\ncomment\n"}}),
    );
    assert_eq!(first.id.as_deref(), Some("T09"));
    let second = apply(
        &mut working,
        json!({"op":"record.append","document":records::TASTE,"record":{"timestamp":"2026-10-01T10:01","target":"N01","tag":"endorse","object":"evidence","comment":"Second"}}),
    );
    assert_eq!(second.id.as_deref(), Some("T10"));
    assert_eq!(
        yaml(&working, records::TASTE)["entries"][0]["comment"],
        "Exact\n\ncomment\n"
    );
    assert_eq!(working.text("trace/exploration_tree.yaml").unwrap(), tree);
    let bad = op(
        json!({"op":"record.append","document":records::TASTE,"record":{"timestamp":"2026-10-01T10:02","target":"N02","tag":"reject","object":"claim","comment":"Question"}}),
    );
    assert!(plan_operation(&mut working, &bad).is_err());
    let id = start(&mut working, "2026-10-01");
    apply(
        &mut working,
        json!({"op":"session.log","session":id,"timestamp":"2026-10-01T10:01"}),
    );
    apply(
        &mut working,
        json!({"op":"record.append","document":records::REASONING,"record":{"turn":format!("{id}#1"),"notes":["Literal @file\n",""]}}),
    );
    records::validate_references(&working).unwrap();
    let entries = yaml(&working, records::REASONING);
    assert_eq!(
        entries["entries"][1]["notes"],
        json!(["Literal @file\n", ""])
    );
    assert!(plan_operation(&mut working,&op(json!({"op":"record.append","document":records::REASONING,"record":{"turn":format!("{id}#1"),"notes":[],"extra":true}}))).is_err());
}

#[test]
fn stale_requires_three_distinct_proven_subsequent_session_days() {
    let (_dir, mut working) = fixture();
    let id = stage(&mut working);
    for date in ["2026-10-02", "2026-10-03", "2026-10-04"] {
        let session = start(&mut working, date);
        apply(
            &mut working,
            json!({"op":"session.log","session":session,"timestamp":format!("{date}T10:01"),"ai_actions":[{"action":"Unrelated action","provenance":"ai-executed","files_changed":[]}]}),
        );
    }
    for days in [
        json!(["2026-10-02", "2026-10-03"]),
        json!(["2026-10-02", "2026-10-02", "2026-10-03"]),
        json!(["2026-10-02", "2026-10-03", "2026-10-05"]),
    ] {
        assert!(plan_operation(&mut working, &op(stale(&id, days, "2026-10-04"))).is_err());
    }
    apply(
        &mut working,
        stale(
            &id,
            json!(["2026-10-02", "2026-10-03", "2026-10-04"]),
            "2026-10-04",
        ),
    );
    assert_eq!(
        yaml(&working, staging::OBSERVATIONS)["observations"][0]["stale"],
        true
    );
    staging::validate_references(&working).unwrap();
    records::validate_references(&working).unwrap();
    let reasoning = yaml(&working, records::REASONING);
    let note = reasoning["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| {
            entry["notes"][0]
                == "Caller decided the topic has been abandoned.\nKeep exact wording.\n"
        })
        .unwrap();
    assert_eq!(note["turn"], "2026-10-04_001#1");
    assert_eq!(note["notes"][2], "Caller audit note");
    let evidence: Value = serde_json::from_str(note["notes"][1].as_str().unwrap()).unwrap();
    assert_eq!(
        evidence["session_days"],
        json!(["2026-10-02", "2026-10-03", "2026-10-04"])
    );
    assert_eq!(evidence["last_reference"], "2026-10-01");
    assert_eq!(evidence["bound_to"], json!(["N01"]));
    assert_eq!(evidence["signal"], "user-directive");
    assert_eq!(evidence["provenance"], "user");
    assert_eq!(
        evidence["audit"],
        json!({"session":"2026-10-04_001","turn":1,"source_refs":["trace/sessions/2026-10-04_001.yaml"]})
    );
    assert_eq!(
        evidence["session_sources"],
        json!([
            {"date":"2026-10-02","document":"trace/sessions/2026-10-02_001.yaml"},
            {"date":"2026-10-03","document":"trace/sessions/2026-10-03_001.yaml"},
            {"date":"2026-10-04","document":"trace/sessions/2026-10-04_001.yaml"}
        ])
    );
}

#[test]
fn absent_rolling_lists_stay_absent_until_explicit_empty_and_archive_preserves_absence() {
    let (dir, mut working) = fixture();
    let id = start(&mut working, "2026-10-01");
    let path = format!("trace/sessions/{id}.yaml");
    // Seed a valid older dialect without optional continuity lists and retain opaque data.
    let record = format!(
        "session:\n  id: {id}\n  date: 2026-10-01\n  started: \"2026-10-01T10:00\"\n  last_turn: \"2026-10-01T10:00\"\n  turn_count: 0\n  summary: Exact seed\n  unknown: {{nested: [keep, values]}}\nevents_logged: []\nai_actions: []\nclaims_touched: []\nlogic_revisions: []\nkey_context: []\n"
    );
    let index = format!(
        "sessions:\n  - id: {id}\n    date: 2026-10-01\n    summary: Exact seed\n    turn_count: 0\n    events_count: 0\n    claims_touched: []\n    open_threads: 0\n    unknown: retained\n"
    );
    std::fs::create_dir_all(dir.path().join("trace/sessions")).unwrap();
    std::fs::write(dir.path().join(&path), record).unwrap();
    std::fs::write(dir.path().join(sessions::INDEX), index).unwrap();
    working = WorkingArtifact::new(ArtifactSnapshot::load(dir.path()).unwrap());
    apply(
        &mut working,
        json!({"op":"session.log","session":id,"timestamp":"2026-10-01T10:01"}),
    );
    let value = yaml(&working, &path);
    assert!(value.get("open_threads").is_none());
    assert!(value.get("ai_suggestions_pending").is_none());
    apply(
        &mut working,
        json!({"op":"session.log","session":id,"timestamp":"2026-10-01T10:02","open_threads":[],"ai_suggestions_pending":[]}),
    );
    let archive = yaml(&working, records::REASONING);
    let transition = &archive["entries"][1]["session_metadata"];
    assert!(transition["before"].get("open_threads").is_none());
    assert!(transition["before"].get("ai_suggestions_pending").is_none());
    assert_eq!(transition["after"]["open_threads"], json!([]));
    assert_eq!(transition["after"]["ai_suggestions_pending"], json!([]));
    assert!(
        working
            .text(&path)
            .unwrap()
            .contains("unknown: {nested: [keep, values]}")
    );
    assert!(
        working
            .text(sessions::INDEX)
            .unwrap()
            .contains("unknown: retained")
    );
}

#[test]
fn reference_evidence_resets_stale_window_and_empty_sessions_do_not_count() {
    let (_dir, mut working) = fixture();
    let id = stage(&mut working);
    for date in ["2026-10-02", "2026-10-03", "2026-10-04"] {
        let session = start(&mut working, date);
        let mut turn =
            json!({"op":"session.log","session":session,"timestamp":format!("{date}T10:01")});
        if date == "2026-10-03" {
            turn["events"] = json!([{"type":"observation","id":id,"routing":"staged","provenance":"ai-executed","summary":"Referenced explicitly"}]);
        }
        apply(&mut working, turn);
    }
    assert!(
        plan_operation(
            &mut working,
            &op(stale(
                &id,
                json!(["2026-10-02", "2026-10-03", "2026-10-04"]),
                "2026-10-04"
            ))
        )
        .is_err()
    );
    for date in ["2026-10-05", "2026-10-06"] {
        let session = start(&mut working, date);
        apply(
            &mut working,
            json!({"op":"session.log","session":session,"timestamp":format!("{date}T10:01")}),
        );
    }
    apply(
        &mut working,
        stale(
            &id,
            json!(["2026-10-04", "2026-10-05", "2026-10-06"]),
            "2026-10-06",
        ),
    );
    assert_eq!(
        yaml(&working, staging::OBSERVATIONS)["observations"][0]["stale"],
        true
    );

    let (_dir, mut working) = fixture();
    let id = stage(&mut working);
    for date in ["2026-10-02", "2026-10-03", "2026-10-04"] {
        start(&mut working, date);
    }
    assert!(
        plan_operation(
            &mut working,
            &op(stale(
                &id,
                json!(["2026-10-02", "2026-10-03", "2026-10-04"]),
                "2026-10-04"
            ))
        )
        .is_err()
    );
}

#[test]
fn concrete_forward_references_resolve_only_in_complete_candidate() {
    let (_dir, mut working) = fixture();
    apply(
        &mut working,
        json!({"op":"observation.stage","content":"Bound forward","potential_type":"concept","provenance":"user","timestamp":"2026-10-01T10:00","bound_to":["N03"]}),
    );
    assert!(staging::validate_references(&working).is_err());
    apply(
        &mut working,
        json!({"op":"node.add","id":"N03","type":"experiment","parent":"root","title":"Later concrete node","fields":{"provenance":"ai-executed","result":"Supplied result"}}),
    );
    staging::validate_references(&working).unwrap();
    let session = start(&mut working, "2026-10-01");
    apply(
        &mut working,
        json!({"op":"record.append","document":records::REASONING,"record":{"turn":format!("{session}#1"),"notes":["Caller record before turn allocation"]}}),
    );
    assert!(records::validate_references(&working).is_err());
    apply(
        &mut working,
        json!({"op":"session.log","session":session,"timestamp":"2026-10-01T10:01"}),
    );
    records::validate_references(&working).unwrap();
}

#[test]
fn promoted_tuple_rejects_invalid_signals_and_conflict_annotations_are_additive() {
    let (_dir, mut working) = fixture();
    let id = stage(&mut working);
    let original = working.text(staging::OBSERVATIONS).unwrap().to_owned();
    for (to, signal) in [
        ("claim", "silent-approval"),
        ("unknown", "verbal-affirmation"),
    ] {
        assert!(plan_operation(&mut working,&op(json!({"op":"observation.promote","observation":id,"to":to,"title":"Rejected","fields":claim_fields(),"signal":signal}))).is_err());
        assert_eq!(working.text(staging::OBSERVATIONS).unwrap(), original);
    }
    staging::annotate(
        &mut working,
        &id,
        "conflict",
        &["C01".into()],
        "Exact disagreement\n",
    )
    .unwrap();
    staging::validate_references(&working).unwrap();
    assert!(
        working
            .text(staging::OBSERVATIONS)
            .unwrap()
            .contains("# CONFLICT: see C01\n")
    );
    let row = yaml(&working, staging::OBSERVATIONS)["observations"][0].clone();
    assert_eq!(row["content"], "原文 = quoted\n\n  indentation\r\nending\n");
    assert_eq!(
        row["conflict_annotations"][0]["comment"],
        "Exact disagreement\n"
    );
    assert_eq!(row["conflict_annotations"][0]["references"], json!(["C01"]));
    let before = working.text(staging::OBSERVATIONS).unwrap().to_owned();
    assert!(
        staging::annotate(
            &mut working,
            &id,
            "conflict",
            &["C01".into()],
            "Exact disagreement\n"
        )
        .unwrap()
        .no_op
    );
    assert_eq!(working.text(staging::OBSERVATIONS).unwrap(), before);
}

#[test]
fn session_index_rejects_missing_record_and_mismatched_record_identity() {
    let (_dir, mut working) = fixture();
    let id = start(&mut working, "2026-10-01");
    let path = format!("trace/sessions/{id}.yaml");
    working
        .replace_yaml_field(&path, &["session".into()], "id", &json!("2026-10-01_002"))
        .unwrap();
    assert!(sessions::validate_index(&working).is_err());

    let (_dir, mut working) = fixture();
    working.create(sessions::INDEX,"sessions:\n  - id: 2026-10-01_001\n    date: 2026-10-01\n    summary: Dangling\n    turn_count: 0\n    events_count: 0\n    claims_touched: []\n    open_threads: 0\n").unwrap();
    assert!(sessions::validate_index(&working).is_err());
}

#[test]
fn opaque_unknown_yaml_values_survive_stage_and_session_writes() {
    let (dir, mut working) = fixture();
    let id = start(&mut working, "2026-10-01");
    let path = format!("trace/sessions/{id}.yaml");
    let mut session_text = working.text(&path).unwrap().to_owned();
    session_text.push_str("\nopaque: &keep {nested: [unknown, values]}\nopaque_alias: *keep\n");
    let index_text = working.text(sessions::INDEX).unwrap().to_owned();
    std::fs::create_dir_all(dir.path().join("trace/sessions")).unwrap();
    std::fs::create_dir_all(dir.path().join("staging")).unwrap();
    std::fs::write(dir.path().join(&path), session_text).unwrap();
    std::fs::write(dir.path().join(sessions::INDEX), index_text).unwrap();
    std::fs::write(dir.path().join(staging::OBSERVATIONS),"observations:\n  - id: O07\n    timestamp: \"2026-10-01T10:00\"\n    provenance: user\n    content: Historical exact\n    potential_type: unknown\n    promoted: false\n    promoted_to: null\n    crystallized_via: null\n    stale: false\n    bound_to: []\n    opaque: &keep {nested: [unknown, values]}\n    opaque_alias: *keep\n").unwrap();
    working = WorkingArtifact::new(ArtifactSnapshot::load(dir.path()).unwrap());
    assert_eq!(stage(&mut working), "O08");
    apply(
        &mut working,
        json!({"op":"session.log","session":id,"timestamp":"2026-10-01T10:01"}),
    );
    for document in [&path, staging::OBSERVATIONS] {
        let source = working.text(document).unwrap();
        assert!(source.contains("opaque: &keep {nested: [unknown, values]}"));
        assert!(source.contains("opaque_alias: *keep"));
    }
    sessions::validate_index(&working).unwrap();
}

#[test]
fn unrelated_writes_do_not_retroactively_upgrade_legacy_sessions() {
    let (dir, _) = fixture();
    std::fs::create_dir_all(dir.path().join("trace/sessions")).unwrap();
    std::fs::write(dir.path().join("trace/sessions/2026-09-30_001.yaml"),"session:\n  id: 2026-09-30_001\n  timestamp: \"2026-09-30T10:00\"\n  summary: Legacy unchanged\nevents_logged:\n  - id: N01\n    type: experiment\n    provenance: ai-executed\n    summary: Legacy exact event\n").unwrap();
    std::fs::write(dir.path().join(sessions::INDEX),"sessions:\n  - id: 2026-09-30_001\n    timestamp: \"2026-09-30T10:00\"\n    summary: Legacy unchanged\n").unwrap();
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(dir.path()).unwrap());
    let path = "trace/sessions/2026-09-30_001.yaml";
    let historical = working.text(path).unwrap().to_owned();
    stage(&mut working);
    sessions::validate_authored(&working).unwrap();
    assert_eq!(working.text(path).unwrap(), historical);
    assert!(plan_operation(&mut working,&op(json!({"op":"session.start","date":"2026-10-01","started":"2026-10-01T10:00","summary":"Must not silently normalize history"}))).is_err());
    assert_eq!(working.text(path).unwrap(), historical);
}

#[test]
fn native_record_locators_allow_whole_documents_and_registered_knowledge_sections() {
    let (dir, _) = fixture();
    std::fs::write(dir.path().join("PAPER.md"),"---\ntitle: Writer fixture\nknowledge_paths: [knowledge/custom.md]\n---\n# Writer fixture\n").unwrap();
    std::fs::create_dir_all(dir.path().join("knowledge")).unwrap();
    std::fs::write(
        dir.path().join("knowledge/custom.md"),
        "# Custom knowledge\n\n## Boundary\nExact current body.\n",
    )
    .unwrap();
    let working = WorkingArtifact::new(ArtifactSnapshot::load(dir.path()).unwrap());
    for reference in [
        "PAPER.md",
        "logic/problem.md",
        "knowledge/custom.md",
        "knowledge/custom.md#Boundary",
    ] {
        sessions::require_entry_reference(&working, reference).unwrap();
    }
    assert!(sessions::require_entry_reference(&working, "knowledge/unregistered.md").is_err());
    assert!(sessions::require_entry_reference(&working, "knowledge/custom.md#Missing").is_err());
    assert!(sessions::require_entry_reference(&working, "logic/concepts.md").is_err());
}

#[test]
fn authored_revisions_must_resolve_final_native_identity_and_strict_selectors() {
    for entry in [
        json!("C999"),
        json!({"document":"logic/claims.md","entry":"C01","heading":["Existing"]}),
        json!({"document":"logic/claims.md","heading":["Missing"]}),
    ] {
        let (_dir, mut working) = fixture();
        let session = start(&mut working, "2026-10-01");
        apply(
            &mut working,
            json!({"op":"session.log","session":session,"timestamp":"2026-10-01T10:01","logic_revisions":[{"entry":entry,"field":"Statement","before":"Caller old","after":"Caller new","signal":"user-directive","provenance":"user"}]}),
        );
        assert!(sessions::validate_authored(&working).is_err());
    }
    let (_dir, mut working) = fixture();
    let session = start(&mut working, "2026-10-01");
    apply(
        &mut working,
        json!({"op":"session.log","session":session,"timestamp":"2026-10-01T10:01","logic_revisions":[
            {"entry":"PAPER.md","field":"document","before":"Caller old PAPER","after":"Caller new PAPER","signal":"user-directive","provenance":"user"},
            {"entry":{"document":"logic/problem.md","heading":[]},"field":"Body","before":"Caller old body","after":"Caller new body","signal":"user-directive","provenance":"user"},
            {"entry":{"document":"logic/claims.md","entry":"C01"},"field":"Statement","before":"Caller old statement","after":"Caller new statement","signal":"user-directive","provenance":"user"}
        ]}),
    );
    sessions::validate_authored(&working).unwrap();
}

#[test]
fn registered_revision_cannot_survive_unregistering_its_document_in_same_batch() {
    let (dir, _) = fixture();
    std::fs::write(
        dir.path().join("PAPER.md"),
        "---\ntitle: Writer fixture\nknowledge_paths: [knowledge/new.md]\n---\n# Writer fixture\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("knowledge")).unwrap();
    std::fs::write(
        dir.path().join("knowledge/new.md"),
        "# New knowledge\n\n## Boundary\nCaller original body.\n",
    )
    .unwrap();
    let mut working = WorkingArtifact::new(ArtifactSnapshot::load(dir.path()).unwrap());
    let session = start(&mut working, "2026-10-01");
    apply(
        &mut working,
        json!({"op":"session.log","session":session,"timestamp":"2026-10-01T10:01","logic_revisions":[{"entry":{"document":"knowledge/new.md","heading":["Boundary"]},"field":"Body","before":"Caller original body.\n","after":"Caller revised body.\n","signal":"user-directive","provenance":"user"}]}),
    );
    sessions::validate_authored(&working).unwrap();
    working
        .replace_document(
            "PAPER.md",
            "---\ntitle: Writer fixture\nknowledge_paths: []\n---\n# Writer fixture\n",
            "simulate later registration removal in candidate",
        )
        .unwrap();
    assert!(sessions::validate_authored(&working).is_err());
}

#[test]
fn exact_native_structural_audit_preserves_new_revision_to_renamed_identity() {
    let (_dir, mut working) = fixture();
    let session = start(&mut working, "2026-10-01");
    apply(
        &mut working,
        json!({"op":"session.log","session":session,"timestamp":"2026-10-01T10:01"}),
    );
    let target = ara_core::write::EntrySelector::Id { id: "C01".into() };
    let original = ara_core::write::logic::resolve(&working, &target).unwrap();
    let expected = ara_core::write::source::digest(
        working.text(&original.document).unwrap()[original.range].as_bytes(),
    );
    apply(
        &mut working,
        json!({"op":"entry.rename","target":{"id":"C01"},"name":"C02","expected":expected,"session":session,"turn":1,"signal":"user-directive","provenance":"user"}),
    );
    let pending = std::mem::take(&mut working.revisions);
    for revision in pending {
        sessions::append_revision(
            &mut working,
            &revision.session,
            revision.turn,
            &revision.record,
        )
        .unwrap();
    }
    sessions::validate_authored(&working).unwrap();
    let value = yaml(&working, &format!("trace/sessions/{session}.yaml"));
    assert_eq!(value["logic_revisions"][0]["entry"], json!({"id":"C01"}));
    assert_eq!(value["logic_revisions"][0]["field"], "entry");
    assert!(!working.text("logic/claims.md").unwrap().contains("## C01:"));
    assert!(working.text("logic/claims.md").unwrap().contains("## C02:"));
}

#[test]
fn claim_removal_rejects_and_retains_original_source() {
    let (_dir, mut working) = fixture();
    let session = start(&mut working, "2026-10-01");
    apply(
        &mut working,
        json!({"op":"session.log","session":session,"timestamp":"2026-10-01T10:01"}),
    );
    let target = ara_core::write::EntrySelector::Id { id: "C01".into() };
    let original = ara_core::write::logic::resolve(&working, &target).unwrap();
    let before = working.text("logic/claims.md").unwrap().to_owned();
    let expected = ara_core::write::source::digest(before[original.range].as_bytes());
    let error=plan_operation(&mut working,&op(json!({"op":"entry.remove","target":{"id":"C01"},"expected":expected,"session":session,"turn":1,"signal":"user-directive","provenance":"user"}))).unwrap_err();
    assert_eq!(error.code, "write.claim_retention");
    assert_eq!(working.text("logic/claims.md").unwrap(), before);
    assert!(!working.exists("trace/logic_mutations.yaml"));
}

#[test]
fn stale_requires_reason_owned_turn_and_exact_notes_and_rechecks_final_references() {
    let (_dir, mut working) = fixture();
    let id = stage(&mut working);
    for date in ["2026-10-02", "2026-10-03", "2026-10-04"] {
        let session = start(&mut working, date);
        apply(
            &mut working,
            json!({"op":"session.log","session":session,"timestamp":format!("{date}T10:01")}),
        );
    }
    let before = working.text(staging::OBSERVATIONS).unwrap().to_owned();
    let mut request = stale(
        &id,
        json!(["2026-10-02", "2026-10-03", "2026-10-04"]),
        "2026-10-04",
    );
    request["reason"] = json!(" \n");
    assert!(plan_operation(&mut working, &op(request)).is_err());
    assert_eq!(working.text(staging::OBSERVATIONS).unwrap(), before);
    working
        .replace_yaml_field(
            staging::OBSERVATIONS,
            &[
                "observations".into(),
                ara_core::write::positions::PathPart::Index(0),
            ],
            "stale",
            &json!(true),
        )
        .unwrap();
    assert!(staging::validate_references(&working).is_err());
    working
        .replace_yaml_field(
            staging::OBSERVATIONS,
            &[
                "observations".into(),
                ara_core::write::positions::PathPart::Index(0),
            ],
            "stale",
            &json!(false),
        )
        .unwrap();
    apply(
        &mut working,
        stale(
            &id,
            json!(["2026-10-02", "2026-10-03", "2026-10-04"]),
            "2026-10-04",
        ),
    );
    staging::validate_references(&working).unwrap();
    let owner = ("2026-10-04_001".into(), 1);
    let timestamp = working.owned_turns.remove(&owner).unwrap();
    assert!(staging::validate_references(&working).is_err());
    working.owned_turns.insert(owner, timestamp);
    // A later reference in the same atomic candidate invalidates the evidence.
    apply(
        &mut working,
        json!({"op":"session.log","session":"2026-10-04_001","timestamp":"2026-10-04T10:02","events":[{"type":"observation","id":id,"routing":"staged","provenance":"user","summary":"Actual later reference"}]}),
    );
    assert!(staging::validate_references(&working).is_err());
}

#[test]
fn stale_evidence_counts_bound_references_and_not_empty_session_days() {
    let (_dir, mut working) = fixture();
    let id = stage(&mut working);
    for date in ["2026-10-02", "2026-10-03", "2026-10-04"] {
        let session = start(&mut working, date);
        let mut event =
            json!({"op":"session.log","session":session,"timestamp":format!("{date}T10:01")});
        if date == "2026-10-03" {
            event["events"] = json!([{"type":"experiment","id":"N01","routing":"direct","provenance":"user","summary":"Bound node referenced"}]);
        }
        apply(&mut working, event);
    }
    assert!(
        plan_operation(
            &mut working,
            &op(stale(
                &id,
                json!(["2026-10-02", "2026-10-03", "2026-10-04"]),
                "2026-10-04"
            ))
        )
        .is_err()
    );
    assert_eq!(
        yaml(&working, staging::OBSERVATIONS)["observations"][0]["stale"],
        false
    );
}

#[test]
fn stale_is_consumer_visible_and_legacy_flags_and_notes_remain_byte_identical() {
    let (dir, mut working) = fixture();
    let id = stage(&mut working);
    for date in ["2026-10-02", "2026-10-03", "2026-10-04"] {
        let session = start(&mut working, date);
        apply(
            &mut working,
            json!({"op":"session.log","session":session,"timestamp":format!("{date}T10:01")}),
        );
    }
    apply(
        &mut working,
        stale(
            &id,
            json!(["2026-10-04", "2026-10-02", "2026-10-03"]),
            "2026-10-04",
        ),
    );
    staging::validate_references(&working).unwrap();
    records::validate_references(&working).unwrap();
    for path in working.changed_paths() {
        let target = dir.path().join(&path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, working.bytes(&path).unwrap()).unwrap();
    }
    let (manifest, _report) = ara_core::parse_dir(dir.path()).unwrap();
    let observation = manifest
        .observations
        .iter()
        .find(|observation| observation.id.as_str() == id)
        .unwrap();
    assert_eq!(observation.stale, Some(true));
    assert_eq!(
        observation.content,
        "原文 = quoted\n\n  indentation\r\nending\n"
    );
    let historical_observations = std::fs::read(dir.path().join(staging::OBSERVATIONS)).unwrap();
    let historical_reasoning = std::fs::read(dir.path().join(records::REASONING)).unwrap();
    let mut legacy = WorkingArtifact::new(ArtifactSnapshot::load(dir.path()).unwrap());
    stage(&mut legacy);
    staging::validate_references(&legacy).unwrap();
    assert!(
        legacy
            .bytes(staging::OBSERVATIONS)
            .unwrap()
            .starts_with(&historical_observations)
    );
    assert_eq!(
        legacy.bytes(records::REASONING).unwrap(),
        historical_reasoning
    );
    legacy
        .replace_yaml_field(
            staging::OBSERVATIONS,
            &[
                "observations".into(),
                ara_core::write::positions::PathPart::Index(0),
            ],
            "stale",
            &json!(false),
        )
        .unwrap();
    assert!(staging::validate_references(&legacy).is_err());
}

#[test]
fn stale_batches_substitute_audit_session_bindings_and_reject_missing_context() {
    let (_dir, mut working) = fixture();
    let id = stage(&mut working);
    for date in ["2026-10-02", "2026-10-03", "2026-10-04"] {
        let session = start(&mut working, date);
        apply(
            &mut working,
            json!({"op":"session.log","session":session,"timestamp":format!("{date}T10:01")}),
        );
    }
    let mut request = stale(
        &id,
        json!(["2026-10-02", "2026-10-03", "2026-10-04"]),
        "2026-10-05",
    );
    request["audit"]["session"] = json!("$audit");
    let operations = vec![
        op(
            json!({"op":"session.start","id":"$audit","date":"2026-10-05","started":"2026-10-05T10:00","summary":"Caller's audit session"}),
        ),
        op(request),
        op(json!({"op":"session.log","session":"$audit","timestamp":"2026-10-05T10:01"})),
    ];
    let (_, bindings) = ara_core::write::batch::plan_batch(&mut working, &operations).unwrap();
    assert_eq!(bindings["$audit"], "2026-10-05_001");
    staging::validate_references(&working).unwrap();
    records::validate_references(&working).unwrap();
    assert!(ara_core::write::batch::parse_batch(br#"{"op":"observation.mark_stale","observation":"O01","session_days":["2026-10-02","2026-10-03","2026-10-04"]}"#).is_err());
}

#[test]
fn imported_historical_stale_flags_are_readable_without_fabricating_a_native_audit() {
    let (_dir, mut working) = fixture();
    let source = "observations:\n  - id: O07\n    timestamp: \"2026-09-30T10:00\"\n    provenance: user\n    content: Historical observation\n    potential_type: unknown\n    promoted: false\n    stale: true\n    bound_to: []\n";
    let files = BTreeMap::from([(staging::OBSERVATIONS, STANDARD.encode(source.as_bytes()))]);
    let ledger = json!({"format":"ara.merge-log/v1","records":[
        {"kind":"enrollment","source_key":"history-fixture","label":"historical","time":"2026-10-01T10:00"},
        {"kind":"revision","source_key":"history-fixture","fingerprint":"synthetic-captured-revision","base":"synthetic-base","predecessor":null,"time":"2026-10-01T10:00","git":null,"files":files,"mappings":[]}
    ]});
    working
        .create(
            "trace/merge_log.yaml",
            &ara_core::write::source::render_yaml(&ledger, 0, "\n"),
        )
        .unwrap();
    working
        .stage_create(staging::OBSERVATIONS, source.as_bytes())
        .unwrap();
    staging::validate_references(&working).unwrap();
    assert_eq!(working.text(staging::OBSERVATIONS).unwrap(), source);
    assert!(!working.exists(records::REASONING));
}
