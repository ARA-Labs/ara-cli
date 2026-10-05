//! Native loads: stray-fence claims recovery, protected metadata and dash
//! claim separators, with exactly one fence diagnostic per document.
#![cfg(feature = "native")]

use std::path::Path;

use ara_core::{ParseReport, RuleCode, Severity, parse_dir};
use tempfile::TempDir;

const TREE: &str = "tree:\n  - id: N01\n    type: experiment\n    evidence: [C01, C02]\n";

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

fn fence_diagnostics(report: &ParseReport) -> Vec<(RuleCode, Severity, String)> {
    report
        .errors()
        .iter()
        .chain(report.warnings())
        .filter(|d| {
            matches!(
                d.code,
                RuleCode::RecoveredStrayFence | RuleCode::UnclosedFrontmatter
            )
        })
        .map(|d| (d.code, d.severity, d.path.clone()))
        .collect()
}

#[test]
fn reported_stray_opener_resolves_claims_with_one_warning() {
    for claims in [
        "---\n# Claims\n\n## C01: First\n- **Statement**: a\n\n## C02 — Second\n- **Status**: open\n",
        "\u{feff}---\r\n# Claims\r\n\r\n## C01: First\r\n- **Statement**: a\r\n## C02 - Second\r\n",
    ] {
        let dir = artifact(&[
            ("trace/exploration_tree.yaml", TREE),
            ("logic/claims.md", claims),
        ]);
        let (manifest, report) = parse_dir(dir.path()).expect("claims resolve");
        assert_eq!(manifest.claims.len(), 2);
        assert_eq!(manifest.bindings.len(), 2);
        assert_eq!(
            fence_diagnostics(&report),
            [(
                RuleCode::RecoveredStrayFence,
                Severity::Warning,
                "logic/claims.md:1".to_string()
            )]
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("logic/claims.md")).unwrap(),
            claims,
            "source bytes stay unchanged"
        );
    }
}

#[test]
fn protected_metadata_keeps_original_errors_and_one_warning() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        (
            "logic/claims.md",
            "\n---\ntitle: x\n# Claims\n## C01: First\n- **Statement**: a\n",
        ),
        ("PAPER.md", "---\ntitle: Paper\n# Paper\n## Hidden\n"),
        ("logic/problem.md", "---\ntitle: Problem\n---\n# Problem\n"),
    ]);
    let report = report_of(dir.path());
    assert_eq!(
        fence_diagnostics(&report),
        [(
            RuleCode::UnclosedFrontmatter,
            Severity::Warning,
            "logic/claims.md:2".to_string()
        ),]
    );
    // The knowledge registry already reports PAPER.md's unclosed fence once.
    let paper: Vec<_> = report
        .warnings()
        .iter()
        .filter(|d| d.path.starts_with("PAPER.md"))
        .map(|d| d.code)
        .collect();
    assert_eq!(paper, [RuleCode::MalformedAgentLayer]);
    // The hidden claims stay hidden, so the references keep their error severity.
    assert_eq!(
        report
            .errors()
            .iter()
            .filter(|d| d.code == RuleCode::UnknownEvidenceClaim)
            .count(),
        2
    );
}

#[test]
fn closed_frontmatter_claims_are_unchanged() {
    let dir = artifact(&[
        ("trace/exploration_tree.yaml", TREE),
        (
            "logic/claims.md",
            "---\ntype: claims\n---\n# Claims\n## C01: First\n## C02: Second\n",
        ),
    ]);
    let (manifest, report) = parse_dir(dir.path()).expect("closed frontmatter parses");
    assert_eq!(manifest.claims.len(), 2);
    assert!(fence_diagnostics(&report).is_empty());
}

#[test]
fn vendored_em_dash_claims_resolve_every_evidence_reference() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/corpus/speedrun/nanogpt-speedrun");
    let (manifest, report) = parse_dir(&dir).expect("em-dash claims resolve");
    assert_eq!(manifest.claims.len(), 10);
    assert!(
        !report
            .errors()
            .iter()
            .any(|d| d.code == RuleCode::UnknownEvidenceClaim)
    );
    assert_eq!(
        manifest.claims[0].title,
        "16× Training Speedup Through Incremental Optimization"
    );
}
