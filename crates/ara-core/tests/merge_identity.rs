#![cfg(feature = "native")]
use ara_core::merge::{
    MergeOptions, fingerprint, plan_merge, plan_resolution, resolve, resolve_local,
};
use ara_core::write::source::{FileSnapshot, digest};
use ara_core::write::{ArtifactSnapshot, WorkingArtifact};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn snapshot(files: &[(&str, &str)]) -> ArtifactSnapshot {
    ArtifactSnapshot {
        root: std::path::PathBuf::from("/virtual/merge-test"),
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
fn put(snapshot: &mut ArtifactSnapshot, path: &str, text: impl Into<String>) {
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
fn text<'a>(snapshot: &'a ArtifactSnapshot, path: &str) -> &'a str {
    std::str::from_utf8(&snapshot.files[path].bytes).unwrap()
}
fn materialized(working: &WorkingArtifact) -> ArtifactSnapshot {
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
fn options() -> MergeOptions {
    MergeOptions {
        source_key: "bob-fork".into(),
        label: "bob".into(),
        time: "2026-10-01T12:00:00Z".into(),
        git: None,
        predecessor: None,
    }
}
const TREE: &str = "tree:\n  - id: N01\n    type: question\n    title: root\n    children:\n      - id: N02\n        type: experiment\n        title: parent\n        result: base\n        children: []\n";
const CLAIMS: &str = "# Claims\n\n## C01: shared\n- **Statement**: base\n- **Status**: hypothesis\n- **Provenance**: user\n- **Dependencies**: []\n";
fn base() -> ArtifactSnapshot {
    snapshot(&[
        ("trace/exploration_tree.yaml", TREE),
        ("logic/claims.md", CLAIMS),
    ])
}

#[test]
fn numeric_source_only_import_is_fresh_even_without_collision_and_replay_is_exact() {
    let base = base();
    let ours = base.clone();
    let mut theirs = base.clone();
    put(
        &mut theirs,
        "logic/claims.md",
        format!(
            "{CLAIMS}\n## C77: incoming\n- **Statement**: complete source\n- **Status**: hypothesis\n- **Provenance**: user\n- **Dependencies**: [C01]\n"
        ),
    );
    let plan = plan_merge(&base, &ours, &theirs, &options()).unwrap();
    assert_eq!(plan.report.exit_code(), 0);
    let result = materialized(&plan.working);
    assert!(text(&result, "logic/claims.md").contains("## C02: incoming"));
    assert_eq!(resolve(&result, "bob:C77").unwrap(), "C02");
    assert_eq!(resolve(&result, "bob:C01").unwrap(), "C01");
    let replay = plan_merge(&base, &result, &theirs, &options()).unwrap();
    assert_eq!(replay.report.exit_code(), 0);
    assert_eq!(replay.working.changed_paths(), Vec::<String>::new());
    assert_eq!(
        fingerprint(&materialized(&replay.working)),
        fingerprint(&result)
    );
}

#[test]
fn unresolved_field_conflict_replay_keeps_exact_ids_values_and_bytes() {
    let base = base();
    let mut ours = base.clone();
    let mut theirs = base.clone();
    put(
        &mut ours,
        "logic/claims.md",
        CLAIMS.replace("Statement**: base", "Statement**: ours"),
    );
    put(
        &mut theirs,
        "logic/claims.md",
        CLAIMS.replace("Statement**: base", "Statement**: theirs"),
    );
    let plan = plan_merge(&base, &ours, &theirs, &options()).unwrap();
    assert_eq!(plan.report.exit_code(), 1);
    let conflict = plan
        .report
        .logic_conflicts
        .iter()
        .find(|c| String::from_utf8_lossy(&c.theirs.bytes).contains("theirs"))
        .unwrap();
    assert!(
        std::str::from_utf8(&conflict.base.bytes)
            .unwrap()
            .contains("base")
    );
    assert!(
        std::str::from_utf8(&conflict.ours.bytes)
            .unwrap()
            .contains("ours")
    );
    assert!(
        std::str::from_utf8(&conflict.theirs.bytes)
            .unwrap()
            .contains("theirs")
    );
    let result = materialized(&plan.working);
    assert_eq!(
        text(&result, "logic/claims.md"),
        text(&ours, "logic/claims.md")
    );
    let replay = plan_merge(&base, &result, &theirs, &options()).unwrap();
    assert_eq!(replay.report.exit_code(), 1);
    assert_eq!(replay.report.conflicts, plan.report.conflicts);
    assert!(replay.working.changed_paths().is_empty());
}

#[test]
fn advancing_source_uses_previous_imported_values_and_mapping_then_rejects_regression() {
    let base = base();
    let mut first = base.clone();
    put(
        &mut first,
        "logic/claims.md",
        format!(
            "{CLAIMS}\n## C77: imported\n- **Statement**: first source\n- **Status**: hypothesis\n- **Provenance**: user\n"
        ),
    );
    let initial = plan_merge(&base, &base, &first, &options()).unwrap();
    let ours = materialized(&initial.working);
    let mut second = first.clone();
    put(
        &mut second,
        "logic/claims.md",
        text(&first, "logic/claims.md").replace("first source", "advancing source"),
    );
    let advanced = plan_merge(&first, &ours, &second, &options()).unwrap();
    assert_eq!(advanced.report.exit_code(), 0);
    let result = materialized(&advanced.working);
    assert!(text(&result, "logic/claims.md").contains("## C02: imported"));
    assert!(text(&result, "logic/claims.md").contains("advancing source"));
    assert!(!text(&result, "logic/claims.md").contains("first source"));
    let regression = plan_merge(&base, &result, &first, &options())
        .err()
        .unwrap();
    assert_eq!(regression.code, "merge.source_regression");
    assert_eq!(regression.exit_code(), 1);
}

#[test]
fn changed_source_without_ancestry_proof_and_reused_labels_fail_closed() {
    let base = base();
    let mut first_source = base.clone();
    put(
        &mut first_source,
        "logic/claims.md",
        CLAIMS.replace("Statement**: base", "Statement**: first source"),
    );
    let first = plan_merge(&base, &base, &first_source, &options()).unwrap();
    let ours = materialized(&first.working);
    let mut advancing = first_source.clone();
    put(
        &mut advancing,
        "logic/claims.md",
        CLAIMS.replace("Statement**: base", "Statement**: advanced"),
    );
    assert_eq!(
        plan_merge(&base, &ours, &advancing, &options())
            .err()
            .unwrap()
            .code,
        "merge.unproven_source_revision"
    );
    let mut reused = options();
    reused.source_key = "unrelated".into();
    assert_eq!(
        plan_merge(&base, &ours, &first_source, &reused)
            .err()
            .unwrap()
            .code,
        "merge.ambiguous_label"
    );
    let mut renamed = options();
    renamed.label = "robert".into();
    let new_label = plan_merge(&base, &ours, &first_source, &renamed).unwrap();
    let result = materialized(&new_label.working);
    assert_eq!(resolve(&result, "robert:C01").unwrap(), "C01");
    assert_eq!(resolve(&result, "bob:C01").unwrap(), "C01");
    assert!(
        plan_merge(&base, &result, &first_source, &renamed)
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
}

#[test]
fn opaque_and_external_conflicts_retain_full_binary_bytes_without_copying() {
    let base = base();
    let ours = base.clone();
    let mut theirs = base.clone();
    for (path, data) in [
        ("logic/private.bin", vec![0, 255, 10, 128]),
        ("evidence/results.bin", vec![1, 254, 13, 129]),
        ("src/work.bin", vec![2, 253, 14, 130]),
        ("evidence/empty.bin", vec![]),
    ] {
        theirs.files.insert(
            path.into(),
            FileSnapshot {
                digest: format!("{:x}", Sha256::digest(&data)),
                bytes: data,
                existed: true,
                permissions: None,
            },
        );
    }
    let plan = plan_merge(&base, &ours, &theirs, &options()).unwrap();
    assert_eq!(plan.report.exit_code(), 1);
    for path in [
        "logic/private.bin",
        "evidence/results.bin",
        "src/work.bin",
        "evidence/empty.bin",
    ] {
        let conflict = plan
            .report
            .conflicts
            .iter()
            .find(|c| c.path == path)
            .unwrap();
        assert_eq!(conflict.theirs.bytes, theirs.files[path].bytes);
        assert!(!plan.working.exists(path));
    }
    let result = materialized(&plan.working);
    let replay = plan_merge(&base, &result, &theirs, &options()).unwrap();
    assert_eq!(replay.report.conflicts, plan.report.conflicts);
    assert!(replay.working.changed_paths().is_empty());

    let ledger =
        ara_core::write::positions::YamlDocument::parse(text(&result, "trace/merge_log.yaml"))
            .unwrap()
            .root
            .to_json()
            .unwrap();
    let revision = ledger["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["kind"] == "revision")
        .unwrap();
    for (path, captured) in &theirs.files {
        assert_eq!(
            STANDARD
                .decode(revision["files"][path].as_str().unwrap())
                .unwrap(),
            captured.bytes
        );
    }

    // Import an artifact carrying portable history, so transport capture and
    // foreign source replay exercise the same byte codec through the merger.
    let mut transported_options = options();
    transported_options.source_key = "transport-fork".into();
    transported_options.label = "transport".into();
    let transported = plan_merge(&base, &base, &result, &transported_options).unwrap();
    let transported_result = materialized(&transported.working);
    let transported_ledger = ara_core::write::positions::YamlDocument::parse(text(
        &transported_result,
        "trace/merge_log.yaml",
    ))
    .unwrap()
    .root
    .to_json()
    .unwrap();
    for path in ["trace/merge_log.yaml", "trace/aliases.yaml"] {
        let transport = transported_ledger["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["kind"] == "transport" && record["path"] == path)
            .unwrap();
        assert_eq!(
            STANDARD
                .decode(transport["bytes"].as_str().unwrap())
                .unwrap(),
            result.files[path].bytes
        );
    }
    let replay = plan_merge(&base, &transported_result, &result, &transported_options).unwrap();
    assert_eq!(replay.report.conflicts, transported.report.conflicts);
    assert!(replay.working.changed_paths().is_empty());

    for corrupt in [
        serde_json::json!("AA"),
        serde_json::json!("AB=="),
        serde_json::json!("AA==="),
        serde_json::json!("AA==\n"),
        serde_json::json!("_w=="),
        serde_json::json!([0, 255, 10, 128]),
    ] {
        let mut corrupt_ledger = ledger.clone();
        let revision = corrupt_ledger["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|record| record["kind"] == "revision")
            .unwrap();
        revision["files"]["logic/private.bin"] = corrupt;
        let mut damaged = result.clone();
        put(
            &mut damaged,
            "trace/merge_log.yaml",
            corrupt_ledger.to_string(),
        );
        let error = plan_merge(&base, &damaged, &theirs, &options())
            .err()
            .unwrap();
        assert_eq!(error.code, "merge.corrupt_ledger");
        assert_eq!(
            error.evidence[0].ours.bytes,
            damaged.files["trace/merge_log.yaml"].bytes
        );
    }
}

#[test]
fn alias_cycles_and_dangling_targets_are_content_errors_with_full_evidence() {
    for aliases in [
        "format: ara.aliases/v1\naliases:\n  - {source_key: a, label: a, original: N01, target: 'a:N02', revision: x}\n  - {source_key: a, label: a, original: N02, target: 'a:N01', revision: x}\n",
        "format: ara.aliases/v1\naliases:\n  - {source_key: a, label: a, original: N01, target: N999, revision: x}\n",
    ] {
        let base = base();
        let mut theirs = base.clone();
        put(&mut theirs, "trace/aliases.yaml", aliases);
        let error = plan_merge(&base, &base, &theirs, &options()).err().unwrap();
        assert_eq!(error.exit_code(), 1);
        assert!(matches!(
            error.code.as_str(),
            "merge.alias_cycle" | "merge.alias_dangling"
        ));
        assert!(!error.evidence.is_empty());
    }
}

#[test]
fn local_redirects_follow_chains_and_reject_cycles_or_missing_targets() {
    let mut artifact = base();
    put(
        &mut artifact,
        "trace/logic_mutations.yaml",
        "mutations:\n  - {action: rename, from: 'logic/claims.md:C77', to: 'logic/claims.md:C88', historical_references: []}\n  - {action: rename, from: 'logic/claims.md:C88', to: 'logic/claims.md:C01', historical_references: []}\n",
    );
    assert_eq!(
        resolve_local(&artifact, "logic/claims.md:C77").unwrap(),
        "C01"
    );
    assert_eq!(resolve_local(&artifact, "C77").unwrap(), "C01");
    put(
        &mut artifact,
        "trace/logic_mutations.yaml",
        "mutations:\n  - {action: rename, from: 'logic/claims.md:C77', to: 'logic/claims.md:C88', historical_references: []}\n  - {action: rename, from: 'logic/claims.md:C88', to: 'logic/claims.md:C77', historical_references: []}\n",
    );
    assert_eq!(
        resolve_local(&artifact, "C77").err().unwrap().code,
        "merge.alias_cycle"
    );
}

fn session(id: &str, node: &str) -> String {
    format!(
        "session:\n  id: '{id}'\n  date: '{}'\n  started: '{}T10:00:00Z'\n  last_turn: '{}T11:00:00Z'\n  turn_count: 1\n  summary: full history\nevents_logged:\n  - {{turn: 1, type: experiment, id: {node}, routing: exploration, provenance: ai-executed, summary: event}}\nai_actions:\n  - {{turn: 1, action: complete, provenance: ai-executed, files_changed: []}}\nclaims_touched: []\nlogic_revisions:\n  - {{turn: 1, entry: C01, field: Statement, before: base, after: base, signal: user-directive, provenance: user}}\nkey_context:\n  - {{turn: 1, excerpt: complete context}}\nopen_threads: [thread]\nai_suggestions_pending: [suggestion]\n",
        &id[..10],
        &id[..10],
        &id[..10]
    )
}
fn index(ids: &[&str]) -> String {
    let mut result = String::from("sessions:\n");
    for id in ids {
        result.push_str(&format!("  - {{id: '{id}', date: '{}', summary: full history, turn_count: 1, events_count: 1, claims_touched: [], open_threads: 1}}\n",&id[..10]));
    }
    result
}
fn full_base() -> ArtifactSnapshot {
    let mut base = base();
    for (path, content) in [
        (
            "PAPER.md",
            "---\ntitle: lineage\nknowledge_paths: [appendix/details.md]\n---\n# Artifact\n\n## Overview\nbase overview\n",
        ),
        ("logic/problem.md", "# Problem\n\n## Gap\nbase problem\n"),
        (
            "logic/solution/algorithm.md",
            "# Algorithm\n\n## Method\nbase method\n",
        ),
        (
            "logic/solution/heuristics.md",
            "# Heuristics\n\n## H01: baseline\n- **Rationale**: base rationale\n- **Provenance**: user\n",
        ),
        (
            "logic/experiments.md",
            "# Experiments\n\n## E01: baseline\n- **Question**: base question\n- **Status**: planned\n",
        ),
        (
            "logic/concepts.md",
            "# Concepts\n\n## Term\n- **Definition**: base definition\n",
        ),
        (
            "logic/related_work.md",
            "# Related Work\n\n## RW01: prior\n- **Title**: base title\n- **What**: base relation\n",
        ),
        (
            "rubric/requirements.md",
            "# Requirements\n\n## Accuracy\nbase rubric\n",
        ),
        (
            "appendix/details.md",
            "# Details\n\n## Explanation\nbase appendix\n",
        ),
        ("staging/observations.yaml", "observations: []\n"),
        ("trace/taste_log.yaml", "entries: []\n"),
        ("trace/pm_reasoning_log.yaml", "entries: []\n"),
    ] {
        put(&mut base, path, content);
    }
    put(
        &mut base,
        "trace/sessions/2026-09-30_001.yaml",
        session("2026-09-30_001", "N02"),
    );
    put(
        &mut base,
        "trace/sessions/session_index.yaml",
        index(&["2026-09-30_001"]),
    );
    base
}

#[test]
fn generated_full_layer_histories_preserve_directional_union_and_all_import_addresses() {
    let mut random = 0x7ad3_u64;
    for _ in 0..12 {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        let seed = random >> 32;
        let base = full_base();
        let mut ours = base.clone();
        let mut theirs = base.clone();
        put(
            &mut ours,
            "trace/exploration_tree.yaml",
            format!(
                "{TREE}      - id: N124\n        type: experiment\n        title: ours-{seed}\n        result: ours N124 remains\n"
            ),
        );
        put(&mut theirs,"trace/exploration_tree.yaml",TREE.replace("        children: []","        children:\n          - id: N124\n            type: experiment\n            title: theirs\n            result: incoming N124\n            unknown: {nested: [1, '雪', {exact: true}]}"));
        for (path, prefix, field) in [
            ("logic/claims.md", "C", "Statement"),
            ("logic/solution/heuristics.md", "H", "Rationale"),
            ("logic/experiments.md", "E", "Question"),
        ] {
            let original = text(&base, path);
            put(
                &mut ours,
                path,
                format!(
                    "{original}\n## {prefix}99: ours-{seed}\n- **{field}**: ours N124\n- **Provenance**: user\n"
                ),
            );
            put(
                &mut theirs,
                path,
                format!(
                    "{original}\n## {prefix}99: theirs-{seed}\n- **{field}**: incoming N124\n- **Provenance**: user\n"
                ),
            );
        }
        put(
            &mut ours,
            "staging/observations.yaml",
            "observations:\n  - {id: O99, timestamp: '2026-10-01T10:00:00Z', provenance: user, content: ours, potential_type: claim, bound_to: [N124], promoted: false, promoted_to: null, crystallized_via: null, stale: false}\n",
        );
        put(
            &mut theirs,
            "staging/observations.yaml",
            "observations:\n  - {id: O99, timestamp: '2026-10-01T10:00:00Z', provenance: user, content: incoming, potential_type: claim, bound_to: [N124], promoted: true, promoted_to: 'logic/claims.md:C99', crystallized_via: artifact-commitment, stale: false}\n",
        );
        for fork in [&mut ours, &mut theirs] {
            put(
                fork,
                "trace/sessions/2026-10-01_001.yaml",
                session("2026-10-01_001", "N124"),
            );
            put(
                fork,
                "trace/sessions/session_index.yaml",
                index(&["2026-09-30_001", "2026-10-01_001"]),
            );
            put(
                fork,
                "trace/taste_log.yaml",
                "entries:\n  - {id: T99, timestamp: '2026-10-01T11:00:00Z', target: N124, tag: endorse, object: evidence, comment: complete original comment}\n",
            );
            put(
                fork,
                "trace/pm_reasoning_log.yaml",
                "entries:\n  - {turn: '2026-10-01_001#1', notes: [complete original reasoning]}\n",
            );
        }
        for (path, old, new) in [
            (
                "logic/concepts.md",
                "base definition",
                "incoming definition",
            ),
            (
                "logic/related_work.md",
                "base relation",
                "incoming relation",
            ),
            ("logic/problem.md", "base problem", "incoming problem"),
            (
                "logic/solution/algorithm.md",
                "base method",
                "incoming method",
            ),
            ("PAPER.md", "base overview", "incoming overview"),
            ("rubric/requirements.md", "base rubric", "incoming rubric"),
            ("appendix/details.md", "base appendix", "incoming appendix"),
        ] {
            put(&mut theirs, path, text(&base, path).replace(old, new));
        }
        let plan = plan_merge(&base, &ours, &theirs, &options()).unwrap();
        assert_eq!(plan.report.exit_code(), 0, "{:?}", plan.report.conflicts);
        let result = materialized(&plan.working);
        let expected: BTreeMap<&str, &str> = BTreeMap::from([
            ("N124", "N125"),
            ("C99", "C100"),
            ("H99", "H100"),
            ("E99", "E100"),
            ("O99", "O100"),
            ("T99", "T100"),
            ("2026-10-01_001", "2026-10-01_002"),
        ]);
        for (original, target) in expected {
            assert_eq!(
                resolve(&result, &format!("bob:{original}")).unwrap(),
                target
            );
        }
        assert!(
            text(&result, "trace/exploration_tree.yaml").contains(&format!("title: ours-{seed}"))
        );
        assert!(
            text(&result, "trace/exploration_tree.yaml")
                .contains("unknown: {nested: [1, '雪', {exact: true}]}")
        );
        assert!(
            text(&result, "trace/exploration_tree.yaml")
                .contains("            result: incoming N125")
        );
        assert!(text(&result, "logic/claims.md").contains("ours N124"));
        assert!(text(&result, "logic/claims.md").contains("incoming N125"));
        assert!(text(&result, "staging/observations.yaml").contains("C100"));
        assert!(text(&result, "trace/sessions/2026-10-01_002.yaml").contains("id: N125"));
        for path in [
            "logic/concepts.md",
            "logic/related_work.md",
            "logic/problem.md",
            "logic/solution/algorithm.md",
            "PAPER.md",
            "rubric/requirements.md",
            "appendix/details.md",
        ] {
            assert_eq!(text(&result, path), text(&theirs, path));
            assert_eq!(resolve(&result, &format!("bob:{path}")).unwrap(), path);
        }
        let replay = plan_merge(&base, &result, &theirs, &options()).unwrap();
        assert_eq!(replay.report.exit_code(), 0);
        assert!(replay.working.changed_paths().is_empty());
        assert_eq!(
            fingerprint(&result),
            fingerprint(&materialized(&replay.working))
        );
    }
}

#[test]
fn mutable_resolution_is_explicit_audited_and_stale_fingerprints_reject() {
    let base = full_base();
    let mut ours = base.clone();
    let mut theirs = base.clone();
    put(
        &mut ours,
        "logic/claims.md",
        CLAIMS.replace("Statement**: base", "Statement**: ours"),
    );
    put(
        &mut theirs,
        "logic/claims.md",
        CLAIMS.replace("Statement**: base", "Statement**: theirs"),
    );
    let initial = plan_merge(&base, &ours, &theirs, &options()).unwrap();
    let current = materialized(&initial.working);
    let conflict = &initial.report.logic_conflicts[0];
    let resolved = plan_resolution(
        &current,
        &conflict.id,
        "ours",
        "2026-09-30_001",
        2,
        "user-directive",
        "user",
    )
    .unwrap();
    let result = materialized(&resolved);
    assert_eq!(
        text(&result, "logic/claims.md"),
        text(&ours, "logic/claims.md")
    );
    assert!(text(&result, "trace/merge_log.yaml").contains("resolution"));
    assert!(text(&result, "trace/sessions/2026-09-30_001.yaml").contains(&conflict.id));
    let replay = plan_merge(&base, &result, &theirs, &options()).unwrap();
    assert_eq!(replay.report.exit_code(), 0);
    assert!(replay.working.changed_paths().is_empty());

    let mut relay_options = options();
    relay_options.source_key = "relay-fork".into();
    relay_options.label = "relay".into();
    let relayed = plan_merge(&base, &ours, &current, &relay_options).unwrap();
    let relay_before = materialized(&relayed.working);
    let acknowledged = plan_merge(&current, &relay_before, &result, &relay_options).unwrap();
    assert_eq!(acknowledged.report.exit_code(), 0);
    let relay_after = materialized(&acknowledged.working);
    let mut ledger =
        ara_core::write::positions::YamlDocument::parse(text(&relay_after, "trace/merge_log.yaml"))
            .unwrap()
            .root
            .to_json()
            .unwrap();
    let imported_resolution = ledger["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["kind"] == "imported_resolution")
        .unwrap();
    let decoded = STANDARD
        .decode(imported_resolution["evidence"].as_str().unwrap())
        .unwrap();
    let mut evidence: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(evidence["conflict_id"], conflict.id);
    assert_eq!(evidence["take"], "ours");
    let replay = plan_merge(&current, &relay_after, &result, &relay_options).unwrap();
    assert_eq!(replay.report.exit_code(), 0);
    assert!(replay.working.changed_paths().is_empty());

    evidence["selected_fingerprint"] = serde_json::json!("tampered-selected-value");
    imported_resolution["evidence"] =
        serde_json::json!(STANDARD.encode(serde_json::to_vec(&evidence).unwrap()));
    let mut tampered = relay_after.clone();
    put(&mut tampered, "trace/merge_log.yaml", ledger.to_string());
    let error = plan_merge(&current, &tampered, &result, &relay_options)
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.corrupt_ledger");
    let mut stale = current.clone();
    put(
        &mut stale,
        "logic/claims.md",
        text(&current, "logic/claims.md").replace("Statement**: ours", "Statement**: later"),
    );
    assert_eq!(
        plan_resolution(
            &stale,
            &conflict.id,
            "theirs",
            "2026-09-30_001",
            2,
            "user-directive",
            "user"
        )
        .err()
        .unwrap()
        .code,
        "merge.stale_conflict"
    );
}

#[test]
fn exact_temporary_components_are_excluded_without_suppressing_ordinary_files() {
    let original = base();
    let mut private = original.clone();
    for path in [
        ".git/config",
        ".ara/transactions/active.json.prepared",
        "src/.git/config",
        "logic/.ara-write-123-456",
        "evidence/.ara-write-123-456/body",
    ] {
        put(&mut private, path, "private");
    }
    assert_eq!(fingerprint(&original), fingerprint(&private));
    let plan = plan_merge(&original, &original, &private, &options()).unwrap();
    assert_eq!(plan.report.exit_code(), 0);
    assert!(
        !plan
            .report
            .imports
            .iter()
            .any(|mapping| mapping.original.contains(".ara-write-")
                || mapping.original.contains(".git/"))
    );
    let result = materialized(&plan.working);
    assert!(
        plan_merge(&original, &result, &private, &options())
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
    put(&mut private, "logic/.ara-write-not-a-temp", "ordinary");
    assert_ne!(fingerprint(&original), fingerprint(&private));
    assert!(
        plan_merge(&original, &original, &private, &options())
            .unwrap()
            .report
            .conflicts
            .iter()
            .any(|item| item.path == "logic/.ara-write-not-a-temp")
    );
}

fn install_mutation_audit(snapshot: &mut ArtifactSnapshot, mutation: serde_json::Value) {
    let id = "2026-10-01_001";
    let mut value = ara_core::write::positions::YamlDocument::parse(&session(id, "N02"))
        .unwrap()
        .root
        .to_json()
        .unwrap();
    value["logic_revisions"] = serde_json::json!([{"turn":1,"entry":mutation["from_selector"],"field":"entry","before":mutation["before"],"after":mutation["after"],"signal":mutation["signal"],"provenance":mutation["provenance"]}]);
    put(
        snapshot,
        "trace/sessions/2026-10-01_001.yaml",
        ara_core::write::source::render_yaml(&value, 0, "\n"),
    );
    put(snapshot, "trace/sessions/session_index.yaml", index(&[id]));
    put(
        snapshot,
        "trace/logic_mutations.yaml",
        format!("mutations:\n  - {mutation}\n"),
    );
}
#[test]
fn retired_source_numeric_id_is_reserved_and_resolves_through_imported_rename() {
    let base = base();
    let mut theirs = base.clone();
    let after = "## C88: renamed source claim\n- **Statement**: alive\n- **Status**: hypothesis\n- **Provenance**: user\n";
    let before = after.replace("C88:", "C77:");
    put(&mut theirs, "logic/claims.md", format!("{CLAIMS}\n{after}"));
    install_mutation_audit(
        &mut theirs,
        serde_json::json!({"action":"rename","from":"logic/claims.md:C77","to":"logic/claims.md:C88","from_selector":{"document":"logic/claims.md","heading":[],"entry":"C77"},"to_selector":{"document":"logic/claims.md","heading":[],"entry":"C88"},"before":before,"after":after,"session":"2026-10-01_001","turn":1,"signal":"user-directive","provenance":"user","historical_references":[]}),
    );
    let plan = plan_merge(&base, &base, &theirs, &options()).unwrap();
    let result = materialized(&plan.working);
    let retired = plan
        .report
        .imports
        .iter()
        .find(|mapping| mapping.original == "C77")
        .unwrap();
    let live = plan
        .report
        .imports
        .iter()
        .find(|mapping| mapping.original == "C88")
        .unwrap();
    assert_ne!(retired.target, live.target);
    assert_eq!(resolve(&result, "bob:C77").unwrap(), live.target);
    assert_eq!(resolve(&result, "bob:C88").unwrap(), live.target);
    let maximum = retired.target[1..]
        .parse::<u64>()
        .unwrap()
        .max(live.target[1..].parse().unwrap());
    assert_eq!(
        plan.working
            .allocate_id('C', &["C01".into(), live.target.clone()], None)
            .unwrap(),
        format!("C{:02}", maximum + 1)
    );
    assert_eq!(
        plan.working
            .allocate_id(
                'C',
                &["C01".into(), live.target.clone()],
                Some(&retired.target)
            )
            .unwrap_err()
            .code,
        "write.id_collision"
    );
    let rows = ara_core::write::positions::YamlDocument::parse(text(
        &result,
        "trace/logic_mutations.yaml",
    ))
    .unwrap()
    .root
    .to_json()
    .unwrap();
    assert_eq!(rows["mutations"][0]["before"], before);
    assert_eq!(rows["mutations"][0]["after"], after);
    assert!(
        plan_merge(&base, &result, &theirs, &options())
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
}

#[test]
fn removed_source_tombstone_reserves_mapping_without_a_false_live_alias() {
    let base = base();
    let mut theirs = base.clone();
    let before = "## C77: retired source claim\n- **Statement**: archived complete source\n";
    install_mutation_audit(
        &mut theirs,
        serde_json::json!({"action":"remove","from":"logic/claims.md:C77","to":null,"from_selector":{"document":"logic/claims.md","heading":[],"entry":"C77"},"to_selector":null,"before":before,"after":"","session":"2026-10-01_001","turn":1,"signal":"user-directive","provenance":"user","historical_references":[]}),
    );
    let plan = plan_merge(&base, &base, &theirs, &options()).unwrap();
    let result = materialized(&plan.working);
    assert!(
        plan.report
            .imports
            .iter()
            .any(|mapping| mapping.original == "C77" && mapping.layer == "historical_identity")
    );
    assert_eq!(
        resolve(&result, "bob:C77").unwrap_err().code,
        "merge.unknown_identity"
    );
    assert!(
        plan_merge(&base, &result, &theirs, &options())
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
}

#[test]
fn native_short_heading_redirects_are_canonical_and_protected_or_cross_type_origins_reject() {
    let mut artifact = base();
    put(
        &mut artifact,
        "logic/solution/method.md",
        "# Method\n\n## New section\ncurrent body\n",
    );
    put(
        &mut artifact,
        "trace/logic_mutations.yaml",
        "mutations:\n  - {action: rename, from: 'logic/solution/method.md:Method/Old section', to: 'logic/solution/method.md:Method/New section', from_selector: {document: logic/solution/method.md, heading: [Method, Old section], entry: null}, to_selector: {document: logic/solution/method.md, heading: [Method, New section], entry: null}, historical_references: []}\n",
    );
    assert_eq!(
        resolve_local(&artifact, "logic/solution/method.md:Old section").unwrap(),
        "logic/solution/method.md#Method/New section"
    );
    assert_eq!(
        resolve_local(&artifact, "logic/solution/method.md#Method/Old section").unwrap(),
        "logic/solution/method.md#Method/New section"
    );
    for from in [
        "trace:N77",
        "staging/observations.yaml:O77",
        "src/work.md:C77",
        "evidence/results.md:C77",
    ] {
        put(
            &mut artifact,
            "trace/logic_mutations.yaml",
            format!(
                "mutations:\n  - {{action: rename, from: '{from}', to: 'logic/claims.md:C01', historical_references: []}}\n"
            ),
        );
        assert_eq!(
            resolve_local(&artifact, from).unwrap_err().code,
            "merge.redirect_data"
        );
    }
    put(
        &mut artifact,
        "logic/solution/heuristics.md",
        "# Heuristics\n\n## H01: live\n- **Rationale**: current\n",
    );
    put(
        &mut artifact,
        "trace/logic_mutations.yaml",
        "mutations:\n  - {action: rename, from: 'logic/claims.md:C77', to: 'logic/solution/heuristics.md:H01', historical_references: []}\n",
    );
    assert_eq!(
        resolve_local(&artifact, "C77").unwrap_err().code,
        "merge.redirect_data"
    );
}

fn foreign_history_source() -> ArtifactSnapshot {
    let base = base();
    let mut source = base.clone();
    put(
        &mut source,
        "logic/claims.md",
        format!(
            "{CLAIMS}\n## C77: foreign source\n- **Statement**: foreign source value\n- **Status**: hypothesis\n- **Provenance**: user\n"
        ),
    );
    let mut foreign = options();
    foreign.source_key = "charlie-fork".into();
    foreign.label = "charlie".into();
    materialized(&plan_merge(&base, &base, &source, &foreign).unwrap().working)
}

#[test]
fn native_portable_metadata_deletion_or_rewrite_rejects_with_complete_preimages() {
    let base = foreign_history_source();
    for path in ["trace/aliases.yaml", "trace/merge_log.yaml"] {
        let mut deleted = base.clone();
        deleted.files.remove(path);
        let error = plan_merge(&base, &base, &deleted, &options())
            .err()
            .unwrap();
        assert_eq!(error.code, "merge.protected_content");
        let item = error
            .evidence
            .iter()
            .find(|item| item.path == path)
            .unwrap();
        assert_eq!(item.base.bytes, base.files[path].bytes);
        assert!(!item.theirs.present);
        let mut rewritten = base.clone();
        let old = text(&base, path);
        let new = if path.ends_with("aliases.yaml") {
            old.replace("\"original\":\"C77\"", "\"original\":\"C777\"")
        } else {
            old.replace("2026-10-01T12:00:00Z", "2026-10-01T13:00:00Z")
        };
        assert_ne!(old, new);
        put(&mut rewritten, path, new);
        let error = plan_merge(&base, &base, &rewritten, &options())
            .err()
            .unwrap();
        assert_eq!(error.code, "merge.protected_content");
        assert_eq!(
            error
                .evidence
                .iter()
                .find(|item| item.path == path)
                .unwrap()
                .theirs
                .bytes,
            rewritten.files[path].bytes
        );
    }
}

#[test]
fn advancing_foreign_source_restores_exact_metadata_and_all_qualified_aliases() {
    let base = base();
    let first = foreign_history_source();
    let ours = materialized(
        &plan_merge(&base, &base, &first, &options())
            .unwrap()
            .working,
    );
    let mut second = first.clone();
    put(
        &mut second,
        "logic/claims.md",
        text(&first, "logic/claims.md").replace("foreign source value", "next source value"),
    );
    let advanced = plan_merge(&first, &ours, &second, &options()).unwrap();
    let result = materialized(&advanced.working);
    assert_eq!(advanced.report.exit_code(), 0);
    assert_eq!(
        resolve(&result, "charlie:C77").unwrap(),
        resolve(&result, "bob:C02").unwrap()
    );
    assert!(text(&result, "logic/claims.md").contains("next source value"));
    assert!(
        plan_merge(&first, &result, &second, &options())
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
    let mut dropped = second.clone();
    dropped.files.remove("trace/aliases.yaml");
    let error = plan_merge(&second, &result, &dropped, &options())
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.protected_content");
    assert_eq!(
        error.evidence[0].base.bytes,
        second.files["trace/aliases.yaml"].bytes
    );
}

#[test]
fn captured_git_ancestor_keeps_source_metadata_and_rejects_corrupt_prior_inventory() {
    let base = base();
    let first = foreign_history_source();
    let ours = materialized(
        &plan_merge(&base, &base, &first, &options())
            .unwrap()
            .working,
    );
    let mut second = first.clone();
    put(
        &mut second,
        "logic/claims.md",
        text(&first, "logic/claims.md").replace("foreign source value", "git advanced value"),
    );
    let mut advanced_options = options();
    advanced_options.predecessor = Some(fingerprint(&first));
    advanced_options.git = Some(ara_core::merge::GitMergeProvenance {
        repo_relative_root: "ara".into(),
        head: "a".repeat(40),
        theirs: "b".repeat(40),
        base: "c".repeat(40),
    });
    let advanced = plan_merge(&base, &ours, &second, &advanced_options).unwrap();
    let result = materialized(&advanced.working);
    assert_eq!(advanced.report.exit_code(), 0);
    assert!(text(&result, "logic/claims.md").contains("git advanced value"));
    assert_eq!(
        resolve(&result, "charlie:C77").unwrap(),
        resolve(&result, "bob:C02").unwrap()
    );
    let mut damaged = ours.clone();
    let document =
        ara_core::write::positions::YamlDocument::parse(text(&ours, "trace/merge_log.yaml"))
            .unwrap();
    let mut value = document.root.to_json().unwrap();
    let revision = value["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|record| record["kind"] == "revision" && record["source_key"] == "bob-fork")
        .unwrap();
    revision["files"]["logic/claims.md"] =
        serde_json::json!(STANDARD.encode(b"# Claims\n\n## C02: corrupted previous input\n"));
    put(&mut damaged, "trace/merge_log.yaml", value.to_string());
    let error = plan_merge(&base, &damaged, &second, &advanced_options)
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.corrupt_ledger");
    assert_eq!(
        error.evidence[0].ours.bytes,
        damaged.files["trace/merge_log.yaml"].bytes
    );
}

#[test]
fn acknowledging_read_only_external_conflict_is_audited_without_copying_its_body() {
    let base = full_base();
    let mut theirs = base.clone();
    let source_bytes = vec![0, 255, 10, 128];
    theirs.files.insert(
        "evidence/results.bin".into(),
        FileSnapshot {
            digest: digest(&source_bytes),
            bytes: source_bytes.clone(),
            existed: true,
            permissions: None,
        },
    );
    let initial = plan_merge(&base, &base, &theirs, &options()).unwrap();
    let current = materialized(&initial.working);
    let item = initial
        .report
        .conflicts
        .iter()
        .find(|item| item.path == "evidence/results.bin")
        .unwrap();
    assert_eq!(item.allowed, ["ours"]);
    assert_eq!(
        plan_resolution(
            &current,
            &item.id,
            "theirs",
            "2026-09-30_001",
            2,
            "user-directive",
            "user"
        )
        .err()
        .unwrap()
        .code,
        "merge.resolution_not_allowed"
    );
    let acknowledged = plan_resolution(
        &current,
        &item.id,
        "ours",
        "2026-09-30_001",
        2,
        "user-directive",
        "user",
    )
    .unwrap();
    assert!(!acknowledged.exists("evidence/results.bin"));
    assert!(
        !acknowledged
            .changed_paths()
            .contains(&"evidence/results.bin".into())
    );
    let result = materialized(&acknowledged);
    assert!(text(&result, "trace/sessions/2026-09-30_001.yaml").contains(&item.id));
    let document =
        ara_core::write::positions::YamlDocument::parse(text(&result, "trace/merge_log.yaml"))
            .unwrap();
    let value = document.root.to_json().unwrap();
    let retained = value["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["kind"] == "conflict" && record["conflict"]["id"] == item.id)
        .unwrap();
    assert_eq!(
        retained["conflict"]["theirs"]["bytes"],
        serde_json::json!(source_bytes)
    );
    assert_eq!(
        plan_merge(&base, &result, &theirs, &options())
            .unwrap()
            .report
            .exit_code(),
        0
    );
}

#[test]
fn filename_shaped_source_scopes_never_fall_through_to_native_identities() {
    let base = base();
    let mut ours = base.clone();
    let mut theirs = base.clone();
    put(
        &mut ours,
        "logic/claims.md",
        format!(
            "{CLAIMS}\n## C02: ours only\n- **Statement**: local identity\n- **Provenance**: user\n"
        ),
    );
    put(
        &mut theirs,
        "logic/claims.md",
        format!(
            "{CLAIMS}\n## C77: incoming\n- **Statement**: imported identity\n- **Provenance**: user\n"
        ),
    );
    let mut options = options();
    options.source_key = "fork.md".into();
    options.label = "fork-view.md".into();
    let plan = plan_merge(&base, &ours, &theirs, &options).unwrap();
    let result = materialized(&plan.working);
    assert_eq!(resolve(&result, "fork.md:C77").unwrap(), "C03");
    assert_eq!(resolve(&result, "fork-view.md:C77").unwrap(), "C03");
    assert_eq!(resolve(&result, "C02").unwrap(), "C02");
    for address in ["fork.md:C02", "fork-view.md:C02"] {
        assert_eq!(
            resolve(&result, address).unwrap_err().code,
            "merge.unknown_identity"
        );
        let selector = ara_core::write::EntrySelector::Id { id: address.into() };
        assert_eq!(
            ara_core::merge::resolve_selector(&result, &selector)
                .unwrap_err()
                .code,
            "merge.unknown_identity"
        );
    }
}

#[test]
fn literal_numeric_looking_headings_keep_real_targets_and_portable_aliases() {
    let base = base();
    let mut source = base.clone();
    put(
        &mut source,
        "logic/claims.md",
        format!(
            "{CLAIMS}\n## C77: source\n- **Statement**: source body\n\n### C77/Scope\nLiteral child below a real numeric claim.\n"
        ),
    );
    put(
        &mut source,
        "logic/solution/method.md",
        "# Method\n\n## C77/Scope\nLiteral heading, not numeric ancestry.\n\n### C77: narrative\nStill a literal solution heading.\n\n# C77\nA numeric-looking literal root.\n",
    );
    let initial = plan_merge(&base, &base, &source, &options()).unwrap();
    let current = materialized(&initial.working);
    for headings in [
        vec!["Method", "C77/Scope"],
        vec!["Method", "C77/Scope", "C77: narrative"],
        vec!["C77"],
    ] {
        let display = format!("logic/solution/method.md#{}", headings.join("/"));
        let original = if headings.iter().any(|component| component.contains('/')) {
            serde_json::to_string(&ara_core::write::EntrySelector::Document {
                document: "logic/solution/method.md".into(),
                heading: headings.into_iter().map(str::to_owned).collect(),
                entry: None,
            })
            .unwrap()
        } else {
            display.clone()
        };
        let mapping = initial
            .report
            .imports
            .iter()
            .find(|mapping| mapping.original == original)
            .unwrap();
        assert_eq!(mapping.target, original);
        assert_eq!(
            resolve(&current, &format!("bob:{display}")).unwrap(),
            display
        );
    }
    assert_eq!(resolve(&current, "bob:C77").unwrap(), "C02");
    assert!(resolve(&current, "logic/solution/method.md#Scope").is_err());
    let selector_key = |id: &str| {
        serde_json::to_string(&ara_core::write::EntrySelector::Document {
            document: "logic/claims.md".into(),
            heading: vec!["Claims".into(), id.into(), "C77/Scope".into()],
            entry: None,
        })
        .unwrap()
    };
    let nested = selector_key("C77");
    assert_eq!(
        initial
            .report
            .imports
            .iter()
            .find(|row| row.original == nested)
            .unwrap()
            .target,
        selector_key("C02")
    );
    assert_eq!(
        resolve(&current, "bob:logic/claims.md#Claims/C77/C77/Scope").unwrap(),
        "logic/claims.md#Claims/C02/C77/Scope"
    );
    assert!(
        plan_merge(&base, &current, &source, &options())
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
}

#[test]
fn previously_enrolled_source_without_a_revision_is_not_enrolled_twice() {
    let base = base();
    let mut ours = base.clone();
    let mut source = base.clone();
    put(
        &mut ours,
        "trace/merge_log.yaml",
        "format: ara.merge-log/v1\nrecords:\n  - {kind: enrollment, source_key: bob-fork, label: bob, time: '2026-10-01T12:00Z'}\n",
    );
    put(
        &mut source,
        "logic/claims.md",
        format!("{CLAIMS}\n## C02: source\n- **Statement**: source-only C02\n"),
    );
    let plan = plan_merge(&base, &ours, &source, &options()).unwrap();
    let current = materialized(&plan.working);
    assert_eq!(resolve(&current, "bob:C02").unwrap(), "C02");
    let records =
        ara_core::write::positions::YamlDocument::parse(text(&current, "trace/merge_log.yaml"))
            .unwrap()
            .root
            .to_json()
            .unwrap();
    assert_eq!(
        records["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|record| record["kind"] == "enrollment" && record["source_key"] == "bob-fork")
            .count(),
        1
    );
    assert!(
        plan_merge(&base, &current, &source, &options())
            .unwrap()
            .working
            .changed_paths()
            .is_empty()
    );
}

#[test]
fn staged_concept_names_and_aliases_require_exact_owning_native_revision() {
    use ara_core::merge::concept_reference_exists;
    let mut artifact = base();
    let before = "## Old/Name\nExact old concept body mentioning C01.\n";
    let after = "## New/Name\nExact old concept body mentioning C01.\n";
    put(
        &mut artifact,
        "logic/concepts.md",
        format!("# Concepts\n\n{after}"),
    );
    install_mutation_audit(
        &mut artifact,
        serde_json::json!({"action":"rename","from":"logic/concepts.md:Concepts/Old/Name","to":"logic/concepts.md:Concepts/New/Name","from_selector":{"document":"logic/concepts.md","heading":["Concepts","Old/Name"],"entry":null},"to_selector":{"document":"logic/concepts.md","heading":["Concepts","New/Name"],"entry":null},"before":before,"after":after,"session":"2026-10-01_001","turn":1,"signal":"user-directive","provenance":"user","historical_references":[]}),
    );
    put(
        &mut artifact,
        "trace/aliases.yaml",
        "format: ara.aliases/v1\naliases:\n  - {source_key: fork-source, label: bob, original: 'logic/concepts.md#old', target: 'logic/concepts.md#Old/Name', revision: source}\n  - {source_key: fork-source, label: bob, original: N02, target: N02, revision: source}\n",
    );
    let working = ara_core::write::WorkingArtifact::new(artifact.clone());
    for name in [
        "New/Name",
        "logic/concepts.md#Concepts/New/Name",
        "Old/Name",
        "logic/concepts.md:Concepts/Old/Name",
        "bob:logic/concepts.md#old",
    ] {
        assert!(concept_reference_exists(&working, name).unwrap(), "{name}");
    }
    for name in ["C01", "logic/claims.md:C01", "Missing/Name"] {
        assert!(!concept_reference_exists(&working, name).unwrap(), "{name}");
    }
    let session_path = "trace/sessions/2026-10-01_001.yaml";
    let mut owner = ara_core::write::positions::YamlDocument::parse(text(&artifact, session_path))
        .unwrap()
        .root
        .to_json()
        .unwrap();
    owner["logic_revisions"][0]["before"] =
        serde_json::json!("different unauthenticated historical body");
    put(
        &mut artifact,
        session_path,
        ara_core::write::source::render_yaml(&owner, 0, "\n"),
    );
    assert!(
        concept_reference_exists(&ara_core::write::WorkingArtifact::new(artifact), "Old/Name")
            .is_err()
    );
}

#[test]
fn accepted_writer_parent_rename_keeps_colliding_literal_vectors_readable_and_mergeable() {
    use ara_core::write::{self, EntrySelector, WriteOperation};
    let owner = tempfile::TempDir::new().unwrap();
    let document = "logic/solution/architecture.md";
    let original = "# Architecture\n\n## Parent\nContainer body.\n### A/B\nLiteral child body.\n### A\nNested parent body.\n#### B\nNested child body.\n";
    std::fs::create_dir_all(owner.path().join("logic/solution")).unwrap();
    std::fs::create_dir_all(owner.path().join("trace")).unwrap();
    std::fs::write(owner.path().join(document), original).unwrap();
    std::fs::write(
        owner.path().join("trace/exploration_tree.yaml"),
        "tree: [{id: N01, type: question, title: Unrelated}]\n",
    )
    .unwrap();
    let original_snapshot = ArtifactSnapshot::load_complete(owner.path()).unwrap();
    let mut working = WorkingArtifact::new(original_snapshot.clone());
    for value in [
        serde_json::json!({"op":"session.start","id":"2026-10-01_001","date":"2026-10-01","started":"2026-10-01T10:00","summary":"Exact writer acceptance"}),
        serde_json::json!({"op":"session.log","session":"2026-10-01_001","timestamp":"2026-10-01T10:01"}),
    ] {
        write::plan_operation(&mut working, &serde_json::from_value(value).unwrap()).unwrap();
    }
    let parent = EntrySelector::Document {
        document: document.into(),
        heading: vec!["Architecture".into(), "Parent".into()],
        entry: None,
    };
    let range = write::logic::resolve(&working, &parent).unwrap().range;
    let expected = digest(working.text(document).unwrap()[range].as_bytes());
    write::plan_operation(
        &mut working,
        &WriteOperation::EntryRename {
            target: parent,
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
    let renamed = materialized(&working);
    assert_eq!(resolve_local(&renamed, "N01").unwrap(), "N01");
    for tail in [vec!["A/B"], vec!["A", "B"]] {
        for ancestor in ["Parent", "Renamed parent"] {
            let mut heading = vec!["Architecture".into(), ancestor.into()];
            heading.extend(tail.iter().map(|part| (*part).into()));
            let requested = EntrySelector::Document {
                document: document.into(),
                heading,
                entry: None,
            };
            let actual = ara_core::merge::resolve_selector(&renamed, &requested).unwrap();
            let mut expected_heading = vec!["Architecture".into(), "Renamed parent".into()];
            expected_heading.extend(tail.iter().map(|part| (*part).into()));
            assert_eq!(
                actual,
                EntrySelector::Document {
                    document: document.into(),
                    heading: expected_heading,
                    entry: None
                }
            );
            let selected = write::logic::resolve(&working, &actual).unwrap();
            assert!(working.text(document).unwrap()[selected.range].starts_with(
                if tail.len() == 1 {
                    "### A/B\nLiteral child body."
                } else {
                    "#### B\nNested child body."
                }
            ));
        }
    }
    for display in [
        "logic/solution/architecture.md:Architecture/Parent/A/B",
        "logic/solution/architecture.md#Architecture/Renamed parent/A/B",
    ] {
        assert_eq!(
            resolve_local(&renamed, display).unwrap_err().code,
            "merge.redirect_ambiguous"
        );
    }
    let merged = plan_merge(&original_snapshot, &original_snapshot, &renamed, &options()).unwrap();
    assert!(merged.report.conflicts.is_empty());
    let imported = materialized(&merged.working);
    assert_eq!(resolve_local(&imported, "N01").unwrap(), "N01");
    let source_ledger = working
        .yaml("trace/logic_mutations.yaml")
        .unwrap()
        .root
        .to_json()
        .unwrap();
    let imported_ledger = merged
        .working
        .yaml("trace/logic_mutations.yaml")
        .unwrap()
        .root
        .to_json()
        .unwrap();
    for (original, actual) in source_ledger["mutations"]
        .as_array()
        .unwrap()
        .iter()
        .zip(imported_ledger["mutations"].as_array().unwrap())
    {
        assert_eq!(actual["before"], original["before"]);
        assert_eq!(actual["after"], original["after"]);
        let from: EntrySelector = serde_json::from_value(actual["from_selector"].clone()).unwrap();
        let to: EntrySelector = serde_json::from_value(actual["to_selector"].clone()).unwrap();
        assert_eq!(
            ara_core::merge::resolve_selector(&imported, &from).unwrap(),
            to
        );
    }
    let replay = plan_merge(&original_snapshot, &imported, &renamed, &options()).unwrap();
    assert!(replay.working.changed_paths().is_empty());
    assert_eq!(
        fingerprint(&materialized(&replay.working)),
        fingerprint(&imported)
    );
}

#[test]
fn inventory_errors_keep_input_precedence_for_small_and_large_captures() {
    let tree = "trace/exploration_tree.yaml";
    for padding in [0, 1024 * 1024] {
        let comment = format!("# {}\n", "a".repeat(padding));
        let mut base = snapshot(&[(tree, &format!("tree: []\n{comment}"))]);
        base.files.get_mut(tree).unwrap().bytes.push(0xff);
        let ours = snapshot(&[(tree, &format!("tree: ]\n{comment}"))]);
        let theirs = snapshot(&[(tree, &format!("tree: []\n{comment}"))]);
        let error = plan_merge(&base, &ours, &theirs, &options()).err().unwrap();
        assert_eq!(error.code, "merge.encoding");
    }
}
