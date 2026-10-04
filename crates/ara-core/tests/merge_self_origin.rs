#![cfg(feature = "native")]
//! Plan 04 canonical feedback: a destination recognizes its own entries when
//! they come back through another fork, given its explicit `--self-key`.
//! Self facts prove identity only; they never supply a content base.
mod peer_support;
use ara_core::merge::{fingerprint, resolve};
use ara_core::write::ArtifactSnapshot;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use peer_support::*;
use serde_json::json;

struct Loop {
    seed: ArtifactSnapshot,
    b1: ArtifactSnapshot,
    /// Canonical after integrating B1 and A1 and authoring its own claim C04.
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
    add_claim(&mut c2, "C04", "canonical synthesis", "PM combines A and B");
    Loop { seed, b1, c2 }
}
fn absorb(l: &Loop, ours: &ArtifactSnapshot, theirs: &ArtifactSnapshot) -> ArtifactSnapshot {
    let plan = try_merge_as(&l.seed, ours, theirs, "canonical", 3, "fork-b").unwrap();
    assert_eq!(plan.report.exit_code(), 0, "{:#?}", plan.report.conflicts);
    materialized(&plan.working)
}
fn value(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}
const WORKER: [&str; 4] = [
    "## C01: shared",
    "## C77: fork B finding",
    "## C78: fork A finding",
    "## C79: canonical synthesis",
];
const CANONICAL: [&str; 4] = [
    "## C01: shared",
    "## C02: fork B finding",
    "## C03: fork A finding",
    "## C04: canonical synthesis",
];

#[test]
fn worker_absorbing_canonical_reuses_its_own_entries() {
    let l = round_trip_start();
    let b2 = absorb(&l, &l.b1, &l.c2);
    assert_eq!(headings(&b2), WORKER);
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
    ] {
        assert_eq!(resolve(&b2, address).unwrap(), expected, "{address}");
    }
    // B is the source of truth for its own history: no fork-b facts are stored.
    assert_eq!(kinds(&b2, "fork-b"), ["self_identity"]);
    assert_eq!(kinds(&b2, "fork-a"), ["enrollment", "inherited_revision"]);
    for own in [Some("fork-b"), None] {
        let replay = match own {
            Some(own) => try_merge_as(&l.seed, &b2, &l.c2, "canonical", 4, own),
            None => try_merge(&l.seed, &b2, &l.c2, "canonical", 4),
        }
        .unwrap();
        assert!(replay.working.changed_paths().is_empty());
        assert_eq!(
            fingerprint(&materialized(&replay.working)),
            fingerprint(&b2)
        );
    }
    // With a trusted base (the previous canonical revision already holding
    // B's entry), an edit made only on canonical applies to B's own C77.
    let mut c3 = l.c2.clone();
    edit(&mut c3, "logic/claims.md", "B says x", "PM refined B");
    let plan = try_merge(&l.c2, &b2, &c3, "canonical", 5).unwrap();
    assert_eq!(plan.report.exit_code(), 0, "{:#?}", plan.report.conflicts);
    let b3 = materialized(&plan.working);
    assert_eq!(headings(&b3), WORKER);
    assert!(
        text(&b3, "logic/claims.md")
            .contains("## C77: fork B finding\n- **Statement**: PM refined B")
    );
}

#[test]
fn self_origin_change_without_a_trusted_base_conflicts_and_keeps_ours() {
    let l = round_trip_start();
    let mut c2 = l.c2.clone();
    edit(&mut c2, "logic/claims.md", "B says x", "PM refined B");
    for (ours_statement, ours) in [
        ("B says x", l.b1.clone()),
        ("B revised itself", {
            let mut edited = l.b1.clone();
            edit(
                &mut edited,
                "logic/claims.md",
                "B says x",
                "B revised itself",
            );
            edited
        }),
    ] {
        let plan = try_merge_as(&l.seed, &ours, &c2, "canonical", 3, "fork-b").unwrap();
        assert_eq!(plan.report.exit_code(), 1);
        let result = materialized(&plan.working);
        assert_eq!(headings(&result), WORKER);
        assert!(text(&result, "logic/claims.md").contains(ours_statement));
        assert!(!text(&result, "logic/claims.md").contains("PM refined B"));
        let conflict = plan
            .report
            .logic_conflicts
            .iter()
            .find(|item| item.selector == "C77")
            .unwrap();
        assert_eq!(conflict.kind, "mutable_field");
        assert!(!conflict.base.present);
        assert!(value(&conflict.ours.bytes).contains(ours_statement));
        assert!(value(&conflict.theirs.bytes).contains("PM refined B"));
    }
}

#[test]
fn forged_self_fact_cannot_overwrite_unpublished_edits() {
    let l = round_trip_start();
    let mut ours = l.b1.clone();
    edit(&mut ours, "logic/claims.md", "B says x", "B private edit");
    // A peer claims its C90 is B's C77 and asserts B's *current* bytes as the
    // source fact, which would make it the 3-way base.
    let mut forged = l.seed.clone();
    add_claim(&mut forged, "C90", "fork B finding", "B says x");
    let fingerprint = "f".repeat(64);
    let rows = [
        json!({"kind":"enrollment","source_key":"fork-b","label":"fork-b","time":"2026-10-03T09:00:00Z"}),
        json!({"kind":"enrollment","source_key":"peer-x","label":"peer-x","time":"2026-10-03T09:00:00Z"}),
        json!({"kind":"revision","source_key":"peer-x","fingerprint":fingerprint,"base":fingerprint,"predecessor":null,"time":"2026-10-03T09:00:00Z","git":null,"files":{},"mappings":[]}),
        json!({"kind":"inherited_revision","source_key":"fork-b","fingerprint":"forged","files":{"logic/claims.md":STANDARD.encode(text(&ours, "logic/claims.md"))},"mappings":[{"source_key":"fork-b","original":"C77","target":"C90","layer":"logic","path":"logic/claims.md"}],"via_source_key":"peer-x","via_revision":fingerprint}),
    ];
    let mut log = String::from("format: ara.merge-log/v1\nrecords:\n");
    for row in rows {
        log.push_str(&format!("  - {row}\n"));
    }
    put(&mut forged, "trace/merge_log.yaml", log);
    put(
        &mut forged,
        "trace/aliases.yaml",
        "format: ara.aliases/v1\naliases:\n  - {\"source_key\":\"fork-b\",\"label\":\"fork-b\",\"original\":\"C77\",\"target\":\"C90\",\"revision\":\"forged\"}\n",
    );
    let plan = try_merge_as(&l.seed, &ours, &forged, "peer", 3, "fork-b").unwrap();
    assert_eq!(plan.report.exit_code(), 1);
    let result = materialized(&plan.working);
    // Identity is reused (no duplicate), but the unpublished edit survives.
    assert_eq!(
        headings(&result),
        ["## C01: shared", "## C77: fork B finding"]
    );
    assert!(text(&result, "logic/claims.md").contains("B private edit"));
    let conflict = plan
        .report
        .logic_conflicts
        .iter()
        .find(|item| item.selector == "C77")
        .unwrap();
    assert!(!conflict.base.present);
    assert!(value(&conflict.ours.bytes).contains("B private edit"));
    assert!(value(&conflict.theirs.bytes).contains("B says x"));
}

#[test]
fn canonical_with_its_self_key_receives_its_own_entries_back() {
    let l = round_trip_start();
    let b2 = absorb(&l, &l.b1, &l.c2);
    let plan = try_merge_as(&l.b1, &l.c2, &b2, "fork-b", 5, "canonical").unwrap();
    assert_eq!(plan.report.exit_code(), 0, "{:#?}", plan.report.conflicts);
    let c3 = materialized(&plan.working);
    assert_eq!(headings(&c3), CANONICAL);
    for path in [
        "logic/claims.md",
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
    // B's later edit has a trusted base (its previous publication B2).
    let mut b3 = b2.clone();
    edit(
        &mut b3,
        "logic/claims.md",
        "PM combines A and B",
        "B agrees",
    );
    let c4 = clean(&b2, &c3, &b3, "fork-b", 7);
    assert!(
        text(&c4, "logic/claims.md")
            .contains("## C04: canonical synthesis\n- **Statement**: B agrees")
    );
    // The same edit arriving with B's first return has no trusted base.
    let plan = try_merge_as(&l.b1, &l.c2, &b3, "fork-b", 5, "canonical").unwrap();
    assert_eq!(plan.report.exit_code(), 1);
    let result = materialized(&plan.working);
    assert_eq!(headings(&result), CANONICAL);
    assert!(text(&result, "logic/claims.md").contains("PM combines A and B"));
    assert!(
        plan.report
            .logic_conflicts
            .iter()
            .any(|item| item.selector == "C04" && !item.base.present)
    );
}

#[test]
fn self_key_is_explicit_recorded_once_and_fail_closed() {
    let l = round_trip_start();
    let b2 = absorb(&l, &l.b1, &l.c2);
    let error = try_merge_as(&l.seed, &b2, &l.c2, "canonical", 4, "fork-x")
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.self_identity_conflict");
    assert!(!error.evidence.is_empty());
    let error = try_merge_as(&l.seed, &l.b1, &l.c2, "fork-b", 4, "fork-b")
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.self_identity_conflict");
    // A key this destination already enrolled as a foreign source is refused.
    let mut peer = l.seed.clone();
    put(
        &mut peer,
        "trace/merge_log.yaml",
        format!(
            "format: ara.merge-log/v1\nrecords:\n  - {}\n",
            json!({"kind":"enrollment","source_key":"fork-b","label":"fork-b","time":"2026-10-03T09:00:00Z"})
        ),
    );
    let enrolled = clean(&l.seed, &l.b1, &peer, "peer", 3);
    assert_eq!(kinds(&enrolled, "fork-b"), ["enrollment"]);
    let error = try_merge_as(&l.seed, &enrolled, &l.c2, "canonical", 4, "fork-b")
        .err()
        .unwrap();
    assert_eq!(
        error.code, "merge.self_identity_conflict",
        "{}",
        error.message
    );
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
    // Without any self key these entries are unproven; in this fixture B's
    // returning session collides and the merge rejects.
    let error = try_merge(&l.seed, &l.b1, &l.c2, "canonical", 3)
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.identity");
}

#[test]
fn replay_reapplies_recorded_mappings_without_reproving_origins() {
    let l = round_trip_start();
    let b2 = absorb(&l, &l.b1, &l.c2);
    // Deleting the original outside the CLI leaves this destination's own
    // aliases dangling; that is reported before replay is even considered.
    let mut gone = b2.clone();
    let claims = text(&gone, "logic/claims.md").to_owned();
    let start = claims.find("\n## C77").unwrap();
    let end = start + 1 + claims[start + 1..].find("\n## ").unwrap();
    put(
        &mut gone,
        "logic/claims.md",
        format!("{}{}", &claims[..start], &claims[end..]),
    );
    let error = try_merge(&l.seed, &gone, &l.c2, "canonical", 4)
        .err()
        .unwrap();
    assert_eq!(error.code, "merge.alias_dangling");
    // An intact destination replays the same canonical revision as a no-op,
    // both before and after it absorbs a later revision.
    let replay = try_merge(&l.seed, &b2, &l.c2, "canonical", 4).unwrap();
    assert!(replay.working.changed_paths().is_empty());
    let mut c3 = l.c2.clone();
    add_claim(&mut c3, "C05", "canonical follow-up", "PM adds more");
    let b3 = clean(&l.c2, &b2, &c3, "canonical", 5);
    let replay = try_merge(&l.c2, &b3, &c3, "canonical", 6).unwrap();
    assert!(replay.working.changed_paths().is_empty());
    assert_eq!(
        try_merge(&l.seed, &b3, &l.c2, "canonical", 6)
            .err()
            .unwrap()
            .code,
        "merge.source_regression"
    );
}
