//! Every validate-layer diagnostic site is stamped with its own stable
//! [`RuleCode`] (issue #43). Each case below triggers one site and pins the code
//! it must carry; the final check proves every validate-layer rule in the
//! registry is reachable from at least one case, so a new rule cannot be added
//! without a test that fires it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ara_core::{Diagnostic, ParseReport, RuleCode, RuleLayer, Severity, parse_dir};
use tempfile::TempDir;

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

/// Writes `files` (relative path, contents) into a fresh temp directory.
fn artifact(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (rel, body) in files {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    dir
}

fn report_of(dir: &Path) -> ParseReport {
    match parse_dir(dir) {
        Ok((_, report)) | Err(report) => report,
    }
}

fn all_diags(report: &ParseReport) -> Vec<Diagnostic> {
    report
        .errors()
        .iter()
        .chain(report.warnings())
        .cloned()
        .collect()
}

/// Asserts `report` carries a diagnostic with `code` whose message contains
/// `needle`, and that its severity is the rule's default.
fn assert_has(report: &ParseReport, code: RuleCode, needle: &str) {
    let diags = all_diags(report);
    let hit = diags
        .iter()
        .find(|d| d.code == code && d.message.contains(needle))
        .unwrap_or_else(|| panic!("no {code} diagnostic containing {needle:?}; got: {diags:#?}"));
    assert_eq!(hit.severity, code.default_severity(), "{code}");
    let bucket = match hit.severity {
        Severity::Error => report.errors(),
        Severity::Warning => report.warnings(),
    };
    assert!(
        bucket.contains(hit),
        "{code} filed under the wrong severity"
    );
}

const Q: &str = "tree:\n  - id: N01\n    type: question\n";

/// Artifact files as (relative path, contents). Empty means "no tree file at all".
type Files = Vec<(&'static str, &'static str)>;

/// (expected code, message needle, artifact files).
fn tree_cases() -> Vec<(RuleCode, &'static str, Files)> {
    let tree = "trace/exploration_tree.yaml";
    let claims = "logic/claims.md";
    vec![
        (
            RuleCode::MalformedTree,
            "",
            vec![(tree, "tree: not-a-list\n")],
        ),
        (RuleCode::UnreadableTree, "cannot read", vec![]),
        (
            RuleCode::TreeAndRoot,
            "both `tree:` and `root:`",
            vec![(tree, "tree: []\nroot:\n  id: N01\n")],
        ),
        (
            RuleCode::MissingTree,
            "neither `tree:` nor `root:`",
            vec![(tree, "meta: hi\n")],
        ),
        (
            RuleCode::MissingNodeId,
            "missing an `id`",
            vec![(tree, "tree:\n  - type: question\n")],
        ),
        (
            RuleCode::DuplicateNodeId,
            "duplicate node id",
            vec![(
                tree,
                "tree:\n  - id: N01\n    type: question\n  - id: N01\n    type: insight\n",
            )],
        ),
        (
            RuleCode::DuplicateClaimId,
            "duplicate claim id",
            vec![(tree, Q), (claims, "## C01: A\n## C01: B\n")],
        ),
        (
            RuleCode::UnknownEvidenceClaim,
            "unknown claim `C02`",
            vec![
                (
                    tree,
                    "tree:\n  - id: N01\n    type: question\n    evidence: [C02]\n",
                ),
                (claims, "## C01: A\n"),
            ],
        ),
        (
            RuleCode::UnknownDependencyNode,
            "unknown node `N99`",
            vec![(
                tree,
                "tree:\n  - id: N01\n    type: question\n    also_depends_on: [N99]\n",
            )],
        ),
        (
            RuleCode::UnknownClaimDependency,
            "unknown claim `C09`",
            vec![
                (tree, Q),
                (claims, "## C01: A\n- **Dependencies**: [C09]\n"),
            ],
        ),
        (
            RuleCode::DependencyCycle,
            "cycle detected",
            vec![(
                tree,
                "tree:\n  - id: N01\n    type: question\n    also_depends_on: [N02]\n  \
                 - id: N02\n    type: question\n    also_depends_on: [N01]\n",
            )],
        ),
        (
            RuleCode::UnknownDocumentField,
            "unknown field `top_bogus`",
            vec![(
                tree,
                "tree:\n  - id: N01\n    type: question\ntop_bogus: 1\n",
            )],
        ),
        (
            RuleCode::UnknownNodeField,
            "unknown field `bogus`",
            vec![(
                tree,
                "tree:\n  - id: N01\n    type: question\n    bogus: 1\n",
            )],
        ),
        (
            RuleCode::EmptyTree,
            "empty manifest",
            vec![(tree, "tree: []\n")],
        ),
        (
            RuleCode::MissingNodeType,
            "missing a `type`",
            vec![(tree, "tree:\n  - id: N01\n")],
        ),
        (
            RuleCode::FieldDroppedMissingType,
            "dropped for missing type",
            vec![(tree, "tree:\n  - id: N01\n    result: x\n")],
        ),
        (
            RuleCode::FieldDroppedUnknownType,
            "dropped for unknown type `weird`",
            vec![(tree, "tree:\n  - id: N01\n    type: weird\n    result: x\n")],
        ),
        (
            RuleCode::FieldDroppedForType,
            "dropped for type `question`",
            vec![(
                tree,
                "tree:\n  - id: N01\n    type: question\n    result: x\n",
            )],
        ),
        (
            RuleCode::UnresolvedClaimReference,
            "unresolved",
            vec![(
                tree,
                "tree:\n  - id: N01\n    type: question\n    evidence: [C01]\n",
            )],
        ),
        (
            RuleCode::RedundantAncestorDependency,
            "redundant `also_depends_on`",
            vec![(
                tree,
                "tree:\n  - id: N01\n    type: question\n    children:\n      \
                 - id: N02\n        type: insight\n        also_depends_on: [N01]\n",
            )],
        ),
        (
            RuleCode::DuplicateLink,
            "duplicate DependsOn link",
            vec![(
                tree,
                "tree:\n  - id: N01\n    type: question\n    also_depends_on: [N02, N02]\n  \
                 - id: N02\n    type: question\n",
            )],
        ),
        (
            RuleCode::DuplicateExhibitBasename,
            "duplicate exhibit basename",
            vec![
                (tree, Q),
                ("evidence/figures/X1.md", "fig"),
                ("evidence/tables/X1.md", "table"),
            ],
        ),
    ]
}

/// Committed fixtures that exercise the logic-section and evidence readers.
fn fixture_cases() -> Vec<(RuleCode, &'static str, &'static str)> {
    vec![
        (
            RuleCode::MalformedPaperFrontmatter,
            "malformed frontmatter",
            "sections/malformed",
        ),
        (
            RuleCode::ConceptMissingDefinition,
            "no definition",
            "sections/malformed",
        ),
        (
            RuleCode::RelatedWorkMissingDoi,
            "no DOI",
            "sections/malformed",
        ),
        (
            RuleCode::ExhibitMissingIndexRow,
            "no index row",
            "evidence/malformed",
        ),
        (
            RuleCode::IndexRowMissingExhibit,
            "no body under evidence/",
            "evidence/malformed",
        ),
    ]
}

#[test]
fn every_validate_site_carries_its_rule_code() {
    let mut covered = BTreeSet::new();
    for (code, needle, files) in tree_cases() {
        let dir = artifact(&files);
        assert_has(&report_of(dir.path()), code, needle);
        covered.insert(code);
    }
    for (code, needle, rel) in fixture_cases() {
        assert_has(&report_of(&fixture(rel)), code, needle);
        covered.insert(code);
    }

    let expected: BTreeSet<RuleCode> = RuleCode::ALL
        .iter()
        .copied()
        .filter(|r| r.layer() == RuleLayer::Validate)
        .collect();
    assert_eq!(covered, expected, "a validate rule has no triggering case");
}

/// Official fixtures are clean, so no rule fires at all.
#[test]
fn official_fixtures_fire_no_rules() {
    for name in ["minimal-artifact", "resnet-ara-example"] {
        let report = report_of(&fixture("official").join(name));
        assert!(all_diags(&report).is_empty(), "{name}: {report}");
    }
}

/// The rule table in `docs/stage-5-check.md` lists every registered rule, in
/// order, with the registry's name, severity, and fixability.
#[test]
fn docs_rule_table_matches_registry() {
    let doc_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/stage-5-check.md");
    let doc = std::fs::read_to_string(&doc_path).expect("read stage-5 doc");
    let section = doc
        .split("\n## Rule codes\n")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("`## Rule codes` section");
    let rows: Vec<Vec<&str>> = section
        .lines()
        .filter(|l| l.starts_with("| `ARA"))
        .map(|l| l.trim_matches('|').split(" | ").map(str::trim).collect())
        .collect();
    assert_eq!(rows.len(), RuleCode::ALL.len(), "one docs row per rule");
    for (row, &rule) in rows.iter().zip(RuleCode::ALL) {
        assert_eq!(row[0], format!("`{rule}`"));
        assert_eq!(row[1], rule.name(), "{rule} name");
        assert_eq!(
            row[3],
            rule.default_severity().to_string(),
            "{rule} severity"
        );
        let fixable = if rule.fixable() { "yes" } else { "no" };
        assert_eq!(row[4], fixable, "{rule} fixable");
    }
}
