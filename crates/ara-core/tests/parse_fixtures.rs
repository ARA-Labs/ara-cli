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
/// `dead_end`, `from`/`to`/`trigger` on `pivot`) must produce ZERO unknown-field
/// warnings — they are now first-class, not dropped.
#[test]
fn pivot_deadend_has_no_field_warnings() {
    let yaml = read("synthetic/pivot_deadend.yaml");
    let (_manifest, report) = parse_sources(&yaml, None).expect("ok");
    let widened = ["hypothesis", "failure_mode", "lesson", "from", "to", "trigger"];
    for w in report.warnings() {
        for field in widened {
            assert!(
                !w.message.contains(field),
                "unexpected warning mentioning `{field}`: {}",
                w.message
            );
        }
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
/// (`paper`/`problem`/`concepts`/`related_work`/`recipes`). The evidence-derived
/// fields (`exhibits`/`built_on`/`node_exhibits`) stay empty for this slice.
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
    assert_eq!(manifest.concepts.first().map(|c| c.term.as_str()), Some("CompoNet"));
    assert_eq!(manifest.related_work.len(), 9);
    assert_eq!(manifest.related_work[0].id, "RW01");
    assert_eq!(manifest.recipes.len(), 4); // solution/*.md, sorted
    assert!(manifest.exhibits.is_empty());
    assert!(manifest.built_on.is_empty());
    assert!(manifest.node_exhibits.is_empty());
    insta::assert_json_snapshot!("self_composing_policies_manifest", manifest);
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
