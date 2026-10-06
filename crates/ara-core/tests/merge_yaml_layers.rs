#![cfg(feature = "native")]

use ara_core::{
    merge::{MergeOptions, plan_merge},
    write::{
        ArtifactSnapshot, WorkingArtifact,
        source::{FileSnapshot, digest},
    },
};
use serde_json::{Value, json};
use std::path::PathBuf;

const TREE: &str = "trace/exploration_tree.yaml";
const INDEX: &str = "trace/sessions/session_index.yaml";
const SESSION: &str = "trace/sessions/2026-10-01_001.yaml";
fn snapshot(files: &[(&str, &str)]) -> ArtifactSnapshot {
    ArtifactSnapshot {
        root: PathBuf::from("/nonexistent/merge-yaml-fixture"),
        identity_paths: Default::default(),
        files: files
            .iter()
            .map(|(path, text)| {
                (
                    (*path).into(),
                    FileSnapshot {
                        bytes: text.as_bytes().to_vec(),
                        existed: true,
                        permissions: None,
                        digest: digest(text.as_bytes()),
                    },
                )
            })
            .collect(),
    }
}
fn options() -> MergeOptions {
    MergeOptions {
        source_key: "yaml-fork".into(),
        label: "bob".into(),
        time: "2026-10-01T12:00Z".into(),
        git: None,
        predecessor: None,
    }
}
fn staged(working: &WorkingArtifact) -> ArtifactSnapshot {
    let mut snapshot = working.base.clone();
    for (path, bytes) in &working.files {
        snapshot.files.insert(
            path.clone(),
            FileSnapshot {
                bytes: bytes.clone(),
                existed: true,
                permissions: None,
                digest: digest(bytes),
            },
        );
    }
    snapshot
}
fn document(working: &WorkingArtifact, path: &str) -> Value {
    working.yaml(path).unwrap().root.to_json().unwrap()
}

#[test]
fn colliding_nested_branch_uses_actual_mapped_parent_once_and_keeps_ours_bytes() {
    let base_text = "# exact ours header\ntree:\n  - id: N01\n    type: question\n    title: Parent\n    children: []\n";
    let ours_text = "# exact ours header\ntree:\n  - id: N01\n    type: question\n    title: Parent\n    children:\n      - id: N124\n        type: decision\n        title: 'OURS N124' # lexical winner\n";
    let theirs_text = "# exact ours header\ntree:\n  - id: N01\n    type: question\n    title: Parent\n    children:\n      - id: N124\n        type: decision\n        title: Incoming\n        extension: {nested: ['retain exactly', 7]}\n        children:\n          - id: N125\n            type: experiment\n            title: Child\n            also_depends_on: [N124]\n";
    let base = snapshot(&[(TREE, base_text)]);
    let ours = snapshot(&[(TREE, ours_text)]);
    let theirs = snapshot(&[(TREE, theirs_text)]);
    let plan = plan_merge(&base, &ours, &theirs, &options()).unwrap();
    assert_eq!(plan.report.unresolved_count, 0);
    assert!(plan.working.text(TREE).unwrap().starts_with(ours_text));
    let tree = document(&plan.working, TREE);
    let children = tree["tree"][0]["children"].as_array().unwrap();
    assert_eq!(children[0]["id"], "N124");
    assert_eq!(children[1]["id"], "N125");
    assert_eq!(children[1]["children"][0]["id"], "N126");
    assert_eq!(
        children[1]["children"][0]["also_depends_on"],
        json!(["N125"])
    );
    assert!(
        plan.working
            .text(TREE)
            .unwrap()
            .contains("extension: {nested: ['retain exactly', 7]}")
    );
    let replay = plan_merge(&base, &staged(&plan.working), &theirs, &options()).unwrap();
    assert_eq!(replay.working.changed_paths(), Vec::<String>::new());
}

#[test]
fn identical_illegal_unknown_mutations_on_both_forks_retain_full_evidence() {
    let original = "tree:\n  - id: N01\n    type: question\n    title: Keep\n    extension: {nested: [one, two], null_value: null}\n";
    let illegal = original.replace("one, two", "one, CHANGED");
    let base = snapshot(&[(TREE, original)]);
    let peer = snapshot(&[(TREE, &illegal)]);
    let error = plan_merge(&base, &peer, &peer, &options()).err().unwrap();
    assert_eq!(error.code, "merge.protected_content");
    let field = error
        .evidence
        .iter()
        .find(|c| c.field == "extension")
        .unwrap();
    assert_eq!(
        field.base.bytes,
        b"{nested: [one, two], null_value: null}\n"
    );
    assert_eq!(
        field.ours.bytes,
        b"{nested: [one, CHANGED], null_value: null}\n"
    );
    assert_eq!(field.theirs.bytes, field.ours.bytes);
    assert!(field.allowed.is_empty());
}

#[test]
fn both_forks_deleting_or_moving_protected_nodes_is_rejected() {
    let original = "tree:\n  - id: N01\n    type: question\n    children:\n      - id: N02\n        type: decision\n        title: History\n  - id: N03\n    type: question\n";
    let deleted = "tree:\n  - id: N01\n    type: question\n  - id: N03\n    type: question\n";
    let moved = "tree:\n  - id: N03\n    type: question\n    children:\n      - id: N02\n        type: decision\n        title: History\n  - id: N01\n    type: question\n";
    let base = snapshot(&[(TREE, original)]);
    for text in [deleted, moved] {
        let fork = snapshot(&[(TREE, text)]);
        let error = plan_merge(&base, &fork, &fork, &options()).err().unwrap();
        assert_eq!(error.code, "merge.protected_content");
        assert!(error.evidence.iter().any(|c| c.selector == "N02"));
    }
}

fn session(summary: &str, last: &str, turns: u64, rows: bool) -> String {
    let mut value = json!({"session":{"id":"2026-10-01_001","date":"2026-10-01","started":"2026-10-01T10:00Z","last_turn":last,"turn_count":turns,"summary":summary},"events_logged":[],"ai_actions":[],"claims_touched":[],"logic_revisions":[],"key_context":[],"open_threads":[],"ai_suggestions_pending":[]});
    if rows {
        value["events_logged"] = json!([{"turn":2,"type":"decision","id":"N01","routing":"direct","provenance":"user","summary":"same independent event"}]);
        value["ai_actions"] = json!([{"turn":2,"action":"inspect","provenance":"ai-executed","files_changed":["logic/claims.md"]}]);
        value["claims_touched"] = json!([{"turn":2,"id":"C01","action":"revised"}]);
        value["logic_revisions"] = json!([{"turn":2,"entry":"C01","field":"Statement","before":"complete prior C01","after":"complete next C01","signal":"user-directive","provenance":"user"}]);
        value["key_context"] = json!([{"turn":2,"excerpt":"retain complete context"}]);
    }
    ara_core::write::source::render_yaml(&value, 0, "\n") + "\n"
}
fn index(summary: &str, turns: u64, count: u64) -> String {
    ara_core::write::source::render_yaml(
        &json!({"sessions":[{"id":"2026-10-01_001","date":"2026-10-01","summary":summary,"turn_count":turns,"events_count":count,"claims_touched":if count>0{json!(["C01"])}else{json!([])},"open_threads":0}]}),
        0,
        "\n",
    ) + "\n"
}
const BASIC_TREE: &str = "tree:\n  - id: N01\n    type: decision\n    title: Existing\n";
const CLAIMS: &str = "# Claims\n\n## C01: Claim\n- **Statement**: Known statement\n";
#[test]
fn disjoint_session_metadata_and_identical_same_turn_rows_preserve_two_occurrences() {
    let bs = session("base", "2026-10-01T10:00Z", 1, false);
    let bi = index("base", 1, 0);
    let os = session("ours summary", "2026-10-01T10:01Z", 2, true);
    let oi = index("ours summary", 2, 1);
    let ts = session("base", "2026-10-01T10:02Z", 2, true);
    let ti = index("base", 2, 1);
    let make = |s: &str, i: &str| {
        snapshot(&[
            (TREE, BASIC_TREE),
            ("logic/claims.md", CLAIMS),
            (SESSION, s),
            (INDEX, i),
        ])
    };
    let base = make(&bs, &bi);
    let ours = make(&os, &oi);
    let theirs = make(&ts, &ti);
    let plan = plan_merge(&base, &ours, &theirs, &options()).unwrap();
    assert_eq!(plan.report.unresolved_count, 0);
    let final_session = document(&plan.working, SESSION);
    assert_eq!(final_session["session"]["summary"], "ours summary");
    assert_eq!(final_session["session"]["last_turn"], "2026-10-01T10:02Z");
    assert_eq!(final_session["session"]["turn_count"], 3);
    for name in [
        "events_logged",
        "ai_actions",
        "claims_touched",
        "logic_revisions",
        "key_context",
    ] {
        let rows = final_session[name].as_array().unwrap();
        assert_eq!(rows, &vec![rows[0].clone(), rows[0].clone()], "{name}");
    }
    assert_eq!(
        final_session["logic_revisions"][1]["before"],
        "complete prior C01"
    );
    let row = &document(&plan.working, INDEX)["sessions"][0];
    assert_eq!(row["events_count"], 2);
    assert_eq!(row["turn_count"], 3);
    assert_eq!(row["summary"], "ours summary");
    assert_eq!(row["claims_touched"], json!(["C01"]));
    let replay = plan_merge(&base, &staged(&plan.working), &theirs, &options()).unwrap();
    assert!(replay.working.changed_paths().is_empty());
}

#[test]
fn session_collision_relocates_filename_index_and_complete_history_references() {
    let base = snapshot(&[(TREE, BASIC_TREE), ("logic/claims.md", CLAIMS)]);
    let os = session("ours", "2026-10-01T10:01Z", 2, true);
    let oi = index("ours", 2, 1);
    let ts = session("theirs", "2026-10-01T10:02Z", 2, true);
    let ti = index("theirs", 2, 1);
    let reason = "entries:\n  - turn: '2026-10-01_001#2'\n    session_metadata:\n      session: 2026-10-01_001\n      before: {summary: prior, open_threads: [old]}\n      after: {summary: next, open_threads: [new]}\n";
    let ours = snapshot(&[
        (TREE, BASIC_TREE),
        ("logic/claims.md", CLAIMS),
        (SESSION, &os),
        (INDEX, &oi),
    ]);
    let theirs = snapshot(&[
        (TREE, BASIC_TREE),
        ("logic/claims.md", CLAIMS),
        (SESSION, &ts),
        (INDEX, &ti),
        ("trace/pm_reasoning_log.yaml", reason),
    ]);
    let plan = plan_merge(&base, &ours, &theirs, &options()).unwrap();
    assert_eq!(plan.working.text(SESSION).unwrap(), os);
    let imported = document(&plan.working, "trace/sessions/2026-10-01_002.yaml");
    assert_eq!(imported["session"]["id"], "2026-10-01_002");
    assert_eq!(
        imported["logic_revisions"][0]["before"],
        "complete prior C01"
    );
    let rows = document(&plan.working, INDEX);
    assert_eq!(rows["sessions"][1]["id"], "2026-10-01_002");
    let archive = document(&plan.working, "trace/pm_reasoning_log.yaml");
    assert_eq!(archive["entries"][0]["turn"], "2026-10-01_002#2");
    assert_eq!(
        archive["entries"][0]["session_metadata"]["session"],
        "2026-10-01_002"
    );
    assert_eq!(
        archive["entries"][0]["session_metadata"]["before"]["open_threads"],
        json!(["old"])
    );
}

#[test]
fn promotion_target_race_keeps_ours_and_records_all_exact_tuple_candidates() {
    let observation = "observations:\n  - id: O01\n    content: Protected content\n    promoted: false\n    promoted_to: null\n    crystallized_via: null\n";
    let ours_obs = observation
        .replace("promoted: false", "promoted: true")
        .replace("promoted_to: null", "promoted_to: 'logic/claims.md:C01'")
        .replace(
            "crystallized_via: null",
            "crystallized_via: verbal-affirmation",
        );
    let theirs_obs = ours_obs.replace("C01'", "C02'");
    let claims = format!("{CLAIMS}\n## C02: Other\n- **Statement**: Other statement\n");
    let make = |text: &str| {
        snapshot(&[
            (TREE, BASIC_TREE),
            ("logic/claims.md", &claims),
            ("staging/observations.yaml", text),
        ])
    };
    let plan = plan_merge(
        &make(observation),
        &make(&ours_obs),
        &make(&theirs_obs),
        &options(),
    )
    .unwrap();
    assert_eq!(
        plan.working.text("staging/observations.yaml").unwrap(),
        ours_obs
    );
    let target = plan
        .report
        .conflicts
        .iter()
        .find(|c| c.kind == "promotion")
        .unwrap();
    let tuple = |bytes: &[u8]| serde_json::from_slice::<Value>(bytes).unwrap();
    assert_eq!(tuple(&target.base.bytes)["promoted_to"], "null\n");
    assert_eq!(
        tuple(&target.ours.bytes)["promoted_to"],
        "'logic/claims.md:C01'\n"
    );
    assert_eq!(
        tuple(&target.theirs.bytes)["promoted_to"],
        "'logic/claims.md:C02'\n"
    );
    let replay = plan_merge(
        &make(observation),
        &staged(&plan.working),
        &make(&theirs_obs),
        &options(),
    )
    .unwrap();
    assert_eq!(replay.report.conflicts, plan.report.conflicts);
    assert!(replay.working.changed_paths().is_empty());
    let source_session = session("base", "2026-10-01T10:00Z", 1, false);
    let source_index = index("base", 1, 0);
    let mut captured = staged(&plan.working);
    for (path, text) in [(SESSION, source_session), (INDEX, source_index)] {
        let bytes = text.into_bytes();
        captured.files.insert(
            path.into(),
            FileSnapshot {
                digest: digest(&bytes),
                bytes,
                existed: true,
                permissions: None,
            },
        );
    }
    let resolved = ara_core::merge::plan_resolution(
        &captured,
        &target.id,
        "theirs",
        &ara_core::merge::AuditOwner {
            session: "2026-10-01_001".into(),
            turn: Some(2),
            timestamp: None,
            summary: None,
            signal: "user-directive".into(),
            provenance: "user".into(),
        },
        "2026-10-01T23:59:00Z",
    )
    .unwrap()
    .working;
    let observation = document(&resolved, "staging/observations.yaml");
    assert_eq!(observation["observations"][0]["promoted"], true);
    assert_eq!(
        observation["observations"][0]["promoted_to"],
        "logic/claims.md:C02"
    );
    assert_eq!(
        observation["observations"][0]["crystallized_via"],
        "verbal-affirmation"
    );
    let audit = &document(&resolved, SESSION)["logic_revisions"][0];
    assert_eq!(audit["before"], serde_json::to_value(&target.ours).unwrap());
    assert_eq!(
        audit["after"],
        serde_json::to_value(&target.theirs).unwrap()
    );
}

#[test]
fn unknown_root_values_are_complete_blocking_evidence_not_silently_omitted() {
    let base = snapshot(&[(TREE, BASIC_TREE)]);
    let incoming = format!(
        "{BASIC_TREE}vendor_extension:\n  nested: [1, {{message: 'all incoming bytes'}}]\n  absent_is_not_null: null\n"
    );
    let plan = plan_merge(&base, &base, &snapshot(&[(TREE, &incoming)]), &options()).unwrap();
    assert_eq!(plan.working.text(TREE).unwrap(), BASIC_TREE);
    let conflict = plan
        .report
        .conflicts
        .iter()
        .find(|c| c.field == "vendor_extension")
        .unwrap();
    assert!(!conflict.base.present);
    assert!(!conflict.ours.present);
    assert_eq!(
        conflict.theirs.bytes,
        b"nested: [1, {message: 'all incoming bytes'}]\n  absent_is_not_null: null\n"
    );

    let path = "trace/reasoning.yaml";
    let empty = "entries: []\n";
    let incoming = "entries: []\nroot: {opaque: ['complete', 7]}\n";
    let base = snapshot(&[(TREE, BASIC_TREE), (path, empty)]);
    let theirs = snapshot(&[(TREE, BASIC_TREE), (path, incoming)]);
    let plan = plan_merge(&base, &base, &theirs, &options()).unwrap();
    assert_eq!(plan.working.text(path).unwrap(), empty);
    let conflict = plan
        .report
        .conflicts
        .iter()
        .find(|conflict| conflict.path == path && conflict.field == "root")
        .unwrap();
    assert!(!conflict.base.present);
    assert_eq!(conflict.theirs.bytes, b"{opaque: ['complete', 7]}\n");
}

#[test]
fn historical_fixture_dangling_session_index_rejects_with_exact_source_evidence() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agent-cli");
    let input = ArtifactSnapshot::load(&root).unwrap();
    let error = plan_merge(&input, &input, &input, &options())
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.session_index");
    for id in [
        "2026-03-12_008",
        "2026-03-12_010",
        "2026-03-17_004",
        "2026-03-22_001",
        "2026-03-22_002",
    ] {
        assert!(error.message.contains(id), "{}", error.message);
    }
    assert_eq!(error.evidence[0].base.bytes, input.files[INDEX].bytes);
    assert_eq!(error.evidence[0].ours.bytes, input.files[INDEX].bytes);
    assert_eq!(error.evidence[0].theirs.bytes, input.files[INDEX].bytes);
    let after = ArtifactSnapshot::load(&root).unwrap();
    for (path, file) in input.files {
        assert_eq!(file.bytes, after.files[&path].bytes, "{path}");
    }
}

#[test]
fn changing_historical_revision_on_both_forks_rejects_complete_before_after_evidence() {
    let source = session("base", "2026-10-01T10:01Z", 2, true);
    let idx = index("base", 2, 1);
    let base = snapshot(&[
        (TREE, BASIC_TREE),
        ("logic/claims.md", CLAIMS),
        (SESSION, &source),
        (INDEX, &idx),
    ]);
    let changed = source.replace("complete next C01", "rewritten historical value");
    let fork = snapshot(&[
        (TREE, BASIC_TREE),
        ("logic/claims.md", CLAIMS),
        (SESSION, &changed),
        (INDEX, &idx),
    ]);
    let error = plan_merge(&base, &fork, &fork, &options()).err().unwrap();
    assert_eq!(error.code, "merge.protected_content");
    let evidence = error
        .evidence
        .iter()
        .find(|c| c.field == "logic_revisions")
        .unwrap();
    assert!(
        std::str::from_utf8(&evidence.base.bytes)
            .unwrap()
            .contains("complete prior C01")
    );
    assert!(
        std::str::from_utf8(&evidence.base.bytes)
            .unwrap()
            .contains("complete next C01")
    );
    assert!(
        std::str::from_utf8(&evidence.theirs.bytes)
            .unwrap()
            .contains("rewritten historical value")
    );
    assert_eq!(evidence.ours.bytes, evidence.theirs.bytes);
}

#[test]
fn historical_unknown_target_is_opaque_even_when_spelling_is_a_native_reference() {
    let source = session("base", "2026-10-01T10:01Z", 2, true);
    let mut value = ara_core::write::positions::YamlDocument::parse(&source)
        .unwrap()
        .root
        .to_json()
        .unwrap();
    value["events_logged"][0]["target"] = json!("trace:N01");
    let source = ara_core::write::source::render_yaml(&value, 0, "\n");
    let idx = index("base", 2, 1);
    let base = snapshot(&[
        (TREE, BASIC_TREE),
        ("logic/claims.md", CLAIMS),
        (SESSION, &source),
        (INDEX, &idx),
    ]);
    value["events_logged"][0]["target"] = json!("N01");
    let changed = ara_core::write::source::render_yaml(&value, 0, "\n");
    let fork = snapshot(&[
        (TREE, BASIC_TREE),
        ("logic/claims.md", CLAIMS),
        (SESSION, &changed),
        (INDEX, &idx),
    ]);
    let error = plan_merge(&base, &fork, &fork, &options()).err().unwrap();
    assert_eq!(error.code, "merge.protected_content");
    let evidence = error
        .evidence
        .iter()
        .find(|c| c.field == "events_logged")
        .unwrap();
    let target = |bytes: &[u8]| {
        let source = format!("events_logged:\n  {}", std::str::from_utf8(bytes).unwrap());
        ara_core::write::positions::YamlDocument::parse(&source)
            .unwrap()
            .root
            .to_json()
            .unwrap()["events_logged"][0]["target"]
            .clone()
    };
    assert_eq!(target(&evidence.base.bytes), json!("trace:N01"));
    assert_eq!(target(&evidence.theirs.bytes), json!("N01"));
    assert_eq!(evidence.ours.bytes, evidence.theirs.bytes);
}

#[test]
fn same_semantic_summary_coalesces_with_ours_lexical_representation() {
    let b = session("base", "2026-10-01T10:00Z", 1, false);
    let bi = index("base", 1, 0);
    let o = session("shared", "2026-10-01T10:00Z", 1, false);
    let oi = index("shared", 1, 0);
    let t = o.replace("\"shared\"", "shared");
    let ti = oi.replace("\"shared\"", "shared");
    let make = |s: &str, i: &str| snapshot(&[(TREE, BASIC_TREE), (SESSION, s), (INDEX, i)]);
    let plan = plan_merge(&make(&b, &bi), &make(&o, &oi), &make(&t, &ti), &options()).unwrap();
    assert_eq!(plan.report.unresolved_count, 0);
    assert_eq!(plan.working.text(SESSION).unwrap(), o);
    assert_eq!(plan.working.text(INDEX).unwrap(), oi);
}

#[test]
fn unknown_reference_extension_requires_full_explicit_relocation_evidence() {
    let base = snapshot(&[(TREE, BASIC_TREE)]);
    let ours_tree =
        format!("{BASIC_TREE}  - id: N02\n    type: decision\n    title: Our independent node\n");
    let theirs_tree = format!(
        "{BASIC_TREE}  - id: N02\n    type: decision\n    title: Incoming\n    extension: {{unreviewed_pointer: N02, complete: ['keep', 9]}}\n"
    );
    let error = plan_merge(
        &base,
        &snapshot(&[(TREE, &ours_tree)]),
        &snapshot(&[(TREE, &theirs_tree)]),
        &options(),
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "merge.unsupported_structured_reference");
    assert_eq!(
        error.evidence[0].theirs.bytes,
        b"{unreviewed_pointer: N02, complete: ['keep', 9]}\n"
    );
}

#[test]
fn index_rejects_duplicate_and_dangling_session_identities() {
    let base = snapshot(&[(TREE, BASIC_TREE)]);
    let duplicate = "sessions:\n  - id: 2026-10-01_001\n  - id: 2026-10-01_001\n";
    let missing = "sessions:\n  - id: 2026-10-01_001\n";
    let s = session("base", "2026-10-01T10:00Z", 1, false);
    for fork in [
        snapshot(&[(TREE, BASIC_TREE), (SESSION, &s), (INDEX, duplicate)]),
        snapshot(&[(TREE, BASIC_TREE), (INDEX, missing)]),
    ] {
        let error = plan_merge(&base, &base, &fork, &options()).err().unwrap();
        assert!(matches!(
            error.code.as_str(),
            "merge.identity" | "merge.session_index"
        ));
    }
}

#[test]
fn append_only_edges_preserve_ours_exact_prefix_and_merge_mapped_targets() {
    let base_text =
        "tree:\n  - id: N01\n    type: decision\n    title: Existing\n    also_depends_on: []\n";
    let ours_text = "tree:\n  - id: N01\n    type: decision\n    title: Existing\n    also_depends_on: ['N02'] # keep our lexical edge\n  - id: N02\n    type: decision\n    title: Our target\n";
    let theirs_text = "tree:\n  - id: N01\n    type: decision\n    title: Existing\n    also_depends_on: [N02]\n  - id: N02\n    type: decision\n    title: Their independent target\n";
    let plan = plan_merge(
        &snapshot(&[(TREE, base_text)]),
        &snapshot(&[(TREE, ours_text)]),
        &snapshot(&[(TREE, theirs_text)]),
        &options(),
    )
    .unwrap();
    assert!(
        plan.working
            .text(TREE)
            .unwrap()
            .contains("also_depends_on: ['N02', N03] # keep our lexical edge")
    );
    assert_eq!(
        document(&plan.working, TREE)["tree"][0]["also_depends_on"],
        json!(["N02", "N03"])
    );
}

#[test]
fn incoming_opaque_header_comments_are_retained_alongside_safe_imports() {
    let original = "# original opaque header\ntree:\n  - id: N01\n    type: question\n    title: Parent\n    children: []\n";
    let incoming = "# incoming complete opaque header\n# second annotation line\ntree:\n  - id: N01\n    type: question\n    title: Parent\n    children:\n      - id: N02\n        type: decision\n        title: Imported\n";
    let base = snapshot(&[(TREE, original)]);
    let theirs = snapshot(&[(TREE, incoming)]);
    let plan = plan_merge(&base, &base, &theirs, &options()).unwrap();
    assert!(
        plan.working
            .text(TREE)
            .unwrap()
            .starts_with("# original opaque header\n")
    );
    assert_eq!(
        document(&plan.working, TREE)["tree"][0]["children"][0]["title"],
        "Imported"
    );
    let conflict = plan
        .report
        .conflicts
        .iter()
        .find(|c| c.kind == "opaque_yaml_comments")
        .unwrap();
    assert_eq!(
        conflict.base.bytes.as_slice(),
        b"# original opaque header\n"
    );
    assert_eq!(conflict.ours.bytes, conflict.base.bytes);
    assert_eq!(
        conflict.theirs.bytes.as_slice(),
        b"# incoming complete opaque header\n# second annotation line\n"
    );
    assert_eq!(conflict.allowed, vec!["ours"]);
    let replay = plan_merge(&base, &staged(&plan.working), &theirs, &options()).unwrap();
    assert_eq!(
        replay
            .report
            .conflicts
            .iter()
            .find(|c| c.kind == "opaque_yaml_comments")
            .unwrap()
            .id,
        conflict.id
    );
    assert!(replay.working.changed_paths().is_empty());
}

#[test]
fn fields_known_in_another_layer_do_not_make_unknown_log_extensions_mutable() {
    let original = "entries:\n  - summary: Existing\n    title: 'Opaque extension'\n";
    let changed = original.replace("'Opaque extension'", "\"Opaque extension\"");
    let base = snapshot(&[(TREE, BASIC_TREE), ("trace/reasoning.yaml", original)]);
    let theirs = snapshot(&[(TREE, BASIC_TREE), ("trace/reasoning.yaml", &changed)]);
    let error = plan_merge(&base, &base, &theirs, &options()).err().unwrap();
    let conflict = error.evidence.iter().find(|c| c.field == "title").unwrap();
    assert_eq!(conflict.kind, "protected_field");
    assert!(String::from_utf8_lossy(&conflict.base.bytes).contains("'Opaque extension'"));
    assert!(String::from_utf8_lossy(&conflict.theirs.bytes).contains("\"Opaque extension\""));
}

#[test]
fn forged_mutable_kinds_cannot_replace_protected_yaml_origins() {
    let cases = [
        (
            TREE,
            "title",
            "tree:\n  - id: N01\n    type: question\n    title: Existing\n",
        ),
        (
            TREE,
            "summary",
            "tree:\n  - id: N01\n    type: question\n    title: Parent\n    summary: Existing\n",
        ),
        (
            "trace/reasoning.yaml",
            "summary",
            "entries:\n  - summary: Existing\n",
        ),
    ];
    for (path, field, original) in cases {
        let changed = original.replace("Existing", "Incoming replacement");
        let base = snapshot(&[(TREE, BASIC_TREE), (path, original)]);
        let incoming = snapshot(&[(TREE, BASIC_TREE), (path, &changed)]);
        let evidence = plan_merge(&base, &base, &incoming, &options())
            .err()
            .unwrap()
            .evidence;
        let protected = evidence.iter().find(|item| item.field == field).unwrap();
        for kind in ["opaque_yaml", "mutable_field", "promotion"] {
            let mut forged = protected.clone();
            forged.kind = kind.into();
            forged.allowed = vec!["theirs".into()];
            forged.id.clear();
            let hash = digest(&serde_json::to_vec(&forged).unwrap());
            forged.id = format!("MC{}", hash.strip_prefix("sha256:").unwrap());
            let log=serde_json::to_string(&json!({"format":"ara.merge-log/v1","records":[{"kind":"conflict","conflict":forged}]})).unwrap();
            let snapshot = snapshot(&[(path, original), ("trace/merge_log.yaml", &log)]);
            let error = ara_core::merge::plan_resolution(
                &snapshot,
                &forged.id,
                "theirs",
                &ara_core::merge::AuditOwner {
                    session: "2026-10-01_001".into(),
                    turn: Some(1),
                    timestamp: None,
                    summary: None,
                    signal: "user-directive".into(),
                    provenance: "user".into(),
                },
                "2026-10-01T23:59:00Z",
            )
            .err()
            .unwrap();
            assert!(
                matches!(
                    error.code.as_str(),
                    "merge.protected_content" | "merge.corrupt_ledger"
                ),
                "forged unproven ledger must fail closed: {path}:{field}:{kind}: {}",
                error.message
            );
        }
    }
}

#[test]
fn explicit_session_thread_resolution_targets_root_and_derives_index() {
    let base_session = session("base", "2026-10-01T10:00Z", 1, false);
    let ours_session = base_session.replace("open_threads: []", "open_threads: [our thread]");
    let theirs_session = base_session.replace(
        "open_threads: []",
        "open_threads: [their first thread, their second thread]",
    );
    let base_index = index("base", 1, 0);
    let ours_index = base_index.replace("open_threads: 0", "open_threads: 1");
    let theirs_index = base_index.replace("open_threads: 0", "open_threads: 2");
    let make = |s: &str, i: &str| snapshot(&[(TREE, BASIC_TREE), (SESSION, s), (INDEX, i)]);
    let plan = plan_merge(
        &make(&base_session, &base_index),
        &make(&ours_session, &ours_index),
        &make(&theirs_session, &theirs_index),
        &options(),
    )
    .unwrap();
    let conflict = plan
        .report
        .conflicts
        .iter()
        .find(|item| item.field == "open_threads")
        .unwrap();
    let resolved = ara_core::merge::plan_resolution(
        &staged(&plan.working),
        &conflict.id,
        "theirs",
        &ara_core::merge::AuditOwner {
            session: "2026-10-01_001".into(),
            turn: Some(2),
            timestamp: None,
            summary: None,
            signal: "user-directive".into(),
            provenance: "user".into(),
        },
        "2026-10-01T23:59:00Z",
    )
    .unwrap()
    .working;
    let session = document(&resolved, SESSION);
    assert_eq!(
        session["open_threads"],
        json!(["their first thread", "their second thread"])
    );
    assert_eq!(session["session"]["turn_count"], 2);
    assert_eq!(document(&resolved, INDEX)["sessions"][0]["open_threads"], 2);
    assert_eq!(session["logic_revisions"][0]["field"], "open_threads");
    assert_eq!(
        session["logic_revisions"][0]["before"]["fingerprint"],
        conflict.ours.fingerprint
    );
}

#[test]
fn imported_revision_relocates_typed_entry_but_preserves_exact_historical_values() {
    let ours_claims = format!("{CLAIMS}\n## C77: Our independent claim\n- **Statement**: Ours\n");
    let theirs_claims =
        format!("{CLAIMS}\n## C77: Incoming independent claim\n- **Statement**: Incoming\n");
    let source_session=session("incoming","2026-10-01T10:00Z",1,false).replace("logic_revisions: []","logic_revisions:\n  - turn: 1\n    entry: C77\n    field: Dependencies\n    before: 'Dependencies: [C77]' # exact source historical value\n    after: 'Dependencies: [C77, C01]'\n    signal: user-directive\n    provenance: user");
    let idx = index("incoming", 1, 0);
    let base = snapshot(&[(TREE, BASIC_TREE), ("logic/claims.md", CLAIMS)]);
    let ours = snapshot(&[(TREE, BASIC_TREE), ("logic/claims.md", &ours_claims)]);
    let theirs = snapshot(&[
        (TREE, BASIC_TREE),
        ("logic/claims.md", &theirs_claims),
        (SESSION, &source_session),
        (INDEX, &idx),
    ]);
    let plan = plan_merge(&base, &ours, &theirs, &options()).unwrap();
    let revision = &document(&plan.working, SESSION)["logic_revisions"][0];
    assert_eq!(revision["entry"], "C78");
    assert_eq!(revision["before"], "Dependencies: [C77]");
    assert_eq!(revision["after"], "Dependencies: [C77, C01]");
    assert!(
        plan.working
            .text(SESSION)
            .unwrap()
            .contains("before: 'Dependencies: [C77]' # exact source historical value")
    );
    assert!(
        plan.working
            .text(SESSION)
            .unwrap()
            .contains("after: 'Dependencies: [C77, C01]'")
    );
}

#[test]
fn opaque_annotations_distinguish_yaml_comments_from_quoted_and_literal_hash_text() {
    let original = "tree:\n  - id: N01\n    type: question\n    title: 'Keep # quoted text' # original inline annotation\n    description: | # original block annotation\n      Complete prose\n      # literal prose, not a YAML comment\n# original outside annotation\n";
    let incoming = original
        .replace("original inline annotation", "incoming inline annotation")
        .replace("original block annotation", "incoming block annotation")
        .replace("original outside annotation", "incoming outside annotation");
    let base = snapshot(&[(TREE, original)]);
    let plan = plan_merge(&base, &base, &snapshot(&[(TREE, &incoming)]), &options()).unwrap();
    assert_eq!(plan.working.text(TREE).unwrap(), original);
    let conflict = plan
        .report
        .conflicts
        .iter()
        .find(|item| item.kind == "opaque_yaml_comments")
        .unwrap();
    assert_eq!(conflict.base.bytes.as_slice(),b"# original inline annotation\n# original block annotation\n# original outside annotation\n");
    assert_eq!(conflict.theirs.bytes.as_slice(),b"# incoming inline annotation\n# incoming block annotation\n# incoming outside annotation\n");
}

#[test]
fn revision_object_selectors_resolve_exact_native_headings_and_reject_missing_or_external_targets()
{
    let concepts = "# Concepts\n\n## Research Method\n\nKnown method.\n";
    let external = "# External\n\n## Research Method\n\nSource content, not a knowledge entry.\n";
    let base = snapshot(&[
        (TREE, BASIC_TREE),
        ("logic/concepts.md", concepts),
        ("src/notes.md", external),
    ]);
    let idx = index("incoming", 1, 0);
    for (selector, valid) in [
        (
            json!({"document":"logic/concepts.md","heading":["Research Method"]}),
            true,
        ),
        (
            json!({"document":"logic/concepts.md","heading":["Absent"]}),
            false,
        ),
        (
            json!({"document":"src/notes.md","heading":["Research Method"]}),
            false,
        ),
    ] {
        let revision = json!({"turn":1,"entry":selector,"field":"body","before":"complete before","after":"complete after","signal":"user-directive","provenance":"user"});
        let session = session("incoming", "2026-10-01T10:00Z", 1, false).replace(
            "logic_revisions: []",
            &format!(
                "logic_revisions: [{}]",
                serde_json::to_string(&revision).unwrap()
            ),
        );
        let source = snapshot(&[
            (TREE, BASIC_TREE),
            ("logic/concepts.md", concepts),
            ("src/notes.md", external),
            (SESSION, &session),
            (INDEX, &idx),
        ]);
        let result = plan_merge(&base, &base, &source, &options());
        if valid {
            let plan = result.unwrap();
            assert_eq!(
                document(&plan.working, SESSION)["logic_revisions"][0]["entry"],
                revision["entry"]
            );
        } else {
            let error = result.err().unwrap();
            assert_eq!(error.code, "merge.session_reference");
            assert_eq!(error.field.as_deref(), Some(SESSION));
            assert!(
                error
                    .evidence
                    .iter()
                    .any(|conflict| conflict.path == SESSION
                        && conflict.theirs.bytes == session.as_bytes())
            );
        }
    }
}

#[test]
fn a_session_closed_after_conflict_capture_cannot_be_revised_by_generic_resolution() {
    let base_session = session("base", "2026-10-01T10:00Z", 1, false);
    let ours_session = session("ours", "2026-10-01T10:00Z", 1, false);
    let theirs_session = session("theirs", "2026-10-01T10:00Z", 1, false);
    let base_index = index("base", 1, 0);
    let ours_index = index("ours", 1, 0);
    let theirs_index = index("theirs", 1, 0);
    let make = |s: &str, i: &str| snapshot(&[(TREE, BASIC_TREE), (SESSION, s), (INDEX, i)]);
    let plan = plan_merge(
        &make(&base_session, &base_index),
        &make(&ours_session, &ours_index),
        &make(&theirs_session, &theirs_index),
        &options(),
    )
    .unwrap();
    let conflict = plan
        .report
        .conflicts
        .iter()
        .find(|item| item.field == "summary")
        .unwrap_or_else(|| {
            panic!(
                "actual conflicts: {:?}; actual session: {}",
                plan.report.conflicts,
                plan.working.text(SESSION).unwrap()
            )
        });
    let captured = staged(&plan.working);
    let mut external_close = WorkingArtifact::new(captured);
    external_close
        .replace_yaml_field(
            SESSION,
            &[ara_core::write::source::PathPart::from("session")],
            "closed",
            &json!(true),
        )
        .unwrap();
    assert_eq!(
        document(&external_close, SESSION)["session"]["closed"],
        true
    );
    let captured = staged(&external_close);
    let error = ara_core::merge::plan_resolution(
        &captured,
        &conflict.id,
        "theirs",
        &ara_core::merge::AuditOwner {
            session: "2026-10-01_001".into(),
            turn: Some(2),
            timestamp: None,
            summary: None,
            signal: "user-directive".into(),
            provenance: "user".into(),
        },
        "2026-10-01T23:59:00Z",
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "merge.protected_content");
}

#[test]
fn imported_mutation_selectors_keep_stable_origin_and_replay_without_self_cycle() {
    let ours_claims = format!("{CLAIMS}\n## C77: Our independent claim\n- **Statement**: Ours\n");
    let source_claims = format!("{CLAIMS}\n## C88: Source title\n- **Statement**: Incoming\n");
    let before = "## C77: Source title\n- **Statement**: Incoming\n";
    let after = "## C88: Source title\n- **Statement**: Incoming\n";
    let row = json!({"action":"rename","from":"logic/claims.md:C77","to":"logic/claims.md:C88","from_selector":{"document":"logic/claims.md","heading":["Claims","C77: Source title"],"entry":null},"to_selector":{"document":"logic/claims.md","heading":["Claims","C88: Source title"],"entry":null},"before":before,"after":after,"session":"2026-10-01_001","turn":1,"signal":"user-directive","provenance":"user","historical_references":[]});
    let mutations = format!("mutations: [{}]\n", serde_json::to_string(&row).unwrap());
    let source_session = session("incoming", "2026-10-01T10:00Z", 1, false);
    let mut source_session = source_session;
    let revision = json!({"turn":1,"entry":row["from_selector"],"field":"entry","before":before,"after":after,"signal":"user-directive","provenance":"user"});
    source_session = source_session.replace(
        "logic_revisions: []",
        &format!("logic_revisions: [{}]", revision),
    );
    let idx = index("incoming", 1, 0);
    let base = snapshot(&[(TREE, BASIC_TREE), ("logic/claims.md", CLAIMS)]);
    let ours = snapshot(&[(TREE, BASIC_TREE), ("logic/claims.md", &ours_claims)]);
    let source = snapshot(&[
        (TREE, BASIC_TREE),
        ("logic/claims.md", &source_claims),
        (SESSION, &source_session),
        (INDEX, &idx),
        ("trace/logic_mutations.yaml", &mutations),
    ]);
    let plan = plan_merge(&base, &ours, &source, &options()).unwrap();
    let retired = &plan
        .report
        .imports
        .iter()
        .find(|mapping| mapping.original == "C77")
        .unwrap()
        .target;
    let live = &plan
        .report
        .imports
        .iter()
        .find(|mapping| mapping.original == "C88")
        .unwrap()
        .target;
    let mutation = &document(&plan.working, "trace/logic_mutations.yaml")["mutations"][0];
    assert_eq!(mutation["from"], format!("logic/claims.md:{retired}"));
    assert_eq!(mutation["to"], format!("logic/claims.md:{live}"));
    assert_eq!(
        mutation["from_selector"]["heading"],
        json!(["Claims", format!("{retired}: Source title")])
    );
    assert_eq!(
        mutation["to_selector"]["heading"],
        json!(["Claims", format!("{live}: Source title")])
    );
    assert_eq!(mutation["before"], before);
    assert_eq!(mutation["after"], after);
    let captured = staged(&plan.working);
    assert_eq!(
        ara_core::merge::resolve(&captured, "bob:C77").unwrap(),
        *live
    );
    let replay = plan_merge(&base, &captured, &source, &options()).unwrap();
    assert!(replay.working.changed_paths().is_empty());
}

#[test]
fn deeply_nested_incoming_tree_is_walked_and_inserted_without_recursive_layer_visitors() {
    let depth = 10_000;
    let base_text = "tree: [{id: N01, type: question, title: Root, children: []}]\n";
    let mut incoming = "tree: [{id: N01, type: question, title: Root, children: [".to_owned();
    for number in 2..=depth {
        incoming.push_str(&format!(
            "{{id: N{number:02}, type: decision, title: Incoming, children: ["
        ));
    }
    incoming.push_str(&"]}".repeat(depth - 1));
    incoming.push_str("]}]\n");
    let base = snapshot(&[(TREE, base_text)]);
    let plan = plan_merge(&base, &base, &snapshot(&[(TREE, &incoming)]), &options()).unwrap();
    let parsed = plan.working.yaml(TREE).unwrap();
    let mut node = &parsed
        .root
        .get("tree")
        .unwrap()
        .unwrap()
        .sequence()
        .unwrap()[0];
    for number in 1..=depth {
        assert_eq!(
            node.get("id").unwrap().unwrap().scalar(),
            Some(format!("N{number:02}").as_str())
        );
        let children = node.get("children").unwrap().unwrap().sequence().unwrap();
        if number == depth {
            assert!(children.is_empty());
        } else {
            node = &children[0];
        }
    }
    assert!(
        plan.working
            .text(TREE)
            .unwrap()
            .starts_with("tree: [{id: N01, type: question, title: Root, children: [")
    );
}

#[test]
fn native_node_payloads_relocate_references_without_rewriting_verbatim_thinking_or_pivot_addresses()
{
    let base = snapshot(&[(TREE, "tree: []\n"), ("logic/claims.md", "# Claims\n")]);
    let ours_claims = "# Claims\n\n## C05: Ours\n- **Statement**: Unrelated local finding\n";
    let source_claims = "# Claims\n\n## C05: Source\n- **Statement**: Imported finding\n";
    let tree = "tree:\n  - id: N01\n    type: decision\n    title: Native decision\n    choice: Adopt evidence\n    alternatives: []\n    evidence: ['C05']\n    thinking: 'Compare C05 with the prior trial'\n    artifacts: [{name: proof, pointer: 'logic/claims.md:C05', what: 'retained C05 narrative', extra: original}]\n    concepts: ['logic/concepts.md#native concept']\n    source_refs: ['logic/claims.md:C05']\n    support_level: explicit\n    rationale: 'Compare C05 carefully'\n  - id: N02\n    type: pivot\n    title: Change optimizer\n    from: 'optimizer: SGD'\n    to: 'optimizer: Adam'\n    trigger: Observed divergence\n";
    let ours = snapshot(&[(TREE, "tree: []\n"), ("logic/claims.md", ours_claims)]);
    let theirs = snapshot(&[
        (TREE, tree),
        ("logic/claims.md", source_claims),
        (
            "logic/concepts.md",
            "# Concepts\n\n## native concept\nExact source concept.\n",
        ),
    ]);
    let plan = plan_merge(&base, &ours, &theirs, &options()).unwrap();
    let value = document(&plan.working, TREE);
    let decision = &value["tree"][0];
    let pivot = &value["tree"][1];
    assert_eq!(decision["evidence"], json!(["C06"]));
    assert_eq!(decision["artifacts"][0]["pointer"], "logic/claims.md:C06");
    assert_eq!(decision["artifacts"][0]["what"], "retained C05 narrative");
    assert_eq!(decision["thinking"], "Compare C05 with the prior trial");
    assert_eq!(decision["source_refs"], json!(["logic/claims.md:C06"]));
    assert_eq!(pivot["from"], "optimizer: SGD");
    assert_eq!(pivot["to"], "optimizer: Adam");
    assert!(
        plan.report
            .needs_review
            .iter()
            .any(|fact| fact.old == "C05")
    );
    assert!(
        plan_merge(&base, &staged(&plan.working), &theirs, &options())
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
}

#[test]
fn distinct_append_unions_replay_and_advance_at_established_destination_occurrences() {
    let bs = session("base", "2026-10-01T10:00Z", 1, false);
    let bi = index("base", 1, 0);
    let os = session("base", "2026-10-01T10:01Z", 2, true)
        .replace("same independent event", "our independent event");
    let ts = session("base", "2026-10-01T10:02Z", 2, true)
        .replace("same independent event", "source independent event");
    let ti = index("base", 2, 1);
    let base_tree =
        "tree:\n  - id: N01\n    type: decision\n    title: Existing\n    also_depends_on: []\n";
    let our_tree=format!("{base_tree}  - id: N02\n    type: decision\n    title: Ours\n    choice: our choice\n    alternatives: []\n").replace("also_depends_on: []","also_depends_on: [N02]");
    let their_tree = our_tree.replace("title: Ours", "title: Source");
    let make = |tree: &str, s: &str, i: &str| {
        snapshot(&[
            (TREE, tree),
            ("logic/claims.md", CLAIMS),
            (SESSION, s),
            (INDEX, i),
        ])
    };
    let base = make(base_tree, &bs, &bi);
    let ours = make(&our_tree, &os, &ti);
    let source = make(&their_tree, &ts, &ti);
    let initial = plan_merge(&base, &ours, &source, &options()).unwrap();
    let captured = staged(&initial.working);
    assert_eq!(
        document(&initial.working, TREE)["tree"][0]["also_depends_on"],
        json!(["N02", "N03"])
    );
    assert_eq!(
        document(&initial.working, SESSION)["events_logged"][0]["summary"],
        "our independent event"
    );
    assert_eq!(
        document(&initial.working, SESSION)["events_logged"][1]["summary"],
        "source independent event"
    );
    let replay = plan_merge(&base, &captured, &source, &options()).unwrap();
    assert!(replay.working.changed_paths().is_empty());
    let mut next = source.clone();
    let parsed = ara_core::write::positions::YamlDocument::parse(
        std::str::from_utf8(&next.files[SESSION].bytes).unwrap(),
    )
    .unwrap();
    let mut value = parsed.root.to_json().unwrap();
    value["session"]["turn_count"] = json!(3);
    value["session"]["last_turn"] = json!("2026-10-01T10:03Z");
    value["events_logged"].as_array_mut().unwrap().push(json!({"turn":3,"type":"decision","id":"N01","routing":"direct","provenance":"user","summary":"new source event"}));
    let bytes = ara_core::write::source::render_yaml(&value, 0, "\n").into_bytes();
    next.files.get_mut(SESSION).unwrap().digest = digest(&bytes);
    next.files.get_mut(SESSION).unwrap().bytes = bytes;
    let bytes = index("base", 3, 2).into_bytes();
    next.files.get_mut(INDEX).unwrap().digest = digest(&bytes);
    next.files.get_mut(INDEX).unwrap().bytes = bytes;
    let advanced = plan_merge(&source, &captured, &next, &options()).unwrap();
    let rows = document(&advanced.working, SESSION)["events_logged"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        rows.iter()
            .map(|row| row["summary"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "our independent event",
            "source independent event",
            "new source event"
        ]
    );
    let imported = advanced
        .report
        .imports
        .iter()
        .find(|mapping| mapping.original == format!("{SESSION}#events_logged/0"))
        .unwrap();
    assert_eq!(imported.target, format!("{SESSION}#events_logged/1"));
    assert!(
        plan_merge(&source, &staged(&advanced.working), &next, &options())
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
}

#[test]
fn independent_observation_annotations_replay_and_advance_by_recorded_occurrences() {
    let observation = "observations:\n  - id: O01\n    content: Protected observation\n    conflict_annotations: []\n";
    let ours = observation.replace(
        "conflict_annotations: []",
        "conflict_annotations: [{references: [C01], comment: our judgment}]",
    );
    let theirs = observation.replace(
        "conflict_annotations: []",
        "conflict_annotations: [{references: [C01], comment: source judgment}]",
    );
    let make = |value: &str| {
        snapshot(&[
            (TREE, BASIC_TREE),
            ("logic/claims.md", CLAIMS),
            ("staging/observations.yaml", value),
        ])
    };
    let base = make(observation);
    let source = make(&theirs);
    let initial = plan_merge(&base, &make(&ours), &source, &options()).unwrap();
    let current = staged(&initial.working);
    assert_eq!(
        document(&initial.working, "staging/observations.yaml")["observations"][0]["conflict_annotations"],
        json!([{"references":["C01"],"comment":"our judgment"},{"references":["C01"],"comment":"source judgment"}])
    );
    assert!(
        plan_merge(&base, &current, &source, &options())
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
    let next = make(&theirs.replace(
        "comment: source judgment}]",
        "comment: source judgment}, {references: [C01], comment: later source judgment}]",
    ));
    let advanced = plan_merge(&source, &current, &next, &options()).unwrap();
    let annotations=document(&advanced.working,"staging/observations.yaml")["observations"][0]["conflict_annotations"].as_array().unwrap().clone();
    assert_eq!(
        annotations
            .iter()
            .map(|row| row["comment"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["our judgment", "source judgment", "later source judgment"]
    );
    assert_eq!(
        advanced
            .report
            .imports
            .iter()
            .find(|row| row.original == "O01#conflict_annotations/0")
            .unwrap()
            .target,
        "O01#conflict_annotations/1"
    );
    assert!(
        plan_merge(&source, &staged(&advanced.working), &next, &options())
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
}

#[test]
fn completed_promotion_destination_cannot_be_rewritten_even_when_both_forks_agree() {
    let original = "observations:\n  - id: O01\n    content: Protected observation\n    promoted: true\n    promoted_to: 'logic/claims.md:C01'\n    crystallized_via: verbal-affirmation\n";
    let changed = original.replace("claims.md:C01", "claims.md:C02");
    let claims = format!(
        "{CLAIMS}\n## C02: Another destination\n- **Statement**: Existing unrelated finding\n"
    );
    let make = |observation: &str| {
        snapshot(&[
            (TREE, BASIC_TREE),
            ("logic/claims.md", &claims),
            ("staging/observations.yaml", observation),
        ])
    };
    let base = make(original);
    let source = make(&changed);
    for ours in [base.clone(), source.clone()] {
        let error = plan_merge(&base, &ours, &source, &options()).err().unwrap();
        assert_eq!(error.code, "merge.protected_content");
        let evidence = error
            .evidence
            .iter()
            .find(|item| item.field == "promoted_to")
            .unwrap();
        assert_eq!(evidence.base.bytes, b"'logic/claims.md:C01'\n");
        assert_eq!(evidence.theirs.bytes, b"'logic/claims.md:C02'\n");
        assert!(evidence.allowed.is_empty());
    }
}

#[test]
fn literal_numeric_concept_names_do_not_follow_colliding_claim_allocations() {
    let base = snapshot(&[(TREE, "tree: []\n"), ("logic/claims.md", "# Claims\n")]);
    let ours = snapshot(&[
        (TREE, "tree: []\n"),
        (
            "logic/claims.md",
            "# Claims\n\n## C05: Destination claim\n- **Statement**: Local claim\n",
        ),
        (
            "logic/concepts.md",
            "# Concepts\n\n## C06\n- **Definition**: Different destination concept\n",
        ),
    ]);
    let source = snapshot(&[
        (
            TREE,
            "tree:\n  - id: N01\n    type: decision\n    title: Uses source concept\n    concepts: ['C05', 'logic/concepts.md#C05', 'logic/concepts.md:Concepts/C05']\n    evidence: [C05]\n    source_refs: ['logic/claims.md:C05']\n    artifacts: [{name: proof, pointer: 'logic/claims.md:C05', what: 'C05 is a literal description'}]\n",
        ),
        (
            "logic/claims.md",
            "# Claims\n\n## C05: Source claim\n- **Statement**: Imported claim\n",
        ),
        (
            "logic/concepts.md",
            "# Concepts\n\n## C05\n- **Definition**: Exact source concept name\n",
        ),
    ]);
    let merged = plan_merge(&base, &ours, &source, &options()).unwrap();
    assert!(merged.report.conflicts.is_empty());
    let tree = document(&merged.working, TREE);
    let node = &tree["tree"][0];
    assert_eq!(
        node["concepts"],
        json!([
            "C05",
            "logic/concepts.md#C05",
            "logic/concepts.md:Concepts/C05"
        ])
    );
    assert_eq!(node["evidence"], json!(["C06"]));
    assert_eq!(node["source_refs"], json!(["logic/claims.md:C06"]));
    assert_eq!(node["artifacts"][0]["pointer"], "logic/claims.md:C06");
    assert_eq!(node["artifacts"][0]["what"], "C05 is a literal description");
    let concepts = merged.working.text("logic/concepts.md").unwrap();
    assert!(concepts.contains("## C05\n- **Definition**: Exact source concept name"));
    assert!(concepts.contains("## C06\n- **Definition**: Different destination concept"));
    ara_core::write::node::validate_references(&merged.working).unwrap();
    let destination = staged(&merged.working);
    let replay = plan_merge(&base, &destination, &source, &options()).unwrap();
    assert!(replay.working.changed_paths().is_empty());
    assert_eq!(
        ara_core::merge::fingerprint(&staged(&replay.working)),
        ara_core::merge::fingerprint(&destination)
    );
}

#[test]
fn closed_source_only_session_replays_in_its_persisted_destination_space() {
    let base = snapshot(&[(TREE, "tree: []\n"), ("logic/claims.md", "# Claims\n")]);
    let ours_session = session("Independent local session", "2026-10-01T10:00Z", 1, false);
    let mut closed: Value = ara_core::write::positions::YamlDocument::parse(&session(
        "Frozen source session",
        "2026-10-01T10:00Z",
        1,
        false,
    ))
    .unwrap()
    .root
    .to_json()
    .unwrap();
    closed["session"]["closed"] = json!(true);
    closed["events_logged"] = json!([{"turn":1,"type":"decision","id":"N01","routing":"direct","provenance":"user","summary":"Source occurrence"}]);
    closed["claims_touched"] = json!([{"turn":1,"id":"C05","action":"created"}]);
    closed["logic_revisions"] = json!([{"turn":1,"entry":"C05","field":"Statement","before":"historical C05 and N01","after":"frozen C05 and N01","signal":"user-directive","provenance":"user"}]);
    closed["ai_actions"] = json!([{"turn":1,"action":"Inspect source","provenance":"ai-executed","files_changed":["logic/claims.md"]}]);
    let source_session = ara_core::write::source::render_yaml(&closed, 0, "\n") + "\n";
    let idx = index("Frozen source session", 1, 1).replace("C01", "C05");
    let ours_idx = index("Independent local session", 1, 0);
    let ours = snapshot(&[
        (
            TREE,
            "tree: [{id: N01, type: question, title: Independent}]\n",
        ),
        (
            "logic/claims.md",
            "# Claims\n\n## C05: Destination claim\n- **Statement**: Local claim\n",
        ),
        (SESSION, &ours_session),
        (INDEX, &ours_idx),
    ]);
    let source = snapshot(&[
        (TREE, "tree: [{id: N01, type: decision, title: Imported}]\n"),
        (
            "logic/claims.md",
            "# Claims\n\n## C05: Source claim\n- **Statement**: Imported claim\n",
        ),
        (SESSION, &source_session),
        (INDEX, &idx),
    ]);
    let merged = plan_merge(&base, &ours, &source, &options()).unwrap();
    assert!(merged.report.conflicts.is_empty());
    let relocated_path = "trace/sessions/2026-10-01_002.yaml";
    let actual = document(&merged.working, relocated_path);
    assert_eq!(actual["session"]["id"], "2026-10-01_002");
    assert_eq!(actual["session"]["closed"], true);
    assert_eq!(actual["events_logged"][0]["id"], "N02");
    assert_eq!(actual["claims_touched"][0]["id"], "C06");
    assert_eq!(actual["logic_revisions"][0]["entry"], "C06");
    assert_eq!(
        actual["logic_revisions"][0]["before"],
        "historical C05 and N01"
    );
    assert_eq!(actual["logic_revisions"][0]["after"], "frozen C05 and N01");
    let destination = staged(&merged.working);
    let before = ara_core::merge::fingerprint(&destination);
    let replay = plan_merge(&base, &destination, &source, &options()).unwrap();
    assert!(replay.working.changed_paths().is_empty());
    assert_eq!(
        ara_core::merge::fingerprint(&staged(&replay.working)),
        before
    );
    for rewrite_destination in [false, true] {
        let mut changed_source = source.clone();
        let mut changed_destination = destination.clone();
        let changed = if rewrite_destination {
            &mut changed_destination
        } else {
            &mut changed_source
        };
        let path = if rewrite_destination {
            relocated_path
        } else {
            SESSION
        };
        let mut history = ara_core::write::positions::YamlDocument::parse(
            std::str::from_utf8(&changed.files[path].bytes).unwrap(),
        )
        .unwrap()
        .root
        .to_json()
        .unwrap();
        history["logic_revisions"][0]["after"] = json!("rewritten frozen history");
        let bytes = ara_core::write::source::render_yaml(&history, 0, "\n").into_bytes();
        changed.files.insert(
            path.into(),
            FileSnapshot {
                digest: digest(&bytes),
                bytes,
                existed: true,
                permissions: None,
            },
        );
        let untouched = ara_core::merge::fingerprint(&changed_destination);
        let error = plan_merge(&source, &changed_destination, &changed_source, &options())
            .err()
            .unwrap();
        assert_eq!(error.code, "merge.protected_content");
        assert!(
            error
                .evidence
                .iter()
                .any(|item| item.kind.starts_with("protected") && item.path == relocated_path)
        );
        assert_eq!(
            ara_core::merge::fingerprint(&changed_destination),
            untouched
        );
    }
}

#[test]
fn flow_parent_without_children_imports_multiple_complete_flow_roots_and_replays() {
    let original = r#"{"tree":[{"id":"N01","type":"question","description":"Original α","opaque":{"keep":"quoted = value"}}]}"#;
    let incoming = r#"{"tree":[{"id":"N01","type":"question","description":"Original α","opaque":{"keep":"quoted = value"},"children":[{"id":"N02","type":"question","description":"First child"},{"id":"N03","type":"question","description":"Second child"}]}]}"#;
    let base = snapshot(&[(TREE, original)]);
    let source = snapshot(&[(TREE, incoming)]);
    let plan = plan_merge(&base, &base, &source, &options()).unwrap();
    let tree = document(&plan.working, TREE);
    let json_tree: serde_json::Value =
        serde_json::from_slice(plan.working.bytes(TREE).unwrap()).unwrap();
    assert_eq!(json_tree, tree);
    assert_eq!(tree["tree"][0]["children"][0]["description"], "First child");
    assert_eq!(
        tree["tree"][0]["children"][1]["description"],
        "Second child"
    );
    assert!(
        plan.working
            .text(TREE)
            .unwrap()
            .contains(r#""opaque":{"keep":"quoted = value"}"#)
    );
    let current = staged(&plan.working);
    assert!(
        plan_merge(&base, &current, &source, &options())
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
}

#[test]
fn resolution_audit_derives_next_turn_and_uses_the_locked_clock_or_explicit_values() {
    let base_session = session("base", "2026-10-01T10:00Z", 1, false);
    let ours_session = base_session.replace("open_threads: []", "open_threads: [our thread]");
    let theirs_session = base_session.replace("open_threads: []", "open_threads: [their thread]");
    let base_index = index("base", 1, 0);
    let ours_index = base_index.replace("open_threads: 0", "open_threads: 1");
    let make = |s: &str, i: &str| snapshot(&[(TREE, BASIC_TREE), (SESSION, s), (INDEX, i)]);
    let plan = plan_merge(
        &make(&base_session, &base_index),
        &make(&ours_session, &ours_index),
        &make(&theirs_session, &ours_index),
        &options(),
    )
    .unwrap();
    let conflict = plan
        .report
        .conflicts
        .iter()
        .find(|item| item.field == "open_threads")
        .unwrap()
        .clone();
    let captured = staged(&plan.working);
    let owner =
        |turn, timestamp: Option<&str>, summary: Option<&str>| ara_core::merge::AuditOwner {
            session: "2026-10-01_001".into(),
            turn,
            timestamp: timestamp.map(str::to_owned),
            summary: summary.map(str::to_owned),
            signal: "user-directive".into(),
            provenance: "user".into(),
        };
    let resolve = |owner: &ara_core::merge::AuditOwner, time: &str| {
        ara_core::merge::plan_resolution(&captured, &conflict.id, "theirs", owner, time)
    };
    // Omitted turn and timestamp: next turn and the locked clock value.
    let audited = resolve(&owner(None, None, None), "2026-10-01T15:00:00Z").unwrap();
    assert_eq!(
        (audited.session.as_str(), audited.turn),
        ("2026-10-01_001", 2)
    );
    let resolved = audited.working;
    let record = document(&resolved, SESSION);
    assert_eq!(record["session"]["last_turn"], "2026-10-01T15:00:00Z");
    assert_eq!(
        record["session"]["summary"], "base",
        "omitted summary keeps the rolling summary"
    );
    assert_eq!(record["logic_revisions"][0]["turn"], 2);
    let ledger = document(&resolved, "trace/merge_log.yaml");
    let resolution = ledger["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row.get("conflict_id").is_some())
        .unwrap()
        .clone();
    assert_eq!(resolution["time"], "2026-10-01T15:00:00Z");
    assert_eq!(resolution["turn"], 2);
    // Explicit timestamp and summary stay exact; a matching turn is accepted.
    let resolved = resolve(
        &owner(
            Some(2),
            Some("2026-10-01T13:00:00+02:00"),
            Some("Caller summary"),
        ),
        "2026-10-02T00:00:00Z",
    )
    .unwrap()
    .working;
    let record = document(&resolved, SESSION);
    assert_eq!(record["session"]["last_turn"], "2026-10-01T13:00:00+02:00");
    assert_eq!(record["session"]["summary"], "Caller summary");
    // A supplied turn must equal the next turn; it never attaches to history.
    for turn in [1, 3] {
        let error = resolve(&owner(Some(turn), None, None), "2026-10-01T15:00:00Z")
            .err()
            .unwrap();
        assert_eq!(error.code, "merge.resolution_session");
        assert!(error.message.contains("next turn 2"), "{}", error.message);
    }
    // A historical session with an omitted timestamp explains the mismatch.
    let error = resolve(&owner(None, None, None), "2026-10-05T00:00:00Z")
        .err()
        .unwrap();
    assert_eq!(error.code, "write.session");
    assert!(
        error.message.contains("is dated 2026-10-01"),
        "{}",
        error.message
    );
}

#[test]
fn protected_repair_audit_derives_next_turn_and_checks_a_supplied_one() {
    let original = "entries:\n  - summary: Existing\n    title: 'Opaque extension'\n";
    let changed = original.replace("'Opaque extension'", "\"Opaque extension\"");
    let base_session = session("base", "2026-10-01T10:00Z", 1, false);
    let base_index = index("base", 1, 0);
    let base = snapshot(&[
        (TREE, BASIC_TREE),
        ("trace/reasoning.yaml", original),
        (SESSION, &base_session),
        (INDEX, &base_index),
    ]);
    let theirs = snapshot(&[
        (TREE, BASIC_TREE),
        ("trace/reasoning.yaml", &changed),
        (SESSION, &base_session),
        (INDEX, &base_index),
    ]);
    let error = plan_merge(&base, &base, &theirs, &options()).err().unwrap();
    let conflict = error
        .evidence
        .iter()
        .find(|c| c.field == "title")
        .unwrap()
        .clone();
    let owner = |turn| ara_core::merge::AuditOwner {
        session: "2026-10-01_001".into(),
        turn,
        timestamp: None,
        summary: Some("Rejected the incoming opaque edit".into()),
        signal: "user-directive".into(),
        provenance: "user".into(),
    };
    let repair = |turn| {
        ara_core::merge::plan_protected_resolution(
            &base,
            &conflict,
            "reject_incoming",
            &conflict.ours.fingerprint,
            &owner(turn),
            "Caller keeps the local history",
            "2026-10-01T16:00:00Z",
        )
    };
    let audited = repair(None).unwrap();
    assert_eq!(audited.turn, 2);
    let repaired = audited.working;
    let record = document(&repaired, SESSION);
    assert_eq!(record["session"]["last_turn"], "2026-10-01T16:00:00Z");
    assert_eq!(
        record["session"]["summary"],
        "Rejected the incoming opaque edit"
    );
    assert_eq!(record["logic_revisions"][0]["turn"], 2);
    assert_eq!(
        repaired.text("trace/reasoning.yaml").unwrap(),
        original,
        "ours stays exact"
    );
    repair(Some(2)).unwrap();
    let error = repair(Some(1)).err().unwrap();
    assert_eq!(error.code, "merge.resolution_session");
}
