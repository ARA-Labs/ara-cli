# Issue #9 — Selectable viewer layout modes (stack vs. split)

## Problem background

The viewer's `.app-main` hard-codes a side-by-side two-column grid
(`grid-template-columns: minmax(320px, 38%) 1fr`) for the `#map` (graph) and
`#detail` panes. On wide screens this wastes space badly: the naturally
wide-and-short exploration DAG is squeezed into the ~38% left column while the
detail pane fills the right ~62% with mostly-empty space (see issue #9).

Issue #9 proposed simply flipping the layout to vertical. The human developer
refined the direction: **don't replace one fixed layout with another — offer two
layout modes and let the user pick, with the vertical "stack" as the default.**

- **Stack** (default): map on top (full viewport width, matching the wide DAG
  shape), detail pane below it. `grid-template-rows`.
- **Split** (previous behaviour): map left, detail right. `grid-template-columns`.

## Proposed solution

Introduce a `LayoutMode` value type + a shared signal owned by `App`, a small
selector control in the header toolbar area, and a modifier class on `.app-main`
that swaps the grid axis (plus the panel divider border). CSS-and-a-thin-state
change only; no changes to the graph renderer, detail model, or Stage-4 server.

### 1. `state.rs` — `LayoutMode` value type (native-testable)

```rust
/// Which way the map/detail panes are arranged in `.app-main`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutMode {
    /// Map on top (full width), detail below. The default — matches the
    /// wide-and-short DAG shape. `grid-template-rows`.
    #[default]
    Stack,
    /// Map left, detail right. `grid-template-columns`.
    Split,
}

impl LayoutMode {
    /// CSS modifier class applied to `.app-main`.
    pub fn css_class(self) -> &'static str {
        match self {
            LayoutMode::Stack => "layout-stack",
            LayoutMode::Split => "layout-split",
        }
    }
    /// Wire token used by the toolbar `<select>` value / round-trip.
    pub fn as_token(self) -> &'static str {
        match self {
            LayoutMode::Stack => "stack",
            LayoutMode::Split => "split",
        }
    }
    /// Parse a `<select>` token back to a mode; unknown → default (Stack).
    pub fn from_token(s: &str) -> Self {
        match s {
            "split" => LayoutMode::Split,
            _ => LayoutMode::Stack,
        }
    }
}
```

Unit tests: `default() == Stack`, `from_token` round-trips both variants and
falls back to `Stack` on garbage, `css_class`/`as_token` mappings.

### 2. `App` (`lib.rs`) — own the signal, apply the class

- `let layout: RwSignal<LayoutMode> = RwSignal::new(LayoutMode::default());`
  (survives manifest swaps, like `filter`/`pan_zoom`/`selected`).
- `.app-main` class becomes reactive:
  `class=move || format!("app-main {}", layout.get().css_class())`.
- Render a `LayoutToggle` control in the `.toolbar-area` (before the filter
  `Toolbar`, so filters stay right-aligned).

### 3. `LayoutToggle` control (new component in `toolbar.rs`)

**Decision (human dev):** a segmented two-button group (not a `<select>`):

```
[ ▭ stack | ▯ split ]
```

- A `role="group"` wrapper (`.layout-toggle`) with two `<button>`s.
- Each button: `on:click` → `layout.set(LayoutMode::Stack | Split)`;
  `class` carries `is-active` when it matches the current `layout.get()`;
  `aria-pressed` reflects active state.
- New CSS for the segmented control (`.layout-toggle` + `.layout-toggle button`
  + `.is-active`), skinned with the warm-cream tokens to match the toolbar.

### 4. `styles.css` — split base grid into two modifier classes

Replace the single hard-coded rule:

```css
.app-main {
  display: grid;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

/* Stack (default): map on top, detail below. */
.app-main.layout-stack {
  grid-template-rows: 2fr minmax(180px, 1fr);
  grid-template-columns: 1fr;
}

/* Split: map left, detail right (previous behaviour). */
.app-main.layout-split {
  grid-template-columns: minmax(320px, 38%) 1fr;
}
```

Panel divider must follow the axis:

- Base `.panel` keeps `border-right` (used by split).
- In stack mode, swap to a bottom border on the map panel and drop the
  right border:
  ```css
  .app-main.layout-stack .panel-map    { border-right: none; border-bottom: 1px solid var(--line); }
  .app-main.layout-stack .panel-detail { border-bottom: none; }
  ```
- `.panel-detail { border-right: none; }` stays for split.

The `.map-hint` (absolute, bottom-left of `.panel-map` via `position: relative`)
and the `≤800px` responsive media query already force a stacked single column —
that stays as-is (mobile is always stacked regardless of the chosen mode). The
detail row's `minmax(180px, 1fr)` keeps the empty "Select a step" placeholder
from reserving an oversized row.

### 5. Docs

Update `docs/stage-3-viewer.md` (or `stage-4-serve.md`, whichever documents the
layout) with a short "Layout modes" note; move this plan into `docs/` as the
design record after implementation, per `AGENTS.md`.

## Implementation steps

1. Add `LayoutMode` + unit tests to `crates/ara-viewer/src/state.rs`.
2. Add the `layout` signal + reactive `.app-main` class + `LayoutToggle` in
   `lib.rs` / `toolbar.rs`.
3. Rework the `.app-main` CSS into base + `.layout-stack` / `.layout-split`
   modifiers and the axis-aware divider borders.
4. Add a `wasm_bindgen_test` in `tests/web.rs`: mount `App` (or the toggle +
   `.app-main`) and assert the class flips `layout-stack` ↔ `layout-split` on
   select change, and that stack is the initial class.
5. `cargo build`, `cargo test --workspace`, and `wasm-pack test --headless
   --chrome crates/ara-viewer`.
6. Bump patch version (`Cargo.toml`) + `CHANGELOG.md` entry.

## Scope / risk

Additive, low-risk. One new value type, one signal, one control, and a CSS
refactor that preserves the old split layout as a selectable mode. No renderer,
detail-model, or server changes.

## Decisions (resolved with human dev)

- **Selector UI:** segmented two-button group (`stack | split`), not a
  `<select>`.
- **Persistence:** session-only. The signal survives manifest swaps within a
  session but resets to `Stack` on reload. localStorage persistence is out of
  scope for this step.
- **Split ratio:** keep the previous `38% / 1fr` for split and `2fr / 1fr` for
  stack; draggable splitter is out of scope.
