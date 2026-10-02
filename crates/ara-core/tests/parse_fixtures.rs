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

/// Preserve scientific fields and exact evidence/reference resolution from a real artifact.
#[test]
#[cfg(feature = "native")]
fn self_composing_policies_fields_and_references() {
    let path = fixtures().join("corpus/paperbench/self-composing-policies");
    let (manifest, _) = ara_core::parse_dir(&path).expect("ok");
    // Check the parsed logic layer against the pinned scientific source.
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
}

#[test]
#[cfg(feature = "native")]
fn figure_images_resolve_and_serialize_end_to_end() {
    let dir = copy_fixture("evidence/e2e-variants");
    let evidence = dir.path().join("evidence");
    let figures = evidence.join("figures");
    std::fs::create_dir_all(&figures).unwrap();
    let body = "Supporting pixels.\n\n| Step | Loss |\n|---|---|\n| 1 | 0.5 |\n";
    std::fs::write(figures.join("f_pixels.md"), body).unwrap();
    std::fs::write(
        figures.join("f_pixels.png"),
        include_bytes!("fixtures/images/pixel.png"),
    )
    .unwrap();
    std::fs::write(
        figures.join("f_raster.jpg"),
        include_bytes!("fixtures/images/pixel.jpg"),
    )
    .unwrap();
    let mut index = std::fs::read_to_string(evidence.join("README.md")).unwrap();
    index.push_str("\n\n| File | Claims | Description |\n|---|---|---|\n| figures/f_pixels.md | C01 | Paired pixels |\n| figures/f_raster.jpg | C01 | Raster pixels |\n");
    std::fs::write(evidence.join("README.md"), index).unwrap();

    let (manifest, report) =
        ara_core::parse_and_layout_dir(dir.path(), &ara_core::LayoutOptions::default()).unwrap();
    assert!(report.is_ok());
    assert!(
        !report
            .warnings()
            .iter()
            .any(|w| w.code == ara_core::RuleCode::InvalidFigureImage)
    );
    let paired = manifest
        .exhibits
        .iter()
        .find(|e| e.id == "f_pixels")
        .unwrap();
    assert_eq!(paired.body, body);
    assert_eq!(paired.file, "evidence/figures/f_pixels.md");
    assert_eq!(
        paired.image.as_deref(),
        Some("evidence/figures/f_pixels.png")
    );
    let raster = manifest
        .exhibits
        .iter()
        .find(|e| e.id == "f_raster")
        .unwrap();
    assert_eq!(raster.body, "");
    assert_eq!(raster.description.as_deref(), Some("Raster pixels"));
    assert_eq!(
        raster.image.as_deref(),
        Some("evidence/figures/f_raster.jpg")
    );
    let linked: Vec<_> = manifest
        .node_exhibits
        .iter()
        .filter(|e| {
            e.node.as_str() == "N02" && matches!(e.exhibit.as_str(), "f_pixels" | "f_raster")
        })
        .map(|e| e.exhibit.as_str())
        .collect();
    assert_eq!(linked, ["f_pixels", "f_raster"]);
    let json = serde_json::to_value(&manifest).unwrap();
    let payloads = json["exhibits"].as_array().unwrap();
    assert_eq!(payloads.iter().filter(|e| e["id"] == "f_pixels").count(), 1);
    assert_eq!(
        payloads.iter().find(|e| e["id"] == "f_pixels").unwrap()["image"],
        "evidence/figures/f_pixels.png"
    );
    assert_eq!(
        serde_json::from_value::<ara_core::Manifest>(json).unwrap(),
        manifest
    );
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

    // provenance + timestamp on every node, pinned by value (not just
    // presence) so a projection swap in the Normalizer is caught.
    assert_eq!(manifest.nodes.len(), 3);
    let expected = [
        ("N01", "2026-03-12"),
        ("N02", "2026-04-08"),
        ("N03", "2026-03-12"),
    ];
    for (id, timestamp) in expected {
        let node = manifest
            .nodes
            .iter()
            .find(|n| n.id.as_str() == id)
            .unwrap_or_else(|| panic!("missing node {id}"));
        assert_eq!(node.provenance.as_deref(), Some("user"), "{id} provenance");
        assert_eq!(node.timestamp.as_deref(), Some(timestamp), "{id} timestamp");
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

    // Native aliases normalize before format repair; the repair preserves values.
    let (manifest, report) = ara_core::parse_dir(&path).expect("ok");
    assert!(report.warnings().is_empty(), "{report}");
    match &manifest.nodes[0].fields {
        NodeFields::Pivot {
            prior_direction,
            new_direction,
            reason,
            ..
        } => {
            assert!(prior_direction.is_some());
            assert!(new_direction.is_some());
            assert!(reason.is_some());
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

#[test]
fn old_wire_manifest_round_trips_without_optional_keys() {
    let old = serde_json::json!({
        "nodes": [], "links": [], "bindings": [],
        "claims": [{"id":"C01","title":"old","statement":null,"status":null,"proof":["E01"],"deps":[]}]
    });
    let manifest: ara_core::Manifest = serde_json::from_value(old.clone()).unwrap();
    assert!(manifest.observations.is_empty());
    assert!(manifest.sessions.is_empty());
    assert!(manifest.heuristics.is_empty());
    assert!(manifest.experiment_plans.is_empty());
    assert!(manifest.taste_comments.is_empty());
    assert_eq!(serde_json::to_value(manifest).unwrap(), old);
}

#[test]
fn complete_claim_prose_and_fields_survive_multiline_markdown() {
    let md = "## C01: Mechanism\n- **Statement**: First line\n  second line\n- **Proof**: E01, supported by:\n  - evidence/tables/results.md\n  - E02 and prose\n- **Conditions**: bounded\n  regime\n- **Sources**: quote «α»\n- **Provenance**: user-revised\n- **Falsification**: counterexample\n- **Tags**: test, mechanism\n- **Last revised**: 2026-10-01 (2026-10-01_001#3)\n- **Custom evidence**: untouched\n  extra proof\n";
    let (manifest, _) = parse_sources("tree: []\n", Some(md)).unwrap();
    let claim = &manifest.claims[0];
    assert_eq!(
        claim.statement.as_deref(),
        Some("First line\n  second line")
    );
    assert_eq!(claim.proof, ["E01", "E02"]);
    assert_eq!(
        claim.proof_content.as_deref(),
        Some("E01, supported by:\n  - evidence/tables/results.md\n  - E02 and prose")
    );
    assert_eq!(claim.conditions.as_deref(), Some("bounded\n  regime"));
    assert_eq!(claim.sources.as_deref(), Some("quote «α»"));
    assert_eq!(claim.provenance.as_deref(), Some("user-revised"));
    assert_eq!(claim.falsification.as_deref(), Some("counterexample"));
    assert_eq!(claim.tags.as_deref(), Some("test, mechanism"));
    assert_eq!(
        claim.last_revised.as_deref(),
        Some("2026-10-01 (2026-10-01_001#3)")
    );
    assert_eq!(
        claim.source_fields.last().unwrap().value,
        "untouched\n  extra proof"
    );
    assert_eq!(claim.body.as_deref(), Some(md));
}

#[test]
#[cfg(feature = "native")]
fn pinned_agent_fixture_preserves_published_source_values() {
    let load = ara_core::parse_dir_detailed(&fixtures().join("agent-cli"));
    assert!(load.io_issues.is_empty(), "{:?}", load.io_issues);
    let manifest = load.manifest.expect("complete fixture");
    let claim = &manifest.claims[0];
    assert_eq!(claim.id.as_str(), "C01");
    assert_eq!(claim.provenance.as_deref(), Some("user"));
    assert_eq!(
        claim.falsification.as_deref(),
        Some(
            "Show that agents achieve equivalent research task performance on PDFs as on structured formats."
        )
    );
    assert_eq!(
        claim.proof_content.as_deref(),
        Some("[evidence/README.md → ResearchCodeBench]")
    );
    assert!(claim.proof.is_empty());
    assert_eq!(claim.tags.as_deref(), Some("motivation, storytelling-tax"));
    let observation = &manifest.observations[1];
    assert_eq!(observation.id.as_str(), "O02");
    assert_eq!(observation.promoted, Some(true));
    assert_eq!(
        observation.promoted_to.as_deref(),
        Some("C09 — now supported with full 23-paper data (45.3% median sufficient)")
    );
    assert!(
        load.report
            .warnings()
            .iter()
            .any(|d| d.code == ara_core::RuleCode::MalformedPromotionDestination)
    );
    let heuristic = &manifest.heuristics[0];
    assert_eq!(heuristic.id.as_str(), "H01");
    assert_eq!(
        heuristic.rationale.as_deref(),
        Some(
            "Structured logic and executable evidence are the primary research object; the narrative paper is a compiled view. This eliminates the Storytelling Tax by encoding all knowledge as typed, queryable data."
        )
    );
    assert_eq!(
        heuristic.code_ref.as_deref(),
        Some("[paper/sections/protocol.tex]")
    );
    assert_eq!(heuristic.sensitivity.as_deref(), Some("high"));
    assert_eq!(heuristic.provenance.as_deref(), Some("user"));
    assert_eq!(
        manifest
            .recipes
            .iter()
            .find(|r| r.name == "heuristics")
            .unwrap()
            .body,
        load.sources["logic/solution/heuristics.md"]
    );
    let experiment = &manifest.experiment_plans[2];
    assert_eq!(experiment.id.as_str(), "E3");
    assert_eq!(experiment.status.as_deref(), Some("completed"));
    assert_eq!(
        experiment.prediction.as_deref(),
        Some(
            "- Cat A: ARA accuracy ≥ baseline (parity threshold; ARA should not lose fidelity)\n- Cat B: ARA accuracy > baseline by substantial margin (hyperparameter recovery)\n- Cat C: ARA accuracy >> baseline (failure knowledge absent from baseline format)\n- Token efficiency: Cat A ARA uses fewer or equal tokens per question (indexed lookup vs. linear scan)"
        )
    );
    let session = manifest
        .sessions
        .iter()
        .find(|s| s.id.as_str() == "2026-06-29_001")
        .unwrap();
    assert_eq!(session.date.as_deref(), Some("2026-06-29"));
    assert_eq!(session.turn_count, Some(1));
    assert_eq!(
        session.body,
        load.sources["trace/sessions/2026-06-29_001.yaml"]
    );
    let ara_core::SourceValue::Mapping(event) = &session.events_logged[0] else {
        panic!("event mapping")
    };
    assert_eq!(event["id"], ara_core::SourceValue::String("N123".into()));
    assert_eq!(event["turn"], ara_core::SourceValue::Integer(1));
    assert!(manifest.taste_comments.is_empty()); // Absent in the pinned revision.
    assert!(
        load.report
            .warnings()
            .iter()
            .any(|d| d.code == ara_core::RuleCode::DanglingSessionIndex)
    );
}

#[cfg(feature = "native")]
fn agent_artifact(files: &[(&str, &str)]) -> tempfile::TempDir {
    let temp = tempfile::TempDir::new().unwrap();
    for (file, source) in files {
        let path = temp.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    temp
}

#[test]
#[cfg(feature = "native")]
fn optional_layers_retain_safe_entries_duplicates_unknowns_and_history() {
    let temp = agent_artifact(&[
        ("trace/exploration_tree.yaml", "tree: []\n"),
        (
            "logic/problem.md",
            "# Problem\n\n## Observations\n### O01: local problem label\n",
        ),
        (
            "staging/observations.yaml",
            "observations:\n  - {id: O01, content: staged, promoted: true, promoted_to: 'logic/claims.md:C01', bound_to: [N01], custom: {nested: [1, true]}}\n  - {id: O01, content: second, stale: false}\n  - 7\n  - {id: O02, content: valid after malformed}\n",
        ),
        (
            "trace/taste_log.yaml",
            "entries:\n  - {id: T01, target: N01, tag: uncertain, object: evidence, comment: 'Need evidence', timestamp: '2026-10-01T10:30', custom: [a, b]}\n",
        ),
        (
            "trace/sessions/2026-10-01_010.yaml",
            "session: {id: '2026-10-01_010', date: '2026-10-01', summary: later}\nlogic_revisions:\n  - {turn: 4, entry: C01, field: Statement, before: old, after: new, signal: user-directive, provenance: user}\n",
        ),
        (
            "trace/sessions/2026-10-01_002.yaml",
            "session: {id: '2026-10-01_002', date: '2026-10-01', summary: earlier, custom: preserved}\nevents_logged: [{turn: 2, id: O01, summary: staged, custom: value}]\n",
        ),
        (
            "trace/sessions/session_index.yaml",
            "sessions:\n  - {id: '2026-10-01_002'}\n  - {id: '2026-10-01_099'}\n",
        ),
        (
            "logic/solution/heuristics.md",
            "# Heuristics\n## H01: preference\n- **Rationale**: explain\n- **Bounds**: source value\n",
        ),
        (
            "logic/experiments.md",
            "# Experiments\n## E01: plan\n**Setup**: first\nsecond\n**Prediction (directional)**: better\n**Falsification condition**: worse\n",
        ),
    ]);
    let load = ara_core::parse_dir_detailed(temp.path());
    let manifest = load.manifest.unwrap();
    assert_eq!(
        manifest
            .observations
            .iter()
            .map(|o| o.content.as_str())
            .collect::<Vec<_>>(),
        ["staged", "second", "valid after malformed"]
    );
    assert_eq!(
        manifest.observations[0].promoted_to.as_deref(),
        Some("logic/claims.md:C01")
    );
    assert_eq!(
        manifest.observations[0].bound_to,
        [ara_core::NodeId::new("N01")]
    );
    assert!(matches!(
        manifest.observations[0].extra["custom"],
        ara_core::SourceValue::Mapping(_)
    ));
    assert_eq!(
        manifest.problem.unwrap().observations,
        ["O01: local problem label"]
    );
    assert_eq!(manifest.taste_comments[0].id.as_str(), "T01");
    assert_eq!(manifest.taste_comments[0].comment, "Need evidence");
    assert_eq!(manifest.taste_comments[0].tag.as_deref(), Some("uncertain"));
    assert_eq!(
        manifest.taste_comments[0].object.as_deref(),
        Some("evidence")
    );
    assert_eq!(
        manifest
            .sessions
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["2026-10-01_002", "2026-10-01_010"]
    );
    assert_eq!(
        manifest.sessions[0].metadata_extra["custom"],
        ara_core::SourceValue::String("preserved".into())
    );
    let ara_core::SourceValue::Mapping(revision) = &manifest.sessions[1].logic_revisions[0] else {
        panic!("revision mapping")
    };
    assert_eq!(
        revision["before"],
        ara_core::SourceValue::String("old".into())
    );
    assert_eq!(
        revision["after"],
        ara_core::SourceValue::String("new".into())
    );
    assert_eq!(revision["turn"], ara_core::SourceValue::Integer(4));
    assert_eq!(
        manifest.heuristics[0].source_fields[1].value,
        "source value"
    );
    assert_eq!(
        manifest.experiment_plans[0].setup.as_deref(),
        Some("first\nsecond")
    );
    let codes = load
        .report
        .warnings()
        .iter()
        .map(|d| d.code)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(codes.contains(&ara_core::RuleCode::MalformedAgentLayer));
    assert!(codes.contains(&ara_core::RuleCode::DuplicateAgentId));
    assert!(codes.contains(&ara_core::RuleCode::DanglingSessionIndex));
    assert!(!codes.contains(&ara_core::RuleCode::MalformedPromotionDestination));
}

#[test]
#[cfg(feature = "native")]
fn native_load_distinguishes_absent_unreadable_and_incomplete_graphs() {
    let temp = agent_artifact(&[("trace/exploration_tree.yaml", "tree: []\n")]);
    let absent = ara_core::parse_dir_detailed(temp.path());
    assert!(absent.io_issues.is_empty());
    assert!(absent.manifest.unwrap().observations.is_empty());
    std::fs::create_dir_all(temp.path().join("staging/observations.yaml")).unwrap();
    let unreadable = ara_core::parse_dir_detailed(temp.path());
    assert_eq!(unreadable.io_issues[0].path, "staging/observations.yaml");
    assert_eq!(
        unreadable.io_issues[0].kind,
        ara_core::LoadIssueKind::Unreadable
    );
    std::fs::write(
        temp.path().join("trace/exploration_tree.yaml"),
        "tree:\n  - {id: N01, type: question}\n  - {id: N01, type: question}\n",
    )
    .unwrap();
    let truncated = ara_core::parse_dir_detailed(temp.path());
    assert!(truncated.manifest.is_none());
    assert!(
        truncated
            .report
            .errors()
            .iter()
            .any(|d| d.code == ara_core::RuleCode::DuplicateNodeId)
    );
    let missing = ara_core::parse_dir_detailed(&temp.path().join("absent"));
    assert!(missing.manifest.is_none());
    assert_eq!(missing.io_issues[0].kind, ara_core::LoadIssueKind::Missing);
}

#[test]
fn same_finding_links_are_independent_of_dependency_edges() {
    let tree = "tree:\n  - id: N01\n    type: question\n    children:\n      - id: N02\n        type: experiment\n        same_as: [N01]\n        artifacts: [{name: result, pointer: 'evidence/results/run.md', what: measurements, custom: preserved}]\n        concepts: [Attention]\n";
    let (manifest, report) = parse_sources(tree, None).unwrap();
    assert!(report.warnings().is_empty());
    assert_eq!(manifest.nodes[1].same_as, [ara_core::NodeId::new("N01")]);
    assert_eq!(manifest.links.len(), 1);
    assert_eq!(manifest.links[0].kind, ara_core::LinkKind::Child);
    assert_eq!(
        manifest.nodes[1].artifacts[0].pointer,
        "evidence/results/run.md"
    );
    assert_eq!(manifest.nodes[1].artifacts[0].what, "measurements");
    assert_eq!(
        manifest.nodes[1].artifacts[0].extra["custom"],
        ara_core::SourceValue::String("preserved".into())
    );
    assert_eq!(manifest.nodes[1].concepts, ["Attention"]);
    let wire = serde_json::to_value(&manifest).unwrap();
    assert!(wire["nodes"][0].get("same_as").is_none());
    assert_eq!(wire["nodes"][1]["same_as"], serde_json::json!(["N01"]));
    let cycle = "tree:\n  - {id: N01, type: question, same_as: [N02]}\n  - {id: N02, type: question, same_as: [N01]}\n";
    let (cycled, report) = parse_sources(cycle, None).unwrap();
    assert!(cycled.links.is_empty());
    assert!(
        report
            .warnings()
            .iter()
            .any(|d| d.code == ara_core::RuleCode::SameAsCycle)
    );
    assert!(report.errors().is_empty());
}

#[test]
#[cfg(feature = "native")]
fn native_concept_links_resolve_existing_terms_without_rewriting_source() {
    let tree = "tree:\n  - {id: N01, type: question, concepts: [Attention, 'logic/concepts.md#Attention']}\n";
    let temp = agent_artifact(&[
        ("trace/exploration_tree.yaml", tree),
        (
            "logic/concepts.md",
            "# Concepts\n## Attention\n- **Definition**: Weighted aggregation.\n",
        ),
    ]);
    let load = ara_core::parse_dir_detailed(temp.path());
    assert!(
        !load
            .report
            .warnings()
            .iter()
            .any(|d| d.code == ara_core::RuleCode::UnknownNodeConcept)
    );
    assert_eq!(load.sources["trace/exploration_tree.yaml"], tree);
    assert_eq!(
        load.manifest.unwrap().nodes[0].concepts,
        ["Attention", "logic/concepts.md#Attention"]
    );
}

#[test]
#[cfg(feature = "native")]
fn knowledge_registry_retains_exact_registered_bodies_and_rejects_unsafe_paths() {
    let paper = "---\ntitle: Example\nknowledge_paths: [appendix/notes.md]\n---\n# Example\n";
    let temp = agent_artifact(&[
        ("trace/exploration_tree.yaml", "tree: []\n"),
        ("PAPER.md", paper),
        ("appendix/notes.md", "# Notes\r\n\r\nEquation $x$.\r\n"),
        (
            "rubric/requirements.md",
            "# Requirements\n## R01: Source grounding\n",
        ),
        (
            "logic/solution/nested/details.md",
            "## Detail\nComplete source.\n",
        ),
    ]);
    let load = ara_core::parse_dir_detailed(temp.path());
    assert_eq!(
        ara_core::knowledge_paths(paper).unwrap(),
        ["appendix/notes.md"]
    );
    assert_eq!(
        load.sources["appendix/notes.md"],
        "# Notes\r\n\r\nEquation $x$.\r\n"
    );
    assert_eq!(
        load.sources["rubric/requirements.md"],
        "# Requirements\n## R01: Source grounding\n"
    );
    assert_eq!(
        load.sources["logic/solution/nested/details.md"],
        "## Detail\nComplete source.\n"
    );
    assert!(load.io_issues.is_empty());
    for path in [
        "../escape.md",
        "/absolute.md",
        "trace/history.md",
        "src/code.md",
        "evidence/table.md",
        ".ara/cache.md",
        ".git/config.md",
        "a//b.md",
        "a/./b.md",
        "a.txt",
        "C:/file.md",
        "a\\b.md",
    ] {
        let source = format!("---\nknowledge_paths: ['{path}']\n---\n");
        assert!(ara_core::knowledge_paths(&source).is_err(), "{path}");
    }
    assert!(ara_core::knowledge_paths("---\nknowledge_paths: [a.md, a.md]\n---\n").is_err());
    assert!(ara_core::knowledge_paths("---\nknowledge_paths: wrong-shape\n---\n").is_err());
    std::fs::write(
        temp.path().join("PAPER.md"),
        "---\nknowledge_paths: [missing.md]\n---\n",
    )
    .unwrap();
    let missing = ara_core::parse_dir_detailed(temp.path());
    assert_eq!(missing.io_issues[0].path, "missing.md");
    assert_eq!(missing.io_issues[0].kind, ara_core::LoadIssueKind::Missing);
}

#[test]
#[cfg(all(feature = "native", unix))]
fn registered_knowledge_paths_reject_symlink_components() {
    let temp = agent_artifact(&[
        ("trace/exploration_tree.yaml", "tree: []\n"),
        ("PAPER.md", "---\nknowledge_paths: [linked/notes.md]\n---\n"),
        ("outside/notes.md", "private source"),
    ]);
    std::os::unix::fs::symlink(temp.path().join("outside"), temp.path().join("linked")).unwrap();
    let load = ara_core::parse_dir_detailed(temp.path());
    assert!(!load.sources.contains_key("linked/notes.md"));
    assert_eq!(load.io_issues[0].kind, ara_core::LoadIssueKind::Unreadable);
}
