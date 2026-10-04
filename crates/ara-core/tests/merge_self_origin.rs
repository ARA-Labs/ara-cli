#![cfg(feature = "native")]
//! Plan 04 canonical feedback: a destination recognizes its own entries when
//! they come back through another fork, given its explicit `--self-key`.
mod peer_support;
use ara_core::merge::{fingerprint, resolve};
use ara_core::write::ArtifactSnapshot;
use peer_support::*;

struct Loop {
    seed: ArtifactSnapshot,
    b1: ArtifactSnapshot,
    /// Canonical after integrating B1 and A1, then editing B's claim and
    /// authoring its own claim C04.
    c2: ArtifactSnapshot,
}
fn round_trip_start() -> Loop {
    let seed = seed();
    let a1 = fork(&seed, "A");
    let b1 = fork(&seed, "B");
    let c = clean(&seed, &seed, &b1, "fork-b", 1);
    let mut c2 = clean(&seed, &c, &a1, "fork-a", 2);
    assert_eq!(resolve(&c2, "fork-b:C77").unwrap(), "C02");
    assert_eq!(resolve(&c2, "fork-a:C77").unwrap(), "C03");
    edit(&mut c2, "logic/claims.md", "B says x", "PM refined B");
    add_claim(&mut c2, "C04", "canonical synthesis", "PM combines A and B");
    Loop { seed, b1, c2 }
}
fn absorb(l: &Loop, ours: &ArtifactSnapshot, minute: u32) -> ArtifactSnapshot {
    let plan = try_merge_as(&l.seed, ours, &l.c2, "canonical", minute, "fork-b").unwrap();
    assert_eq!(plan.report.exit_code(), 0, "{:#?}", plan.report.conflicts);
    materialized(&plan.working)
}

#[test]
fn worker_absorbing_canonical_keeps_its_own_entries_and_takes_canonical_edits() {
    let l = round_trip_start();
    let b2 = absorb(&l, &l.b1, 3);
    assert_eq!(
        headings(&b2),
        [
            "## C01: shared",
            "## C77: fork B finding",
            "## C78: fork A finding",
            "## C79: canonical synthesis"
        ]
    );
    // The edit canonical made to B's claim lands on B's own C77.
    assert!(
        text(&b2, "logic/claims.md")
            .contains("## C77: fork B finding\n- **Statement**: PM refined B")
    );
    // B's own node and session are not duplicated; A's arrive once.
    assert_eq!(
        text(&b2, "trace/sessions/2026-10-02_001.yaml"),
        text(&l.b1, "trace/sessions/2026-10-02_001.yaml")
    );
    assert!(b2.files.contains_key("trace/sessions/2026-10-02_002.yaml"));
    assert!(!b2.files.contains_key("trace/sessions/2026-10-02_003.yaml"));
    assert_eq!(
        text(&b2, "trace/exploration_tree.yaml")
            .matches("id: N0")
            .count(),
        4
    );
    for (address, expected) in [
        ("canonical:C02", "C77"),
        ("canonical:C03", "C78"),
        ("canonical:C04", "C79"),
        ("fork-a:C77", "C78"),
        ("fork-b:C77", "C77"),
        ("C77", "C77"),
    ] {
        assert_eq!(resolve(&b2, address).unwrap(), expected, "{address}");
    }
    // B is the source of truth for its own history: no fork-b facts are stored.
    assert_eq!(kinds(&b2, "fork-b"), ["self_identity"]);
    assert_eq!(kinds(&b2, "fork-a"), ["enrollment", "inherited_revision"]);
    // Replay of the same canonical revision changes nothing, with or without
    // repeating the recorded self key.
    let replay = try_merge_as(&l.seed, &b2, &l.c2, "canonical", 4, "fork-b").unwrap();
    assert!(replay.working.changed_paths().is_empty());
    let replay = try_merge(&l.seed, &b2, &l.c2, "canonical", 4).unwrap();
    assert!(replay.working.changed_paths().is_empty());
    assert_eq!(
        fingerprint(&materialized(&replay.working)),
        fingerprint(&b2)
    );
}

#[test]
fn both_routes_editing_a_self_origin_entry_give_a_mutable_conflict() {
    let l = round_trip_start();
    let mut b1 = l.b1.clone();
    edit(&mut b1, "logic/claims.md", "B says x", "B revised itself");
    let plan = try_merge_as(&l.seed, &b1, &l.c2, "canonical", 3, "fork-b").unwrap();
    assert_eq!(plan.report.exit_code(), 1);
    let result = materialized(&plan.working);
    assert_eq!(headings(&result).len(), 4);
    let conflict = plan
        .report
        .logic_conflicts
        .iter()
        .find(|item| item.selector == "C77")
        .unwrap();
    assert_eq!(conflict.kind, "mutable_field");
    let value = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).unwrap();
    assert!(value(&conflict.base.bytes).contains("B says x"));
    assert!(value(&conflict.ours.bytes).contains("B revised itself"));
    assert!(value(&conflict.theirs.bytes).contains("PM refined B"));
}

#[test]
fn canonical_with_its_self_key_receives_its_own_entries_back_without_duplicates() {
    let l = round_trip_start();
    let mut b2 = absorb(&l, &l.b1, 3);
    edit(
        &mut b2,
        "logic/claims.md",
        "PM combines A and B",
        "PM combines A and B; B agrees",
    );
    let plan = try_merge_as(&l.b1, &l.c2, &b2, "fork-b", 5, "canonical").unwrap();
    assert_eq!(plan.report.exit_code(), 0, "{:#?}", plan.report.conflicts);
    let c3 = materialized(&plan.working);
    assert_eq!(
        headings(&c3),
        [
            "## C01: shared",
            "## C02: fork B finding",
            "## C03: fork A finding",
            "## C04: canonical synthesis"
        ]
    );
    assert!(text(&c3, "logic/claims.md").contains("PM refined B"));
    assert!(text(&c3, "logic/claims.md").contains("B agrees"));
    for path in [
        "trace/exploration_tree.yaml",
        "trace/sessions/2026-10-02_001.yaml",
        "trace/sessions/2026-10-02_002.yaml",
    ] {
        assert_eq!(text(&c3, path), text(&l.c2, path), "{path}");
    }
    for (address, expected) in [
        ("fork-b:C77", "C02"),
        ("fork-b:C78", "C03"),
        ("fork-b:C79", "C04"),
        ("fork-a:C77", "C03"),
        ("canonical:C04", "C04"),
    ] {
        assert_eq!(resolve(&c3, address).unwrap(), expected, "{address}");
    }
    assert_eq!(kinds(&c3, "canonical"), ["self_identity"]);
    assert_eq!(kinds(&c3, "fork-a"), ["enrollment", "revision"]);
    let replay = try_merge(&l.b1, &c3, &b2, "fork-b", 6).unwrap();
    assert!(replay.working.changed_paths().is_empty());
}

#[test]
fn self_key_is_explicit_recorded_once_and_fail_closed() {
    let l = round_trip_start();
    let b2 = absorb(&l, &l.b1, 3);
    // A different key than the recorded one is refused.
    let error = try_merge_as(&l.seed, &b2, &l.c2, "canonical", 4, "fork-x")
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.self_identity_conflict");
    assert!(!error.evidence.is_empty());
    // The self key can never be the transport source.
    let error = try_merge_as(&l.seed, &l.b1, &l.c2, "fork-b", 4, "fork-b")
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.self_identity_conflict");
    // Omitted later: the recorded key still applies to a newer canonical.
    let mut c3 = l.c2.clone();
    edit(
        &mut c3,
        "logic/claims.md",
        "PM refined B",
        "PM refined B twice",
    );
    let plan = try_merge(&l.c2, &b2, &c3, "canonical", 5).unwrap();
    assert_eq!(plan.report.exit_code(), 0);
    let b3 = materialized(&plan.working);
    assert_eq!(headings(&b3), headings(&b2));
    assert!(text(&b3, "logic/claims.md").contains("PM refined B twice"));
    // An original that no longer exists here is refused, never duplicated.
    let mut deleted = l.b1.clone();
    let claims = text(&deleted, "logic/claims.md").to_owned();
    let cut = claims.find("\n## C77").unwrap();
    put(
        &mut deleted,
        "logic/claims.md",
        claims[..cut + 1].to_owned(),
    );
    let error = try_merge_as(&l.seed, &deleted, &l.c2, "canonical", 3, "fork-b")
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.self_origin_missing", "{}", error.message);
    assert_eq!(error.evidence[0].path, "trace/aliases.yaml");
    // Without any self key the destination cannot know these entries are its
    // own; they are unproven. In this fixture B's returning session collides
    // and the merge rejects; a runner must always pass --self-key.
    let error = try_merge(&l.seed, &l.b1, &l.c2, "canonical", 3)
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.identity");
}
