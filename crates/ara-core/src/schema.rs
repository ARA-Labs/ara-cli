//! Raw serde mirror of `trace/exploration_tree.yaml`.
//!
//! Every canonical field is modeled explicitly so that the official examples
//! deserialize with an **empty** `extra` map (→ zero unknown-field warnings).
//! Only genuinely unknown keys land in `extra`; `parse.rs` turns those into
//! warnings. `serde-saphyr` is confined to this module (and `claims.rs`): none
//! of these types appear in the public [`crate::manifest`] API, which keeps a
//! future YAML-backend swap cheap.
//!
//! Values in `extra` use [`serde::de::IgnoredAny`]: `serde-saphyr` has no
//! generic value type, and only the unknown-key *names* are needed for
//! warnings. This means unknown-field *values* are not retained — acceptable
//! because Stage 1 has no consumer for them and the field name is reported.

use serde::Deserialize;
use serde::de::IgnoredAny;
use std::collections::BTreeMap;

/// Top-level document: exactly one of `tree:` / `root:` is expected. Both /
/// neither is validated (as an error) in `parse.rs`.
#[derive(Debug, Deserialize)]
pub(crate) struct RawDoc {
    #[serde(default)]
    pub tree: Option<Vec<RawNode>>,
    #[serde(default)]
    pub root: Option<Box<RawNode>>,
    /// Unknown top-level keys → warnings.
    #[serde(flatten)]
    pub extra: BTreeMap<String, IgnoredAny>,
}

/// Raw node with every canonical field modeled. Type-specific body fields are
/// all optional so a single struct covers all five kinds; `parse.rs` projects
/// them into the typed [`crate::manifest::NodeFields`] per `type:`.
#[derive(Debug, Deserialize)]
pub(crate) struct RawNode {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "type", default)]
    pub ty: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub support_level: Option<String>,
    #[serde(default)]
    pub source_refs: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Marks the root of an isolated subtree. Defaults to `false`.
    #[serde(default)]
    pub isolated: bool,
    // experiment
    #[serde(default)]
    pub result: Option<String>,
    // dead_end
    #[serde(default)]
    pub why_failed: Option<String>,
    // decision
    #[serde(default)]
    pub choice: Option<String>,
    #[serde(default)]
    pub alternatives: Vec<String>,
    #[serde(default)]
    pub rationale: Option<String>,
    // node → claim references (mixed refs + prose)
    #[serde(default)]
    pub evidence: Option<Evidence>,
    // node → node cross edges
    #[serde(default)]
    pub also_depends_on: Vec<String>,
    // nesting edges
    #[serde(default)]
    pub children: Vec<RawNode>,
    /// Unknown node keys → warnings.
    #[serde(flatten)]
    pub extra: BTreeMap<String, IgnoredAny>,
}

/// `evidence:` is either a bare scalar (`"Table 3 ..."`) or a mixed list
/// (`[C01, "Table 2"]`). Each list element is a string; `C##` vs prose is
/// classified in `parse.rs`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum Evidence {
    One(String),
    Many(Vec<String>),
}

impl Evidence {
    /// Flattens to a list of raw string entries in source order.
    pub(crate) fn entries(&self) -> Vec<String> {
        match self {
            Evidence::One(s) => vec![s.clone()],
            Evidence::Many(v) => v.clone(),
        }
    }
}

/// Deserializes one exploration-tree document.
///
/// Returns the raw serde error message (a `String`, not a `serde-saphyr` type)
/// so callers stay decoupled from the YAML backend. Multi-document input, a
/// non-mapping root, and shape mismatches all surface here as `Err` — never a
/// panic.
pub(crate) fn parse_doc(yaml: &str) -> Result<RawDoc, String> {
    serde_saphyr::from_str::<RawDoc>(yaml).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tree_with_canonical_fields_no_extra() {
        let y = "\
tree:
  - id: N01
    type: question
    title: Q?
    children:
      - id: N02
        type: experiment
        result: 28.4 BLEU
        evidence: [C01, \"Table 2\"]
      - id: N03
        type: decision
        choice: sinusoidal
        alternatives:
          - learned
        evidence: \"Table 3 prose\"
";
        let doc = parse_doc(y).expect("parses");
        assert!(doc.extra.is_empty());
        let tree = doc.tree.expect("tree present");
        assert_eq!(tree.len(), 1);
        let n1 = &tree[0];
        assert_eq!(n1.id.as_deref(), Some("N01"));
        assert_eq!(n1.ty.as_deref(), Some("question"));
        assert!(n1.extra.is_empty());
        assert_eq!(n1.children.len(), 2);

        let n2 = &n1.children[0];
        assert_eq!(
            n2.evidence.as_ref().unwrap().entries(),
            vec!["C01", "Table 2"]
        );
        let n3 = &n1.children[1];
        assert_eq!(
            n3.evidence.as_ref().unwrap().entries(),
            vec!["Table 3 prose"]
        );
        assert_eq!(n3.alternatives, vec!["learned"]);
    }

    #[test]
    fn unknown_keys_land_in_extra() {
        let y = "tree:\n  - id: N01\n    type: question\n    bogus: 42\ntop_bogus: 1\n";
        let doc = parse_doc(y).expect("parses");
        assert_eq!(doc.extra.keys().collect::<Vec<_>>(), vec!["top_bogus"]);
        assert_eq!(
            doc.tree.unwrap()[0].extra.keys().collect::<Vec<_>>(),
            vec!["bogus"]
        );
    }

    #[test]
    fn root_single_dialect_parses() {
        let doc = parse_doc("root:\n  id: N01\n  type: question\n").expect("parses");
        assert!(doc.tree.is_none());
        assert_eq!(doc.root.unwrap().id.as_deref(), Some("N01"));
    }

    #[test]
    fn null_children_default_to_empty() {
        let doc = parse_doc("tree:\n  - id: N01\n    children:\n").expect("parses");
        assert!(doc.tree.unwrap()[0].children.is_empty());
    }

    #[test]
    fn multi_document_is_error_not_panic() {
        assert!(parse_doc("tree: []\n---\ntree: []\n").is_err());
    }

    #[test]
    fn non_sequence_tree_is_error_not_panic() {
        assert!(parse_doc("tree: not-a-list\n").is_err());
    }
}
