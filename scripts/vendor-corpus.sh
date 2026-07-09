#!/usr/bin/env bash
#
# vendor-corpus.sh — re-vendor the curated ara-paperbench subset used by the
# always-on `corpus_no_panic` regression test.
#
# The vendored subset lives under
# `crates/ara-core/tests/fixtures/corpus/<artifact>/` and keeps only the two
# files the parser reads: `trace/exploration_tree.yaml` and (where present)
# `logic/claims.md`. Files are copied verbatim — never hand-edit them; re-run
# this script at a new pin instead. Provenance and attribution live in
# `crates/ara-core/tests/fixtures/corpus/SOURCE.md`.
#
# Upstream: https://github.com/AmberLJC/ara-paperbench (CC-BY-4.0)
#
# Usage:
#   scripts/vendor-corpus.sh            # clone at the pinned commit into a temp
#                                       # dir, then copy the subset
#   scripts/vendor-corpus.sh <checkout> # copy from an existing checkout already
#                                       # at the pinned commit
set -euo pipefail

# --- Configuration ----------------------------------------------------------

UPSTREAM_URL="https://github.com/AmberLJC/ara-paperbench.git"
PINNED_COMMIT="3fe7ab4d08f68555d8c4661fa2b4fbfd4d597fd8"

# Artifact subset, chosen to span the drift dimensions the real schema
# exercises (see SOURCE.md). Paths are relative to the upstream `artifacts/`
# directory.
ARTIFACTS=(
  "extra/andes"
  "extra/expbench"
  "paperbench/sample-specific-masks"
  "speedrun/nanogpt-speedrun"
  "rebench/rebench-rust_codecontests"
  "rebench/rebench-restricted_mlm"
)

# Destination: `crates/ara-core/tests/fixtures/corpus/` relative to repo root.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_ROOT="$REPO_ROOT/crates/ara-core/tests/fixtures/corpus"

# --- Resolve the upstream checkout ------------------------------------------

CLEANUP_DIR=""
cleanup() {
  if [[ -n "$CLEANUP_DIR" && -d "$CLEANUP_DIR" ]]; then
    rm -rf "$CLEANUP_DIR"
  fi
}
trap cleanup EXIT

if [[ $# -ge 1 ]]; then
  CHECKOUT="$1"
  echo "Using existing checkout: $CHECKOUT"
  echo "  (assuming it is at commit $PINNED_COMMIT)"
else
  CLEANUP_DIR="$(mktemp -d)"
  CHECKOUT="$CLEANUP_DIR/ara-paperbench"
  echo "Cloning $UPSTREAM_URL at $PINNED_COMMIT ..."
  # Suppress git's progress chatter (>/dev/null) but let stderr through so a
  # clone/fetch failure under `set -e` is diagnosable rather than silent.
  git clone --filter=blob:none --no-checkout "$UPSTREAM_URL" "$CHECKOUT" >/dev/null
  git -C "$CHECKOUT" fetch --depth 1 origin "$PINNED_COMMIT" >/dev/null
  git -C "$CHECKOUT" checkout "$PINNED_COMMIT" >/dev/null
fi

# --- Copy the subset --------------------------------------------------------

echo "Vendoring ${#ARTIFACTS[@]} artifacts into $DEST_ROOT ..."
for artifact in "${ARTIFACTS[@]}"; do
  src="$CHECKOUT/artifacts/$artifact"
  dest="$DEST_ROOT/$artifact"

  tree_src="$src/trace/exploration_tree.yaml"
  if [[ ! -f "$tree_src" ]]; then
    echo "ERROR: missing $tree_src — is the checkout at $PINNED_COMMIT?" >&2
    exit 1
  fi

  mkdir -p "$dest/trace"
  cp "$tree_src" "$dest/trace/exploration_tree.yaml"

  claims_src="$src/logic/claims.md"
  if [[ -f "$claims_src" ]]; then
    mkdir -p "$dest/logic"
    cp "$claims_src" "$dest/logic/claims.md"
  fi

  echo "  vendored $artifact"
done

echo "Done. Review the diff and update SOURCE.md if the pin changed."
