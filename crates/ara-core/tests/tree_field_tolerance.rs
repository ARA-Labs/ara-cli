//! #130: one mistyped value in a node text field no longer fails the tree.
//! Numeric evidence keeps its spelling; a mapping or list where text is
//! expected reads as its exact source text with `ARA230`; `ARA008 --fix`
//! quotes it. Identity fields stay strict.
#![cfg(feature = "native")]

use ara_core::{
    LintRuleId, Manifest, ParseReport, RuleCode, check_sources, fix_dir, parse_sources,
};
use std::fmt::Write;

fn decision(fields: &str) -> String {
    format!("tree:\n  - id: N01\n    type: decision\n    title: Q\n{fields}")
}
fn parsed(source: &str) -> (Manifest, ParseReport) {
    parse_sources(source, None).unwrap_or_else(|report| panic!("{report}"))
}
fn recovered(report: &ParseReport) -> Vec<(String, String)> {
    report
        .warnings()
        .iter()
        .filter(|d| d.code == RuleCode::NonTextTreeField)
        .map(|d| (d.path.clone(), d.message.clone()))
        .collect()
}
fn decision_fields(manifest: &Manifest) -> (Option<String>, Vec<String>, Option<String>) {
    match &manifest.nodes[0].fields {
        ara_core::NodeFields::Decision {
            choice,
            alternatives,
            rationale,
            ..
        } => (choice.clone(), alternatives.clone(), rationale.clone()),
        other => panic!("not a decision: {other:?}"),
    }
}

#[test]
fn evidence_list_scalars_keep_their_spelling_without_a_warning() {
    let (manifest, report) = parsed(&decision(
        "    evidence: [be13e96, 7258910, 0123456, 1e5, true]\n",
    ));
    assert_eq!(
        manifest.nodes[0].evidence_notes,
        ["be13e96", "7258910", "0123456", "1e5", "true"]
    );
    assert!(recovered(&report).is_empty(), "{report}");
}

#[test]
fn bare_numeric_evidence_keeps_its_spelling_with_a_warning() {
    for value in ["0123456", "7258910", "1e5"] {
        let (manifest, report) = parsed(&decision(&format!("    evidence: {value}\n")));
        assert_eq!(manifest.nodes[0].evidence_notes, [value]);
        assert_eq!(recovered(&report).len(), 1, "{value}");
    }
    let (manifest, report) = parsed(&decision("    evidence: ~\n"));
    assert!(manifest.nodes[0].evidence_notes.is_empty());
    assert!(recovered(&report).is_empty());
}

#[test]
fn mappings_and_lists_in_text_fields_read_as_exact_source_text() {
    let (manifest, report) = parsed(&decision(
        "    choice: [a, {b: c}]\n    alternatives:\n    - Build on Debian 12 (rejected: its glibc is above the floor).  # note\n    - Static linking\n",
    ));
    let (choice, alternatives, _) = decision_fields(&manifest);
    assert_eq!(choice.as_deref(), Some("[a, {b: c}]"));
    assert_eq!(
        alternatives,
        [
            "Build on Debian 12 (rejected: its glibc is above the floor).",
            "Static linking"
        ]
    );
    let paths: Vec<String> = recovered(&report).into_iter().map(|(p, _)| p).collect();
    assert_eq!(
        paths,
        [
            "trace/exploration_tree.yaml:5",
            "trace/exploration_tree.yaml:7"
        ]
    );
}

#[test]
fn multi_line_mapping_reads_as_its_source_lines() {
    let (manifest, report) = parsed(&decision(
        "    rationale:\n      first: a\n      second: b\n",
    ));
    let (_, _, rationale) = decision_fields(&manifest);
    assert_eq!(rationale.as_deref(), Some("first: a\n      second: b"));
    assert_eq!(recovered(&report).len(), 1);
}

#[test]
fn identity_fields_stay_strict() {
    for fields in [
        "    parent: {a: b}\n",
        "    also_depends_on: [{a: b}]\n",
        "    isolated: {a: b}\n",
    ] {
        let refused = parse_sources(&decision(fields), None).unwrap_err();
        assert!(
            refused
                .errors()
                .iter()
                .any(|d| d.code == RuleCode::MalformedTree),
            "{fields}: {refused}"
        );
    }
    let refused = parse_sources("tree:\n  - id: {a: b}\n    type: question\n", None).unwrap_err();
    assert!(
        refused
            .errors()
            .iter()
            .any(|d| d.code == RuleCode::MalformedTree)
    );
}

#[test]
fn deep_trees_recover_on_the_arena_path() {
    let mut source = String::from("tree: [\n");
    for id in (1..=70).rev() {
        writeln!(source, "{{id: N{id}, type: question, children: [").unwrap();
    }
    source.push_str("{id: N999, type: decision, title: plain title, rationale: 12, choice: ok, alternatives: [a (rejected: b), plain], evidence: 0123}");
    source.push_str(&"]}\n".repeat(70));
    source.push_str("]\n");
    let (manifest, report) = parsed(&source);
    let leaf = manifest
        .nodes
        .iter()
        .find(|n| n.id.as_str() == "N999")
        .unwrap();
    assert_eq!(leaf.evidence_notes, ["0123"]);
    assert_eq!(leaf.label.as_deref(), Some("plain title"));
    // Ordinary plain text is not reported; only the two non-text values are.
    assert_eq!(recovered(&report).len(), 2, "{report}");
}

#[test]
fn ara008_quotes_one_line_values_and_reports_multi_line_ones() {
    let source = decision(
        "    choice: [a, {b: c}]\n    alternatives:\n    - Debian (rejected: it's old).\n    evidence: 0123\n    rationale:\n      k: v\n      j: w\n",
    );
    let lint: Vec<_> = check_sources(&source, None)
        .diagnostics()
        .iter()
        .filter(|d| d.rule == LintRuleId::UnquotedTreeScalar)
        .cloned()
        .collect();
    assert_eq!(lint.len(), 4, "{lint:?}");
    assert_eq!(lint.iter().filter(|d| d.fixable).count(), 3);

    let dir = tempfile::TempDir::new().unwrap();
    let tree = dir.path().join("trace/exploration_tree.yaml");
    std::fs::create_dir_all(tree.parent().unwrap()).unwrap();
    std::fs::write(&tree, &source).unwrap();
    let (before, _) = parsed(&source);
    let outcome = fix_dir(dir.path());
    assert_eq!(outcome.applied.len(), 3, "{outcome:?}");
    let fixed = std::fs::read_to_string(&tree).unwrap();
    assert!(fixed.contains("    choice: '[a, {b: c}]'\n"), "{fixed}");
    assert!(
        fixed.contains("    - 'Debian (rejected: it''s old).'\n"),
        "{fixed}"
    );
    assert!(fixed.contains("    evidence: '0123'\n"), "{fixed}");
    let (after, report) = parsed(&fixed);
    assert_eq!(before, after);
    // Only the multi-line value is left to quote by hand.
    assert_eq!(recovered(&report).len(), 1);
}
