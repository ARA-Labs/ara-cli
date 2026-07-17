# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/), and the project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Viewer Context, Glossary, and Recipes panels — three more consumers of the
  shared `Modal`. Each has a header launcher (hidden when its data is absent):
  Context (`logic/problem.md` — statement, observations, gaps, key insight),
  Glossary (a count of `logic/concepts.md` terms, each with notation/definition/
  boundary and dotted cross-reference chips for related terms), and Recipes (one
  per `logic/solution/*.md` file). Concept/recipe LaTeX (`$…$`) renders as inert
  monospace, never interpreted. Each panel carries its own case-insensitive
  filter.
- Viewer Dependencies panel, built on a new shared accessible `Modal` component.
  A header launcher button shows a live count of the manifest's `related_work`
  (hidden entirely at a 0 count) and opens a modal listing every reference in
  source order (id, citation, type, DOI, delta, adopted elements, affected-claim
  chips) with its own case-insensitive filter input. The `Modal` is a reusable
  a11y primitive (`role="dialog"` + `aria-modal` + `aria-labelledby`) with a
  full focus trap: focus moves into the dialog on open, Tab/Shift+Tab wrap at
  both ends, Esc and scrim-click close, and focus returns to the invoking
  element on close. It goes full-screen below 800px.
- Viewer detail pane: per-node **BUILT ON** and **RESULT** blocks in the
  corrected hub order. BUILT ON renders chips for the related work a node builds
  on (id + citation, resolved from `built_on` → `related_work`); RESULT renders
  chips for the exhibits linked to a node (id + figure/table label, resolved
  from `node_exhibits` → `exhibits`). Both omit entirely when empty. The
  experiment `result` field is relabelled **WHAT IT DID**. (REASONING is
  intentionally deferred; exhibit table/markdown bodies are not rendered yet.)
- Viewer paper header: title, authors, venue/year, and a collapsible Abstract
  from `PAPER.md`. When the loaded manifest carries a titled `PaperMeta` the app
  header shows the paper metadata (Abstract collapsed by default); otherwise it
  falls back to the "ARA Viewer" brand.
- Model `pivot` nodes (`from`/`to`/`trigger`) and the
  `hypothesis`/`failure_mode`/`lesson` fields on `dead_end` nodes, which
  previously degraded to unknown-field warnings.
- Parse the artifact logic layer into the `Manifest`. `parse_dir` now reads
  `PAPER.md` frontmatter (`paper`), `logic/problem.md` (`problem`),
  `logic/concepts.md` (`concepts`), `logic/related_work.md` (`related_work`),
  and every `logic/solution/*.md` (`recipes`, one per file, sorted). The readers
  are tolerant — an absent file is skipped silently, a malformed present file
  adds a warning without failing the parse. Manifest also gains the (currently
  empty) `exhibits`/`built_on`/`node_exhibits` fields, reserved for a later
  evidence/resolution task. Old manifests and artifacts lacking these files
  round-trip and serialize identically (all new fields skip when empty).
- Parse the `evidence/` layer into `Manifest.exhibits` and resolve node→exhibit
  (`node_exhibits`) and node→related-work (`built_on`) edges. `parse_dir` reads
  `evidence/README.md` (a column-name-tolerant index that handles the eight real
  header shapes, including reordered, `Key refs`, `Used by`, and no-claims-column
  tables) plus every `evidence/figures/*.md` and `evidence/tables/*.md` body
  (stored verbatim). Each exhibit's supported claims come from its index row or,
  when the index has none, from an inline `Supports: C##` line in the body. The
  two resolution passes link a node to an exhibit or related-work entry when
  their claim sets intersect, iterating in source order and deduplicating. The
  reader is tolerant — an absent `evidence/` dir is skipped silently, and a
  missing body file or unindexed body warns without failing the parse.

## [0.1.6] - 2026-07-13

### Added
- Pre-built release binaries and a Homebrew formula, driven by cargo-dist. A
  new tag-triggered `release.yml` cross-builds the `ara` binary for
  `aarch64-apple-darwin` (macOS Apple Silicon) and `x86_64-unknown-linux-gnu`
  (Linux x86_64), attaches tarballs + checksums to the GitHub Release, and
  generates a `curl | sh` installer and the `ara.rb` Homebrew formula. The
  formula targets the `ARA-Labs/homebrew-tap` tap, so users can
  `brew install ARA-Labs/tap/ara`. (Auto-push to the tap is deferred; the
  formula is committed by hand from the release asset for now.)
- Resizable viewer panels: drag (or keyboard-resize) the divider between the
  map and detail panes to rebalance them. The gutter is a real WAI-ARIA
  window-splitter — focusable, Arrow/Home/End operable, and value-bearing —
  with per-mode split ratios (side-by-side vs. stacked), structural pane floors,
  a global cursor/selection lock while dragging, and double-click to reset. The
  ratio is in-memory only (resets on reload); on narrow (<800px) screens the
  layout still collapses to a single column with the gutter hidden.

### Changed
- Bump dependencies: `tower-http` 0.6 → 0.7 and `notify-debouncer-full` 0.5 →
  0.7 (`ara-cli`), and `gloo-net` 0.6 → 0.7 (`ara-viewer`). Regenerated the
  embedded viewer bundle so the shipped wasm matches the `gloo-net` bump.
  (#20, #21, #22)
- Bump CI actions: `docker/setup-buildx-action` 3 → 4 and
  `docker/build-push-action` 6 → 7. (#18, #19)

## [0.1.3] - 2026-07-12

### Added
- `ara serve --hub --ara-root <dir>`: read-only multi-ARA mode. Scans the root
  once at startup, parses each child directory into a per-ARA parse-once cache,
  and serves them under path-based `/a/{id}/` routing (`/a/{id}/api/manifest`
  with ETag/304, `/a/{id}/` viewer index, a root index of available ARAs). Hub
  reads are pure cache hits — no watcher, no reparse after startup. Ids are
  constrained to `[A-Za-z0-9._-]+`; broken ARAs and bad ids are logged and
  skipped, an unreadable root is fatal, and an empty root warns loudly. (#17)
- `--host <ip>` flag for `ara serve` (default `127.0.0.1`; set `0.0.0.0` in a
  container so the port is reachable from the host). (#17)
- Multi-stage musl → distroless `Dockerfile` for the hub (viewer baked into the
  binary; no wasm toolchain in the image; `cargo-chef` dependency-layer cache)
  plus `.dockerignore`, a CI `docker` smoke-test job, and `docs/deploy.md`
  (Docker/compose, systemd, Caddy/nginx reverse-proxy compression, ops notes). (#17)

### Changed
- Viewer: resolves its manifest/live URLs **relative** to the page
  (`api/manifest`, `api/live`) instead of absolute paths, so the same bundle
  serves both local `ara serve` (page at `/`, unchanged behaviour) and the hub
  (per-ARA `<base href="/a/{id}/">`). (#17)

## [0.1.2] - 2026-07-11

### Added
- Viewer: **Tree display mode** — an alternate DOM indented tree-list rendering
  of the exploration graph, reproducing the published `research-visualizer`
  scaffold. Chosen with a **graph | tree** segmented toggle in the header (the
  SVG graph stays the default). Rows show a kind glyph, node id, `⇠ id`
  dependency marker, and title; children nest with a spine; dead ends are struck
  through; isolated-subtree roots render in an `.isobox`; hovering a row
  highlights its dependencies. Drives the same selection + filter as the graph.
  (#7)
- Viewer: **Replay stepper** in the toolbar (`‹ / ▶ Replay⇄⏸ Pause / ›`, `←`/`→`
  keys) that steps the selection through node order in both display modes, with
  a shared `step i / N` · `shown / N steps` readout. 1300 ms auto-play stops at
  the last node; the `←`/`→` keys are guarded so they don't hijack the search
  field. (#7)
- Core: `Node.isolated` boolean manifest field (serde-default `false`, omitted
  from the wire form when false) marking the root of an isolated subtree; drives
  the tree mode's isolated-subtree box. Old manifests round-trip unchanged. (#7)

### Changed
- Viewer: node kind glyphs now use the published `research-visualizer` set —
  `experiment ✦`, `decision →`, `dead_end ✗`, `insight !`, other `•` (question
  stays `Q`) — read from the single `kind_meta` source by both the SVG graph and
  the new tree-list. This visibly restyles the existing SVG graph's node glyphs.
  (#7)

## [0.1.1] - 2026-07-11

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
