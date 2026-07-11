# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and the project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Viewer: selectable layout modes for the map/detail panes via a segmented
  toggle in the header — **Stack** (map on top at full width, detail below; the
  new default, matching the wide exploration-DAG shape) and **Split** (map left,
  detail right; the previous side-by-side behaviour). Session-only; narrow
  viewports always stack. (#9)

### Changed
- Build: `scripts/embed-viewer.sh` is now the canonical way to regenerate the
  viewer bundle baked into `ara-cli`, and a new CI job (`viewer-embed-fresh`)
  runs `--check` to fail a PR when the ara-viewer frontend source changes without
  a matching regen — so `ara serve` can't silently ship a stale embedded UI. (#9)

### Fixed
- Viewer: detail-pane placeholder now reads "Select a step to see its details."
  instead of "…on the left." — the map sits on top in the default stack mode, so
  the directional copy was wrong. (#9)
- Viewer: inactive layout-toggle label now uses `--ink` (~11.6:1) instead of
  `--muted` (~3.57:1), clearing the WCAG AA 4.5:1 contrast threshold for an
  interactive control label. (#9)
- Viewer: the active layout-toggle segment no longer bolds its label, which was
  changing the label width and nudging the segments sideways on every toggle.
  (#9)

## [0.1.0] - 2026-07-11

First published release. The `ara` CLI (`validate` + `layout` + `serve` with a
live-reloading web viewer) and `ara-core` / `ara-wasm` are published to
crates.io.

### Added
- `ara serve <dir>`: axum 0.8 server for a single ARA directory (Stage 4). Serves
  the viewer (**embedded by default** via `include_dir!`, so `cargo install
  ara-cli` works with no extra files; `--assets <dist>` overrides with an on-disk
  Trunk `dist/`, served through `ServeDir` with precompressed brotli/gzip),
  `/api/manifest` (parse-once cache, strong `ETag`, `304` on `If-None-Match`,
  `no-cache`), range-capable `/api/figure/*` sandboxed to `<dir>/evidence`, and a
  `/api/live` WebSocket. A debounced `notify` watcher (`--poll` for network
  mounts) reparses on change, atomically swaps the cache, and pushes the new
  `ETag`; the client re-fetches and re-renders **preserving pan/zoom/selection**.
  Flags: `--port` (default 8080), `--assets`, `--poll`.
- Viewer client (`crates/ara-viewer`): `ManifestSource::Api` live mode — tries
  `/api/manifest`, falls back to the static `manifest.json`, and subscribes to
  `/api/live` for reparse-driven refresh; one wasm bundle works under both
  `ara serve` and static hosting.
- Leptos CSR client (`crates/ara-viewer`): SVG DAG **skinned to the published ARA
  design** (warm-cream theme, glyph+label node kinds, dead-end highlighting) from
  Stage-2 positions via a pure scene-model `GraphRenderer`, with pan/zoom,
  keyboard-accessible nodes, a published-style drill-down pane (per-kind field
  hierarchy, claims with status, graceful degradation), toolbar
  search/type/dead-end filters, full loading/empty/error states, and an enforced
  sub-MB wasm size gate — from a static manifest via a `ManifestSource` seam
  (#6).
- Deterministic layered DAG **node** layout in `ara-core` via `dagre-dgl-rs`;
  positions + bounds added to `Manifest`; `ara layout <dir> --json` and
  `ara validate --layout`. Edge routing deferred to the client; geometry wire
  shape frozen, logical model kept additive.
- Real-ARA no-panic regression coverage: vendored `ara-paperbench` subset under
  `crates/ara-core/tests/fixtures/corpus/` with an always-on test asserting the
  parser never panics and always produces a `ParseReport`; opt-in submodule
  full-sweep test (`RUN_CORPUS_SWEEP=1`) over all 34 real artifacts (#3).
- `ara-core` YAML parser (`serde-saphyr`) with dual-dialect (`tree:`/`root:`)
  normalization to a `Manifest { nodes, links, bindings, claims }`, source-order
  preservation, cycle detection, Markdown claim parsing + binding resolution, and
  tolerant unknown-field capture. Pure `parse_sources` (wasm-safe) plus a native
  `parse_dir`.
- `ara validate <dir>` CLI with `--json` and `--strict`.
- Pinned fixtures copied from the two official ARA examples, plus synthetic and
  broken error-path fixtures and `insta` JSON snapshots of both manifests.

- Cargo workspace scaffold with crates `ara-core`, `ara-cli` (binary `ara`),
  `ara-wasm`, and `ara-viewer`.
- Reserved crate names on crates.io at `0.0.0`: the working `ara-*` crates and
  the defensive `bara-*` placeholders (`bara-core`, `bara-cli`, `bara-wasm`,
  `bara-viewer`).
- Root README documenting the workspace layout and install path.
- CI workflow (`fmt`, `clippy`, `test`, `wasm-build`) on GitHub Actions with a
  pinned Rust toolchain (`1.94.1`), SHA-pinned actions, `--locked` builds, and
  Dependabot for `cargo` + `github-actions`.
- `rust-toolchain.toml`, `rustfmt.toml`, and a `CONTRIBUTING.md` documenting the
  local pre-PR checks that mirror CI.
- `TODOS.md` tracking deferred work (MSRV job, wasm-target clippy, `docs/`).

### Changed
- Dropped the unverified `rust-version = "1.85"` MSRV declaration until it is
  tested at the `0.1.0` publish.
- Bumped workspace version `0.0.0 → 0.0.1`, then `0.0.1 → 0.0.2` (Stage 1),
  then `0.0.2 → 0.0.3` (issue #3), then `0.0.3 → 0.0.4` (Stage 2), then
  `0.0.4 → 0.0.5` (Stage 3), then `0.0.5 → 0.0.6` (Stage 4), then
  `0.0.6 → 0.1.0` (first published release).
