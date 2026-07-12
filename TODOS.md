# TODOS

Deferred work captured during reviews. Each item has enough context to pick up
cold. Remove an item when it lands.

## Deferred from Stage 0 eng review (2026-07-08)

### T-MSRV — MSRV verification job at the 0.1.0 release cut
- **What:** Re-add `rust-version` to `[workspace.package]` and add a CI job that
  builds/tests on that exact MSRV toolchain.
- **Why:** Stage 0 dropped the unverified `rust-version = "1.85"` claim (nothing
  tested it). Once ara-core is published and external crates can depend on it, an
  untested MSRV misleads consumers.
- **Context:** Belongs in the `0.0.5 → 0.1.0` release PR (see
  `plans/stage-overview.md`), which is the first crates.io publish.
- **Depends on:** Stage 4.

### T-WASM-CLIPPY — clippy the wasm32 target once cfg-gated code exists
- **What:** Add `cargo clippy --target wasm32-unknown-unknown` (ara-core,
  ara-viewer) to CI.
- **Why:** Native clippy skips `#[cfg(target_arch = "wasm32")]` branches, so
  wasm-only code can rot while CI stays green.
- **Context:** **Now actionable.** Stage 3 added `#[cfg(target_arch = "wasm32")]`
  code in `ara-viewer` (`src/source.rs` `fetch_manifest` + the `tests/web.rs`
  browser layer), so native clippy no longer covers the full crate. (`ara-wasm`
  was dropped in Stage 3 — target `ara-viewer`, not `ara-wasm`.)
- **Depends on:** — (met: cfg-gated code now exists).

### T-DOCS — create docs/ and wire the plan→docs migration lifecycle
- **What:** Create a `docs/` directory and follow `CLAUDE.md`'s rule that a
  merged stage's plan is folded into `docs/` and removed from `plans/`.
- **Why:** `CLAUDE.md` and `stage-overview.md` both assume completed plans
  migrate to `docs/`, but no `docs/` exists yet — the lifecycle has nowhere to
  land.
- **Context:** Stage 0's own plan is the first migration candidate; do this when
  Stage 0 merges. (`docs/` now exists — `docs/ara-format-feedback.md` was added
  in the Stage 1 review — so the directory part is done; the plan→docs migration
  step still stands.)
- **Depends on:** none.

## Deferred from Stage 1 eng review (2026-07-08)

### T-EVIDENCE — resolve `E##` evidence proof references
- **What:** Add a resolution pass that validates claim `Proof: [E##]` refs
  against an evidence registry and stores evidence content on the `Manifest`.
- **Why:** Stage 1 stores `E##` refs raw and never validates them (no registry
  defines `E01`..`E06`), so a typo in a `Proof:` list is silently accepted.
- **Context:** Blocked on the ARA maintainer defining an evidence registry
  (e.g. `evidence/index.yaml` keyed by `E##`) — see `docs/ara-format-feedback.md`
  item 8. The official corpus has no `E##` definitions today.
- **Depends on:** upstream ARA schema (evidence registry).

### T-REAL-CORPUS — widen the model to the real ARA corpus (Stage 1.5)
- **What:** Extend `ara-core` beyond the two minimal official examples to cover
  the schema the real corpus actually uses. Concretely: model the recurring node
  fields `failure_mode` / `hypothesis` / `lesson` (dead-ends), the metadata
  fields `provenance` / `source` / `status` / `timestamp` / `thinking` /
  `method` / `justification`, the transition fields `from` / `to` / `trigger`,
  and add a `pivot` node kind. Optionally support the `ara-2.0` streams document
  format (`schema_version: "ara-2.0"` with `anchors` / `official_stream` /
  `malt_stream`, no `tree:`).
- **Why:** Stage 1 was scoped (and eng-reviewed) as canonical-only against
  `minimal-artifact` + `resnet-ara-example`. Running `ara validate` over the real
  corpora — `AmberLJC/ara-paperbench` (32 artifacts) and `ARA-Labs/ARA-Demo`
  (2 artifacts) — shows **every** real artifact emits warnings (300 unknown-node-
  field warnings total) and ~half emit errors (real cycles + broken `evidence:`
  claim refs). The parser is robust (0 panics across all 34), but the canonical
  model is too narrow to parse real artifacts cleanly.
- **Context:** Frequencies observed (paperbench + demo): `failure_mode`/
  `hypothesis`/`lesson` ×67 each, `provenance`/`source` ×35, `method` ×13,
  `trigger`/`from`/`to` ×4–6, plus `status`/`timestamp`/`thinking`. One rebench
  artifact (`rebench-restricted_mlm`) is `ara-2.0` and does not use `tree:` at
  all. See `docs/ara-format-feedback.md` §13. Decision (2026-07-08): ship Stage 1
  canonical-only, defer this widening.
- **Status (2026-07-09, issue #3):** still open — widening is deferred. Issue #3
  added a separate **no-panic regression net** (vendored `ara-paperbench` subset
  + opt-in submodule sweep in `crates/ara-core/tests/corpus_no_panic.rs`) that
  proves the parser never unwind-panics on real data and always produces a
  `ParseReport`. That is *not* this task: it does not parse real artifacts
  cleanly, it only guards robustness. Keep the two distinct.
- **Depends on:** none (can start any time); overlaps with T-ARA-SCHEMA if the
  maintainer publishes a schema first.

## Deferred from Issue #3 eng review (2026-07-09)

### T-PARSE-DEPTH — guard parser recursion against stack-overflow abort
- **What:** Add a recursion depth limit to `Normalizer::dfs`
  (`crates/ara-core/src/parse.rs:233`, self-call in the `for child in &raw.children`
  loop) and `visit` (`parse.rs:386`, the cycle-detection DFS). Past a bound (e.g.
  512 levels), emit a clean `report.error(..., "nesting too deep")` and stop
  recursing instead of continuing to descend.
- **Why:** Both functions recurse per-child / per-edge with no depth guard. A
  pathologically deep artifact overflows the stack → `SIGABRT`, which
  `std::panic::catch_unwind` does **not** intercept. Issue #3's no-panic
  regression net therefore has a blind spot exactly on the pathological input a
  robustness net is meant to catch. The net's contract was narrowed to "no
  *unwinding* panic" to be honest about this; T-PARSE-DEPTH is the real fix.
- **Context:** Discovered in the Issue #3 eng review (outside-voice finding,
  confirmed against source). The real ARA corpus (34 artifacts) is not deep
  enough to trip this, so it does not block #3. When fixed, add a synthetic
  deep-nesting fixture (~10k levels) asserting a clean `Err` (not an abort). At
  that point the always-on `corpus_no_panic` test's "no-panic" claim becomes
  literally true end to end.
- **Depends on:** none (independent parser hardening).

### T-ARA-SCHEMA — adopt the upstream ARA schema once published
- **What:** When the ARA format ships a versioned schema, replace Stage 1's
  tolerant workarounds (canonical-only scope, opaque `extra` capture, lenient
  Markdown claim parsing, guessed id grammar) with strict validation against the
  schema, and honor a `schema_version` field for pinning/migration.
- **Why:** The Stage 1 parser guesses field sets, id grammar, and root form
  because no schema exists; a published schema lets it be strict and safely
  broadens dialect support.
- **Context:** Requests logged in `docs/ara-format-feedback.md` (items 1, 4, 7,
  9). Revisit when the maintainer responds.
- **Depends on:** upstream ARA maintainer publishing a schema.

## Deferred from Stage 2 eng review (2026-07-09)

### T-EDGE-ROUTING — compute proper edge routes (bend points / orthogonal)
- **What:** Route DAG edges as polylines/splines that avoid overlapping nodes,
  instead of straight lines from endpoint to endpoint.
- **Why:** Stage 2 ships node positions + bounds only; the Stage 3 client draws
  edges straight from endpoints. On dense/wide DAGs straight edges cross through
  intervening node boxes and read poorly.
- **Context:** Deferred from the Stage 2 eng review. Edge routing is the most
  float-sensitive, most crate-specific part of a dagre port, so it was kept out
  of the byte-deterministic core for now and pushed to the client. When picking
  this up, first decide **where** routing lives: client-side (from endpoint
  positions, no cross-target determinism concern) or back in `ara-core` (then it
  must join the step-1 native≡wasm golden test, and `Link.route: Option<Vec<Point>>`
  re-enters the wire type as an additive geometry field).
- **Depends on:** Stage 3 renderer.

### T-LAYOUT-SPIKE-FALLBACK — client-side dagre.js fallback if the crate spike fails
- **What:** If the Stage 2 step-1 spike finds no pure-Rust Sugiyama crate that is
  simultaneously wasm-safe **and** byte-deterministic native≡wasm, pivot layout
  to client-side dagre.js/elkjs and keep `ara-core` logical-graph-only (no
  geometry in the `Manifest`, no `ara layout` command).
- **Why:** The Rust layered-layout ecosystem is thin and largely low-adoption
  (`rust-sugiyama` ~5.8k dl, `dagre` ~1.9k, others far less). Forcing an
  unsuitable crate would either break the wasm client or fail cross-target
  determinism — the whole reason layout is in core.
- **Context:** This is the documented go/no-go branch for Stage 2 step 1, so the
  pivot is a planned path rather than a mid-stage scramble. Trigger: step-1
  cross-target golden test cannot be made to pass. Mature JS layout is the
  fallback because it is proven and runs client-side for free.
- **Depends on:** outcome of the Stage 2 step-1 spike.

## Deferred from Stage 3 design review (2026-07-10)

### T-DESIGN-TOKENS — extract the published ARA token set into docs
- **What:** Capture the published `research-visualizer` token set (warm-cream
  palette, glyph+label kind encoding, dead-end-only colour rule, `ui-sans-serif`/
  `ui-monospace` fonts) into `docs/design-tokens.md` (or `DESIGN.md`).
- **Why:** Stage 3 vendors these tokens into the client stylesheet; without a
  documented source of truth, Stage 4/5 (and any future surface) will re-derive
  colours ad hoc and drift from the published ARA look.
- **Context:** The reference is `ARA-Labs/ARA-Demo` `*/trajectory.html`
  (`<style>:root`). Design review chose "hybrid: SVG graph, published skin", so the
  tokens are now load-bearing across stages.
- **Depends on:** Stage 3 vendoring the tokens.

### T-GRAPH-KBD-NAV — arrow-key spatial traversal of the SVG graph
- **What:** Let keyboard users walk node-to-node across the SVG DAG by spatial
  adjacency (arrow keys), beyond the Tab-order + search/filter keyboard path
  Stage 3 ships.
- **Why:** Full keyboard parity for power users navigating a large graph; Tab
  order alone is tedious on a wide DAG.
- **Context:** Stage 3 ships focusable nodes (Tab, Enter/Space select) + toolbar
  search/type/dead-end filters as the primary keyboard nav. Spatial arrow-walking
  of an arbitrary DAG is non-trivial and was deferred so Stage 3 lands.
- **Depends on:** Stage 3 SVG graph.

### T-VIEWER-TREE-LIST — published DOM tree-list as an alternate display mode
- **What:** Add the published `research-visualizer` DOM tree-list as a second
  viewer display mode (a `Graph ⇄ Tree` toggle), plus the missing published
  interactions: the Replay stepper (prev/next/play + `←/→` + step counter) and
  the layer-panel overlay (Context/Glossary/Dependencies/Recipes) + header
  Abstract `<details>` (inert until the schema carries them).
- **Why:** `ARA-Labs/ARA-Demo`'s viewer (`nanogpt_ara/trajectory.html`) renders a
  DOM indented tree-list — rows with `⇠ id` dep markers + hover-highlight, not
  SVG edges — and ships replay + layer panels. Stage 3 shipped the reviewed
  SVG-DAG hybrid instead; this restores display/interaction parity as an option
  without dropping the graph.
- **Context:** Keep the SVG graph as the default/Graph mode; reuse `kind_meta`,
  the detail pane, `filter::node_matches`, and the `selected`/`filter`/`pan_zoom`
  signals unchanged. Layer panels + abstract stay inert until T-REAL-CORPUS
  widens the schema. Tracked in GitHub issue #7.
- **Depends on:** Stage 3 viewer (PR #6); layer panels depend on T-REAL-CORPUS.

## Deferred from Stage 3 eng review (2026-07-10)

### T-VIEWER-DIST-PACKAGING — how the ara-viewer frontend reaches users
- **What:** Decide and implement how the Trunk `dist/` (wasm + assets) from
  `crates/ara-viewer` is distributed. `cargo install ara-cli` cannot serve a
  generated/gitignored frontend.
- **Why:** Stage 4 (`ara serve`) serves the `dist/`, and the `0.1.0` release
  publishes `ara-core → ara-wasm → ara-cli` but not `ara-viewer`'s assets. Without
  a plan, the released CLI has no frontend to serve.
- **Context:** Options — embed `dist/` into the `ara-cli` binary at build time
  (e.g. `rust-embed`), copy it during the release, or publish `ara-viewer` assets
  separately. Surfaced by the Stage 3 eng-review outside voice (Codex).
- **Depends on:** Stage 4 (`ara serve`) / the `0.1.0` release cut.

### T-STAGE4-VERSION-BUMP — reconcile Stage 3/4 version collision
- **What:** Update the Stage 4 plan: if Stage 3 takes `0.0.5`, Stage 4 is
  `0.0.5 → 0.0.6` (both plans currently say `0.0.4 → 0.0.5`).
- **Why:** Two PRs can't both bump to `0.0.5`; the per-stage patch-bump chain in
  `stage-overview.md` breaks otherwise.
- **Context:** One-line edit to `plans/stage-4-serve-live-reload.md` when Stage 3
  merges. Surfaced by the Stage 3 eng-review outside voice.
- **Depends on:** Stage 3 merge.

## Deferred from Stage 5 eng review (2026-07-12)

### T-STATIC-EXPORT — evaluate `ara build <root>` static export vs the running hub
- **What:** A command that emits per-ARA `manifest.json` + the shared viewer into
  a static output dir, servable by any file host / CDN with no running process.
- **Why:** The viewer already has a fully-static path (`ManifestSource::Static` +
  `manifest.json` fallback, `crates/ara-viewer/src/source.rs:41`). A static export
  CDN-scales for free with no server, no Docker, no `<base>`-injection, no ingest
  concurrency — and gets parse-once+ETag by hashing the emitted JSON. It could
  obsolete much of the hub's complexity.
- **Context:** Stage 5 deliberately keeps the running server (D3, decided in the
  Stage 5 eng review) because it keeps ONE artifact (same binary local + hub) and
  keeps the `/api` contract identical local vs hub, so the viewer's
  live-with-fallback path is exercised the same way. Static export is the better
  long-term scaling play but a larger product decision. Revisit once the hub sees
  real multi-ARA traffic and we know whether live `/api` semantics matter.
- **Depends on:** Stage 5 shipping first (need the running hub to compare against).

### T-HUB-FIGURES — per-ARA figure serving on the hub
- **What:** `/a/{id}/api/figure/*` for hub mode, plus relative-URL treatment for
  figure `src` in the viewer so figures resolve under `<base href="/a/{id}/">`.
- **Why:** Deferred from the Stage 5 eng review (issue 11). Stage 4 mounts figures
  via a single `nest_service("/api/figure", ServeDir::new(dir))` (`mod.rs:144`) —
  a static prefix bound to ONE dir. axum cannot `nest_service` a `ServeDir` under a
  `{id}` path parameter, so the hub must either register N nested ServeDirs at
  ingest or hand-roll a handler that re-implements `..`-traversal rejection AND
  range support. Either way it's a security-sensitive (directory-escape) surface.
- **Context:** The viewer renders figures **inert** today (`crates/ara-viewer/src/detail.rs:386`,
  T-REAL-CORPUS deferred), so the endpoint would serve nothing yet. Build it in the
  SAME PR that lights up figure rendering, so the endpoint + the viewer's relative
  figure-URL contract are designed and tested together. When built, add
  traversal-attack tests (`../`, absolute path, symlink).
- **Depends on:** figure rendering (T-REAL-CORPUS) shipping in the viewer.
