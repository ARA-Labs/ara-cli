# Stage 3 — Minimal Leptos wasm Viewer (static manifest)

**PR target:** `stage3-wasm-viewer` → `main`. **Depends on:** Stage 2.
**Version bump:** `0.0.3 → 0.0.4`.

## Problem background

The product is a text-heavy drill-down viewer: verbatim quotes, tables, inline
figures, all selectable/searchable/accessible. A DOM framework (Leptos) gives
this at sub-megabyte wasm cost; egui/eframe cannot (canvas, no native
selection/search/a11y, multi-MB bundle) and stays a documented fallback only.
This stage renders a **checked-in static `manifest.json`** (from Stage 2) — no
server yet — proving the render path end to end.

## Proposed solution

A Leptos 0.8 CSR app in `client/` (built with Trunk) that loads a static
manifest, draws the DAG as declarative **SVG** bound to signals, and shows a
DOM drill-down pane on node click. Reuse `ara-core` types over the wire via
`serde_json` (no hand-written TS). Put the graph behind a trait so an SVG→canvas
swap is clean if scale demands it.

## Implementation steps

1. **`client/` crate** (Leptos CSR, Trunk). `Cargo.toml` depends on `ara-core`
   (default-features off; wasm-safe) and `ara-wasm` for any hand-written interop.
   Add `index.html`, `Trunk.toml`, and `client/dist` to `.gitignore`.
2. **Load manifest:** fetch `manifest.json` (checked in under `client/public/`
   for now) → `serde_json::from_str::<Manifest>` → Leptos signal.
3. **Graph view (SVG):** render `<g>/<rect>/<path>/<text>` from `Node.pos` /
   `Link.route`. Per-`NodeKind` CSS classes (question/experiment/decision/
   dead_end/pivot/insight); **dead-end highlighting**. Pan/zoom via `viewBox`.
   Per-element hit-testing for clicks.
4. **`GraphRenderer` trait** with an `SvgRenderer` impl now; leave a
   `CanvasRenderer` (web-sys `CanvasRenderingContext2d` off a `NodeRef`) stub so
   the swap is a one-line change if Stage 2's scale probe or in-browser fps
   demands it.
5. **Drill-down pane (DOM):** on node select, render narrative + structured
   fields: verbatim quotes (selectable), `<table>` evidence, inline `<img>`
   figures. **Graceful degradation:** if `narrative` is absent, render structured
   fields only — never call an LLM at view time.
6. **View-state:** keep pan/zoom + selection in signals (reused by Stage 4 live
   reload to survive manifest refresh).
7. **SVG-vs-canvas decision:** measure pan/zoom fps on the largest corpus tree.
   Record the result in `docs/`; switch renderer if fps < ~30 or DOM element
   count exceeds a few thousand.

## Tests / verification

- `wasm-bindgen-test` headless-browser tests for the graph component: N nodes →
  N node elements; click → pane shows that node's content; dead-end nodes carry
  the highlight class.
- Degradation test: a node without `narrative` renders structured fields, no
  error.
- `trunk build --release` succeeds; bundle size recorded (target sub-MB wasm).

## Milestone / acceptance

Open `index.html`, navigate a real tree: pan/zoom, click nodes, read
selectable drill-down text and see figures/tables. Renderer choice documented.

## Out of scope (deferred)

Any server/HTTP (Stage 4); live reload (Stage 4); figure streaming from disk
(Stage 4 serves figures; here they are static assets).

## CHANGELOG (Unreleased → Added)

- Leptos CSR client (`client/`): SVG DAG with pan/zoom, node-kind styling,
  dead-end highlighting, and a DOM drill-down pane from a static manifest.
