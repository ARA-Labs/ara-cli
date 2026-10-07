//! The rule-code registry: one stable `ARA###` code for every finding `ara check`
//! can report.
//!
//! Codes are an **API surface** — once published they are never renumbered or
//! reused (a retired rule keeps its number). They key suppression and per-rule
//! config (issue #40), CI annotations, and the docs table in
//! `docs/stage-5-check.md`.
//!
//! The code space is partitioned by layer:
//!
//! - `ARA0xx` — format/canonicalization drift detected by the format-lint layer
//!   ([`crate::lint`]); every one is auto-fixable.
//! - `ARA1xx` — structural/reference **errors** from the validate layer.
//! - `ARA2xx` — field/schema **warnings** from the validate layer.
//!
//! Every validate-layer [`crate::Diagnostic`] is stamped with its [`RuleCode`]
//! at its construction site, and every format-lint diagnostic maps to one via
//! [`crate::LintRuleId::code`]. `ara validate` never renders the code (its output
//! is byte-stable); only `ara check` does.

use serde::Serialize;

use crate::report::Severity;

/// Which `ara check` layer emits a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleLayer {
    /// Format-lint layer ([`crate::check_dir`]): canonicalizable drift.
    Format,
    /// Validate layer ([`crate::parse_dir`]): the `ara validate` diagnostics.
    Validate,
}

/// Generates [`RuleCode`] plus its registry table from one list, so the code,
/// name, layer, severity, fixability, and summary of a rule can never drift apart.
macro_rules! rules {
    ($(
        $(#[$doc:meta])*
        $variant:ident => ($code:literal, $name:literal, $layer:ident, $severity:ident, $fixable:literal, $summary:literal),
    )+) => {
        /// A stable rule code. Serializes to its `ARA###` string.
        ///
        /// Iterate [`RuleCode::ALL`] to enumerate every rule; parse a code string
        /// with [`str::parse`] / [`RuleCode::from_code`].
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        pub enum RuleCode {
            $(
                $(#[$doc])*
                #[serde(rename = $code)]
                $variant,
            )+
        }

        impl RuleCode {
            /// Every rule, in ascending code order.
            pub const ALL: &'static [RuleCode] = &[$(RuleCode::$variant),+];

            /// The stable `ARA###` code string.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(RuleCode::$variant => $code,)+
                }
            }

            /// Short kebab-case rule name (e.g. `unknown-evidence-claim`).
            pub const fn name(self) -> &'static str {
                match self {
                    $(RuleCode::$variant => $name,)+
                }
            }

            /// Which layer emits this rule.
            pub const fn layer(self) -> RuleLayer {
                match self {
                    $(RuleCode::$variant => RuleLayer::$layer,)+
                }
            }

            /// The severity the rule is reported at by default. Format-lint rules
            /// are `Error`: an unfixed one fails `ara check`.
            pub const fn default_severity(self) -> Severity {
                match self {
                    $(RuleCode::$variant => Severity::$severity,)+
                }
            }

            /// Whether `ara check --fix` can rewrite the source to resolve it.
            pub const fn fixable(self) -> bool {
                match self {
                    $(RuleCode::$variant => $fixable,)+
                }
            }

            /// One-line description of what the rule detects.
            pub const fn summary(self) -> &'static str {
                match self {
                    $(RuleCode::$variant => $summary,)+
                }
            }
        }
    };
}

rules! {
    // ---- ARA0xx: format/canonicalization (format-lint layer, all fixable) ----
    /// Top-level `root:` single-node dialect (canonical is `tree:`).
    RootDialect => ("ARA001", "root-dialect", Format, Error, true,
        "top-level `root:` instead of a `tree:` list"),
    /// `reason:` on a `dead_end` node (canonical `why_failed:`).
    DeadEndReasonAlias => ("ARA002", "dead-end-reason-alias", Format, Error, true,
        "`reason:` on a `dead_end` node (canonical `why_failed:`)"),
    /// `justification:` on a `decision` node (canonical `rationale:`).
    DecisionRationaleAlias => ("ARA003", "decision-rationale-alias", Format, Error, true,
        "`justification:` on a `decision` node (canonical `rationale:`)"),
    /// Claim header with a dash separator the parser does not accept.
    ClaimHeaderStyle => ("ARA004", "claim-header-style", Format, Error, true,
        "claim header uses an unspaced dash separator the parser drops (canonical `## <id>: <title>`)"),
    /// `from:` on a `pivot` node (canonical `prior_direction:`).
    PivotFromAlias => ("ARA005", "pivot-from-alias", Format, Error, true,
        "`from:` on a `pivot` node (canonical `prior_direction:`)"),
    /// `to:` on a `pivot` node (canonical `new_direction:`).
    PivotToAlias => ("ARA006", "pivot-to-alias", Format, Error, true,
        "`to:` on a `pivot` node (canonical `new_direction:`)"),
    /// `trigger:` on a `pivot` node (canonical `reason:`).
    PivotTriggerAlias => ("ARA007", "pivot-trigger-alias", Format, Error, true,
        "`trigger:` on a `pivot` node (canonical `reason:`)"),

    // ---- ARA1xx: structural/reference errors (validate layer) ----
    /// `trace/exploration_tree.yaml` is not valid YAML or not the expected shape.
    MalformedTree => ("ARA100", "malformed-tree", Validate, Error, false,
        "`trace/exploration_tree.yaml` fails to parse (invalid YAML, multi-document, non-mapping root, wrong field types)"),
    /// `trace/exploration_tree.yaml` could not be read.
    UnreadableTree => ("ARA101", "unreadable-tree", Validate, Error, false,
        "`trace/exploration_tree.yaml` cannot be read"),
    /// Both `tree:` and `root:` are present.
    TreeAndRoot => ("ARA102", "tree-and-root", Validate, Error, false,
        "both `tree:` and `root:` are present; exactly one is allowed"),
    /// Neither `tree:` nor `root:` is present.
    MissingTree => ("ARA103", "missing-tree", Validate, Error, false,
        "neither `tree:` nor `root:` is present"),
    /// A node has no `id` (the node and its subtree are dropped).
    MissingNodeId => ("ARA104", "missing-node-id", Validate, Error, false,
        "node is missing an `id` (node and subtree dropped)"),
    /// Two nodes share an id (the later one and its subtree are dropped).
    DuplicateNodeId => ("ARA105", "duplicate-node-id", Validate, Error, false,
        "two nodes share an id (the later node and its subtree are dropped)"),
    /// Two claims in `logic/claims.md` share an id.
    DuplicateClaimId => ("ARA106", "duplicate-claim-id", Validate, Error, false,
        "two claims in `logic/claims.md` share an id"),
    /// A node's `evidence:` names a claim absent from `logic/claims.md`.
    UnknownEvidenceClaim => ("ARA107", "unknown-evidence-claim", Validate, Error, false,
        "node `evidence:` references a claim not in `logic/claims.md`"),
    /// A node's `also_depends_on:` names a node that does not exist.
    UnknownDependencyNode => ("ARA108", "unknown-dependency-node", Validate, Error, false,
        "`also_depends_on:` references a node that does not exist"),
    /// A claim's `Dependencies:` names a claim that does not exist.
    UnknownClaimDependency => ("ARA109", "unknown-claim-dependency", Validate, Error, false,
        "claim `Dependencies:` references a claim that does not exist"),
    /// The `children:` + `also_depends_on:` graph has a cycle.
    DependencyCycle => ("ARA110", "dependency-cycle", Validate, Error, false,
        "`children:` + `also_depends_on:` edges form a cycle"),
    /// An explicit resumed-branch parent is absent from the artifact.
    UnknownParentNode => ("ARA111", "unknown-parent-node", Validate, Error, false,
        "`parent` references a node that does not exist"),
    /// A nested node contradicts its explicit resumed-branch parent.
    ConflictingParent => ("ARA112", "conflicting-parent", Validate, Error, false,
        "nested and explicit `parent` identities disagree"),
    /// An audited claim identity history is corrupt or cannot reach a live claim.
    InvalidClaimRedirect => ("ARA113", "invalid-claim-redirect", Validate, Error, false,
        "claim identity history is invalid, unaudited, cyclic, or dangling"),

    // ---- ARA2xx: field/schema warnings (validate layer) ----
    /// Unrecognized top-level key in the tree document.
    UnknownDocumentField => ("ARA200", "unknown-document-field", Validate, Warning, false,
        "unrecognized top-level key in `trace/exploration_tree.yaml` (value ignored)"),
    /// Unrecognized key on a node.
    UnknownNodeField => ("ARA201", "unknown-node-field", Validate, Warning, false,
        "unrecognized key on a node (value dropped)"),
    /// `tree: []` — the manifest has no nodes.
    EmptyTree => ("ARA202", "empty-tree", Validate, Warning, false,
        "`tree: []` yields an empty manifest"),
    /// A node has no `type:`.
    MissingNodeType => ("ARA203", "missing-node-type", Validate, Warning, false,
        "node is missing a `type`"),
    /// A typed body field on a node with no `type:` is dropped.
    FieldDroppedMissingType => ("ARA204", "field-dropped-missing-type", Validate, Warning, false,
        "body field dropped because the node has no `type`"),
    /// A typed body field on a node with an unrecognized `type:` is dropped.
    FieldDroppedUnknownType => ("ARA205", "field-dropped-unknown-type", Validate, Warning, false,
        "body field dropped because the node's `type` is not recognized"),
    /// A body field that the node's (known) type does not carry is dropped.
    FieldDroppedForType => ("ARA206", "field-dropped-for-type", Validate, Warning, false,
        "body field not modeled for the node's `type` is dropped"),
    /// A `C##` evidence reference cannot be resolved because there is no
    /// `logic/claims.md`.
    UnresolvedClaimReference => ("ARA207", "unresolved-claim-reference", Validate, Warning, false,
        "claim reference unresolved because `logic/claims.md` is absent"),
    /// `also_depends_on:` on an ancestor restates the nesting and is dropped.
    RedundantAncestorDependency => ("ARA208", "redundant-ancestor-dependency", Validate, Warning, false,
        "`also_depends_on:` on an ancestor is redundant with `children:` nesting (edge dropped)"),
    /// The same edge is declared twice.
    DuplicateLink => ("ARA209", "duplicate-link", Validate, Warning, false,
        "the same edge is declared more than once (duplicate dropped)"),
    /// `PAPER.md` frontmatter is not valid YAML.
    MalformedPaperFrontmatter => ("ARA210", "malformed-paper-frontmatter", Validate, Warning, false,
        "`PAPER.md` frontmatter fails to parse (paper metadata dropped)"),
    /// A `logic/concepts.md` entry has no definition.
    ConceptMissingDefinition => ("ARA211", "concept-missing-definition", Validate, Warning, false,
        "a `logic/concepts.md` entry has no definition"),
    /// A `logic/related_work.md` entry has no DOI.
    RelatedWorkMissingDoi => ("ARA212", "related-work-missing-doi", Validate, Warning, false,
        "a `logic/related_work.md` entry has no DOI"),
    /// The same exhibit basename appears under two `evidence/` categories.
    DuplicateExhibitBasename => ("ARA213", "duplicate-exhibit-basename", Validate, Warning, false,
        "the same exhibit basename appears under two `evidence/` categories"),
    /// An `evidence/` body file has no row in `evidence/README.md`.
    ExhibitMissingIndexRow => ("ARA214", "exhibit-missing-index-row", Validate, Warning, false,
        "an `evidence/` body file has no row in `evidence/README.md`"),
    /// An `evidence/README.md` row points at a body file that does not exist.
    IndexRowMissingExhibit => ("ARA215", "index-row-missing-exhibit", Validate, Warning, false,
        "an `evidence/README.md` row references a body file that does not exist"),
    /// A figure image declaration is missing, unsafe, invalid, or ambiguous.
    InvalidFigureImage => ("ARA216", "invalid-figure-image", Validate, Warning, false,
        "a figure image declaration is missing, unsafe, invalid, or ambiguous"),
    MalformedAgentLayer => ("ARA217", "malformed-agent-layer", Validate, Warning, false,
        "an optional agent-layer document or entry has malformed content"),
    DuplicateAgentId => ("ARA218", "duplicate-agent-id", Validate, Warning, false,
        "an optional layer contains duplicate ids (all entries retained)"),
    MalformedPromotionDestination => ("ARA219", "malformed-promotion-destination", Validate, Warning, false,
        "a staged observation has a malformed promotion destination (retained)"),
    DanglingSessionIndex => ("ARA220", "dangling-session-index", Validate, Warning, false,
        "a session index row references an absent session record"),
    UnreadableOptionalLayer => ("ARA221", "unreadable-optional-layer", Validate, Warning, false,
        "an optional artifact file or directory cannot be read"),
    MalformedSameAs => ("ARA222", "malformed-same-as", Validate, Warning, false,
        "same_as must be a sequence of canonical node ids"),
    DanglingSameAs => ("ARA223", "dangling-same-as", Validate, Warning, false,
        "same_as references an absent node"),
    SelfSameAs => ("ARA224", "self-same-as", Validate, Warning, false,
        "same_as references its own node"),
    SameAsCycle => ("ARA225", "same-as-cycle", Validate, Warning, false,
        "directional same_as annotations form a cycle"),
    MalformedNodeAnnotation => ("ARA226", "malformed-node-annotation", Validate, Warning, false,
        "node artifacts or concepts have malformed content"),
    UnknownNodeConcept => ("ARA227", "unknown-node-concept", Validate, Warning, false,
        "a node concept link references an absent concept term"),
    /// `logic/claims.md` opens with a recognized stray `---` line; its claims are read.
    RecoveredStrayFence => ("ARA228", "recovered-stray-fence", Validate, Warning, false,
        "an unclosed leading `---` in `logic/claims.md` is a stray line before `# Claims` (claims after it are read)"),
    /// A Markdown document's unclosed leading `---` hides the rest of it.
    UnclosedFrontmatter => ("ARA229", "unclosed-frontmatter", Validate, Warning, false,
        "an unclosed leading `---` fence hides the rest of a Markdown document as frontmatter"),
}

impl RuleCode {
    /// Looks up a rule by its `ARA###` code string (exact, case-sensitive).
    pub fn from_code(code: &str) -> Option<RuleCode> {
        RuleCode::ALL.iter().copied().find(|r| r.as_str() == code)
    }
}

impl std::fmt::Display for RuleCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error from parsing an unknown rule code string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRuleCode(pub String);

impl std::fmt::Display for UnknownRuleCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown rule code `{}`", self.0)
    }
}

impl std::error::Error for UnknownRuleCode {}

impl std::str::FromStr for RuleCode {
    type Err = UnknownRuleCode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RuleCode::from_code(s).ok_or_else(|| UnknownRuleCode(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn codes_and_names_are_unique_and_sorted() {
        let codes: Vec<&str> = RuleCode::ALL.iter().map(|r| r.as_str()).collect();
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        assert_eq!(codes, sorted, "ALL must be in ascending code order");
        assert_eq!(codes.iter().collect::<BTreeSet<_>>().len(), codes.len());
        let names: BTreeSet<&str> = RuleCode::ALL.iter().map(|r| r.name()).collect();
        assert_eq!(names.len(), RuleCode::ALL.len());
    }

    #[test]
    fn code_space_matches_layer_and_severity() {
        for &r in RuleCode::ALL {
            let code = r.as_str();
            assert!(code.len() == 6 && code.starts_with("ARA"), "{code}");
            match &code[3..4] {
                "0" => {
                    assert_eq!(r.layer(), RuleLayer::Format, "{code}");
                    assert!(r.fixable(), "{code}");
                }
                "1" => {
                    assert_eq!(r.layer(), RuleLayer::Validate, "{code}");
                    assert_eq!(r.default_severity(), Severity::Error, "{code}");
                    assert!(!r.fixable(), "{code}");
                }
                "2" => {
                    assert_eq!(r.layer(), RuleLayer::Validate, "{code}");
                    assert_eq!(r.default_severity(), Severity::Warning, "{code}");
                    assert!(!r.fixable(), "{code}");
                }
                other => panic!("unexpected code block {other} in {code}"),
            }
        }
    }

    #[test]
    fn round_trips_through_string_and_serde() {
        for &r in RuleCode::ALL {
            assert_eq!(r.as_str().parse::<RuleCode>(), Ok(r));
            assert_eq!(r.to_string(), r.as_str());
            assert_eq!(
                serde_json::to_string(&r).unwrap(),
                format!("\"{}\"", r.as_str())
            );
        }
        assert!("ARA999".parse::<RuleCode>().is_err());
        assert!("ara107".parse::<RuleCode>().is_err());
    }
}
