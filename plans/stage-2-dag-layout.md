# Stage 2 — Layered DAG Layout in `ara-core`

**PR target:** `stage2-dag-layout` → `main`. **Depends on:** Stage 1.
**Version bump:** `0.0.2 → 0.0.3`.

## Problem background

The exploration tree is a typed DAG that reads best with a layered (Sugiyama)
layout — the dagre/ELK family — not force-directed. Computing positions in
`ara-core` (not the client) keeps layout shared between native and wasm and
makes it snapshot-testable and byte-deterministic. This stage adds geometry to
the `Manifest` produced in Stage 1 and **freezes the wire schema**.

## Proposed solution

Integrate a pure-Rust dagre port behind a small `layout.rs` trait so the
concrete crate is swappable. Produce deterministic node positions + edge routes,
attach them to the `Manifest`, and expose `ara validate --layout` to exercise
the path. Pin the layout crate and its tie-break/rank options for determinism.

## Implementation steps

1. **Evaluate + pin the dagre port.** Add the chosen community crate (pinned
   exact). Wrap it behind a trait:
   ```rust
   pub trait Layout { fn layout(&self, m: &Manifest, opts: &LayoutOptions) -> LayoutResult; }
   ```
   so an ELK-style fallback can replace it without touching callers.
2. **`LayoutOptions`** with explicit, pinned determinism knobs: rank direction
   (top-down), node/rank separation, and a **fixed tie-break** (sort by `NodeId`)
   so equal-rank ordering is stable. No randomness anywhere.
3. **Extend `Manifest`** (additive only): `Node.pos: Option<Point>` and
   `Link.route: Option<Vec<Point>>`, plus `Manifest.bounds: Option<Rect>`.
   `Option` keeps Stage 1 snapshots valid when layout is off.
4. **`parse_and_layout(dir, opts) -> Manifest`** convenience that runs Stage 1
   parse then fills geometry. Cycles (if any) are broken deterministically and
   reported as warnings (a DAG should have none; guard anyway).
5. **CLI:** `ara validate --layout` runs layout and reports node/edge counts and
   bounds; add `ara layout <dir> --json` to dump positioned `Manifest` (this JSON
   is exactly what the Stage 3 client will consume).
6. **Freeze the schema.** Document the final `Manifest` JSON shape in
   `docs/manifest-schema.md`; any later change requires an explicit coordinated
   version bump across `ara-core` + client.

## Tests / verification

- Determinism: layout the corpus twice → byte-identical positioned JSON.
- `insta` snapshot of the positioned `Manifest` on the corpus.
- Invariants: all nodes get finite positions; no NaN; edges reference existing
  nodes; layered ranks are monotonic along edges (topological sanity).
- Scale probe: run on the largest corpus tree, record node count + timing (feeds
  the Stage 3 SVG-vs-canvas decision).

## Milestone / acceptance

Positioned `Manifest` is deterministic and snapshot-stable; `ara layout --json`
emits the frozen wire format; schema documented in `docs/`.

## Out of scope (deferred)

Rendering (Stage 3); canvas fast path (decided empirically in Stage 3).

## CHANGELOG (Unreleased → Added)

- Deterministic layered DAG layout in `ara-core`; positions/edge routes added to
  `Manifest`; `ara layout <dir> --json` and `ara validate --layout`.
