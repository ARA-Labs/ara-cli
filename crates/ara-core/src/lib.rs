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
pub mod layout;
pub mod manifest;
mod paper;
mod parse;
pub mod report;
mod schema;
mod sections;

pub use layout::{LayoutOptions, LayoutResult, NodePosition, Point, Rect};
pub use manifest::{
    Binding, BindingRole, BuiltOn, Claim, ClaimId, Concept, Exhibit, ExhibitKind, Link, LinkKind,
    Manifest, Node, NodeExhibit, NodeFields, NodeId, NodeKind, PaperMeta, Problem, Recipe,
    RelatedWork,
};
pub use report::{Diagnostic, ParseReport, Severity};

#[cfg(feature = "native")]
pub use parse::parse_dir;
pub use parse::parse_sources;

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
