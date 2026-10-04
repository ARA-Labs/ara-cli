//! Shared fixtures for the peer-feedback merge tests.
#![allow(dead_code)]
use ara_core::merge::{MergeError, MergeOptions, MergePlan, plan_merge};
use ara_core::write::source::{FileSnapshot, digest};
use ara_core::write::{ArtifactSnapshot, WorkingArtifact};
use serde_json::Value;

pub const TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: root\n    children:\n      - id: N02\n        type: experiment\n        title: parent\n        result: base\n        children: []\n";
pub const CLAIMS: &str = "# Claims\n\n## C01: shared\n- **Statement**: base\n- **Status**: hypothesis\n- **Provenance**: user\n- **Dependencies**: []\n";

pub fn put(snapshot: &mut ArtifactSnapshot, path: &str, text: impl Into<String>) {
    let text = text.into();
    snapshot.files.insert(
        path.into(),
        FileSnapshot {
            digest: digest(text.as_bytes()),
            bytes: text.into_bytes(),
            existed: true,
            permissions: None,
        },
    );
}
pub fn text<'a>(snapshot: &'a ArtifactSnapshot, path: &str) -> &'a str {
    std::str::from_utf8(&snapshot.files[path].bytes).unwrap()
}
pub fn edit(snapshot: &mut ArtifactSnapshot, path: &str, from: &str, to: &str) {
    let current = text(snapshot, path).to_owned();
    assert!(current.contains(from), "{path} lacks {from}");
    put(snapshot, path, current.replace(from, to));
}
pub fn materialized(working: &WorkingArtifact) -> ArtifactSnapshot {
    let mut result = working.base.clone();
    for path in &working.deleted_paths {
        result.files.remove(path);
    }
    for (path, bytes) in &working.files {
        result.files.insert(
            path.clone(),
            FileSnapshot {
                bytes: bytes.clone(),
                existed: true,
                permissions: None,
                digest: digest(bytes),
            },
        );
    }
    result
}
pub fn try_merge(
    base: &ArtifactSnapshot,
    ours: &ArtifactSnapshot,
    theirs: &ArtifactSnapshot,
    key: &str,
    minute: u32,
) -> Result<MergePlan, MergeError> {
    plan_merge(
        base,
        ours,
        theirs,
        &MergeOptions {
            source_key: key.into(),
            label: key.into(),
            time: format!("2026-10-03T10:{minute:02}:00Z"),
            git: None,
            predecessor: None,
            self_key: None,
        },
    )
}
/// Like `try_merge`, naming this destination's own source key.
pub fn try_merge_as(
    base: &ArtifactSnapshot,
    ours: &ArtifactSnapshot,
    theirs: &ArtifactSnapshot,
    key: &str,
    minute: u32,
    own: &str,
) -> Result<MergePlan, MergeError> {
    plan_merge(
        base,
        ours,
        theirs,
        &MergeOptions {
            source_key: key.into(),
            label: key.into(),
            time: format!("2026-10-03T10:{minute:02}:00Z"),
            git: None,
            predecessor: None,
            self_key: Some(own.into()),
        },
    )
}
pub fn merge(
    base: &ArtifactSnapshot,
    ours: &ArtifactSnapshot,
    theirs: &ArtifactSnapshot,
    key: &str,
    minute: u32,
) -> (ArtifactSnapshot, MergePlan) {
    let plan = try_merge(base, ours, theirs, key, minute).unwrap();
    (materialized(&plan.working), plan)
}
pub fn clean(
    base: &ArtifactSnapshot,
    ours: &ArtifactSnapshot,
    theirs: &ArtifactSnapshot,
    key: &str,
    minute: u32,
) -> ArtifactSnapshot {
    let (result, plan) = merge(base, ours, theirs, key, minute);
    assert_eq!(plan.report.exit_code(), 0, "{:#?}", plan.report.conflicts);
    result
}
pub fn claim(id: &str, title: &str, statement: &str) -> String {
    format!(
        "\n## {id}: {title}\n- **Statement**: {statement}\n- **Status**: hypothesis\n- **Provenance**: user\n- **Dependencies**: []\n"
    )
}
pub fn add_claim(snapshot: &mut ArtifactSnapshot, id: &str, title: &str, statement: &str) {
    let current = text(snapshot, "logic/claims.md").to_owned();
    put(
        snapshot,
        "logic/claims.md",
        current + &claim(id, title, statement),
    );
}
pub fn headings(snapshot: &ArtifactSnapshot) -> Vec<String> {
    text(snapshot, "logic/claims.md")
        .lines()
        .filter(|line| line.starts_with("## "))
        .map(str::to_owned)
        .collect()
}
pub fn records(snapshot: &ArtifactSnapshot) -> Vec<Value> {
    text(snapshot, "trace/merge_log.yaml")
        .lines()
        .filter_map(|line| line.strip_prefix("  - "))
        .map(|row| serde_json::from_str(row).unwrap())
        .collect()
}
pub fn kinds(snapshot: &ArtifactSnapshot, key: &str) -> Vec<String> {
    records(snapshot)
        .iter()
        .filter(|row| row["source_key"] == key)
        .map(|row| row["kind"].as_str().unwrap().to_owned())
        .collect()
}
pub fn session(id: &str, summary: &str, node: &str, claim: &str) -> String {
    format!(
        "session:\n  id: '{id}'\n  date: '{date}'\n  started: '{date}T10:00:00Z'\n  last_turn: '{date}T11:00:00Z'\n  turn_count: 1\n  summary: {summary}\nevents_logged:\n  - {{turn: 1, type: experiment, id: {node}, routing: exploration, provenance: ai-executed, summary: event}}\nai_actions:\n  - {{turn: 1, action: complete, provenance: ai-executed, files_changed: []}}\nclaims_touched:\n  - {{id: {claim}, action: created, turn: 1}}\nlogic_revisions: []\nkey_context: []\nopen_threads: []\nai_suggestions_pending: []\n",
        date = &id[..10]
    )
}
pub fn index(rows: &[(&str, &str, &str)]) -> String {
    let mut result = String::from("sessions:\n");
    for (id, summary, claim) in rows {
        result.push_str(&format!(
            "  - {{id: '{id}', date: '{}', summary: {summary}, turn_count: 1, events_count: 1, claims_touched: [{claim}], open_threads: 0}}\n",
            &id[..10]
        ));
    }
    result
}
/// A fork that adds one claim, one tree node, and one session with colliding
/// local identities (C77, N03, 2026-10-02_001).
pub fn fork(seed: &ArtifactSnapshot, who: &str) -> ArtifactSnapshot {
    let mut fork = seed.clone();
    add_claim(
        &mut fork,
        "C77",
        &format!("fork {who} finding"),
        &format!("{who} says x"),
    );
    put(
        &mut fork,
        "trace/exploration_tree.yaml",
        TREE.replace(
            "        children: []\n",
            &format!("        children:\n          - id: N03\n            type: experiment\n            title: {who} experiment\n            result: {who} result\n            children: []\n"),
        ),
    );
    let summary = format!("{who} work");
    put(
        &mut fork,
        "trace/sessions/2026-10-02_001.yaml",
        session("2026-10-02_001", &summary, "N03", "C77"),
    );
    put(
        &mut fork,
        "trace/sessions/session_index.yaml",
        index(&[("2026-10-02_001", &summary, "C77")]),
    );
    fork
}
pub fn seed() -> ArtifactSnapshot {
    let mut seed = ArtifactSnapshot {
        root: std::path::PathBuf::from("/virtual/peer-feedback"),
        identity_paths: Default::default(),
        files: Default::default(),
    };
    put(&mut seed, "trace/exploration_tree.yaml", TREE);
    put(&mut seed, "logic/claims.md", CLAIMS);
    seed
}
