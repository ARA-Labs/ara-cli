//! Integration tests over the pinned fixture corpus.

use std::path::{Path, PathBuf};

use ara_core::parse_sources;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(fixtures().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// Both official artifacts must parse with **zero** errors and **zero**
/// warnings (every canonical field is modeled) — the Stage-1 acceptance bar.
#[test]
#[cfg(feature = "native")]
fn official_fixtures_are_clean() {
    for dir in ["minimal-artifact", "resnet-ara-example"] {
        let path = fixtures().join("official").join(dir);
        let (manifest, report) =
            ara_core::parse_dir(&path).unwrap_or_else(|r| panic!("{dir} failed: {r}"));
        assert!(report.is_ok(), "{dir} has errors: {report}");
        assert!(report.warnings().is_empty(), "{dir} has warnings: {report}");
        assert!(!manifest.nodes.is_empty(), "{dir} produced no nodes");
    }
}

#[test]
#[cfg(feature = "native")]
fn minimal_manifest_snapshot() {
    let path = fixtures().join("official/minimal-artifact");
    let (manifest, _) = ara_core::parse_dir(&path).expect("ok");
    insta::assert_json_snapshot!("minimal_manifest", manifest);
}

#[test]
#[cfg(feature = "native")]
fn resnet_manifest_snapshot() {
    let path = fixtures().join("official/resnet-ara-example");
    let (manifest, _) = ara_core::parse_dir(&path).expect("ok");
    insta::assert_json_snapshot!("resnet_manifest", manifest);
}

/// Parsing the same input twice yields byte-identical JSON across all four
/// vectors — determinism from source-order preservation.
#[test]
#[cfg(feature = "native")]
fn parse_is_deterministic() {
    let path = fixtures().join("official/resnet-ara-example");
    let (a, _) = ara_core::parse_dir(&path).expect("ok");
    let (b, _) = ara_core::parse_dir(&path).expect("ok");
    let ja = serde_json::to_string_pretty(&a).unwrap();
    let jb = serde_json::to_string_pretty(&b).unwrap();
    assert_eq!(ja, jb);
}

/// `parse_dir` on the real minimal artifact resolves the C01 binding (claims.md
/// present) and leaves no unresolved-binding warning.
#[test]
#[cfg(feature = "native")]
fn parse_dir_resolves_bindings() {
    let path = fixtures().join("official/minimal-artifact");
    let (manifest, report) = ara_core::parse_dir(&path).expect("ok");
    assert!(!manifest.bindings.is_empty());
    assert!(
        !report
            .warnings()
            .iter()
            .any(|w| w.message.contains("unresolved"))
    );
}

/// The widened node model (`pivot` + widened `dead_end`) projects to the
/// expected manifest.
#[test]
fn pivot_deadend_snapshot() {
    let yaml = read("synthetic/pivot_deadend.yaml");
    let (manifest, _) = parse_sources(&yaml, None).expect("ok");
    insta::assert_json_snapshot!("pivot_deadend_manifest", manifest);
}

/// The six widened body fields (`hypothesis`/`failure_mode`/`lesson` on
/// `dead_end`, `prior_direction`/`new_direction`/`reason` on `pivot`) must
/// produce ZERO unknown-field warnings — they are first-class, not dropped.
/// (`lesson` is shared by `dead_end` and `pivot`.)
#[test]
fn pivot_deadend_has_no_field_warnings() {
    let yaml = read("synthetic/pivot_deadend.yaml");
    let (manifest, report) = parse_sources(&yaml, None).expect("ok");
    assert!(
        report.warnings().is_empty(),
        "unexpected warnings: {report}"
    );
    let pivot = manifest
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "N01")
        .expect("pivot node");
    match &pivot.fields {
        ara_core::manifest::NodeFields::Pivot {
            prior_direction,
            new_direction,
            reason,
            ..
        } => {
            assert_eq!(prior_direction.as_deref(), Some("Full manual curation"));
            assert!(new_direction.is_some());
            assert!(reason.is_some());
        }
        other => panic!("expected pivot fields, got {other:?}"),
    }
}

#[test]
fn root_single_dialect_normalizes() {
    let yaml = read("synthetic/root_single.yaml");
    let (manifest, report) = parse_sources(&yaml, None).expect("ok");
    assert!(report.is_ok());
    assert_eq!(manifest.nodes.len(), 2);
    assert_eq!(manifest.links.len(), 1); // N01 -> N02
}

#[test]
fn broken_claim_ref_errors() {
    let yaml = read("broken/broken_claim_ref.yaml");
    // Provide claims that lack C99 so the reference is genuinely broken.
    let err = parse_sources(&yaml, Some("## C01: only claim\n")).unwrap_err();
    assert!(
        err.errors()
            .iter()
            .any(|d| d.message.contains("unknown claim")),
        "expected broken-claim error, got: {err}"
    );
}

#[test]
fn dup_id_errors() {
    let yaml = read("broken/dup_id.yaml");
    let err = parse_sources(&yaml, None).unwrap_err();
    assert!(
        err.errors()
            .iter()
            .any(|d| d.message.contains("duplicate node id"))
    );
}

#[test]
fn cycle_errors() {
    let yaml = read("broken/cycle.yaml");
    let err = parse_sources(&yaml, None).unwrap_err();
    assert!(err.errors().iter().any(|d| d.message.contains("cycle")));
}

#[test]
fn ambiguous_root_errors() {
    let yaml = read("broken/ambiguous_root.yaml");
    let err = parse_sources(&yaml, None).unwrap_err();
    assert!(err.errors().iter().any(|d| d.message.contains("both")));
}

/// A missing directory (or missing `exploration_tree.yaml`) is a clean error,
/// not a panic.
#[test]
#[cfg(feature = "native")]
fn missing_dir_is_clean_error() {
    let err = ara_core::parse_dir(Path::new("/no/such/ara/dir")).unwrap_err();
    assert!(!err.is_ok());
    assert!(err.errors()[0].message.contains("cannot read"));
}

/// Full snapshot over a real artifact: locks the logic-section fields
/// (`paper`/`problem`/`concepts`/`related_work`/`recipes`) plus the evidence
/// layer (`exhibits`/`built_on`/`node_exhibits`). Exhibit and recipe bodies are
/// redacted — they are large verbatim blobs; the snapshot locks structure and
/// linkage, not the body text.
#[test]
#[cfg(feature = "native")]
fn self_composing_policies_snapshot() {
    let path = fixtures().join("corpus/paperbench/self-composing-policies");
    let (manifest, _) = ara_core::parse_dir(&path).expect("ok");
    // Sanity-checks on the parsed logic layer before the snapshot locks it.
    let paper = manifest.paper.as_ref().expect("paper present");
    assert_eq!(
        paper.title.as_deref(),
        Some("Self-Composing Policies for Scalable Continual Reinforcement Learning")
    );
    assert_eq!(paper.year.as_deref(), Some("2024"));
    assert_eq!(
        manifest.concepts.first().map(|c| c.term.as_str()),
        Some("CompoNet")
    );
    assert_eq!(manifest.related_work.len(), 9);
    assert_eq!(manifest.related_work[0].id, "RW01");
    assert_eq!(manifest.recipes.len(), 4); // solution/*.md, sorted
    // Evidence layer is now populated: 5 tables + 4 figures.
    assert_eq!(manifest.exhibits.len(), 9);
    assert!(!manifest.node_exhibits.is_empty());
    assert!(!manifest.built_on.is_empty());

    // DoD anchor: N07's node→exhibit resolution is EXACTLY the two scalability
    // exhibits, and its node→RW resolution includes RW01 and RW09.
    let n07_exhibits: Vec<&str> = manifest
        .node_exhibits
        .iter()
        .filter(|ne| ne.node.as_str() == "N07")
        .map(|ne| ne.exhibit.as_str())
        .collect();
    assert_eq!(
        n07_exhibits,
        vec!["fig3_scalability", "figb1_memory_growth"],
        "N07 node_exhibits must be exactly the two scalability exhibits"
    );
    let n07_rw: Vec<&str> = manifest
        .built_on
        .iter()
        .filter(|b| b.node.as_str() == "N07")
        .map(|b| b.related_work.as_str())
        .collect();
    assert!(
        n07_rw.contains(&"RW01") && n07_rw.contains(&"RW09"),
        "N07 built_on must include RW01 and RW09, got: {n07_rw:?}"
    );

    insta::assert_json_snapshot!("self_composing_policies_manifest", manifest, {
        ".exhibits[].body" => "[exhibit body redacted]",
        ".recipes[].body" => "[recipe body redacted]",
    });
}

/// End-to-end over synthetic header-variant fixtures: a single artifact whose
/// `evidence/README.md` mixes the reordered `Claims`, `Key refs`, no-claims-
/// column (`What it shows`), backtick-file-cell, dual-ext, and `Used by` fact
/// shapes. Confirms the column-name resolver extracts the right ids/claims and
/// that resolution wires nodes to exhibits and related work.
#[test]
#[cfg(feature = "native")]
fn evidence_header_variants_resolve_end_to_end() {
    let path = fixtures().join("evidence/e2e-variants");
    let (manifest, report) = ara_core::parse_dir(&path).expect("ok");
    assert!(report.is_ok(), "must not error: {report}");

    let by_id = |id: &str| manifest.exhibits.iter().find(|e| e.id == id);
    // Backtick file cell + reordered Claims column → C01; index source wins.
    let backtick = by_id("t_backtick").expect("t_backtick exhibit");
    assert_eq!(backtick.claims, vec![ara_core::ClaimId::new("C01")]);
    assert_eq!(backtick.source.as_deref(), Some("Table 1")); // index beats body
    // Key refs column → C05.
    let keyrefs = by_id("t_keyrefs").expect("t_keyrefs exhibit");
    assert_eq!(keyrefs.claims, vec![ara_core::ClaimId::new("C05")]);
    assert_eq!(
        keyrefs.description.as_deref(),
        Some("Key-refs carries claims")
    );
    // Dual-ext id + no claims column → claims fall back to body `Supports:` C01.
    let dualext = by_id("f_dualext").expect("f_dualext exhibit");
    assert_eq!(dualext.claims, vec![ara_core::ClaimId::new("C01")]);

    // Resolution: N02 (binds C01) → t_backtick + f_dualext; N03 (binds C05) → t_keyrefs.
    let node_ex = |node: &str| -> Vec<&str> {
        manifest
            .node_exhibits
            .iter()
            .filter(|ne| ne.node.as_str() == node)
            .map(|ne| ne.exhibit.as_str())
            .collect()
    };
    let n02 = node_ex("N02");
    assert!(
        n02.contains(&"t_backtick") && n02.contains(&"f_dualext"),
        "got: {n02:?}"
    );
    assert_eq!(node_ex("N03"), vec!["t_keyrefs"]);

    // built_on: N02 → RW01 (C01), N03 → RW02 (C05).
    let built = |node: &str| -> Vec<&str> {
        manifest
            .built_on
            .iter()
            .filter(|b| b.node.as_str() == node)
            .map(|b| b.related_work.as_str())
            .collect()
    };
    assert_eq!(built("N02"), vec!["RW01"]);
    assert_eq!(built("N03"), vec!["RW02"]);
}

/// Malformed/partial evidence WARNS but never errors: an index row pointing at a
/// missing body file warns; a body file with no index row warns; and a node
/// bound to a claim no exhibit carries yields an EMPTY node_exhibits (no error).
#[test]
#[cfg(feature = "native")]
fn evidence_malformed_warns_not_fatal() {
    let path = fixtures().join("evidence/malformed");
    let (manifest, report) = ara_core::parse_dir(&path).expect("Ok despite malformed evidence");
    assert!(
        report.is_ok(),
        "malformed evidence must not error: {report}"
    );

    // Bodies present → exhibits; the ghost index row is NOT an exhibit.
    assert!(manifest.exhibits.iter().any(|e| e.id == "present"));
    assert!(manifest.exhibits.iter().any(|e| e.id == "orphan"));
    assert!(!manifest.exhibits.iter().any(|e| e.id == "ghost"));

    // Index row with no body file → warning.
    assert!(
        report
            .warnings()
            .iter()
            .any(|w| w.path.contains("ghost") && w.message.contains("no body")),
        "expected missing-file warning, got: {report}"
    );
    // Body file with no index row → warning.
    assert!(
        report
            .warnings()
            .iter()
            .any(|w| w.path.contains("orphan") && w.message.contains("no index row")),
        "expected orphan-body warning, got: {report}"
    );
    // N01 binds C09, which no exhibit carries → empty node_exhibits, no error.
    assert!(
        manifest.node_exhibits.is_empty(),
        "no exhibit carries C09 → node_exhibits must be empty, got: {:?}",
        manifest.node_exhibits
    );
}

/// GAP-1: an OLD manifest JSON lacking the new evidence/logic fields still
/// deserializes (serde defaults), and a fully-populated manifest round-trips
/// serialize→deserialize→equal.
#[test]
fn manifest_forward_and_round_trip_compat() {
    use ara_core::Manifest;

    // OLD-shape JSON: only the four original vectors, none of the new fields.
    let old = r#"{
        "nodes": [],
        "links": [],
        "bindings": [],
        "claims": []
    }"#;
    let m: Manifest = serde_json::from_str(old).expect("old manifest deserializes via defaults");
    assert!(m.paper.is_none());
    assert!(m.exhibits.is_empty());
    assert!(m.built_on.is_empty());
    assert!(m.node_exhibits.is_empty());
    assert!(m.related_work.is_empty());
    assert!(m.concepts.is_empty());
    assert!(m.problem.is_none());
    assert!(m.recipes.is_empty());

    // A fully-populated manifest round-trips exactly.
    let path = fixtures().join("evidence/e2e-variants");
    #[cfg(feature = "native")]
    {
        let (populated, _) = ara_core::parse_dir(&path).expect("ok");
        assert!(!populated.exhibits.is_empty());
        assert!(!populated.node_exhibits.is_empty());
        let json = serde_json::to_string(&populated).expect("serialize");
        let back: Manifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(populated, back, "populated manifest must round-trip equal");
    }
    let _ = path;
}

/// Present-but-malformed logic files WARN (never error): parse_dir still returns
/// `Ok`, partial output is retained, and one warning is raised per defect.
#[test]
#[cfg(feature = "native")]
fn malformed_logic_files_warn_not_fatal() {
    let path = fixtures().join("sections/malformed");
    let (manifest, report) = ara_core::parse_dir(&path).expect("Ok despite malformed logic files");
    assert!(report.is_ok(), "malformed logic must not error: {report}");

    // PAPER.md: broken frontmatter → warning, paper dropped to None.
    assert!(manifest.paper.is_none());
    assert!(
        report
            .warnings()
            .iter()
            .any(|w| w.path == "PAPER.md" && w.message.contains("malformed")),
        "expected PAPER.md malformed warning, got: {report}"
    );

    // concepts.md: block with no Definition → partial concept + warning.
    assert_eq!(manifest.concepts.len(), 1);
    assert!(manifest.concepts[0].definition.is_none());
    assert!(manifest.concepts[0].notation.is_some()); // partial output kept
    assert!(
        report
            .warnings()
            .iter()
            .any(|w| w.path.starts_with("concepts[") && w.message.contains("no definition")),
        "expected concepts warning, got: {report}"
    );

    // related_work.md: block with no DOI (and no Claims affected) → partial + warn.
    assert_eq!(manifest.related_work.len(), 1);
    assert!(manifest.related_work[0].doi.is_none());
    assert!(manifest.related_work[0].claims_affected.is_empty());
    assert_eq!(manifest.related_work[0].kind.as_deref(), Some("baseline"));
    assert!(
        report
            .warnings()
            .iter()
            .any(|w| w.path.starts_with("related_work[") && w.message.contains("no DOI")),
        "expected related_work warning, got: {report}"
    );
}

/// An artifact carrying only `trace/` + `logic/claims.md` parses with ZERO new
/// warnings and no logic-section content — absent files are silently skipped.
#[test]
#[cfg(feature = "native")]
fn absent_logic_files_add_no_warnings() {
    let path = fixtures().join("sections/absent");
    let (manifest, report) = ara_core::parse_dir(&path).expect("ok");
    assert!(report.is_ok());
    assert!(
        report.warnings().is_empty(),
        "absent logic files must not warn: {report}"
    );
    assert!(manifest.paper.is_none());
    assert!(manifest.problem.is_none());
    assert!(manifest.concepts.is_empty());
    assert!(manifest.related_work.is_empty());
    assert!(manifest.recipes.is_empty());
}

// ── Published-fields fixtures (T6) ─────────────────────────────────────────
//
// Fixture dirs (see each fixture's SOURCE.md for the upstream pin):
// - `published-fields/` — reduced the-ara-of-ara slice covering every
//   published field; must be warning-free end to end.
// - `published-fields-unknown/` — same slice + one `bogus_field` key.
// - `published-fields-wrong-kind/` — every scoped body field on a wrong kind.
// - `published-fields-aliased/` — one pivot node on the pre-canonical
//   from/to/trigger aliases.

/// Recursively copies a fixture directory into a fresh temp dir so `fix_dir`
/// can mutate the copy; the fixture source is never touched.
#[cfg(feature = "native")]
fn copy_fixture(rel: &str) -> tempfile::TempDir {
    fn copy_dir(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    let dir = tempfile::TempDir::new().unwrap();
    copy_dir(&fixtures().join(rel), dir.path());
    dir
}

#[cfg(feature = "native")]
fn tree_bytes(dir: &Path) -> Vec<u8> {
    std::fs::read(dir.join("trace/exploration_tree.yaml")).unwrap()
}

/// The canonical published-fields slice: zero errors, zero warnings, zero lint
/// findings, and every published field projects into the manifest (including
/// the `results`/`proofs` evidence categories).
#[test]
#[cfg(feature = "native")]
fn published_fields_fixture_is_clean() {
    use ara_core::manifest::{ExhibitKind, NodeFields};

    let path = fixtures().join("published-fields");
    let (manifest, report) = ara_core::parse_dir(&path).expect("ok");
    assert!(report.is_ok(), "errors: {report}");
    assert!(report.warnings().is_empty(), "warnings: {report}");

    // provenance + timestamp on every node.
    assert_eq!(manifest.nodes.len(), 3);
    for node in &manifest.nodes {
        assert!(
            node.provenance.is_some(),
            "{} misses provenance",
            node.id.as_str()
        );
        assert!(
            node.timestamp.is_some(),
            "{} misses timestamp",
            node.id.as_str()
        );
    }

    // experiment: exploration/outcome/status/result.
    let experiment = manifest
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "N02")
        .expect("experiment node");
    match &experiment.fields {
        NodeFields::Experiment {
            result,
            exploration,
            outcome,
            status,
        } => {
            assert!(result.is_some());
            assert!(exploration.is_some());
            assert!(outcome.is_some());
            assert_eq!(status.as_deref(), Some("completed"));
        }
        other => panic!("expected experiment fields, got {other:?}"),
    }

    // pivot: prior_direction/new_direction/reason/lesson.
    let pivot = manifest
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "N03")
        .expect("pivot node");
    match &pivot.fields {
        NodeFields::Pivot {
            prior_direction,
            new_direction,
            reason,
            lesson,
        } => {
            assert!(prior_direction.is_some());
            assert!(new_direction.is_some());
            assert!(reason.is_some());
            assert!(lesson.is_some());
        }
        other => panic!("expected pivot fields, got {other:?}"),
    }

    // The results/ + proofs/ bodies became exhibits of the right kinds, and
    // the claim bindings resolved against the index rows.
    assert_eq!(
        manifest.exhibits.len(),
        2,
        "exhibits: {:?}",
        manifest.exhibits
    );
    assert!(
        manifest
            .exhibits
            .iter()
            .any(|e| e.kind == ExhibitKind::Proof && e.id == "lemma1")
    );
    assert!(
        manifest
            .exhibits
            .iter()
            .any(|e| e.kind == ExhibitKind::Result && e.id == "main_result")
    );
    assert_eq!(manifest.node_exhibits.len(), 2);

    // Lint-clean too (ARA001-007 all quiet).
    let lint = ara_core::check_dir(&path);
    assert!(lint.is_empty(), "lint: {:?}", lint.diagnostics());
}

/// The `bogus_field` sibling warns (never errors) at the parse layer.
#[test]
#[cfg(feature = "native")]
fn published_fields_unknown_warns_not_errors() {
    let path = fixtures().join("published-fields-unknown");
    let (_manifest, report) = ara_core::parse_dir(&path).expect("ok");
    assert!(
        report.is_ok(),
        "an unknown field must warn, not error: {report}"
    );
    let bogus: Vec<_> = report
        .warnings()
        .iter()
        .filter(|w| w.message.contains("bogus_field"))
        .collect();
    assert_eq!(bogus.len(), 1, "warnings: {report}");
    assert_eq!(bogus[0].path, "nodes[N02]");
    assert_eq!(bogus[0].message, "unknown field `bogus_field`");
    // Not a lint matter: the format rules stay quiet.
    assert!(ara_core::check_dir(&path).is_empty());
}

/// Wrong-kind matrix: every scoped body field on a kind that does not project
/// it produces exactly its own drop warning — nothing is dropped silently.
#[test]
#[cfg(feature = "native")]
fn wrong_kind_matrix_warns_per_field() {
    let path = fixtures().join("published-fields-wrong-kind");
    let (_manifest, report) = ara_core::parse_dir(&path).expect("ok");
    assert!(report.is_ok(), "drop warnings must not error: {report}");

    let expected = [
        ("exploration", "decision"),
        ("status", "question"),
        ("prior_direction", "dead_end"),
        ("choice", "experiment"),
        ("outcome", "question"),
        ("result", "decision"),
        ("alternatives", "experiment"),
        ("rationale", "question"),
        ("hypothesis", "decision"),
        ("failure_mode", "insight"),
        ("why_failed", "decision"),
        ("lesson", "insight"),
        ("new_direction", "experiment"),
        ("reason", "decision"),
    ];
    for (field, kind) in expected {
        let msg = format!("field `{field}` dropped for type `{kind}`");
        assert!(
            report.warnings().iter().any(|w| w.message == msg),
            "missing warning `{msg}` in: {report}"
        );
    }
    assert_eq!(
        report.warnings().len(),
        expected.len(),
        "one warning per wrong-kind field: {report}"
    );
    // None of these match a fixable alias rule — the matrix is parse-only.
    assert!(ara_core::check_dir(&path).is_empty());
}

/// The aliased pivot fires ARA005/006/007 (fixable); `fix_dir` recovers the
/// dropped values into the canonical keys and a second run is a no-op.
#[test]
#[cfg(feature = "native")]
fn aliased_fixture_lints_and_fix_recovers() {
    use ara_core::manifest::NodeFields;

    let path = fixtures().join("published-fields-aliased");
    let lint = ara_core::check_dir(&path);
    let rules: Vec<ara_core::LintRuleId> = lint.diagnostics().iter().map(|d| d.rule).collect();
    assert_eq!(
        rules,
        vec![
            ara_core::LintRuleId::PivotFromAlias,
            ara_core::LintRuleId::PivotToAlias,
            ara_core::LintRuleId::PivotTriggerAlias,
        ],
        "diagnostics: {:?}",
        lint.diagnostics()
    );
    assert!(lint.diagnostics().iter().all(|d| d.fixable));

    // Pre-fix parse: the aliases are unknown fields — warnings, values dropped.
    let (manifest, report) = ara_core::parse_dir(&path).expect("ok");
    for alias in ["from", "to", "trigger"] {
        let msg = format!("unknown field `{alias}`");
        assert!(
            report.warnings().iter().any(|w| w.message == msg),
            "missing warning `{msg}` in: {report}"
        );
    }
    match &manifest.nodes[0].fields {
        NodeFields::Pivot {
            prior_direction,
            new_direction,
            reason,
            ..
        } => {
            assert!(prior_direction.is_none());
            assert!(new_direction.is_none());
            assert!(reason.is_none());
        }
        other => panic!("expected pivot fields, got {other:?}"),
    }

    // Fix on a temp copy: three renames, values recovered, tree warning-free.
    let dir = copy_fixture("published-fields-aliased");
    let first = ara_core::fix_dir(dir.path());
    assert_eq!(first.applied.len(), 3, "applied: {:?}", first.applied);
    assert!(first.skipped.is_empty(), "skipped: {:?}", first.skipped);
    let fixed = String::from_utf8(tree_bytes(dir.path())).unwrap();
    assert!(
        fixed.contains(
            "prior_direction: \"A curated 20-paper corpus of canonical AI papers as the eval set\""
        ),
        "fixed tree: {fixed}"
    );
    assert!(fixed.contains("new_direction: \"PaperBench's 23 papers"));
    assert!(fixed.contains("reason: \"PaperBench provides expert-authored rubrics"));
    assert!(!fixed.contains("\n    from:"), "fixed tree: {fixed}");
    assert!(!fixed.contains("\n    to:"), "fixed tree: {fixed}");
    assert!(!fixed.contains("\n    trigger:"), "fixed tree: {fixed}");

    let (manifest, report) = ara_core::parse_dir(dir.path()).expect("ok");
    assert!(report.warnings().is_empty(), "post-fix warnings: {report}");
    match &manifest.nodes[0].fields {
        NodeFields::Pivot {
            prior_direction,
            new_direction,
            reason,
            ..
        } => {
            assert_eq!(
                prior_direction.as_deref(),
                Some("A curated 20-paper corpus of canonical AI papers as the eval set")
            );
            assert_eq!(
                new_direction.as_deref(),
                Some("PaperBench's 23 papers, extended to 7 RE-Bench tasks")
            );
            assert_eq!(
                reason.as_deref(),
                Some("PaperBench provides expert-authored rubrics; the custom corpus had none")
            );
        }
        other => panic!("expected pivot fields, got {other:?}"),
    }

    // Second run: no-op, byte-identical tree.
    let after_first = tree_bytes(dir.path());
    let second = ara_core::fix_dir(dir.path());
    assert!(second.applied.is_empty(), "applied: {:?}", second.applied);
    assert!(second.changed_files.is_empty());
    assert_eq!(tree_bytes(dir.path()), after_first);
}

/// Idempotency on the canonical fixture: two fix passes change nothing (the
/// fixture is already canonical) and the tree stays byte-identical; the
/// post-fix state is strict-clean (no errors, no warnings, no lint findings).
#[test]
#[cfg(feature = "native")]
fn published_fields_fix_is_idempotent() {
    let dir = copy_fixture("published-fields");
    let before = tree_bytes(dir.path());

    let first = ara_core::fix_dir(dir.path());
    assert!(
        first.applied.is_empty(),
        "canonical fixture must need no fixes: {:?}",
        first.applied
    );
    assert_eq!(tree_bytes(dir.path()), before);

    let second = ara_core::fix_dir(dir.path());
    assert!(second.applied.is_empty());
    assert_eq!(tree_bytes(dir.path()), before);

    let (_manifest, report) = ara_core::parse_dir(dir.path()).expect("ok");
    assert!(report.is_ok(), "errors: {report}");
    assert!(report.warnings().is_empty(), "warnings: {report}");
    assert!(ara_core::check_dir(dir.path()).is_empty());
}
