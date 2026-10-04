#![cfg(feature = "native")]
//! Plan 04 acceptance matrix: native identities through peer-feedback merges.
mod peer_support;
use ara_core::merge::{fingerprint, plan_resolution, resolve};
use ara_core::write::ArtifactSnapshot;
use peer_support::*;
use serde_json::Value;

struct Diamond {
    seed: ArtifactSnapshot,
    a1: ArtifactSnapshot,
    b1: ArtifactSnapshot,
    b2: ArtifactSnapshot,
    canonical: ArtifactSnapshot,
}
/// Canonical imports A1 then B1; B2 is B1 after importing A1. Every local
/// identity of A's entries differs between canonical and B2.
fn diamond() -> Diamond {
    let seed = seed();
    let a1 = fork(&seed, "A");
    let b1 = fork(&seed, "B");
    let canonical = clean(&seed, &seed, &a1, "fork-a", 1);
    let canonical = clean(&seed, &canonical, &b1, "fork-b", 2);
    let b2 = clean(&seed, &b1, &a1, "fork-a", 7);
    for (address, at_canonical, at_b2) in [
        ("fork-a:C77", "C02", "C78"),
        ("fork-a:N03", "N03", "N04"),
        ("fork-a:2026-10-02_001", "2026-10-02_001", "2026-10-02_002"),
    ] {
        assert_eq!(resolve(&canonical, address).unwrap(), at_canonical);
        assert_eq!(resolve(&b2, address).unwrap(), at_b2);
    }
    Diamond {
        seed,
        a1,
        b1,
        b2,
        canonical,
    }
}

#[test]
fn diamond_with_swapped_local_ids_reuses_every_proven_origin() {
    let d = diamond();
    let (result, plan) = merge(&d.b1, &d.canonical, &d.b2, "fork-b", 9);
    assert_eq!(plan.report.exit_code(), 0, "{:#?}", plan.report.conflicts);
    let target = |original: &str| {
        plan.report
            .imports
            .iter()
            .find(|mapping| mapping.original == original)
            .unwrap()
            .target
            .clone()
    };
    // B's own entries keep their previous mapping; A's resolve to canonical's.
    assert_eq!(target("C77"), "C03");
    assert_eq!(target("C78"), "C02");
    assert_eq!(target("N03"), "N04");
    assert_eq!(target("N04"), "N03");
    assert_eq!(target("2026-10-02_002"), "2026-10-02_001");
    for path in [
        "logic/claims.md",
        "trace/exploration_tree.yaml",
        "trace/sessions/2026-10-02_001.yaml",
        "trace/sessions/2026-10-02_002.yaml",
        "trace/sessions/session_index.yaml",
    ] {
        assert_eq!(text(&result, path), text(&d.canonical, path), "{path}");
    }
    assert!(
        !result
            .files
            .contains_key("trace/sessions/2026-10-02_003.yaml")
    );
    for (address, expected) in [
        ("fork-a:C77", "C02"),
        ("fork-b:C77", "C03"),
        ("fork-b:C78", "C02"),
        ("fork-b:N04", "N03"),
        ("fork-a:2026-10-02_001", "2026-10-02_001"),
    ] {
        assert_eq!(resolve(&result, address).unwrap(), expected, "{address}");
    }
    assert_eq!(kinds(&result, "fork-a"), ["enrollment", "revision"]);
    assert!(
        !records(&result)
            .iter()
            .any(|row| row["kind"] == "inherited_revision")
    );
}

#[test]
fn one_entry_edited_on_both_routes_keeps_one_identity_and_a_mutable_conflict() {
    let d = diamond();
    // Only the peer route edits A's claim: the edit lands on canonical's C02.
    let mut b2 = d.b2.clone();
    edit(&mut b2, "logic/claims.md", "A says x", "B refined A");
    let one_sided = clean(&d.b1, &d.canonical, &b2, "fork-b", 9);
    assert!(
        text(&one_sided, "logic/claims.md")
            .contains("## C02: fork A finding\n- **Statement**: B refined A")
    );
    assert_eq!(headings(&one_sided).len(), 3);
    // Both routes edit it: one identity, base/ours/theirs retained.
    let mut canonical = d.canonical.clone();
    edit(&mut canonical, "logic/claims.md", "A says x", "PM reading");
    let (result, plan) = merge(&d.b1, &canonical, &b2, "fork-b", 9);
    assert_eq!(plan.report.exit_code(), 1);
    assert_eq!(
        headings(&result),
        [
            "## C01: shared",
            "## C02: fork A finding",
            "## C03: fork B finding"
        ]
    );
    let conflict = plan
        .report
        .logic_conflicts
        .iter()
        .find(|item| item.selector == "C02")
        .unwrap();
    assert_eq!(conflict.kind, "mutable_field");
    assert_eq!(conflict.allowed, ["ours", "theirs", "base"]);
    let value = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).unwrap();
    assert!(value(&conflict.base.bytes).contains("A says x"));
    assert!(value(&conflict.ours.bytes).contains("PM reading"));
    assert!(value(&conflict.theirs.bytes).contains("B refined A"));
    assert!(text(&result, "logic/claims.md").contains("PM reading"));
}

#[test]
fn canonical_feedback_and_a_non_seed_fork_keep_histories_resolvable() {
    let d = diamond();
    // A new fork starts from the published canonical snapshot, not the seed.
    let start = d.canonical.clone();
    let mut w1 = start.clone();
    add_claim(&mut w1, "C04", "fork W finding", "W says z");
    edit(
        &mut w1,
        "logic/claims.md",
        "B says x",
        "B says x, confirmed by W",
    );
    let result = clean(&start, &d.canonical, &w1, "fork-w", 9);
    assert_eq!(
        headings(&result),
        [
            "## C01: shared",
            "## C02: fork A finding",
            "## C03: fork B finding",
            "## C04: fork W finding"
        ]
    );
    assert!(text(&result, "logic/claims.md").contains("confirmed by W"));
    for (address, expected) in [
        ("fork-a:C77", "C02"),
        ("fork-b:C77", "C03"),
        ("fork-w:C04", "C04"),
        ("fork-w:C02", "C02"),
    ] {
        assert_eq!(resolve(&result, address).unwrap(), expected, "{address}");
    }
    // W carried canonical's own fork-a/fork-b facts: they match and add nothing.
    assert_eq!(kinds(&result, "fork-a"), ["enrollment", "revision"]);
    assert_eq!(kinds(&result, "fork-b"), ["enrollment", "revision"]);

    // An existing worker absorbs canonical (holding only A), then returns.
    let seed = &d.seed;
    let only_a = clean(seed, seed, &d.a1, "fork-a", 1);
    let b2 = clean(seed, &d.b1, &only_a, "canonical", 3);
    assert_eq!(resolve(&b2, "fork-a:C77").unwrap(), "C78");
    let inherited: Vec<Value> = records(&b2)
        .into_iter()
        .filter(|row| row["kind"] == "inherited_revision")
        .collect();
    assert_eq!(inherited.len(), 1);
    assert_eq!(inherited[0]["source_key"], "fork-a");
    assert_eq!(inherited[0]["via_source_key"], "canonical");
    let returned = clean(seed, &only_a, &b2, "fork-b", 5);
    assert_eq!(
        headings(&returned),
        [
            "## C01: shared",
            "## C02: fork A finding",
            "## C03: fork B finding"
        ]
    );
    assert_eq!(resolve(&returned, "fork-a:C77").unwrap(), "C02");
    assert_eq!(resolve(&returned, "fork-b:C78").unwrap(), "C02");
    assert_eq!(resolve(&returned, "fork-b:C77").unwrap(), "C03");
    assert_eq!(kinds(&returned, "fork-a"), ["enrollment", "revision"]);
}

#[test]
fn later_revisions_keep_identities_and_exact_predecessor_checks() {
    let d = diamond();
    let canonical = clean(&d.b1, &d.canonical, &d.b2, "fork-b", 9);
    let mut a2 = d.a1.clone();
    add_claim(&mut a2, "C78", "fork A second", "A adds more");
    // Wrong predecessor is still refused for both routes.
    for (base, theirs, key) in [(&d.seed, &a2, "fork-a"), (&d.b1, &d.b2, "fork-a")] {
        assert_eq!(
            try_merge(base, &canonical, theirs, key, 11)
                .err()
                .unwrap()
                .code,
            "merge.unproven_source_revision"
        );
    }
    let b3 = clean(&d.a1, &d.b2, &a2, "fork-a", 12);
    assert_eq!(resolve(&b3, "fork-a:C78").unwrap(), "C79");

    // Order 1: the source advances directly, then the peer returns it.
    let direct = clean(&d.a1, &canonical, &a2, "fork-a", 13);
    assert_eq!(resolve(&direct, "fork-a:C78").unwrap(), "C04");
    assert!(
        try_merge(&d.b1, &direct, &b3, "fork-b", 14)
            .err()
            .is_some_and(|error| error.code == "merge.unproven_source_revision")
    );
    let both = clean(&d.b2, &direct, &b3, "fork-b", 14);
    assert_eq!(headings(&both).len(), 4);
    assert_eq!(resolve(&both, "fork-b:C79").unwrap(), "C04");
    assert_eq!(resolve(&both, "fork-a:C77").unwrap(), "C02");
    assert_eq!(
        kinds(&both, "fork-a"),
        ["enrollment", "revision", "revision"]
    );

    // Order 2: the peer returns A2 first; the later direct import is a no-op
    // for content and keeps the identity the destination already allocated.
    let via_peer = clean(&d.b2, &canonical, &b3, "fork-b", 13);
    assert_eq!(resolve(&via_peer, "fork-a:C78").unwrap(), "C04");
    assert_eq!(
        kinds(&via_peer, "fork-a"),
        ["enrollment", "revision", "inherited_revision"]
    );
    let then_direct = clean(&d.a1, &via_peer, &a2, "fork-a", 14);
    assert_eq!(
        text(&then_direct, "logic/claims.md"),
        text(&via_peer, "logic/claims.md")
    );
    assert_eq!(resolve(&then_direct, "fork-a:C78").unwrap(), "C04");
    assert_eq!(
        kinds(&then_direct, "fork-a"),
        ["enrollment", "revision", "inherited_revision", "revision"]
    );
}

#[test]
fn latest_replay_is_a_no_op_and_an_older_revision_is_a_regression() {
    let d = diamond();
    let canonical = clean(&d.b1, &d.canonical, &d.b2, "fork-b", 9);
    let replay = try_merge(&d.b1, &canonical, &d.b2, "fork-b", 10).unwrap();
    assert!(replay.working.changed_paths().is_empty());
    assert_eq!(
        fingerprint(&materialized(&replay.working)),
        fingerprint(&canonical)
    );
    let older = try_merge(&d.seed, &canonical, &d.b1, "fork-b", 10)
        .err()
        .unwrap();
    assert_eq!(older.code, "merge.source_regression");
    assert!(!older.evidence.is_empty());
}

#[test]
fn forged_alias_ambiguous_origin_and_protected_edits_reject_with_evidence() {
    let d = diamond();
    // An alias with no backing source fact proves nothing.
    let mut forged = d.b1.clone();
    add_claim(&mut forged, "C78", "fork A finding", "A says x");
    put(
        &mut forged,
        "trace/aliases.yaml",
        "format: ara.aliases/v1\naliases:\n  - {\"source_key\":\"fork-a\",\"label\":\"fork-a\",\"original\":\"C77\",\"target\":\"C78\",\"revision\":\"forged\"}\n",
    );
    let error = try_merge(&d.b1, &d.canonical, &forged, "fork-b", 9)
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.alias_conflict");
    assert_eq!(error.evidence[0].path, "trace/aliases.yaml");
    assert!(error.evidence[0].ours.present && error.evidence[0].theirs.present);

    // A backing fact whose source bytes differ from the destination's copy.
    let mut tampered = d.b2.clone();
    let log = text(&tampered, "trace/merge_log.yaml").to_owned();
    let changed = log.replace(
        &base64(text(&d.a1, "logic/claims.md")),
        &base64(&text(&d.a1, "logic/claims.md").replace("A says x", "A said y")),
    );
    assert_ne!(changed, log);
    put(&mut tampered, "trace/merge_log.yaml", changed);
    let error = try_merge(&d.b1, &d.canonical, &tampered, "fork-b", 9)
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.foreign_mapping_conflict");
    assert!(!error.evidence.is_empty());

    // One incoming entry proven to two destination identities.
    let copy = d.a1.clone();
    let canonical = clean(&d.seed, &d.canonical, &copy, "fork-c", 8);
    assert_eq!(resolve(&canonical, "fork-c:C77").unwrap(), "C04");
    let mut ambiguous = d.b2.clone();
    let mut log = text(&ambiguous, "trace/merge_log.yaml").to_owned();
    for row in records(&d.b2) {
        if row["source_key"] == "fork-a" && row["kind"] != "transport" {
            let copied = serde_json::to_string(&row)
                .unwrap()
                .replace("\"fork-a\"", "\"fork-c\"");
            log.push_str(&format!("  - {copied}\n"));
        }
    }
    put(&mut ambiguous, "trace/merge_log.yaml", log);
    let mut aliases = text(&ambiguous, "trace/aliases.yaml").to_owned();
    for line in text(&d.b2, "trace/aliases.yaml").lines() {
        if line.contains("\"source_key\":\"fork-a\"") {
            aliases.push_str(&line.replace("\"fork-a\"", "\"fork-c\""));
            aliases.push('\n');
        }
    }
    put(&mut ambiguous, "trace/aliases.yaml", aliases);
    let error = try_merge(&d.b1, &canonical, &ambiguous, "fork-b", 9)
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.ambiguous_origin", "{}", error.message);
    assert_eq!(error.evidence[0].path, "trace/aliases.yaml");

    // The peer route changed A's immutable tree node before returning it.
    let mut rewritten = d.b2.clone();
    edit(
        &mut rewritten,
        "trace/exploration_tree.yaml",
        "result: A result",
        "result: rewritten by B",
    );
    let error = try_merge(&d.b1, &d.canonical, &rewritten, "fork-b", 9)
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.protected_content");
    let item = &error.evidence[0];
    assert_eq!(item.kind, "protected_inherited_entry");
    assert_eq!(item.selector, "N03");
    let value = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).unwrap();
    assert!(value(&item.base.bytes).contains("A result"));
    assert!(value(&item.ours.bytes).contains("A result"));
    assert!(value(&item.theirs.bytes).contains("rewritten by B"));
}
fn base64(text: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.encode(text)
}

#[test]
fn external_files_and_inherited_conflicts_stay_source_owned() {
    let seed = seed();
    let mut a1 = fork(&seed, "A");
    put(&mut a1, "src/a.py", "print('A')\n");
    let mut b1 = fork(&seed, "B");
    put(&mut b1, "src/a.py", "print('B')\n");
    let (canonical, plan) = merge(&seed, &seed, &a1, "fork-a", 1);
    assert_eq!(plan.report.unresolved_count, 1);
    let (canonical, _) = merge(&seed, &canonical, &b1, "fork-b", 2);
    let (b2, plan) = merge(&seed, &b1, &a1, "fork-a", 3);
    let peer_conflict = plan
        .report
        .conflicts
        .iter()
        .find(|item| item.path == "src/a.py")
        .unwrap()
        .clone();
    assert_eq!(peer_conflict.kind, "external_read_only");
    assert_eq!(text(&b2, "src/a.py"), "print('B')\n");

    let (result, plan) = merge(&b1, &canonical, &b2, "fork-b", 4);
    assert!(!result.files.contains_key("src/a.py"));
    let imported = plan
        .report
        .conflicts
        .iter()
        .find(|item| item.id == peer_conflict.id)
        .unwrap();
    assert_eq!(imported.kind, "imported_unresolved");
    assert!(imported.allowed.is_empty());
    for item in plan
        .report
        .conflicts
        .iter()
        .filter(|item| item.path == "src/a.py")
    {
        assert!(
            item.kind == "external_read_only" && item.allowed == ["ours"]
                || item.kind == "imported_unresolved" && item.allowed.is_empty()
        );
    }
    let error = plan_resolution(
        &result,
        &peer_conflict.id,
        "ours",
        "2026-10-02_001",
        2,
        "user-directive",
        "user",
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "merge.resolution_not_allowed");
    assert_eq!(headings(&result).len(), 3);
    assert_eq!(resolve(&result, "fork-b:C78").unwrap(), "C02");

    // Only the owning fork resolves it; canonical receives that resolution
    // through the next import of the fork.
    let acknowledged = plan_resolution(
        &b2,
        &peer_conflict.id,
        "ours",
        "2026-10-02_001",
        2,
        "user-directive",
        "user",
    )
    .unwrap();
    let b3 = materialized(&acknowledged);
    assert_eq!(text(&b3, "src/a.py"), "print('B')\n");
    let (after, plan) = merge(&b2, &result, &b3, "fork-b", 5);
    assert!(!after.files.contains_key("src/a.py"));
    assert!(
        !plan
            .report
            .conflicts
            .iter()
            .any(|item| item.id == peer_conflict.id)
    );
    let received = records(&after)
        .into_iter()
        .filter(|row| row["kind"] == "imported_resolution")
        .collect::<Vec<_>>();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0]["conflict_id"], peer_conflict.id.as_str());
    assert_eq!(headings(&after).len(), 3);
}

#[test]
fn artifacts_without_import_metadata_take_the_unchanged_path() {
    let seed = seed();
    let a1 = fork(&seed, "A");
    let (result, plan) = merge(&seed, &seed, &a1, "fork-a", 1);
    assert_eq!(plan.report.exit_code(), 0);
    assert_eq!(
        headings(&result),
        ["## C01: shared", "## C02: fork A finding"]
    );
    assert_eq!(kinds(&result, "fork-a"), ["enrollment", "revision"]);
    let replay = try_merge(&seed, &result, &a1, "fork-a", 2).unwrap();
    assert!(replay.working.changed_paths().is_empty());
}

#[test]
fn a_peer_holding_only_a_newer_source_revision_cannot_wedge_later_imports() {
    let seed = seed();
    let a1 = fork(&seed, "A");
    let mut a2 = a1.clone();
    edit(&mut a2, "logic/claims.md", "A says x", "A says x, revised");
    let b1 = fork(&seed, "B");
    // Canonical imports A1 directly; B imports only A2 (never A1).
    let canonical = clean(&seed, &seed, &a1, "fork-a", 1);
    let canonical = clean(&seed, &canonical, &b1, "fork-b", 2);
    let b2 = clean(&seed, &b1, &a2, "fork-a", 3);
    let before = canonical.clone();
    let outcome = try_merge(&b1, &canonical, &b2, "fork-b", 4);
    let error = outcome.err().expect("unshared origin must be refused");
    // Canonical already aliases fork-a's originals, so the unproven copy is
    // refused before any write.
    assert_eq!(error.code, "merge.alias_conflict", "{}", error.message);
    assert!(!error.evidence.is_empty());
    // Nothing was written, so the direct source still advances normally, and
    // afterwards the peer's copy is proven through the shared A2 fact.
    let advanced = clean(&a1, &before, &a2, "fork-a", 5);
    assert!(text(&advanced, "logic/claims.md").contains("A says x, revised"));
    let returned = clean(&b1, &advanced, &b2, "fork-b", 6);
    assert_eq!(headings(&returned), headings(&advanced));
    assert_eq!(resolve(&returned, "fork-b:C78").unwrap(), "C02");
    let replay = try_merge(&a1, &returned, &a2, "fork-a", 7).unwrap();
    assert!(replay.working.changed_paths().is_empty());
}

#[test]
fn an_unshared_foreign_revision_is_refused_before_it_can_wedge_the_ledger() {
    let seed = seed();
    let a1 = fork(&seed, "A");
    let mut a2 = a1.clone();
    edit(&mut a2, "logic/claims.md", "A says x", "A says x, revised");
    let b1 = fork(&seed, "B");
    let canonical = clean(&seed, &seed, &a1, "fork-a", 1);
    let mut canonical = clean(&seed, &canonical, &b1, "fork-b", 2);
    // Without an alias for fork-a's originals nothing else stops the merge:
    // appending fork-a A2 with fresh targets would contradict the recorded
    // A1 mapping and make every later fork-a import fail.
    let aliases = text(&canonical, "trace/aliases.yaml")
        .lines()
        .filter(|line| !line.contains("\"source_key\":\"fork-a\""))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    put(&mut canonical, "trace/aliases.yaml", aliases);
    let b2 = clean(&seed, &b1, &a2, "fork-a", 3);
    let error = try_merge(&b1, &canonical, &b2, "fork-b", 4).err().unwrap();
    assert_eq!(
        error.code, "merge.unshared_origin_revision",
        "{}",
        error.message
    );
    assert_eq!(error.evidence[0].path, "trace/merge_log.yaml");
    let advanced = clean(&a1, &canonical, &a2, "fork-a", 5);
    assert!(text(&advanced, "logic/claims.md").contains("A says x, revised"));
}
