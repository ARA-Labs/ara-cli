//! `ara-core`: the shared core of the ARA viewer runtime.
//!
//! This crate holds all parsing, normalization, binding resolution, and DAG
//! layout for the ARA viewer. It is compiled to both native targets (used by
//! `ara-cli`) and `wasm32-unknown-unknown` (used by the browser client), so it
//! is the single source of truth that keeps the server and client from
//! drifting.
//!
//! See <https://github.com/EYH0602/bara>.

pub mod manifest;
mod schema;

pub use manifest::{
    Binding, BindingRole, Claim, ClaimId, Link, LinkKind, Manifest, Node, NodeFields, NodeId,
    NodeKind,
};

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
