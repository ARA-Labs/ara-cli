//! Normalized wire types — the single manifest every downstream consumer reads.
//!
//! This is the logical graph produced by [`crate::parse`]. It is the *only*
//! public data model: no `serde-saphyr` types leak here (that stays confined to
//! [`crate::schema`] / [`crate::claims`]), which keeps a future YAML-backend
//! swap cheap. Layout/geometry is **not** part of Stage 1 — it lands in Stage 2.
//!
//! Ordering is significant and always mirrors the source: `nodes` are in
//! pre-order DFS of the tree, `links`/`bindings` follow per-node source order.
//! Nothing is ever sorted by id.

use serde::{Deserialize, Serialize};

use crate::layout::{Point, Rect};

/// A node identifier (`^N\d+$`, case-sensitive, trimmed).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(String);

/// A claim identifier (`^C\d+$`, case-sensitive, trimmed).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClaimId(String);

impl NodeId {
    /// Wraps an already-normalized id. Callers pass a trimmed string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True when the id matches the canonical grammar `^N\d+$`.
    pub fn is_canonical(&self) -> bool {
        is_canonical_id(&self.0, 'N')
    }
}

impl ClaimId {
    /// Wraps an already-normalized id. Callers pass a trimmed string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The underlying string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True when the id matches the canonical grammar `^C\d+$`.
    pub fn is_canonical(&self) -> bool {
        is_canonical_id(&self.0, 'C')
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::fmt::Display for ClaimId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Checks `^<prefix>\d+$`: the given prefix followed by one or more ASCII
/// digits, nothing else. Regex-free to keep the dependency surface small.
pub(crate) fn is_canonical_id(s: &str, prefix: char) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c == prefix => {}
        _ => return false,
    }
    let rest = chars.as_str();
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
}

/// The normalized artifact: the logical exploration graph plus claim content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Pre-order DFS of the tree, source order preserved.
    pub nodes: Vec<Node>,
    /// Node → node edges (`children` and `also_depends_on`).
    pub links: Vec<Link>,
    /// Node → claim references, resolved against `claims`.
    pub bindings: Vec<Binding>,
    /// Claim content, for the viewer.
    pub claims: Vec<Claim>,
    /// Bounding rectangle enclosing all laid-out nodes. Populated by layout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Rect>,
}

/// One exploration node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Unique node id.
    pub id: NodeId,
    /// Node type.
    pub kind: NodeKind,
    /// Display label, from `title:` only. Consumers fall back to `id`.
    pub label: Option<String>,
    /// `explicit` | `inferred` when present.
    pub support_level: Option<String>,
    /// Free-form provenance refs (`§1`, `Fig. 1`, ...).
    pub source_refs: Vec<String>,
    /// Prose description.
    pub description: Option<String>,
    /// Typed per-kind body.
    pub fields: NodeFields,
    /// Free-text evidence entries (the non-`C##` part of `evidence:`).
    pub evidence_notes: Vec<String>,
    /// Whether this node is the root of an *isolated* subtree — a branch the
    /// exploration reached but that hangs off the main tree on its own. Drives
    /// the viewer's "isolated subtree" partition. Defaults to `false`; only the
    /// root of a subtree carries it (children inherit placement from their root).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub isolated: bool,
    /// Center position assigned by layout. Absent when layout has not run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<Point>,
}

/// The five canonical node types, plus a preserved escape hatch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Question,
    Experiment,
    Decision,
    DeadEnd,
    Insight,
    /// An unrecognized `type:`; the raw string is preserved.
    Other(String),
}

/// Typed body fields, one variant per canonical kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeFields {
    Question,
    Experiment {
        result: Option<String>,
    },
    Decision {
        choice: Option<String>,
        alternatives: Vec<String>,
        rationale: Option<String>,
    },
    DeadEnd {
        why_failed: Option<String>,
    },
    Insight,
    /// Unknown kind: body fields are captured (as warnings) at the raw layer.
    Other,
}

/// A directed node → node edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: LinkKind,
}

/// Kind of a node → node edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// Nesting edge, from `children:`.
    Child,
    /// Cross-reference edge, from `also_depends_on:`.
    DependsOn,
}

/// A resolved node → claim reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub node: NodeId,
    pub claim: ClaimId,
    pub role: BindingRole,
}

/// Role of a node → claim reference.
///
/// `non_exhaustive`: `Verifies` (SOULFuzz-only) is intentionally out of scope
/// and may be added later without breaking consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BindingRole {
    /// From a node's `evidence:` list.
    Evidence,
}

/// Claim content, parsed from `logic/claims.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    pub id: ClaimId,
    pub title: String,
    pub statement: Option<String>,
    pub status: Option<String>,
    /// `E##` proof refs, stored raw. Not validated — no evidence registry yet.
    pub proof: Vec<String>,
    /// Claim → claim dependencies.
    pub deps: Vec<ClaimId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_id_grammar() {
        assert!(is_canonical_id("N01", 'N'));
        assert!(is_canonical_id("N7", 'N'));
        assert!(is_canonical_id("C123", 'C'));
        assert!(!is_canonical_id("N", 'N')); // no digits
        assert!(!is_canonical_id("n01", 'N')); // case-sensitive
        assert!(!is_canonical_id("C01", 'N')); // wrong prefix
        assert!(!is_canonical_id("N01a", 'N')); // trailing junk
        assert!(!is_canonical_id("", 'N'));
    }

    #[test]
    fn id_accessors_and_display() {
        let n = NodeId::new("N01");
        assert_eq!(n.as_str(), "N01");
        assert_eq!(n.to_string(), "N01");
        assert!(n.is_canonical());
        assert!(!NodeId::new("nope").is_canonical());
        assert!(ClaimId::new("C02").is_canonical());
    }
}
