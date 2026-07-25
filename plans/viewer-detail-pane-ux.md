# Plan: Detail-pane UX parity with the deployed ara-hub viewer

Tracking issues: #55, #56, #57, #58, #59 (all viewer-only, all in
`crates/ara-viewer`). Reference: deployed hub viewer at
https://www.agenticresearch.sh/ara/AmberLJC/ara-paperbench/artifacts/speedrun/nanogpt-speedrun
(`research-visualizer` scaffold), design-of-record `docs/hub-parity.md`.

## Problem background

A side-by-side comparison of the `ara serve` viewer against the deployed
ara-hub trajectory viewer found that our detail pane drops data the schema
already carries and lacks the hub's cross-node navigation and keyboard UX:

1. **#55 — exhibit captions dropped.** `Exhibit.description` is parsed
   (`crates/ara-core/src/manifest.rs:232`) but `ExhibitView` never carries it;
   the RESULT block shows only `id · kind` chips + body
   (`crates/ara-viewer/src/detail.rs:552-575`).
2. **#56 — no node id, no isolated signal in the detail header.** The header
   shows title + kind chip + support pill only (`detail.rs:423-437`).
   `Node.isolated` only affects tree-mode isoboxes; graph mode and the detail
   pane give no isolated-subtree indication.
3. **#57 — no cross-node navigation.** `DependsOn` links exist only as dashed
   graph edges. The hub renders clickable dependency chips that jump-select the
   target node.
4. **#58 — blocks always expanded.** Table-heavy artifacts repeat large
   exhibits on many nodes; the hub wraps each block in `<details>` with a
   count tag.
5. **#59 — no global replay keys.** `←`/`→` work only inside the splitter,
   tree, and graph widgets; the hub binds them globally to replay stepping.

## Scope

In: #55, #56, #57, #58, #59 — one PR, all confined to `crates/ara-viewer`
(+ CSS in `public/styles.css`). No schema changes, no `ara serve` changes.

Out (separate issues): #60 figure images (needs serve/payload work), #61–#63
schema widening, #64 reasoning narrative (blocked on upstream #12).

## Proposed solution

### #55 Exhibit captions (`detail.rs`, `styles.css`)

- Add `description: Option<String>` to `ExhibitView`; populate in
  `detail_model` from `Exhibit.description`.
- Render as a `.exhibit-caption` element (muted mono, matching the hub's
  `figcaption` treatment) directly above each rendered `.exhibit-body`, and
  skip when absent. Chips row unchanged.

### #56 Node id + isolated pill (`detail.rs`, `styles.css`)

- Add `id: String` and `isolated: bool` to `DetailModel`.
- Render the id in muted mono in `.detail-meta` (before the kind chip).
- `isolated` = the node's own flag OR membership in an isolated subtree.
  Tree mode already computes subtree membership for isoboxes — locate that
  logic (`tree.rs` / `scene.rs`) and extract a shared helper
  (`fn is_in_isolated_subtree(node, manifest) -> bool`) rather than
  duplicating it. Render an `isolated` pill (neutral `--iso-*` tokens) when
  true.

### #57 Depends-on jump chips (`detail.rs`, `lib.rs`)

- Resolve `manifest.links` where `kind == DependsOn` and `from == id`
  ("depends on") or `to == id` ("depended on by") into id + label chips in a
  new `.block.deps-block`, placed after BUILT ON, matching hub block order.
- `DetailPane` currently only reads the selection signal; pass a selection
  setter (or callback) down from `lib.rs` so clicking a chip sets selection to
  the target node. The existing selected-node ring/fill in both display modes
  is the highlight — no separate flash animation in v1.
- Chips for unknown target ids render non-clickable (defensive; cannot happen
  after validation but must not panic).

### #58 Collapsible blocks (`detail.rs`, `styles.css`)

- Convert the EVIDENCE, BUILT ON, RESULT, and SOURCES blocks (not the
  description or primary "what it did" block) to `<details open>` +
  `<summary>`, reusing the existing `.block-label` styling on the summary.
- Right-aligned muted count tag in each summary: claims+notes count, built-on
  count, exhibits count, sources count.
- No persisted state; collapse state resets on selection change (native
  `<details>` behaviour on re-render is acceptable).

### #59 Global replay arrow keys (`replay.rs`, `lib.rs`)

- Extract the ‹ / › button handlers into a shared `step_selection(delta)`
  helper that steps through the current (filtered) order.
- Register a window-level `keydown` listener (Leptos `use_event_listener` on
  `window`): `ArrowLeft`/`ArrowRight` call `step_selection(-1/+1)`.
- Guards: ignore when `ev.target` is an `input`/`select`/`textarea`, when a
  modal panel is open, or when focus is inside the splitter separator (its own
  arrow-key handling takes precedence — the separator only handles keys when
  focused, so no conflict by construction).

## Implementation steps

1. #55 captions (smallest; establishes `ExhibitView` change pattern).
2. #56 id + isolated pill (shared helper extraction first, with a unit test).
3. #58 collapsible blocks (pure view/CSS; lands before #57 so the new deps
   block is born collapsible).
4. #57 jump chips (selection setter plumbing + new block).
5. #59 global keys (replay helper extraction + listener).
6. Update `docs/stage-3-viewer.md` (detail-pane section) and
   `docs/hub-parity.md` (mark these parity items landed).
7. Bump patch version in `Cargo.toml` + `CHANGELOG.md` entry (functional
   change to the shipped viewer). Regenerate the embedded viewer bundle via
   `scripts/embed-viewer.sh` so the `viewer-embed-fresh` CI job passes.

## Testing

- `detail.rs` unit tests (existing pattern): caption present/absent; id always
  rendered; isolated pill for root and inherited-child; deps block
  omitted/both-directions/unknown-target; counts in summaries.
- Native tests for the pure helpers (`is_in_isolated_subtree`,
  `step_selection` order math).
- `cargo test` workspace + manual smoke:
  `cargo run -- serve` on a fixture artifact and on a local copy of the
  nanogpt-speedrun ARA if obtainable; verify graph + tree modes, <800px
  responsive, wasm build (`trunk build`) under the CI size budget (no new
  deps expected, so budget impact ~0).

## Risks / notes

- No new crates → wasm bundle size and `viewer-size` CI gate unaffected.
- All additions are additive to `DetailModel`; no manifest or wire-format
  changes, so old artifacts render identically except where they already
  carry the newly-surfaced data (captions).
- #57 chip ordering: preserve `manifest.links` order for determinism
  (matches the codebase's order-preservation convention).
