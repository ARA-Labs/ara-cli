# Stage 3 — `ara-viewer`: Leptos wasm viewer (static manifest)

Design record for the browser client shipped in Stage 3. It renders a
checked-in static `manifest.json` (produced by `ara layout --json`) as an
interactive, drill-down DAG in the **published ARA visual language**. No server
yet — that is Stage 4; Stage 3 proves the render path end to end and leaves the
seams Stage 4 slots into.

Companion docs: [`manifest-schema.md`](manifest-schema.md) (the frozen wire
shape), [`stage-3-svg-spike.md`](stage-3-svg-spike.md) (the SVG-vs-canvas gate +
fps/size measurements).

## What shipped

A Leptos 0.8 CSR app built with [Trunk](https://trunkrs.dev) in
`crates/ara-viewer` (the reserved umbrella crate, repurposed from its stub
binary into a `lib` + thin `bin`). It:

- reads the `ara_core::Manifest` over the wire via `serde_json` (no hand-written
  TS; the empty `ara-wasm` skeleton was **dropped** — the viewer needs no
  hand-written interop);
- renders the DAG as declarative **SVG** from Stage-2 `Node.pos` +
  `Manifest.bounds`, skinned to the published token set;
- renders a **DOM drill-down pane** with the published detail structure;
- degrades gracefully for every field the schema does not yet carry.

Milestone: `trunk serve` (or `trunk build --release` + a static server — a CSR
app's `fetch` needs an HTTP context, not `file://`), then navigate a real tree:
pan/zoom the skinned SVG, click/keyboard-select nodes, read the per-kind
drill-down. Dead ends are unmistakable without relying on colour. Every
load/empty/error state is handled.

## Design system (from the published `research-visualizer` scaffold)

The viewer looks and behaves like the already-published ARA viewer, not a new
visual direction. Warm-cream light theme, single terracotta accent; **node kind
is read from a glyph + lowercase badge, never from colour** — only `dead_end` is
coloured (`--warn`, white ink), which keeps the encoding colourblind-safe. The
token set is vendored verbatim into `public/styles.css`:

| Token | Value | Use |
|-------|-------|-----|
| `--bg` | `#faf6ef` | app background |
| `--panel` / `--panel2` | `#fffdf8` / `#f4ecdf` | surfaces |
| `--ink` / `--muted` | `#2f2a23` / `#90856f` | text / secondary text |
| `--accent` | `#bf6a2e` | single accent (selection, spines, links) |
| `--warn` / `--ok` | `#a23b2d` / `#5d7c3f` | dead_end / positive status |
| `--glyph-bg` / `--glyph-ink` | `#e7ddca` / `#5a5142` | neutral kind glyph |
| `--sel-bg` | `#f7ead2` | selected node fill |

Body `ui-sans-serif`, ids/code `ui-monospace`. No new palette, no default font
stack. A two-pane `<main>` grid holds `#map` (exploration tree) and `#detail`
(drill-down); its arrangement is user-selectable via a header toggle (see
**Layout modes** below). Below 800px it always collapses to a single column.

### Layout modes

The map and detail panes arrange in one of two user-selectable modes, chosen
with a segmented **stack | split** toggle in the header toolbar:

- **Stack** (default) — map on top at full viewport width, detail below
  (`grid-template-rows: 2fr minmax(180px, 1fr)`). Matches the naturally
  wide-and-short exploration DAG so the graph uses the full width instead of
  being squeezed into a narrow column (issue #9).
- **Split** — map left, detail right (`grid-template-columns: minmax(320px, 38%)
  1fr`); the pre-#9 side-by-side behaviour, kept as an opt-in.

The choice is a `LayoutMode` signal in `App` (session-only, survives manifest
swaps like the other view-state signals) applied as a `.layout-stack` /
`.layout-split` modifier class on `.app-main`, which also flips the draggable
divider between the split (vertical) and stack (horizontal) axes (see
**Resizable panels** below). Narrow viewports (≤ 800px) stack regardless of the
selected mode.

### Resizable panels

The map/detail divider is a **draggable gutter**, not a static border: users
drag it to rebalance the panes — left/right in split, up/down in stack. The
mechanism is a single CSS custom property `--split` written on `<main>`; the
stylesheet keeps ownership of `grid-template-*` via `var(--split, <default>)`,
so the `@media (max-width: 800px)` collapse still wins in **pure CSS** (it
overrides `grid-template-*` directly and hides the gutter). No `matchMedia`, no
viewport signal, no listener lifecycle.

- **Two per-mode ratio signals** in `App` — `split_ratio` (default **0.38**) and
  `stack_ratio` (default **0.667**), matching the pre-resize proportions. Two
  signals because a column fraction and a row fraction aren't comparable across
  axes, so a single shared ratio would bleed one into the other on mode toggle.
  Both are plain `RwSignal`s — **in-memory only**, reset to the default on
  reload (correct for arahub multi-ARA hub mode; no per-ARA storage).
- **Gutter-aware ratio math** — the draggable line is the gutter *centre*
  (`pane1 + gutter/2`), so the pointer→ratio conversion subtracts `gutter/2` and
  `clamp_split_ratio` takes `gutter_px`; the floors aren't off by the gutter
  width. Pane floors are **structural** — `minmax()` on *both* grid tracks — so
  they hold on initial render, reset, mode-toggle, and resize, not just during a
  drag: split → map ≥ 320px / detail ≥ 240px; stack → map ≥ 180px / detail ≥
  180px.
- **Accessible WAI-ARIA window-splitter** — the gutter is `role="separator"`,
  `tabindex=0`, with `aria-orientation` per mode and `aria-valuenow` +
  *reachable-bounds* `aria-valuemin`/`aria-valuemax` (the clamp window, not a
  nominal 0–100). Arrow keys step ±0.02 along the mode axis, `Home`/`End` jump to
  the clamp ends, and both key and pointer paths run through the same
  `clamp_split_ratio`. Double-click resets the active mode's ratio to its default
  — the only recovery path given the in-memory ratio.
- **Reactive drag state, global body lock** — a `dragging` signal is folded into
  `<main>`'s reactive class closure (never patched imperatively, which Leptos
  would wipe on re-render). During a drag a `body.is-resizing` class sets both
  `user-select: none` and the resize cursor (on `body`, not `.app-main`), so a
  fast drag over the header keeps the cursor and selects no text. The pointer
  handlers release pointer capture and clear the lock on **both** `pointerup` and
  `pointercancel`, so a cancelled drag can't strand the global lock.
- **Thin line, wide hit area** — the painted divider is a 1px `::before`; the
  interactive track is `--gutter` (6px mouse, 24px under `@media (pointer:
  coarse)`). Hover/focus brightens the 1px line with the app-standard `0.12s`
  transition using a mid-tone; the full `--accent` fill is reserved for the
  active-drag state.

The pure resize core and the `Splitter` component live in `splitter.rs`;
`clamp_split_ratio` and the keyboard-step helper are native-unit-tested (both
mode floor sets, gutter subtraction, midpoint pass-through, degenerate axis, and
key-step/pointer parity), and the headless-chrome layer covers the drag,
keyboard, double-click, per-mode preservation, body-lock cleanup, and the
mandatory <800px single-column-collapse regression.

### Display modes

The `#map` pane renders the exploration graph in one of two user-selectable
modes, chosen with a segmented **graph | tree** toggle in the header toolbar
(a `DisplayMode` signal in `App`, session-only like `LayoutMode`):

- **Graph** (default) — the interactive SVG DAG with pan/zoom (issue #7 keeps
  this as the default; see [`stage-3-svg-spike.md`](stage-3-svg-spike.md)).
- **Tree** — the published `research-visualizer` **DOM indented tree-list**, an
  exact reproduction of the static `trajectory.html` scaffold. Rows show a kind
  glyph, the node id, an optional `⇠ id` dependency marker, and the node title;
  children nest in indented `.kid` containers with a spine. A dead-end row is
  struck through in `--warn`. Roots whose node carries `isolated: true` render
  inside a trailing "isolated subtree" box (`.isobox`); the demo has none, so no
  box ships today. Hovering a row highlights the rows it depends on
  (`.deptarget`). The tree rows label off `title ?? body ?? "(untitled)"` and
  drive the same shared `selected` signal as the graph, so the detail pane and
  filter dimming work identically in both modes. All tree CSS is scoped under
  `.tree-map`, using the reference class names (`.node`/`.glyph`/`.dep`/`.kid`/
  `.sel`/`.dim`) without colliding with the SVG graph.

Both toggles are built from one generic `seg_toggle` control (a `SegMode`
trait), so `LayoutToggle` and `DisplayToggle` share one implementation of the
segment loop + `is-active`/`aria-pressed`/`data-mode` a11y.

The `Node.isolated` flag that drives the tree partition is a logical (not
geometry) manifest field, serde-default `false` so old manifests round-trip
unchanged — see [`manifest-schema.md`](manifest-schema.md).

The kind glyphs are the published set — `question 'Q'`, `experiment '✦'`,
`decision '→'`, `dead_end '✗'`, `insight '!'`, other `'•'` — read from the same
`kind_meta` single source of truth by both renderers, so the SVG graph and the
tree-list show identical glyphs.

### Replay stepper

A **replay stepper** in the toolbar (`‹` / `▶ Replay`⇄`⏸ Pause` / `›`) steps the
shared `selected` signal through node order (`manifest.nodes`, i.e. pre-order
DFS), and works in **both** display modes. Prev/next stop any running replay
first, then move one step, clamping at both ends with no wrap — stepping
"previous" from no selection lands on the first node (the reference clamp
`indexOf(None) = -1`). Play toggles a 1300 ms interval that advances each tick
and auto-stops at the last node (no loop); the interval is torn down on pause,
on reaching the last node, and on unmount. The `←` / `→` arrow keys mirror
prev/next via a document listener, guarded to ignore keystrokes while a search
`<input>`/`<select>` is focused. A shared `#rstat` readout in the toolbar shows
`step {i} / {N}` when a node is selected, else `{shown} / {N} steps` for the
active filter — the same span the reference shares between filter and replay.

## Architecture

Crate layout (`lib` + `bin` so components are import-testable):

| Module | Responsibility |
|--------|----------------|
| `lib.rs` | `App` shell + `MapPane`; owns the view-state signals (incl. per-mode `split_ratio`/`stack_ratio` + `dragging`) |
| `main.rs` | 8-line binary → `ara_viewer::mount()` |
| `kind.rs` | `kind_meta(&NodeKind)` — the single source of truth for wire css class / glyph / badge |
| `state.rs` | `LoadState`, `MapSurface`/`map_surface`, `safe_viewbox`, `PanZoom`/`ViewState`/`apply_manifest`, `LayoutMode`, `parse_manifest` |
| `source.rs` | `ManifestSource` seam + the wasm-only `fetch_manifest` |
| `scene.rs` | pure scene model + `GraphRenderer` trait + `SvgRenderer` + the interactive `GraphView` |
| `detail.rs` | pure `detail_model` + the `DetailPane` component |
| `filter.rs` | `FilterState` + the pure `node_matches` predicate |
| `splitter.rs` | pure `clamp_split_ratio` + keyboard-step math + the draggable `Splitter` (WAI-ARIA window-splitter) |
| `toolbar.rs` | the header `Toolbar` (filters) + `LayoutToggle` (stack/split) components |
| `canvas.rs` | `CanvasRenderer` stub (the fps contingency, unbuilt) |

Three ideas keep the stage honest and Stage-4-ready:

- **`GraphRenderer` = pure scene-model trait**, not view-returning. `scene(&Manifest, &LayoutView)`
  computes a `GraphScene { nodes:[{id, rect, kind, glyph, …}], edges:[{path,
  link_kind}], bounds }`; `SvgRenderer` renders that scene to SVG; a future
  `CanvasRenderer` renders the same scene to canvas. **Edge paths are derived
  client-side** from a `NodeId → pos` map (there is no `Link.route` in the
  schema); a node/edge with `pos: None` or an unknown endpoint is skipped, never
  panicked. Scene compute is pure → native-unit-tested (no browser).
- **`ManifestSource` seam** — `Static(url)` (fetch `manifest.json`) now;
  `Api { endpoint, live }` documented for Stage 4. `apply_manifest` preserves
  selection + pan/zoom, making the Stage-4 live-reload survival promise concrete.
- **Schema is authoritative.** All rendering binds to the frozen Stage-2
  `Manifest`. CSS classes bind to the serde **snake_case wire form**
  (`dead_end`, `depends_on`, …) via `kind_meta`. For `NodeKind::Other(raw)` the
  CSS class is the fixed sanitized token `other`; the raw string is display text
  only, never a class name.

View-state (`selected`, `pan_zoom`, `filter`) lives as signals in `App`, above
the scene render, so a Stage-4 manifest swap will not reset it.

## Render behaviour

- **Graph (`#map`)** — one interactive `<g>` per node: glyph chip + `label ?? id`
  clamped to 2 lines + ellipsis (full text in `<title>`); kind badge; `dead_end`
  coloured + strikethrough. `Child` edges solid, `DependsOn` dashed. Selected
  (`--sel-bg` + accent border) and focus (distinct dashed ring) states. Pan/zoom
  is a single reactive `viewBox` update (wheel zoom clamped 0.2–5.0, pointer-drag
  pan). Nodes are `tabindex=0`, `role="button"`, `aria-label = "label, kind"`;
  Enter/Space selects.
- **Detail (`#detail`)** — header (mono node id, `label ?? id` title, kind
  chip+badge, `support_level` pill, `isolated` pill when the node belongs to an
  isolated subtree) → description → per-kind typed fields in canonical order
  (`Experiment`: result; `Decision`: choice → rationale → alternatives; `DeadEnd`:
  `why_failed` as the primary `.block.reason` lead; `Question`/`Insight`/`Other`:
  none) → evidence notes + claims resolved through `bindings` with
  supported/refuted/hypothesis status pills → BUILT ON chips → DEPENDS ON jump
  chips (outgoing + "depended on by"; clicking a chip selects that node) → RESULT
  (exhibit chips, `description` captions, rendered bodies) → `source_refs`
  provenance chips. The evidence / built-on / depends-on / result / sources
  blocks are collapsible `<details open>` with a right-aligned item count (#58).
  Empty nodes show "Nothing recorded for this node."; every block is omitted when
  its data is absent. Richer blocks (quote/figure/table/diff/glossary/recipe)
  are skinned but **inert pending `T-REAL-CORPUS`**. No LLM is called at view time.
- **Toolbar** — case-insensitive search (label/id/kind/bound-claim text), a type
  `<select>` (options derived from kinds present), and a "dead ends only"
  checkbox. Non-matching nodes are **dimmed** (`.node.dimmed`), never removed, so
  the graph shape stays stable. Selection is independent of the filter.

### Interaction states (all handled)

| Surface | State | User sees |
|---------|-------|-----------|
| manifest fetch | loading | "Loading artifact…" skeleton |
| manifest fetch | load failure | error card: "Couldn't load manifest" + reason |
| graph | empty (`nodes: []`) | "No nodes in this artifact." + safe `viewBox` |
| node | `pos: None` | skipped in scene compute; never panics |
| detail | no selection | "Select a step on the left." |
| detail | node with no fields | "Nothing recorded for this node." |
| node | `Other`/unknown kind | neutral glyph + raw-kind badge (fixed `other` class) |

## Tests & CI

Two layers, split by the eng review:

- **Native `cargo test`** (no browser) — the pure logic: `kind_meta`, scene
  compute / edge derive / `pos:None` skip, `map_surface` + `safe_viewbox`,
  `apply_manifest` preservation, `detail_model` per-kind ordering + claim
  resolution + degradation, and the `node_matches` filter predicate; plus a
  round-trip that the checked-in `manifest.json` parses into `Manifest`. All
  browser-only code (`fetch_manifest`, `gloo-net`) is `#[cfg(target_arch = "wasm32")]`
  so the native workspace build stays green.
- **`wasm-bindgen-test` headless-browser layer** (`tests/web.rs`, gated to
  wasm32) — mounts `GraphView`/`DetailPane` with a synthetic manifest and asserts
  node/edge counts + classes, node a11y attributes, click→detail sync,
  search→dimming sync, the per-kind detail hierarchy, bound-claim rendering, and
  degradation. Run in CI by the `viewer-web-test` job (`wasm-pack test --headless
  --chrome`).

**Build gates.** A `[profile.wasm-release]` (`opt-level="z"`, `lto="fat"`,
`codegen-units=1`, `panic="abort"`, `strip`) is selected only for release/CI via
`TRUNK_BUILD_CARGO_PROFILE`, so `trunk serve` dev stays fast. `wasm-opt -Oz`
runs in the Trunk build (`data-wasm-opt="z"` + `--enable-bulk-memory` params).
The `viewer-size` CI job fails if `dist/*_bg.wasm` exceeds the budget. Measured:
**291,879 B uncompressed / 106,753 B brotli** (budgets 1 MB / 350 KB).

**Renderer decision** ([`stage-3-svg-spike.md`](stage-3-svg-spike.md)): keep
SVG. The demo emits ~139 SVG elements (largest known corpus ~290), far below the
few-thousand switch threshold, and pan/zoom held ~120 fps (single O(1) `viewBox`
update). The `CanvasRenderer` stays a stub.

## Deferred (tracked in `TODOS.md`)

- **`T-REAL-CORPUS`** — the richer detail blocks (quotes/figures/tables/diffs/
  glossary/recipes) are skinned but inert until the schema widens.
- **`T-GRAPH-KBD-NAV`** — arrow-key spatial graph traversal (search/filter + Tab
  is the keyboard nav this stage).
- **`T-VIEWER-DIST-PACKAGING`** — how `dist/` reaches users (`cargo install
  ara-cli` can't serve generated assets) is a `0.1.0` release / Stage-4 concern.
- **`T-WASM-CLIPPY`** — now actionable: Stage 3 added `#[cfg(target_arch =
  "wasm32")]` code, so a wasm-target clippy job is worth adding.
- Any server/HTTP, live reload, figure streaming — Stage 4 (this stage adds only
  the `ManifestSource::Static` seam so Stage 4 slots in without a rewrite).
