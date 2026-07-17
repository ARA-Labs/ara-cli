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
    /// Paper-level metadata from `PAPER.md` frontmatter. Absent when the file is
    /// missing or has no frontmatter fence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paper: Option<PaperMeta>,
    /// Typed prior-work dependencies from `logic/related_work.md`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_work: Vec<RelatedWork>,
    /// Glossary terms from `logic/concepts.md`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concepts: Vec<Concept>,
    /// Problem framing from `logic/problem.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<Problem>,
    /// Solution recipes, one per `logic/solution/*.md` file (source order).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipes: Vec<Recipe>,
    /// Figures/tables from `evidence/`. Populated by a later evidence task.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exhibits: Vec<Exhibit>,
    /// Node → related-work edges. Populated by a later resolution task.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub built_on: Vec<BuiltOn>,
    /// Node → exhibit edges. Populated by a later resolution task.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_exhibits: Vec<NodeExhibit>,
}

/// Paper-level metadata, parsed from `PAPER.md` YAML frontmatter.
///
/// Every field is optional: a `PAPER.md` with no frontmatter fence still yields
/// a `PaperMeta` carrying only the `title` (from the first `# H1`). `year` is
/// normalized to a `String` even when the source encodes it as an integer.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PaperMeta {
    /// Paper title.
    pub title: Option<String>,
    /// Author names, in source order.
    pub authors: Vec<String>,
    /// Publication year, normalized from int or string.
    pub year: Option<String>,
    /// Venue string.
    pub venue: Option<String>,
    /// DOI or arXiv id. `None` when the source is `null` or absent.
    pub doi: Option<String>,
    /// Abstract text (`abstract` in the source).
    #[serde(rename = "abstract")]
    pub abstract_: Option<String>,
    /// Keyword list, in source order.
    pub keywords: Vec<String>,
}

/// One typed prior-work dependency, parsed from `logic/related_work.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelatedWork {
    /// Reference id (`RW01`).
    pub id: String,
    /// Citation text — the header content after the id.
    pub cite: String,
    /// DOI or arXiv id, when present.
    pub doi: Option<String>,
    /// Relationship kind (`Type:` value), raw — may combine (e.g.
    /// `baseline, extends`).
    pub kind: Option<String>,
    /// `Delta → What changed`.
    pub what_changed: Option<String>,
    /// `Delta → Why`.
    pub why: Option<String>,
    /// `Adopted elements`.
    pub adopted: Option<String>,
    /// Claims this reference affects, from the inline `C##` list. A prose
    /// `none` resolves to an empty list.
    pub claims_affected: Vec<ClaimId>,
}

/// One glossary term, parsed from `logic/concepts.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Concept {
    /// The term (the `## <Term>` header text).
    pub term: String,
    /// Notation, LaTeX preserved verbatim.
    pub notation: Option<String>,
    /// Definition prose.
    pub definition: Option<String>,
    /// Boundary conditions (`Boundary` or `Boundary conditions`).
    pub boundary: Option<String>,
    /// Related term names, split from a comma-separated list.
    pub related: Vec<String>,
}

/// Problem framing, parsed from `logic/problem.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    /// Intro prose before the first `##` section.
    pub statement: Option<String>,
    /// `O#` observation items, full text including the id, in source order.
    pub observations: Vec<String>,
    /// `G#` gap items, full text including the id, in source order.
    pub gaps: Vec<String>,
    /// Key-insight / `I#` items, in source order.
    pub insights: Vec<String>,
}

/// One solution recipe, one per `logic/solution/*.md` file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    /// Filename stem (e.g. `algorithm`).
    pub name: String,
    /// First `# Title` in the file, when present.
    pub title: Option<String>,
    /// Raw markdown body, verbatim.
    pub body: String,
}

/// The kind of an exhibit. `Other` preserves anything not a figure or table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExhibitKind {
    Figure,
    Table,
    Other,
}

/// One figure or table, parsed from `evidence/`. Populated by a later task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exhibit {
    /// Exhibit id.
    pub id: String,
    /// Source file, relative to the artifact root.
    pub file: String,
    /// Figure / table / other.
    pub kind: ExhibitKind,
    /// Origin of the exhibit, when stated.
    pub source: Option<String>,
    /// Caption / description prose.
    pub description: Option<String>,
    /// Claims this exhibit supports.
    pub claims: Vec<ClaimId>,
    /// Raw markdown body, verbatim.
    pub body: String,
}

/// A node → related-work edge. Populated by a later resolution task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuiltOn {
    pub node: NodeId,
    pub related_work: String,
}

/// A node → exhibit edge. Populated by a later resolution task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeExhibit {
    pub node: NodeId,
    pub exhibit: String,
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

/// The canonical node types, plus a preserved escape hatch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Question,
    Experiment,
    Decision,
    DeadEnd,
    Insight,
    Pivot,
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
        hypothesis: Option<String>,
        failure_mode: Option<String>,
        lesson: Option<String>,
        why_failed: Option<String>,
    },
    Insight,
    Pivot {
        from: Option<String>,
        to: Option<String>,
        trigger: Option<String>,
    },
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
