#!/usr/bin/env bash
#
# embed-viewer.sh — regenerate (or freshness-check) the viewer bundle that
# `ara-cli` bakes into its binary via `include_dir!`.
#
# `ara serve` embeds a prebuilt `trunk` bundle (crates/ara-cli/assets/viewer/)
# so `cargo install ara-cli` works with no wasm toolchain. The cost: those
# committed bytes lag the ara-viewer *source* until someone regenerates them.
# This script is the single canonical regen path, plus a `--check` mode CI runs
# to fail the build when the embed is stale.
#
# Freshness is tracked by a content hash of the frontend SOURCE INPUTS (not the
# build output). Build output (wasm) is not guaranteed byte-reproducible across
# machines/toolchains, so a byte-diff of the bundle would be flaky; a source
# hash is deterministic and toolchain-independent.
#
# Scope/limitation: the hash covers the ara-viewer crate's own inputs (src/,
# public/, index.html, Trunk.toml, Cargo.toml). A change that only touches an
# upstream crate (e.g. ara-core) can alter the compiled wasm without changing
# this hash — regenerate manually after such changes.
#
# Cargo.toml is hashed with version-number literals (version = "x.y.z")
# normalized away, because every functional PR bumps the workspace version and
# rewrites the ara-core dep pin without changing the built UI. A real dependency
# add/remove/rename still moves the hash.
#
# Usage:
#   scripts/embed-viewer.sh            # rebuild + copy into assets/viewer/ + write hash
#   scripts/embed-viewer.sh --check    # recompute hash, compare to committed; nonzero if stale
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VIEWER_DIR="$ROOT/crates/ara-viewer"
EMBED_DIR="$ROOT/crates/ara-cli/assets/viewer"
# Kept OUTSIDE EMBED_DIR so `include_dir!` doesn't bake it into the binary and
# serve it as a viewer asset — it's build metadata, not part of the UI.
HASH_FILE="$ROOT/crates/ara-cli/assets/viewer.source-hash"

# The frontend source inputs whose content determines the built bundle.
# Sorted, per-file sha256, then a sha256 over that list → one stable digest.
compute_source_hash() {
  {
    # Recursive inputs.
    find "$VIEWER_DIR/src" "$VIEWER_DIR/public" -type f
    # Single-file inputs.
    printf '%s\n' \
      "$VIEWER_DIR/index.html" \
      "$VIEWER_DIR/Trunk.toml" \
      "$VIEWER_DIR/Cargo.toml"
  } | LC_ALL=C sort | while read -r f; do
    # Emit "<sha256>  <repo-relative-path>" so the digest is location-stable.
    case "$f" in
      */Cargo.toml)
        # Normalize version-number literals (version = "x.y.z") to a placeholder
        # before hashing. Every functional PR bumps the workspace version, which
        # rewrites the ara-core dep pin here; that churn does NOT change the built
        # UI, so it must not mark the embed stale. A real dep add/remove/rename
        # still moves the hash. (version.workspace = true has no quoted literal
        # and is untouched.)
        sed -E 's/version = "[^"]*"/version = "*"/g' "$f" \
          | shasum -a 256 | awk -v p="${f#"$ROOT"/}" '{print $1 "  " p}'
        ;;
      *)
        shasum -a 256 "$f" | awk -v p="${f#"$ROOT"/}" '{print $1 "  " p}'
        ;;
    esac
  done | shasum -a 256 | awk '{print $1}'
  # LC_ALL=C pins the sort to a bytewise order so the file list — and thus the
  # digest — is identical across locales (macOS en_US vs CI C/POSIX). Without it
  # mixed-case names (Cargo.toml/Trunk.toml vs lowercase paths) reorder per
  # locale and the same content produces a different hash on different machines.
}

case "${1:-}" in
  --check)
    if [ ! -f "$HASH_FILE" ]; then
      echo "FAIL: $HASH_FILE missing — run scripts/embed-viewer.sh to generate it." >&2
      exit 1
    fi
    committed="$(cat "$HASH_FILE")"
    current="$(compute_source_hash)"
    if [ "$committed" != "$current" ]; then
      echo "FAIL: embedded viewer is stale." >&2
      echo "  committed source hash: $committed" >&2
      echo "  current source hash:   $current" >&2
      echo "  The ara-viewer frontend source changed but crates/ara-cli/assets/viewer/" >&2
      echo "  was not regenerated. Run: scripts/embed-viewer.sh" >&2
      exit 1
    fi
    echo "OK: embedded viewer is up to date ($current)."
    ;;
  "")
    command -v trunk >/dev/null 2>&1 || {
      echo "ERROR: trunk not found. Install with: cargo install trunk --locked" >&2
      exit 1
    }
    echo "Building viewer (wasm-release profile)…"
    ( cd "$VIEWER_DIR" && TRUNK_BUILD_CARGO_PROFILE=wasm-release trunk build --release )

    echo "Replacing $EMBED_DIR with fresh dist/…"
    rm -rf "$EMBED_DIR"
    mkdir -p "$EMBED_DIR"
    cp -R "$VIEWER_DIR/dist/." "$EMBED_DIR/"

    compute_source_hash > "$HASH_FILE"
    echo "Wrote source hash: $(cat "$HASH_FILE")"
    echo "Done. Review + commit crates/ara-cli/assets/viewer/."
    ;;
  *)
    echo "Usage: $0 [--check]" >&2
    exit 2
    ;;
esac
