//! Viewer asset delivery — embedded by default, `--assets <dir>` to override.
//!
//! The embedded path serves the `trunk`-built viewer baked into the binary via
//! `include_dir!`, so `cargo install ara-cli` → `ara serve` works with zero
//! config. `--assets <dir>` swaps in an on-disk `dist/` (dev + the Stage-5
//! Docker `--assets /assets` model), served by `tower-http`'s `ServeDir` with
//! precompressed brotli/gzip.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use include_dir::{Dir, include_dir};

/// The viewer `dist/` baked into the binary at build time.
///
/// Built by `trunk build --release` and copied to `assets/viewer/`. Refreshed
/// as part of the release process (the committed bytes are the accepted cost of
/// the embed-by-default UX — see `plans/stage-4-serve-live-reload.md`).
static VIEWER: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/assets/viewer");

/// How viewer assets are delivered for this server run.
#[derive(Clone)]
pub enum Assets {
    /// Serve the `include_dir!`-embedded viewer (default).
    Embedded,
    /// Serve an on-disk `dist/` directory (`--assets <dir>`).
    Dir(PathBuf),
}

/// True when at least one file is embedded — lets startup warn if a release was
/// built without first running `trunk build`.
pub fn embedded_is_populated() -> bool {
    VIEWER.entries().iter().next().is_some()
}

/// Serve an embedded asset by request path, falling back to `index.html` for
/// unknown routes (CSR single-page app behaviour).
pub async fn embedded_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match VIEWER.get_file(path) {
        Some(file) => file_response(path, file.contents()),
        // Unknown path → hand back index.html so client-side routing works.
        None => match VIEWER.get_file("index.html") {
            Some(index) => file_response("index.html", index.contents()),
            None => (
                StatusCode::NOT_FOUND,
                "viewer assets not embedded; rebuild with `trunk build` or pass --assets",
            )
                .into_response(),
        },
    }
}

/// Build a response for an embedded file: correct MIME + cache policy.
///
/// Hashed bundles (Trunk fingerprints js/css/wasm) are safe to cache forever;
/// `index.html` and `manifest.json` must always be revalidated.
fn file_response(path: &str, bytes: &'static [u8]) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache_control = if path == "index.html" || path == "manifest.json" {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    };

    (
        [
            (header::CONTENT_TYPE, mime.as_ref()),
            (header::CACHE_CONTROL, cache_control),
        ],
        Body::from(bytes),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_viewer_is_populated() {
        // The committed dist must contain the entry point; a release built
        // without `trunk build` would ship an empty viewer.
        assert!(embedded_is_populated());
        assert!(VIEWER.get_file("index.html").is_some());
    }

    #[test]
    fn hashed_assets_are_immutable_index_is_not() {
        // index.html revalidates.
        let resp = file_response("index.html", b"<html>");
        let cc = resp.headers().get(header::CACHE_CONTROL).unwrap();
        assert_eq!(cc, "no-cache");

        // A fingerprinted bundle is immutable.
        let resp = file_response("ara-viewer-abc123_bg.wasm", b"\0asm");
        let cc = resp.headers().get(header::CACHE_CONTROL).unwrap();
        assert!(cc.to_str().unwrap().contains("immutable"));
        let ct = resp.headers().get(header::CONTENT_TYPE).unwrap();
        assert_eq!(ct, "application/wasm");
    }
}
