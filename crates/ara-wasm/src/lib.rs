//! `ara-wasm`: WebAssembly bindings for the ARA viewer browser client.
//!
//! Exposes hand-written `wasm-bindgen` interop over [`ara-core`] for the Leptos
//! client. This is a skeleton reservation release; bindings land in a later
//! version. See <https://github.com/ARA-Labs/ara-cli>.

/// Returns the version of `ara-wasm`, taken from the crate manifest.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
