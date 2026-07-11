//! Pure filter predicate for the toolbar — no web-sys, fully native-testable.
//!
//! [`FilterState`] carries the three toolbar controls (text query, kind filter,
//! dead-ends-only toggle).  [`node_matches`] evaluates all three conditions ANDed
//! together against a [`Node`] + [`Manifest`].  An all-default [`FilterState`]
//! matches every node.

use ara_core::{Manifest, Node, NodeKind};

use crate::kind::kind_meta;

// ── Filter state ──────────────────────────────────────────────────────────────

/// View-state for the toolbar filter controls.
///
/// All fields are public because the toolbar component writes them directly via
/// signal updates.  The struct is cheap to clone and is kept in an
/// `RwSignal<FilterState>` in `App` so it survives manifest swaps.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FilterState {
    /// Free-text search — case-insensitive substring.  Empty = no text filter.
    pub query: String,
    /// CSS-class wire tag to match (e.g. `"decision"`).  `None` = all kinds.
    pub kind: Option<String>,
    /// When `true`, only `DeadEnd` nodes pass.
    pub dead_ends_only: bool,
}

// ── Predicate ─────────────────────────────────────────────────────────────────

/// Return `true` when `node` passes **all** active filters in `filter`.
///
/// The three conditions are AND-ed:
///
/// 1. **Text query** — if `filter.query` (trimmed) is non-empty, the node
///    must contain it (case-insensitive) in any of:
///    - `node.label` (if `Some`)
///    - `node.id.as_str()`
///    - `kind_meta(&node.kind).badge` (e.g. `"dead end"`, raw string for `Other`)
///    - `kind_meta(&node.kind).css_class` (e.g. `"dead_end"`)
///    - the `title` of any bound claim (resolved via `manifest.bindings` →
///      `manifest.claims`)
///    - the `statement` of any bound claim (if `Some`)
///
/// 2. **Kind filter** — if `filter.kind` is `Some(k)`, the node's
///    `kind_meta(...).css_class` must equal `k`.
///
/// 3. **Dead-ends-only** — if `filter.dead_ends_only` is `true`, only nodes
///    whose `kind` is `NodeKind::DeadEnd` pass.
///
/// An all-default [`FilterState`] (empty query, `kind: None`,
/// `dead_ends_only: false`) matches every node.
pub fn node_matches(node: &Node, manifest: &Manifest, filter: &FilterState) -> bool {
    text_matches(node, manifest, &filter.query)
        && kind_matches(node, &filter.kind)
        && dead_end_matches(node, filter.dead_ends_only)
}

// ── Sub-predicates ────────────────────────────────────────────────────────────

fn text_matches(node: &Node, manifest: &Manifest, query: &str) -> bool {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return true;
    }

    // Candidate strings from the node itself.
    let meta = kind_meta(&node.kind);

    let node_label = node.label.as_deref().unwrap_or("").to_ascii_lowercase();
    let node_id = node.id.as_str().to_ascii_lowercase();
    let badge = meta.badge.to_ascii_lowercase();
    let css = meta.css_class.to_ascii_lowercase();

    if node_label.contains(&q) || node_id.contains(&q) || badge.contains(&q) || css.contains(&q) {
        return true;
    }

    // Candidate strings from bound claims.
    manifest
        .bindings
        .iter()
        .filter(|b| b.node == node.id)
        .any(|b| {
            manifest
                .claims
                .iter()
                .find(|c| c.id == b.claim)
                .is_some_and(|claim| {
                    let title_lc = claim.title.to_ascii_lowercase();
                    let stmt_lc = claim
                        .statement
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    title_lc.contains(&q) || stmt_lc.contains(&q)
                })
        })
}

fn kind_matches(node: &Node, kind_filter: &Option<String>) -> bool {
    match kind_filter {
        None => true,
        Some(k) => kind_meta(&node.kind).css_class == k.as_str(),
    }
}

fn dead_end_matches(node: &Node, dead_ends_only: bool) -> bool {
    if dead_ends_only {
        matches!(node.kind, NodeKind::DeadEnd)
    } else {
        true
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ara_core::{
        Binding, BindingRole, Claim, ClaimId, Manifest, Node, NodeFields, NodeId, NodeKind,
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn bare_manifest() -> Manifest {
        Manifest {
            nodes: vec![],
            links: vec![],
            bindings: vec![],
            claims: vec![],
            bounds: None,
        }
    }

    fn make_node(id: &str, kind: NodeKind, label: Option<&str>) -> Node {
        Node {
            id: NodeId::new(id),
            kind,
            label: label.map(|s| s.to_string()),
            support_level: None,
            source_refs: vec![],
            description: None,
            fields: NodeFields::Question,
            evidence_notes: vec![],
            isolated: false,
            pos: None,
        }
    }

    fn make_dead_end(id: &str, label: Option<&str>) -> Node {
        Node {
            fields: NodeFields::DeadEnd { why_failed: None },
            ..make_node(id, NodeKind::DeadEnd, label)
        }
    }

    // ── Default FilterState matches every node ────────────────────────────────

    #[test]
    fn default_filter_matches_question() {
        let node = make_node("N01", NodeKind::Question, Some("What is attention?"));
        assert!(node_matches(
            &node,
            &bare_manifest(),
            &FilterState::default()
        ));
    }

    #[test]
    fn default_filter_matches_dead_end() {
        let node = make_dead_end("N02", Some("Gradient Collapse"));
        assert!(node_matches(
            &node,
            &bare_manifest(),
            &FilterState::default()
        ));
    }

    #[test]
    fn default_filter_matches_other_kind() {
        let node = make_node("N03", NodeKind::Other("custom".into()), None);
        assert!(node_matches(
            &node,
            &bare_manifest(),
            &FilterState::default()
        ));
    }

    // ── Text query: label substring ───────────────────────────────────────────

    #[test]
    fn text_matches_label_substring_case_insensitive() {
        let node = make_node("N01", NodeKind::Question, Some("What is Attention?"));
        let filter = FilterState {
            query: "attention".into(),
            ..Default::default()
        };
        assert!(node_matches(&node, &bare_manifest(), &filter));
    }

    #[test]
    fn text_matches_label_mixed_case() {
        let node = make_node("N01", NodeKind::Question, Some("SOFTMAX Stability"));
        let filter = FilterState {
            query: "softmax".into(),
            ..Default::default()
        };
        assert!(node_matches(&node, &bare_manifest(), &filter));
    }

    #[test]
    fn text_non_matching_query_excludes_node() {
        let node = make_node("N01", NodeKind::Question, Some("Attention mechanism"));
        let filter = FilterState {
            query: "xyzzy".into(),
            ..Default::default()
        };
        assert!(!node_matches(&node, &bare_manifest(), &filter));
    }

    // ── Text query: id match ──────────────────────────────────────────────────

    #[test]
    fn text_matches_node_id() {
        let node = make_node("N42", NodeKind::Experiment, None);
        let filter = FilterState {
            query: "n42".into(), // case-insensitive
            ..Default::default()
        };
        assert!(node_matches(&node, &bare_manifest(), &filter));
    }

    #[test]
    fn text_matches_node_id_exact() {
        let node = make_node("N07", NodeKind::Decision, Some("Use sinusoidal"));
        let filter = FilterState {
            query: "N07".into(),
            ..Default::default()
        };
        assert!(node_matches(&node, &bare_manifest(), &filter));
    }

    // ── Text query: kind badge and css_class ──────────────────────────────────

    #[test]
    fn text_matches_kind_badge() {
        // DeadEnd badge = "dead end"
        let node = make_dead_end("N05", None);
        let filter = FilterState {
            query: "dead end".into(),
            ..Default::default()
        };
        assert!(node_matches(&node, &bare_manifest(), &filter));
    }

    #[test]
    fn text_matches_kind_css_class() {
        // DeadEnd css_class = "dead_end"
        let node = make_dead_end("N05", None);
        let filter = FilterState {
            query: "dead_end".into(),
            ..Default::default()
        };
        assert!(node_matches(&node, &bare_manifest(), &filter));
    }

    #[test]
    fn text_matches_other_kind_badge_is_raw_string() {
        // Other("weird") → badge = "weird", css_class = "other"
        let node = make_node("N06", NodeKind::Other("weird_custom".into()), None);
        let filter = FilterState {
            query: "weird_custom".into(),
            ..Default::default()
        };
        assert!(node_matches(&node, &bare_manifest(), &filter));
    }

    // ── Text query: bound claim title / statement ─────────────────────────────

    fn manifest_with_claim(
        node_id: &str,
        claim_id: &str,
        title: &str,
        statement: Option<&str>,
    ) -> Manifest {
        let mut m = bare_manifest();
        m.bindings.push(Binding {
            node: NodeId::new(node_id),
            claim: ClaimId::new(claim_id),
            role: BindingRole::Evidence,
        });
        m.claims.push(Claim {
            id: ClaimId::new(claim_id),
            title: title.to_string(),
            statement: statement.map(|s| s.to_string()),
            status: None,
            proof: vec![],
            deps: vec![],
        });
        m
    }

    #[test]
    fn text_matches_bound_claim_title() {
        let node = make_node("N01", NodeKind::Experiment, None);
        let manifest = manifest_with_claim("N01", "C01", "ResNet convergence proof", None);
        let filter = FilterState {
            query: "resnet".into(),
            ..Default::default()
        };
        assert!(node_matches(&node, &manifest, &filter));
    }

    #[test]
    fn text_matches_bound_claim_statement() {
        let node = make_node("N01", NodeKind::Experiment, None);
        let manifest = manifest_with_claim(
            "N01",
            "C01",
            "Convergence",
            Some("The model converges in 50 epochs."),
        );
        let filter = FilterState {
            query: "50 epochs".into(),
            ..Default::default()
        };
        assert!(node_matches(&node, &manifest, &filter));
    }

    #[test]
    fn text_does_not_match_claim_bound_to_different_node() {
        let node = make_node("N01", NodeKind::Experiment, None);
        // Binding is for N02, not N01.
        let manifest = manifest_with_claim("N02", "C01", "Unrelated claim", None);
        let filter = FilterState {
            query: "unrelated".into(),
            ..Default::default()
        };
        assert!(!node_matches(&node, &manifest, &filter));
    }

    #[test]
    fn text_non_matching_query_not_in_claim_either() {
        let node = make_node("N01", NodeKind::Experiment, Some("Experiment A"));
        let manifest = manifest_with_claim("N01", "C01", "Some Claim", Some("Some statement"));
        let filter = FilterState {
            query: "xyzzy_no_match".into(),
            ..Default::default()
        };
        assert!(!node_matches(&node, &manifest, &filter));
    }

    // ── Kind filter ───────────────────────────────────────────────────────────

    #[test]
    fn kind_filter_matches_exact_css_class() {
        let node = make_node("N01", NodeKind::Decision, Some("Choose arch"));
        let filter = FilterState {
            kind: Some("decision".into()),
            ..Default::default()
        };
        assert!(node_matches(&node, &bare_manifest(), &filter));
    }

    #[test]
    fn kind_filter_excludes_wrong_kind() {
        let node = make_node("N01", NodeKind::Question, Some("What?"));
        let filter = FilterState {
            kind: Some("decision".into()),
            ..Default::default()
        };
        assert!(!node_matches(&node, &bare_manifest(), &filter));
    }

    #[test]
    fn kind_filter_none_matches_all() {
        for kind in [
            NodeKind::Question,
            NodeKind::Experiment,
            NodeKind::Decision,
            NodeKind::DeadEnd,
            NodeKind::Insight,
            NodeKind::Other("custom".into()),
        ] {
            let node = make_node("N01", kind, None);
            let filter = FilterState {
                kind: None,
                ..Default::default()
            };
            assert!(
                node_matches(&node, &bare_manifest(), &filter),
                "kind=None should match every kind"
            );
        }
    }

    /// `Other("weird")` has css_class = "other", so it matches `Some("other")`
    /// but NOT `Some("weird")`.
    #[test]
    fn other_kind_matches_css_other_not_raw_string() {
        let node = make_node("N01", NodeKind::Other("weird".into()), None);

        let filter_other = FilterState {
            kind: Some("other".into()),
            ..Default::default()
        };
        assert!(
            node_matches(&node, &bare_manifest(), &filter_other),
            "Other(\"weird\") must match kind filter \"other\""
        );

        let filter_weird = FilterState {
            kind: Some("weird".into()),
            ..Default::default()
        };
        assert!(
            !node_matches(&node, &bare_manifest(), &filter_weird),
            "Other(\"weird\") must NOT match kind filter \"weird\" (css_class is fixed \"other\")"
        );
    }

    // ── Dead-ends-only ────────────────────────────────────────────────────────

    #[test]
    fn dead_ends_only_passes_dead_end() {
        let node = make_dead_end("N01", Some("Gradient collapse"));
        let filter = FilterState {
            dead_ends_only: true,
            ..Default::default()
        };
        assert!(node_matches(&node, &bare_manifest(), &filter));
    }

    #[test]
    fn dead_ends_only_excludes_non_dead_end() {
        let node = make_node("N01", NodeKind::Experiment, Some("Experiment A"));
        let filter = FilterState {
            dead_ends_only: true,
            ..Default::default()
        };
        assert!(!node_matches(&node, &bare_manifest(), &filter));
    }

    #[test]
    fn dead_ends_only_false_allows_all_kinds() {
        let node = make_node("N01", NodeKind::Insight, Some("Insight A"));
        let filter = FilterState {
            dead_ends_only: false,
            ..Default::default()
        };
        assert!(node_matches(&node, &bare_manifest(), &filter));
    }

    // ── ANDing of conditions ──────────────────────────────────────────────────

    /// A DeadEnd node whose label does NOT contain the query should be excluded
    /// when both dead_ends_only and a non-matching text query are active.
    #[test]
    fn dead_ends_only_and_text_both_must_pass() {
        let node = make_dead_end("N01", Some("Gradient vanished"));
        let filter = FilterState {
            query: "xyzzy".into(), // doesn't match label, id, badge, or css
            dead_ends_only: true,
            ..Default::default()
        };
        assert!(!node_matches(&node, &bare_manifest(), &filter));
    }

    /// A DeadEnd whose label contains the query passes both.
    #[test]
    fn dead_ends_only_and_matching_text_both_pass() {
        let node = make_dead_end("N01", Some("Gradient vanished"));
        let filter = FilterState {
            query: "gradient".into(),
            dead_ends_only: true,
            ..Default::default()
        };
        assert!(node_matches(&node, &bare_manifest(), &filter));
    }

    /// Kind filter + text query: only a Decision whose label matches passes.
    #[test]
    fn kind_filter_and_text_both_must_pass() {
        let passing = make_node("N01", NodeKind::Decision, Some("Use sinusoidal encoding"));
        let wrong_kind = make_node("N02", NodeKind::Experiment, Some("Use sinusoidal encoding"));
        let wrong_label = make_node("N03", NodeKind::Decision, Some("Something else entirely"));

        let filter = FilterState {
            query: "sinusoidal".into(),
            kind: Some("decision".into()),
            ..Default::default()
        };
        assert!(node_matches(&passing, &bare_manifest(), &filter));
        assert!(!node_matches(&wrong_kind, &bare_manifest(), &filter));
        assert!(!node_matches(&wrong_label, &bare_manifest(), &filter));
    }

    /// All three conditions active — only a DeadEnd with matching label and
    /// css_class "dead_end" passes.
    #[test]
    fn all_three_conditions_anded() {
        let passes = make_dead_end("N01", Some("Gradient collapse"));
        let not_dead_end = make_node("N02", NodeKind::Experiment, Some("Gradient collapse"));
        let wrong_label = make_dead_end("N03", Some("Something else"));

        let filter = FilterState {
            query: "gradient".into(),
            kind: Some("dead_end".into()),
            dead_ends_only: true,
        };
        assert!(node_matches(&passes, &bare_manifest(), &filter));
        assert!(!node_matches(&not_dead_end, &bare_manifest(), &filter));
        assert!(!node_matches(&wrong_label, &bare_manifest(), &filter));
    }
}
