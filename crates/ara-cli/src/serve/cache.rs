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
    ///
    /// `None` on the hub path ([`CachedAra::from_dir_lean`]): the hub serves
    /// only `manifest_json` + `figures_dir` and never reads the parsed graph,
    /// so holding it would ~2× resident memory (parsed graph + serialized
    /// bytes) per ARA for data it never touches. The local `ara serve` path
    /// keeps it via [`CachedAra::from_dir`].
    #[allow(dead_code)]
    pub manifest: Option<Arc<Manifest>>,
    /// The manifest serialised to JSON exactly once; cloned per request.
    pub manifest_json: Arc<Bytes>,
    /// Strong validator for `ETag` / `If-None-Match` (quoted, e.g. `"a1b2"`).
    pub etag: String,
    /// Directory figures are served from (`<dir>/evidence`), sandboxed.
    pub figures_dir: PathBuf,
}

impl CachedAra {
    /// Parse + lay out `dir`, then serialise and hash the manifest, keeping the
    /// parsed graph (local `ara serve`).
    ///
    /// Returns the parse report on failure (cycles, missing tree, …) so the
    /// caller can decide whether to abort (startup) or keep the old cache
    /// (reparse). Layout warnings do not fail the build.
    pub fn from_dir(dir: &Path) -> Result<Self, ParseReport> {
        Self::build(dir, true)
    }

    /// Like [`CachedAra::from_dir`] but drops the parsed `Manifest` after
    /// serialising it — the hub never reads it, so this halves resident memory
    /// per ARA (see the `manifest` field docs).
    pub fn from_dir_lean(dir: &Path) -> Result<Self, ParseReport> {
        Self::build(dir, false)
    }

    /// Shared constructor: parse + lay out + serialise + hash. `keep_manifest`
    /// decides whether the parsed graph is retained (local) or dropped (hub).
    fn build(dir: &Path, keep_manifest: bool) -> Result<Self, ParseReport> {
        let (manifest, _report) = parse_and_layout_dir(dir, &LayoutOptions::default())?;

        // Serialisation cannot fail for our own types; treat an error as an
        // empty body rather than panicking.
        let json = serde_json::to_vec(&manifest).unwrap_or_default();
        let etag = etag_for(&json);

        // Drop the parsed graph immediately on the hub path so it never becomes
        // resident alongside the serialized bytes.
        let manifest = keep_manifest.then(|| Arc::new(manifest));

        Ok(Self {
            manifest,
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
        let manifest = cache.manifest.as_ref().expect("from_dir keeps the manifest");
        assert!(!manifest.nodes.is_empty());
        assert!(!cache.manifest_json.is_empty());
        assert!(cache.etag.starts_with('"') && cache.etag.ends_with('"'));
        assert!(cache.figures_dir.ends_with("evidence"));
    }

    #[test]
    fn lean_cache_drops_manifest_but_keeps_json() {
        let cache = CachedAra::from_dir_lean(&fixture("resnet-ara-example"))
            .expect("resnet fixture must parse");
        // The hub path drops the parsed graph to save memory …
        assert!(cache.manifest.is_none(), "lean build must drop the manifest");
        // … but the serialized body + etag + figures dir are identical to a
        // full build, so serving is unaffected.
        assert!(!cache.manifest_json.is_empty());
        assert!(cache.etag.starts_with('"') && cache.etag.ends_with('"'));
        assert!(cache.figures_dir.ends_with("evidence"));

        let full = CachedAra::from_dir(&fixture("resnet-ara-example")).unwrap();
        assert_eq!(cache.etag, full.etag, "lean + full etags must match");
        assert_eq!(cache.manifest_json, full.manifest_json);
    }

    #[test]
    fn etag_is_stable_and_content_addressed() {
        assert_eq!(etag_for(b"hello"), etag_for(b"hello"));
        assert_ne!(etag_for(b"hello"), etag_for(b"hello!"));
    }
}
