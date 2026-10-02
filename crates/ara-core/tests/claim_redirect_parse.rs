use ara_core::{
    ClaimId, SourceValue, parse_sources, parse_sources_with_claim_redirects, source_node_fields,
};
use std::collections::BTreeMap;

const TREE: &str =
    "tree: [{id: N01, type: question, description: 'C01 is historical prose', evidence: [C01]}]";
const CLAIMS: &str = "## C05: Current finding\n- **Statement**: C01 names the original trial.\n- **Proof**: E01\n\n## C07: Consequence\n- **Dependencies**: [C01]\n";

#[test]
fn explicit_claim_redirects_normalize_only_known_pointers() {
    let redirects = BTreeMap::from([("C01".into(), "C03".into()), ("C03".into(), "C05".into())]);
    let (manifest, report) =
        parse_sources_with_claim_redirects(TREE, Some(CLAIMS), &redirects).unwrap();
    assert!(report.is_ok());
    assert_eq!(manifest.bindings[0].claim, ClaimId::new("C05"));
    assert_eq!(manifest.claims[1].deps, [ClaimId::new("C05")]);
    assert_eq!(
        manifest.nodes[0].description.as_deref(),
        Some("C01 is historical prose")
    );
    assert_eq!(
        manifest.claims[0].statement.as_deref(),
        Some("C01 names the original trial.")
    );
    assert_eq!(manifest.claims[0].proof, ["E01"]);
    assert_eq!(
        manifest.claims[0].body.as_deref(),
        Some(
            "## C05: Current finding\n- **Statement**: C01 names the original trial.\n- **Proof**: E01\n\n"
        )
    );
    assert_eq!(
        source_node_fields(TREE, &["N01"]).unwrap()["N01"]["evidence"],
        SourceValue::Sequence(vec![SourceValue::String("C01".into())])
    );
    let original = parse_sources(TREE, Some(CLAIMS)).unwrap_err();
    assert!(
        original
            .errors()
            .iter()
            .any(|error| error.code == ara_core::RuleCode::UnknownEvidenceClaim)
    );
}

#[test]
fn invalid_redirects_cannot_resurrect_or_mask_missing_claims() {
    for pairs in [
        vec![("C05", "C07")],
        vec![("C01", "N01")],
        vec![("C01", "logic/claims.md:C05")],
        vec![("C01", "C03"), ("C03", "C01")],
        vec![("C01", "C99")],
    ] {
        let redirects = pairs
            .into_iter()
            .map(|(from, to)| (from.to_owned(), to.to_owned()))
            .collect();
        let report =
            parse_sources_with_claim_redirects(TREE, Some(CLAIMS), &redirects).unwrap_err();
        assert!(
            report
                .errors()
                .iter()
                .any(|error| error.code == ara_core::RuleCode::InvalidClaimRedirect)
        );
    }
}

#[test]
fn long_redirect_chain_preserves_exact_native_identity() {
    let redirects = (1..=10_000)
        .map(|number| (format!("C{number}"), format!("C{}", number + 1)))
        .collect();
    let (manifest, _) = parse_sources_with_claim_redirects(
        "tree: [{id: N1, type: question, evidence: [C1]}]",
        Some("## C10001: Final finding\n"),
        &redirects,
    )
    .unwrap();
    assert_eq!(manifest.bindings[0].claim.as_str(), "C10001");
    assert_eq!(manifest.claims[0].id.as_str(), "C10001");
}
