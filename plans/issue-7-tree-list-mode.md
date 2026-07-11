# Issue #7 — Viewer: DOM tree-list as an alternate display mode + replay stepper

## Problem background

The Stage-3 viewer (`crates/ara-viewer`) renders the exploration graph as an
**SVG DAG** with pan/zoom (`GraphView` + the pure `scene.rs` model). The
published reference — `ARA-Labs/ARA-Demo`'s `research-visualizer` scaffold
(`nanogpt_ara/trajectory.html`) — instead renders a **DOM indented tree-list**
and ships a **replay stepper** and **layer-panel overlays** we don't have.

Stage 3 deliberately chose the SVG-DAG hybrid (eng + design reviewed) and named
the DOM tree-list as the documented pivot. Issue #7's decision: **keep the SVG
graph as the default, and add the published DOM tree-list as an alternate
display mode** (a Graph ⇄ Tree toggle) plus the replay stepper, so the viewer
can match the published ARA interaction/display when desired. This is
**additive** — the SVG graph and the Stage-2 layout stay untouched.

### Scope decision (confirmed with human dev)

- **This PR ships parts 1–3** of issue #7: the display-mode toggle, the DOM
  tree-list mode, and the replay stepper. All three are user-visible and
  testable against today's `Manifest`.
- **Part 4 (layer panels + abstract) is deferred** to the `T-REAL-CORPUS` PR
  that actually widens the schema to carry context / glossary / dependencies /
  recipes / abstract. There is nothing to render inertly that isn't already a
  no-op today, so we do not add dead layer-panel chrome now. The reference
  tokens part 4 needs (`--code-bg --reason-bg --iso-*` etc.) are added only as
  far as the tree-list itself uses them (`--iso-*`); the diff/scrim/shadow
  tokens land with part 4.
- **Tree CSS classes** use the **published reference names** (`.node`, `.kid`,
  `.nid`, `.ntitle`, `.isobox`, `.deptarget`, `.dim`) but are **scoped under a
  `.tree-map` container** so they never collide with the SVG graph's existing
  `.graph-svg .node` / `.node.dimmed` rules.

## Reuse (already built, display-agnostic — carries over unchanged)

`kind::kind_meta`, `detail.rs` (`DetailPane` + `detail_model`), the
`filter::node_matches` predicate, and the shared `selected` / `filter` /
`pan_zoom` / `layout` signals in `App`. The pure `scene.rs` model stays for
Graph mode. `ManifestSource` and the live-reload path are untouched.

## Proposed solution

### 1. `DisplayMode` value type (`state.rs`, native-testable)

Mirror the existing `LayoutMode` pattern exactly:

```rust
/// Which renderer the `#map` pane uses for the exploration graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode {
    /// Today's interactive SVG DAG (pan/zoom). The default.
    #[default]
    Graph,
    /// The published DOM indented tree-list.
    Tree,
}
```

with `css_class()` (`"display-graph"` / `"display-tree"` — unused by CSS today
but kept for symmetry/future), `as_token()` (`"graph"` / `"tree"`), and
`from_token()` (unknown → `Graph`). Unit tests match the `LayoutMode` tests:
default, token round-trip, unknown-token fallback.

A `display: RwSignal<DisplayMode>` signal is owned by `App` alongside `layout`
(session-only; survives manifest swaps).

### 2. `DisplayToggle` control (`toolbar.rs`)

A second segmented two-button group (`graph | tree`), structurally identical to
`LayoutToggle`, rendered in the header `.toolbar-area` before `LayoutToggle`.
Reuses the existing `.layout-toggle*` CSS classes (rename the CSS comment to
"segmented control", the class names already read generically) so no new toggle
skin is needed — or a shared `.seg-toggle` class if cleaner. Active segment gets
`is-active` + `aria-pressed="true"`; `data-mode` carries the token for tests.

### 3. Pure tree model (`tree.rs`, new module, native-testable)

A pure builder — no `web-sys`, fully unit-tested on native — that turns a
`&Manifest` into a renderable forest:

```rust
pub struct TreeRow {
    pub id: NodeId,
    pub label: String,          // label ?? id
    pub glyph: char,
    pub css_class: &'static str, // from kind_meta
    pub badge: String,
    pub is_dead_end: bool,
    pub dep_targets: Vec<NodeId>, // outgoing DependsOn edges, source order
}
pub struct TreeNode { pub row: TreeRow, pub children: Vec<TreeNode> }
pub struct TreeModel { pub roots: Vec<TreeNode>, pub isolated: Vec<TreeNode> }

pub fn tree_model(manifest: &Manifest) -> TreeModel;
```

Build rules (deterministic, source-order preserving):

- **Child adjacency** from `LinkKind::Child` links: `from → [to…]` in link
  source order.
- **Roots** = nodes (in `manifest.nodes` order — already pre-order DFS) with no
  incoming `Child` edge.
- Each root is expanded recursively via the child map into a `TreeNode`. A
  **visited set guards against cycles** (a malformed manifest with a Child cycle
  must not infinite-loop — a node already visited on the current path is not
  re-expanded).
- **`dep_targets`** per row = the `to` ids of that node's outgoing
  `LinkKind::DependsOn` links, in source order.
- **Isolated partition:** the **first root** heads the main exploration tree →
  `roots[0]`. **Every other root** is an isolated subtree → grouped into
  `isolated`, matching the reference `.isobox` ("isolated subtree") behaviour
  (the ARA norm is a single root question; disconnected roots are stragglers).
  *(Open decision for review — see below.)*
- Empty manifest → empty `TreeModel`.

Unit tests: single-tree nesting + depth; multiple roots → first is main, rest
isolated; `dep_targets` populated from DependsOn only (not Child); dead-end row
flagged; cycle guard terminates; `label ?? id` fallback; a round-trip against
the checked-in `public/manifest.json` (asserts the ResNet demo's known root +
node count).

### 4. `TreeView` component (`tree.rs`)

Renders a `TreeModel` as scoped DOM inside `.tree-map`:

- Recursive `render_subtree(&TreeNode) -> AnyView`: emits a `.node` flex row
  (`.chip` glyph + `.nid` id + `.ntitle` title) then, when it has children, a
  nested `<div class="kid">` holding the recursively-rendered children.
- `.node.dead` (dead-end) strikes through `.ntitle` (scoped rule, not the SVG
  one).
- Isolated roots render inside a trailing `<div class="isobox">` with a small
  "isolated subtree" caption.
- **`depends_on`** rendered as a quiet `⇠ id` marker (`.dep-marker`) at the end
  of the row for each `dep_target`.
- **Selection:** each row is `tabindex=0`, `role="button"`, `aria-label =
  "label, kind"`; `on:click` / Enter / Space set the shared `selected` signal →
  the existing `DetailPane` updates. The selected row gets `.selected`.
- **Filter dimming:** reuse the `matching: Memo<HashSet<NodeId>>` already built
  in `MapPane`; rows whose id is not in the set get `.dim`. A live
  **`X / Y steps`** count (`X` = matching, `Y` = total) renders at the top of
  `.tree-map` (reference behaviour). *(Count shown in Tree mode only this PR;
  wiring it into Graph mode too is a trivial later addition.)*
- **Dependency hover highlight:** a `hovered_deps: RwSignal<HashSet<NodeId>>`
  local to `TreeView`; `on:pointerenter`/`on:pointerleave` on a row set/clear it
  to that row's `dep_targets`. Rows whose id is in the set get `.deptarget`
  (dashed accent outline). Keyboard-only users still get the `⇠ id` text marker.

### 5. `MapPane` — branch on `DisplayMode`

`MapPane` gains a `display: RwSignal<DisplayMode>` prop. The `MapSurface::Graph`
arm (nodes present) becomes: build the shared `matching` Memo once, render the
**`ReplayBar`** (step 6) above, then switch on `display.get()`:

- `Graph` → today's `GraphView` (+ the pan/zoom map-hint).
- `Tree` → `TreeView` (+ the `X / Y steps` count).

Loading / Error / Empty surfaces are unchanged and mode-independent.

### 6. Replay stepper (`replay.rs` pure helpers + `ReplayBar` component)

Works in **both** modes; steps the shared `selected` signal through node order.

Pure (native-testable):

```rust
pub enum Step { Next, Prev }
pub fn node_order(manifest: &Manifest) -> Vec<NodeId>; // manifest.nodes order (pre-order DFS)
pub fn step(order: &[NodeId], current: Option<&NodeId>, dir: Step) -> Option<NodeId>;
pub fn counter(order: &[NodeId], current: Option<&NodeId>) -> (usize, usize); // (i, N), i is 1-based, 0 when no selection
```

- `step` from `None`: `Next` → first, `Prev` → last. At an end it **clamps**
  (does not wrap) so the counter reads naturally. Unknown current id → treat as
  no selection.
- `ReplayBar` component: `‹` (prev) / `▶`|`⏸` (play/pause) / `›` (next) buttons +
  a `step i / N` counter. Buttons update `selected` via `step`. Play toggles an
  interval (`web_sys` `set_interval`, wasm-only, ~1.1 s) that advances until the
  last node, then auto-stops. Interval setup/teardown is `#[cfg(target_arch =
  "wasm32")]`; on native the play button is inert (component still compiles).
- **`←` / `→` keys:** a window-level `keydown` listener (wasm-only, installed in
  `App` via an effect) maps ArrowLeft/ArrowRight to `step` Prev/Next, **guarded**
  so it is ignored when the event target is the search `<input>` (so typing in
  the filter box isn't hijacked).

Unit tests (native): `node_order` equals `manifest.nodes` ids; `step` first/last
/ clamp-at-ends / from-None / unknown-id; `counter` 1-based + `(0, N)` when
unselected.

### 7. `styles.css` — scoped tree-list skin + `--iso-*` tokens

- Add `--iso-line`, `--iso-bg`, `--iso-ink` tokens (reference values) for the
  isobox.
- Add a `.tree-map` block: `.tree-map .node` (flex row, gap, padding, hover
  bg), `.tree-map .kid` (left indent + faint spine), `.nid` (mono, muted),
  `.ntitle`, `.node.dead .ntitle` (line-through), `.chip` (reuses `--glyph-bg`;
  `.node.dead .chip` → `--warn`), `.dep-marker` (quiet muted `⇠`), `.deptarget`
  (dashed accent outline), `.node.dim` (opacity), `.node.selected` (sel-bg +
  accent), `.isobox` (`--iso-*`), and the `.step-count` readout.
- Add `.replay-bar` + button skin (reuse toolbar/segment tokens).
- All tree rules are **prefixed with `.tree-map`** so `.node` etc. never touch
  the SVG graph. The `≤800px` responsive rules already stack the panes and need
  no tree-specific change.

### 8. Docs

- Add a **"Display modes"** section to `docs/stage-3-viewer.md` (next to the
  existing "Layout modes"): Graph (SVG DAG, default) vs Tree (DOM tree-list),
  the toggle, and the replay stepper.
- Note the tree model's root/isolated rule and that `depends_on` shows as `⇠ id`
  + hover `.deptarget`.
- After merge, per `AGENTS.md`, fold this plan into the design doc and remove it
  from `plans/`.

## Architecture summary (new/changed files)

| File | Change |
|------|--------|
| `state.rs` | + `DisplayMode` enum + tests |
| `tree.rs` | **new** — pure `tree_model` + `TreeView` component + tests |
| `replay.rs` | **new** — pure `node_order` / `step` / `counter` + `ReplayBar` + tests |
| `toolbar.rs` | + `DisplayToggle` component |
| `lib.rs` | + `display` signal; pass to `MapPane`; render `DisplayToggle`; wasm-only ←/→ key listener; branch `MapPane` on mode + render `ReplayBar` |
| `public/styles.css` | + `.tree-map` scoped skin, `.replay-bar`, `--iso-*` tokens |
| `tests/web.rs` | + tree render / toggle / replay browser tests |
| `docs/stage-3-viewer.md` | + "Display modes" section |

## Implementation steps

1. `DisplayMode` in `state.rs` + native tests.
2. `tree.rs`: pure `tree_model` + `TreeRow`/`TreeNode`/`TreeModel` + native
   tests (build, isolated partition, deps, cycle guard, demo round-trip).
3. `replay.rs`: pure `node_order` / `step` / `counter` + native tests.
4. `TreeView` component in `tree.rs`; `ReplayBar` in `replay.rs`.
5. `DisplayToggle` in `toolbar.rs`.
6. Wire `lib.rs`: `display` signal, `MapPane` mode branch, `ReplayBar`, header
   toggle, wasm-only key listener.
7. `.tree-map` scoped CSS + `.replay-bar` + `--iso-*` tokens in `styles.css`.
8. Browser tests in `tests/web.rs`: tree rows + nesting + `.kid`, dead
   strikethrough class, `.isobox` present, `⇠` dep marker, `DisplayToggle`
   flips + swaps the rendered surface, replay next/prev updates `selected`.
9. `cargo build`, `cargo test --workspace`, `wasm-pack test --headless --chrome
   crates/ara-viewer`.
10. Regenerate the embedded viewer bundle (`scripts/embed-viewer.sh`) so
    `ara serve` ships the new UI; the `viewer-embed-fresh` CI check requires it.
11. Bump patch version in `Cargo.toml` + `CHANGELOG.md` `[Unreleased]` entry.
12. Update `docs/stage-3-viewer.md`.

## Scope / risk

Additive, medium size. No changes to `ara-core`, the manifest schema, the
Stage-2 layout, the `scene.rs` graph model, or the Stage-4 server. Graph mode is
byte-for-byte the current default. New surface area: one enum, one pure tree
builder, one pure replay helper set, two components (`TreeView`, `ReplayBar`),
one toggle, and a scoped CSS block. Main risks: (a) CSS class collision — fully
mitigated by the `.tree-map` scope; (b) the ←/→ listener hijacking search input
— mitigated by the target guard; (c) the play-interval leaking — mitigated by
tearing the interval down on pause / unmount / reaching the last node.

## Decisions to confirm in review

1. **Isolated-subtree rule.** Proposed: first root = main tree, all other roots
   grouped into the isobox. Alternative: only lone childless roots are isolated
   (multi-node disconnected trees render as their own top-level trees). The
   reference treats the primary root's component as the main tree, which matches
   the proposal; confirm before implementing.
2. **`X / Y steps` count in Graph mode too?** Proposed Tree-only this PR to keep
   scope tight. Trivial to also show in Graph mode if wanted.
3. **Play speed / autoplay.** Proposed ~1.1 s per step, auto-stops at the last
   node, no looping. Confirm the interval feel.
