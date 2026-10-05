//! `ara-core`: the shared core of the ARA viewer runtime.
//!
//! This crate holds all parsing, normalization, binding resolution, and DAG
//! layout for the ARA viewer. It is compiled to both native targets (used by
//! `ara-cli`) and `wasm32-unknown-unknown` (used by the browser client), so it
//! is the single source of truth that keeps the server and client from
//! drifting.
//!
//! See <https://github.com/ARA-Labs/ara-cli>.

mod claims;
pub mod figure;
mod flat_yaml;
pub mod layout;
pub mod lint;
pub mod manifest;
pub mod markdown;
mod parse;
pub mod query;
pub mod report;
pub mod rules;
mod schema;
pub(crate) mod stray_fence;
// The fix applier reads/writes source files and re-parses through `parse_dir`'s
// building blocks, so it is native-only like `check_dir`/`parse_dir`.
#[cfg(feature = "native")]
mod fix;
// The `PAPER.md` / `logic/*` / `evidence/` readers are consumed only by the
// native `parse_dir`; gating them keeps the wasm client build (which only
// deserializes the already-built manifest) free of dead-code warnings.
#[cfg(feature = "native")]
mod agent_layers;
#[cfg(feature = "native")]
mod evidence;
#[cfg(feature = "native")]
pub mod merge;
#[cfg(feature = "native")]
mod paper;
#[cfg(feature = "native")]
mod sections;
#[cfg(feature = "native")]
pub mod write;

pub use claims::{claim_heading, unparsed_claim_headings};
pub use layout::{LayoutOptions, LayoutResult, NodePosition, Point, Rect};
pub use manifest::{
    Binding, BindingRole, BuiltOn, Claim, ClaimId, Concept, Exhibit, ExhibitKind, ExperimentId,
    ExperimentPlan, Heuristic, HeuristicId, Link, LinkKind, Manifest, Node, NodeArtifact,
    NodeExhibit, NodeFields, NodeId, NodeKind, Observation, ObservationId, PaperMeta, Problem,
    Recipe, RelatedWork, Session, SessionId, SourceField, SourceValue, TasteComment, TasteId,
};
pub use report::{Diagnostic, ParseReport, Severity};
pub use rules::{RuleCode, RuleLayer, UnknownRuleCode};

pub use lint::{FixCandidate, LintDiagnostic, LintFile, LintReport, LintRuleId};
#[cfg(feature = "native")]
pub use lint::{check_dir, check_sources};

#[cfg(feature = "native")]
pub use fix::{AppliedFix, FixOutcome, SkippedFix, fix_dir, fix_dir_with};

#[cfg(feature = "native")]
pub use agent_layers::{
    FILE_ACCESS_ROOTS, file_access_location, file_access_path, knowledge_paths,
};
pub use flat_yaml::source_node_fields;
#[cfg(feature = "native")]
pub use parse::{LoadIssue, LoadIssueKind, NativeLoad, parse_dir, parse_dir_detailed};
pub use parse::{parse_sources, parse_sources_with_claim_redirects};

/// Parses and lays out an in-memory ARA artifact.
///
/// On parse success, runs layout and returns the positioned manifest. On parse
/// error (including cycles), returns the report unchanged and skips layout.
pub fn parse_and_layout(
    tree_yaml: &str,
    claims_md: Option<&str>,
    opts: &LayoutOptions,
) -> Result<(Manifest, ParseReport), ParseReport> {
    let (mut manifest, report) = parse_sources(tree_yaml, claims_md)?;
    let result = layout::layout(&manifest, opts);
    for np in result.positions {
        if let Some(node) = manifest.nodes.iter_mut().find(|n| n.id == np.id) {
            node.pos = Some(np.pos);
        }
    }
    manifest.bounds = Some(result.bounds);
    Ok((manifest, report))
}

/// Reads, parses, and lays out an ARA artifact directory. Native only.
#[cfg(feature = "native")]
pub fn parse_and_layout_dir(
    dir: &std::path::Path,
    opts: &LayoutOptions,
) -> Result<(Manifest, ParseReport), ParseReport> {
    let (mut manifest, report) = parse_dir(dir)?;
    let result = layout::layout(&manifest, opts);
    for np in result.positions {
        if let Some(node) = manifest.nodes.iter_mut().find(|n| n.id == np.id) {
            node.pos = Some(np.pos);
        }
    }
    manifest.bounds = Some(result.bounds);
    Ok((manifest, report))
}

/// Returns the version of `ara-core`, taken from the crate manifest.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_reported() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
