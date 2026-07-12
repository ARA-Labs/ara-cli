//! Read-only multi-ARA hub: parse-once-at-ingest, serve-many.
//!
//! Where local `ara serve` watches one directory and reparses on change, the
//! hub scans an `--ara-root` **once at startup**, parses each immediate child
//! directory into a [`CachedAra`], and holds them in an immutable
//! `Arc<HashMap>`. Reads are lock-free — no watcher, no `RwLock`, no reparse
//! after startup. A hot upload/ingest API (which would reintroduce `RwLock` /
//! `ArcSwap`) is explicitly out of scope; adding ARAs means a restart.

// Wired into the router + `run` in the hub-routing step; until then the ingest
// API has no caller, so the dead-code lint would fire. Removed once
// `build_hub_router` consumes it.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::cache::CachedAra;

/// The hub's immutable ARA table: id → cached artifact. Built once at startup,
/// never mutated, so reads need no lock.
pub type Aras = Arc<HashMap<String, Arc<CachedAra>>>;

/// Why a child directory was skipped during [`ingest`]. Carried in the summary
/// so startup can log a per-ARA reason rather than a bare count.
pub struct Skipped {
    /// The directory name (raw, may contain rejected characters).
    pub name: String,
    /// Human-readable reason (rejected id, parse failure, duplicate id).
    pub reason: String,
}

/// Result of scanning an `--ara-root`: the immutable table plus the skip list.
pub struct Ingest {
    pub aras: Aras,
    pub skipped: Vec<Skipped>,
}

/// True if `name` is a safe ARA id: `[A-Za-z0-9._-]+`.
///
/// This single guard covers both path-segment safety (no `/`, no `..`, no
/// spaces) and URL/HTML-encoding concerns: a constrained id is safe to
/// interpolate into `<base href="/a/{id}/">` and into a route match without any
/// percent- or HTML-encoding. `.` is allowed but `..` is rejected as a whole
/// (it would still match the charset, so it is checked explicitly below).
fn is_valid_id(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Scan `root`'s immediate subdirectories, parsing each into a [`CachedAra`].
///
/// Returns `Err` only when `root` itself cannot be read (missing / not a
/// directory / unreadable) — that is a fatal misconfiguration the caller turns
/// into a non-zero exit. Individual child failures (rejected id, parse error,
/// duplicate id) are **logged and skipped**, never fatal: one bad artifact must
/// not sink the whole hub.
///
/// Parsing uses [`CachedAra::from_dir_lean`] so the hub does not retain the
/// parsed graph it never reads. Ingest is serial (one-time, off the request
/// path); parallel ingest is deferred until corpus size warrants it.
pub fn ingest(root: &Path) -> std::io::Result<Ingest> {
    let mut aras: HashMap<String, Arc<CachedAra>> = HashMap::new();
    let mut skipped: Vec<Skipped> = Vec::new();

    // read_dir on `root` itself errors (missing / not a dir / permission) →
    // fatal. A per-entry error (transient IO on one child) is recorded as a
    // skip rather than dropped, honouring the "logged and skipped" contract.
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(root)? {
        match entry {
            Ok(e) => entries.push(e.path()),
            Err(e) => skipped.push(Skipped {
                name: "<unreadable entry>".into(),
                reason: format!("directory entry error: {e}"),
            }),
        }
    }
    // Deterministic order so ingest logs + duplicate-resolution are stable
    // regardless of filesystem iteration order.
    entries.sort();

    for path in entries {
        // `is_dir` follows symlinks by design: bind-mounting an individual ARA
        // into the root via a symlink is a supported deployment shape. Stray
        // files (non-dirs) at the root are not ARAs; ignore them silently.
        if !path.is_dir() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => {
                // Non-UTF-8 dir name → not a valid id.
                let lossy = path.file_name().unwrap_or_default().to_string_lossy();
                skipped.push(Skipped {
                    name: lossy.into_owned(),
                    reason: "non-UTF-8 directory name".into(),
                });
                continue;
            }
        };

        if !is_valid_id(&name) {
            skipped.push(Skipped {
                name,
                reason: "id must match [A-Za-z0-9._-]+".into(),
            });
            continue;
        }

        match CachedAra::from_dir_lean(&path) {
            Ok(cached) => insert_or_skip(&mut aras, &mut skipped, name, cached),
            Err(report) => {
                skipped.push(Skipped {
                    name,
                    reason: format!("{} parse error(s)", report.errors().len()),
                });
            }
        }
    }

    Ok(Ingest {
        aras: Arc::new(aras),
        skipped,
    })
}

/// Insert `cached` under `name`, or record a skip if `name` is already present.
///
/// Keeps the first ARA seen for an id and skips any later duplicate rather than
/// silently overwriting it. With `id == dir_name` a collision cannot arise from
/// two distinct directory entries on a case-sensitive FS, so this is
/// defence-in-depth — split into its own function to keep the guard unit-tested
/// (a filesystem fixture cannot construct the colliding state).
fn insert_or_skip(
    aras: &mut HashMap<String, Arc<CachedAra>>,
    skipped: &mut Vec<Skipped>,
    name: String,
    cached: CachedAra,
) {
    use std::collections::hash_map::Entry;
    match aras.entry(name) {
        Entry::Vacant(slot) => {
            slot.insert(Arc::new(cached));
        }
        Entry::Occupied(existing) => skipped.push(Skipped {
            name: existing.key().clone(),
            reason: "duplicate id (already ingested)".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ara-core/tests/fixtures/official")
            .join(name)
    }

    /// Recursively copy a fixture ARA into `dst`.
    fn copy_dir(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let to = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &to);
            } else {
                std::fs::copy(entry.path(), &to).unwrap();
            }
        }
    }

    /// Copy the minimal fixture into `root/<id>` so it parses as an ARA.
    fn good_ara(root: &Path, id: &str) {
        copy_dir(&fixture("minimal-artifact"), &root.join(id));
    }

    #[test]
    fn is_valid_id_accepts_and_rejects() {
        assert!(is_valid_id("resnet"));
        assert!(is_valid_id("my-ara_1.2"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("."));
        assert!(!is_valid_id(".."));
        assert!(!is_valid_id("my ara")); // space
        assert!(!is_valid_id("a/b")); // slash
        assert!(!is_valid_id("café")); // non-ASCII
    }

    #[test]
    fn ingests_two_good_aras() {
        let root = tempdir().unwrap();
        good_ara(root.path(), "alpha");
        good_ara(root.path(), "beta");

        let ingest = ingest(root.path()).expect("readable root");
        assert_eq!(ingest.aras.len(), 2);
        assert!(ingest.aras.contains_key("alpha"));
        assert!(ingest.aras.contains_key("beta"));
        assert!(ingest.skipped.is_empty());
    }

    #[test]
    fn broken_ara_is_skipped_not_fatal() {
        let root = tempdir().unwrap();
        good_ara(root.path(), "good");
        // A directory that is not a valid ARA (no trace/logic) → parse error.
        std::fs::create_dir_all(root.path().join("broken")).unwrap();
        std::fs::write(root.path().join("broken/README.md"), "not an ara").unwrap();

        let ingest = ingest(root.path()).expect("readable root");
        assert!(ingest.aras.contains_key("good"));
        assert!(!ingest.aras.contains_key("broken"));
        assert_eq!(ingest.skipped.len(), 1);
        assert_eq!(ingest.skipped[0].name, "broken");
    }

    #[test]
    fn rejected_charset_dir_is_skipped() {
        let root = tempdir().unwrap();
        good_ara(root.path(), "good");
        // A dir name with a space is a valid ARA on disk but an invalid id.
        good_ara(root.path(), "bad id");

        let ingest = ingest(root.path()).expect("readable root");
        assert!(ingest.aras.contains_key("good"));
        assert_eq!(ingest.aras.len(), 1, "the space-named dir must be skipped");
        assert!(ingest.skipped.iter().any(|s| s.name == "bad id"));
    }

    #[test]
    fn empty_root_yields_empty_map() {
        let root = tempdir().unwrap();
        let ingest = ingest(root.path()).expect("readable empty root");
        assert!(ingest.aras.is_empty());
        assert!(ingest.skipped.is_empty());
    }

    #[test]
    fn nonexistent_root_is_err() {
        let root = tempdir().unwrap();
        let missing = root.path().join("does-not-exist");
        assert!(ingest(&missing).is_err(), "missing root must be fatal (Err)");
    }

    #[test]
    fn duplicate_id_is_skipped_not_overwritten() {
        // A filesystem fixture can't produce two dirs with the same name, so
        // drive the collision guard directly. The first insert wins; the second
        // is recorded as a skip and does not replace it.
        let mut aras: HashMap<String, Arc<CachedAra>> = HashMap::new();
        let mut skipped: Vec<Skipped> = Vec::new();

        let first = CachedAra::from_dir_lean(&fixture("minimal-artifact")).unwrap();
        let first_etag = first.etag.clone();
        insert_or_skip(&mut aras, &mut skipped, "dup".into(), first);

        let second = CachedAra::from_dir_lean(&fixture("resnet-ara-example")).unwrap();
        insert_or_skip(&mut aras, &mut skipped, "dup".into(), second);

        assert_eq!(aras.len(), 1);
        assert_eq!(
            aras["dup"].etag, first_etag,
            "the first ARA for an id must be kept, not overwritten"
        );
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].name, "dup");
        assert!(skipped[0].reason.contains("duplicate"));
    }

    #[test]
    fn stray_file_at_root_is_ignored() {
        let root = tempdir().unwrap();
        good_ara(root.path(), "good");
        std::fs::write(root.path().join("NOTES.txt"), "loose file").unwrap();

        let ingest = ingest(root.path()).expect("readable root");
        assert_eq!(ingest.aras.len(), 1);
        assert!(ingest.skipped.is_empty(), "a loose file is not a skipped ARA");
    }
}
