//! Parse-once cache for a single ARA directory.
//!
//! `ara serve` parses + lays out the artifact once, serialises the manifest to
//! JSON a single time, and hands out cheap `Arc<Bytes>` clones on every request.
//! A reparse (triggered by the file watcher) builds a fresh [`CachedAra`] and the
//! server atomically swaps it via `ArcSwap`.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ara_core::{LayoutOptions, Manifest, ParseReport, parse_and_layout_dir};
use bytes::Bytes;

/// An immutable snapshot of a parsed artifact, ready to serve.
pub struct CachedAra {
    /// The positioned manifest (kept for potential in-process consumers).
    #[allow(dead_code)]
    pub manifest: Arc<Manifest>,
    /// The manifest serialised to JSON exactly once; cloned per request.
    pub manifest_json: Arc<Bytes>,
    /// Strong validator for `ETag` / `If-None-Match` (quoted, e.g. `"a1b2"`).
    pub etag: String,
    /// Directory figures are served from (`<dir>/evidence`), sandboxed.
    pub figures_dir: PathBuf,
}

impl CachedAra {
    /// Parse + lay out `dir`, then serialise and hash the manifest.
    ///
    /// Returns the parse report on failure (cycles, missing tree, …) so the
    /// caller can decide whether to abort (startup) or keep the old cache
    /// (reparse). Layout warnings do not fail the build.
    pub fn from_dir(dir: &Path) -> Result<Self, ParseReport> {
        let (manifest, _report) = parse_and_layout_dir(dir, &LayoutOptions::default())?;

        // Serialisation cannot fail for our own types; treat an error as an
        // empty body rather than panicking.
        let json = serde_json::to_vec(&manifest).unwrap_or_default();
        let etag = etag_for(&json);

        Ok(Self {
            manifest: Arc::new(manifest),
            manifest_json: Arc::new(Bytes::from(json)),
            etag,
            figures_dir: dir.join("evidence"),
        })
    }
}

/// Compute a quoted `ETag` from the manifest JSON bytes.
///
/// A content hash is all an `ETag` needs: identical bytes ⇒ identical tag, and
/// any edit changes it. `DefaultHasher` is deterministic within a process run,
/// which is the only lifetime an `If-None-Match` validator must survive.
fn etag_for(bytes: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("\"{:016x}\"", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ara-core/tests/fixtures/official")
            .join(name)
    }

    #[test]
    fn builds_cache_from_resnet_fixture() {
        let cache =
            CachedAra::from_dir(&fixture("resnet-ara-example")).expect("resnet fixture must parse");
        assert!(!cache.manifest.nodes.is_empty());
        assert!(!cache.manifest_json.is_empty());
        assert!(cache.etag.starts_with('"') && cache.etag.ends_with('"'));
        assert!(cache.figures_dir.ends_with("evidence"));
    }

    #[test]
    fn etag_is_stable_and_content_addressed() {
        assert_eq!(etag_for(b"hello"), etag_for(b"hello"));
        assert_ne!(etag_for(b"hello"), etag_for(b"hello!"));
    }
}
