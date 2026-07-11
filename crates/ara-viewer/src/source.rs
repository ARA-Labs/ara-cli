//! Manifest source seam — where the viewer gets its `Manifest` from.
//!
//! Only [`ManifestSource::Static`] is implemented in Stage 3.  The `Api`
//! variant (live-reload from a dev server endpoint) is deferred to Stage 4;
//! it is documented here so the seam is visible and the enum can be extended
//! without breaking callsites.

use crate::state::LoadState;
#[cfg(target_arch = "wasm32")]
use crate::state::parse_manifest;

/// Where the viewer fetches its [`ara_core::Manifest`].
// The inner `String` of `Static` is consumed by the wasm fetch path.  On
// native (cargo test) that branch is cfg'd out so rustc reports the field as
// dead; the allow suppresses the spurious warning.
#[allow(dead_code)]
pub enum ManifestSource {
    /// Fetch a checked-in JSON file by URL.
    ///
    /// Under `trunk serve` / any static host the relative URL `"manifest.json"`
    /// resolves to the file copied to `dist/` by the Trunk `copy-file`
    /// directive in `index.html`.
    Static(String),
    // /// Live-reload from a dev-server endpoint.  Implemented in Stage 4.
    // Api { endpoint: String, live: bool },
}

impl Default for ManifestSource {
    /// The default source is the checked-in manifest under `dist/manifest.json`.
    ///
    /// The relative URL `"manifest.json"` is correct for both `trunk serve`
    /// (served from `dist/`) and any static host that copies `dist/` verbatim.
    fn default() -> Self {
        Self::Static("manifest.json".into())
    }
}

/// Initiate an asynchronous manifest fetch and write the result into `set_state`.
///
/// Compiled **only for `wasm32-unknown-unknown`**; all network I/O is
/// browser-native.  On native the function is replaced by a no-op stub so
/// `cargo test` continues to compile.
#[cfg(target_arch = "wasm32")]
pub fn fetch_manifest(source: ManifestSource, set_state: impl Fn(LoadState) + 'static) {
    use wasm_bindgen_futures::spawn_local;

    let url = match source {
        ManifestSource::Static(url) => url,
    };

    spawn_local(async move {
        let response = match gloo_net::http::Request::get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                set_state(LoadState::Failed(format!("Network error: {e}")));
                return;
            }
        };

        if !response.ok() {
            set_state(LoadState::Failed(format!(
                "{} {}",
                response.status(),
                response.status_text()
            )));
            return;
        }

        let text = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                set_state(LoadState::Failed(format!(
                    "Failed to read response body: {e}"
                )));
                return;
            }
        };

        match parse_manifest(&text) {
            Ok(manifest) => set_state(LoadState::Loaded(manifest)),
            Err(reason) => set_state(LoadState::Failed(format!("Parse error: {reason}"))),
        }
    });
}

/// Native stub — the viewer never runs natively; the stub keeps `cargo test`
/// compiling without pulling in any wasm-only dependencies.
#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_manifest(_source: ManifestSource, _set_state: impl Fn(LoadState) + 'static) {
    // No-op on native: network fetch is browser-only.
}
