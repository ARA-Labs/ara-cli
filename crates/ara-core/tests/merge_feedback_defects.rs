#![cfg(feature = "native")]
//! Plan 04b regressions found by the ara-eval phase-4 runner: canonical
//! feedback must survive forks with their own external files, same-day
//! sessions, positional reasoning entries, and destinations without a
//! `.gitignore`.
mod peer_support;
use ara_core::merge::resolve;
use ara_core::write::ArtifactSnapshot;
use peer_support::*;

fn bare_session(id: &str, summary: &str) -> String {
    format!(
        "session:\n  id: '{id}'\n  date: '{date}'\n  started: '{date}T10:00:00Z'\n  last_turn: '{date}T11:00:00Z'\n  turn_count: 1\n  summary: {summary}\nevents_logged: []\nai_actions: []\nclaims_touched: []\nlogic_revisions: []\nkey_context: []\nopen_threads: []\nai_suggestions_pending: []\n",
        date = &id[..10]
    )
}
fn bare_index(rows: &[(&str, &str)]) -> String {
    let mut result = String::from("sessions:\n");
    for (id, summary) in rows {
        result.push_str(&format!(
            "  - {{id: '{id}', date: '{}', summary: {summary}, turn_count: 1, events_count: 0, claims_touched: [], open_threads: 0}}\n",
            &id[..10]
        ));
    }
    result
}
fn with_sessions(snapshot: &mut ArtifactSnapshot, rows: &[(&str, &str)]) {
    for (id, summary) in rows {
        put(
            snapshot,
            &format!("trace/sessions/{id}.yaml"),
            bare_session(id, summary),
        );
    }
    put(
        snapshot,
        "trace/sessions/session_index.yaml",
        bare_index(rows),
    );
}
fn reasoning(sessions: &[&str]) -> String {
    let mut result = String::from("entries:\n");
    for session in sessions {
        result.push_str(&format!(
            "  - session_metadata:\n      after: {{summary: work, turn_count: 1}}\n      before: {{summary: work, turn_count: 0}}\n      session: \"{session}\"\n    turn: \"{session}#1\"\n"
        ));
    }
    result
}
fn ids(plan: &ara_core::merge::MergePlan, prefix: &str) -> Vec<(String, String)> {
    plan.report
        .imports
        .iter()
        .filter(|mapping| mapping.original.starts_with(prefix))
        .map(|mapping| (mapping.original.clone(), mapping.target.clone()))
        .collect()
}

#[test]
fn return_import_of_a_fork_holding_its_own_external_file_keeps_identities() {
    let seed = seed();
    let mut a1 = fork(&seed, "A");
    put(&mut a1, "evidence/a.json", "{}\n");
    let (canonical, plan) = merge(&seed, &seed, &a1, "fork-a", 1);
    assert_eq!(plan.report.unresolved_count, 1);
    assert!(!canonical.files.contains_key("evidence/a.json"));
    // Canonical's ledger maps the uninstalled file; in K1's inventory it is a
    // historical identity, while fork A holds it as live external content.
    let plan = try_merge_as(&seed, &a1, &canonical, "canonical", 2, "fork-a").unwrap();
    // Canonical's own unresolved acknowledgment travels as source-owned.
    assert!(
        plan.report
            .conflicts
            .iter()
            .all(|item| item.kind == "imported_unresolved" && item.allowed.is_empty())
    );
    let a2 = materialized(&plan.working);
    assert!(text(&a2, "trace/aliases.yaml").contains("\"original\":\"evidence/a.json\""));
    let mut a2 = a2;
    edit(
        &mut a2,
        "logic/claims.md",
        "A says x",
        "A says x, after feedback",
    );
    let plan = try_merge_as(&a1, &canonical, &a2, "fork-a", 3, "canonical").unwrap();
    let back = materialized(&plan.working);
    assert_eq!(headings(&back), headings(&canonical));
    assert!(text(&back, "logic/claims.md").contains("A says x, after feedback"));
    assert!(!back.files.contains_key("evidence/a.json"));
    assert_eq!(resolve(&back, "fork-a:C77").unwrap(), "C02");
    assert!(
        plan.report
            .conflicts
            .iter()
            .all(|item| item.path == "evidence/a.json")
    );
}

#[test]
fn several_colliding_same_day_sessions_relocate_without_collision() {
    let seed = seed();
    let mut canonical = seed.clone();
    with_sessions(
        &mut canonical,
        &[("2026-10-02_001", "first"), ("2026-10-02_002", "second")],
    );
    let mut fork_c = seed.clone();
    with_sessions(&mut fork_c, &[("2026-10-02_001", "own")]);
    let plan = try_merge_as(&seed, &fork_c, &canonical, "canonical", 1, "fork-c").unwrap();
    assert_eq!(plan.report.exit_code(), 0, "{:#?}", plan.report.conflicts);
    assert_eq!(
        ids(&plan, "2026-10-02_"),
        [
            ("2026-10-02_001".to_owned(), "2026-10-02_003".to_owned()),
            ("2026-10-02_002".to_owned(), "2026-10-02_002".to_owned()),
        ]
    );
    let result = materialized(&plan.working);
    for (id, summary) in [
        ("2026-10-02_001", "own"),
        ("2026-10-02_002", "second"),
        ("2026-10-02_003", "first"),
    ] {
        assert!(
            text(&result, &format!("trace/sessions/{id}.yaml"))
                .contains(&format!("summary: {summary}")),
            "{id}"
        );
    }
    assert_eq!(
        text(&result, "trace/sessions/session_index.yaml")
            .matches("id:")
            .count(),
        3
    );
    assert_eq!(
        resolve(&result, "canonical:2026-10-02_001").unwrap(),
        "2026-10-02_003"
    );
    // Without a collision the relocation targets stay exactly as before.
    let mut lone = seed.clone();
    with_sessions(&mut lone, &[("2026-10-02_001", "first")]);
    let plan = try_merge_as(&seed, &fork_c, &lone, "canonical", 1, "fork-c").unwrap();
    assert_eq!(
        ids(&plan, "2026-10-02_"),
        [("2026-10-02_001".to_owned(), "2026-10-02_002".to_owned())]
    );
}

#[test]
fn positional_reasoning_entries_relocated_on_both_routes_return_unchanged() {
    let seed = seed();
    // Canonical and fork A each logged a turn of their own same-day session,
    // so both hold `trace/pm_reasoning_log.yaml#entries/0` for different turns.
    let mut canonical = seed.clone();
    with_sessions(&mut canonical, &[("2026-10-02_001", "pm")]);
    put(
        &mut canonical,
        "trace/pm_reasoning_log.yaml",
        reasoning(&["2026-10-02_001"]),
    );
    let mut a1 = seed.clone();
    add_claim(&mut a1, "C77", "fork A finding", "A says x");
    with_sessions(&mut a1, &[("2026-10-02_001", "a")]);
    put(
        &mut a1,
        "trace/pm_reasoning_log.yaml",
        reasoning(&["2026-10-02_001"]),
    );
    // A absorbs canonical: canonical's session and its reasoning entry move.
    let plan = try_merge_as(&seed, &a1, &canonical, "canonical", 1, "fork-a").unwrap();
    assert_eq!(plan.report.exit_code(), 0, "{:#?}", plan.report.conflicts);
    let a2 = materialized(&plan.working);
    assert!(text(&a2, "trace/pm_reasoning_log.yaml").contains("\"2026-10-02_002#1\""));
    // Canonical receives A: its own entry comes back unchanged after relocation.
    let plan = try_merge_as(&seed, &canonical, &a2, "fork-a", 2, "canonical").unwrap();
    assert_eq!(plan.report.exit_code(), 0, "{:#?}", plan.report.conflicts);
    let back = materialized(&plan.working);
    let log = text(&back, "trace/pm_reasoning_log.yaml");
    assert_eq!(log.matches("session_metadata").count(), 2);
    assert!(log.starts_with(&reasoning(&["2026-10-02_001"])));
    // Fork A's own entry follows its own session, relocated here.
    assert!(log.contains("\"2026-10-02_003#1\""));
    assert!(text(&back, "trace/sessions/2026-10-02_003.yaml").contains("summary: a"));
    assert_eq!(
        resolve(&back, "fork-a:trace/pm_reasoning_log.yaml#entries/1").unwrap(),
        "trace/pm_reasoning_log.yaml#entries/0"
    );
    // A real change to that inherited entry is still protected history.
    let mut tampered = a2.clone();
    edit(
        &mut tampered,
        "trace/pm_reasoning_log.yaml",
        "      after: {summary: work, turn_count: 1}\n      before: {summary: work, turn_count: 0}\n      session: \"2026-10-02_002\"",
        "      after: {summary: rewritten, turn_count: 1}\n      before: {summary: work, turn_count: 0}\n      session: \"2026-10-02_002\"",
    );
    let error = try_merge_as(&seed, &canonical, &tampered, "fork-a", 2, "canonical")
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.protected_content");
    assert_eq!(error.evidence[0].kind, "protected_inherited_entry");
}

#[test]
fn an_inherited_alias_to_an_uninstalled_file_is_not_copied() {
    // The seed has no `.gitignore`; every CLI write adds one, so a fork that
    // was never written lacks it while canonical and fork A have it.
    let seed = seed();
    let mut canonical = seed.clone();
    put(&mut canonical, ".gitignore", ".ara/\n");
    let mut a1 = fork(&seed, "A");
    put(&mut a1, ".gitignore", ".ara/\n");
    let a2 = materialized(
        &try_merge_as(&seed, &a1, &canonical, "canonical", 1, "fork-a")
            .unwrap()
            .working,
    );
    assert!(text(&a2, "trace/aliases.yaml").contains(
        "\"source_key\":\"canonical\",\"label\":\"canonical\",\"original\":\".gitignore\""
    ));
    let fork_c = seed.clone();
    let plan = try_merge_as(&seed, &fork_c, &a2, "fork-a", 2, "fork-c").unwrap();
    let result = materialized(&plan.working);
    assert!(!result.files.contains_key(".gitignore"));
    let opaque = plan
        .report
        .conflicts
        .iter()
        .find(|item| item.path == ".gitignore")
        .unwrap();
    assert_eq!(
        (opaque.kind.as_str(), opaque.allowed.clone()),
        ("opaque_file", vec!["ours".to_owned()])
    );
    assert!(!text(&result, "trace/aliases.yaml").contains("\"original\":\".gitignore\""));
    assert_eq!(resolve(&result, "fork-a:C77").unwrap(), "C02");
}
