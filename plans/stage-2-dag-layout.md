# Stage 2 — Layered DAG Layout in `ara-core`

**PR target:** `stage2-dag-layout` → `main`. **Depends on:** Stage 1.
**Version bump:** `0.0.3 → 0.0.4` (workspace `Cargo.toml` is already at `0.0.3`
from issue #3; the plan's old `0.0.2 → 0.0.3` line was stale).

## Problem background

The exploration tree is a typed DAG that reads best with a layered (Sugiyama)
layout — the dagre/ELK family — not force-directed. Computing positions in
`ara-core` (not the client) keeps layout shared between native and wasm and
makes it snapshot-testable and byte-deterministic. This stage adds **node
geometry** to the `Manifest` produced in Stage 1 and freezes the **geometry**
wire shape (the logical model stays additively extensible — see step 6).

### Why layout in core, not client-side dagre.js (rationale — was implicit)

Native today is a headless CLI with no GUI, so the only *native* consumer of
positions is the snapshot test. Core-side layout is still the right call because:
one language for the whole pipeline; deterministic, server-side snapshot testing
of geometry; an offline `ara layout --json`; and no JS layout dependency in the
wasm client. **This is contingent on the step-1 cross-target spike passing**
(see step 1). If native≡wasm determinism cannot be achieved, fall back to
client-side dagre.js and keep `ara-core` logical-graph-only
(tracked as `T-LAYOUT-SPIKE-FALLBACK`).

## Proposed solution

Integrate a pure-Rust Sugiyama/dagre-port layout as a **plain function** in
`layout.rs` (no trait — one impl, one caller; YAGNI). Produce deterministic
**node positions + bounds** and attach them to the `Manifest`. Edge *routing*
is **deferred to the Stage 3 client** (`Link.route` is NOT added this stage).
Pin the layout crate and its tie-break/rank options for determinism.

```
parse_sources / parse_dir            layout.rs (NEW, wasm-safe)
   (Stage 1, unchanged)         ┌─────────────────────────────────┐
        │                       │ layout(&Manifest, &LayoutOptions) │
        ▼                       │   -> LayoutResult                 │
 Result<(Manifest, Report),     │ • rank (top-down)                 │
        Report>                 │ • fixed tie-break (sort NodeId)   │
        │  Ok only              │ • fixed node W/H (LayoutOptions)  │
        └──────parse_and_layout─┤ • canonicalize f64 coords         │
                (Err → skip     │ • bounds = union of node rects    │
                 layout, return └───────────────┬─────────────────┘
                 Report as-is)                  ▼
                                Manifest { …, nodes[i].pos, bounds }
```

## Implementation steps

1. **Crate spike + cross-target determinism gate (go/no-go, do this FIRST).**
   Evaluate candidate pure-Rust Sugiyama crates (`rust-sugiyama`, `dagre`,
   `dagre-dgl-rs`, `rusty-mermaid-dagre`, …). Selection criteria are **hard
   gates**, all mandatory:
   - **wasm-safe:** compiles for `wasm32-unknown-unknown`, no threads / `rand` /
     `SystemTime` / filesystem.
   - **deterministic:** supports a fixed tie-break; no internal randomness.
   - **native ≡ wasm:** produce a golden positioned-`Manifest` JSON and assert it
     is **byte-identical** when layout runs native vs `wasm32` (wasmtime/node
     harness in CI). This is the real determinism contract — laying out twice on
     the *same* binary does not test it.

   Pin the chosen crate exactly. **If no crate passes all gates**, do not force
   it: fall back to client-side dagre.js (`T-LAYOUT-SPIKE-FALLBACK`) and stop
   here. `ara-core` layout must also stay in the CI wasm-build check (feeds
   `T-WASM-CLIPPY`).

2. **`LayoutOptions`** with explicit, pinned determinism knobs: rank direction
   (top-down), node/rank separation, a **fixed tie-break** (sort by `NodeId`) so
   equal-rank ordering is stable, and **fixed node dimensions** (e.g. default
   `180 × 60`). No randomness anywhere.

   *Node-sizing tradeoff (document in code + `docs/`):* Sugiyama needs node
   width/height as **input**, but real box sizes depend on browser text
   measurement. Core lays out with the fixed `LayoutOptions` dimensions; labels
   are truncated/ellipsized to fit, OR the client may relayout (same `ara-core`
   wasm) if it needs exact text fit. State this limitation plainly so core's
   output is understood as "authoritative for the fixed-size case."

3. **Coordinate type + canonicalization.** `Point`/`Rect` use **`f64`**, plus a
   canonicalization step before serialization: round to N decimals and normalize
   `-0.0 → 0.0` so JSON is byte-stable across platforms. This is the serialize-
   side half of the determinism contract (step 1 is the compute-side half).

4. **Extend `Manifest`** (additive only): `Node.pos: Option<Point>` and
   `Manifest.bounds: Option<Rect>`. **No `Link.route`** (routing deferred).
   All new fields use `#[serde(skip_serializing_if = "Option::is_none")]` so
   Stage 1 snapshots stay byte-identical when layout is off (see regression test).

5. **`parse_and_layout(dir, opts) -> Result<(Manifest, ParseReport), ParseReport>`.**
   Runs Stage 1 parse; **on parse error (including cycles) returns the report
   unchanged and skips layout** — Stage 1 already treats cycles as ERRORS, so a
   cyclic graph never reaches layout. Drop the "break cycles / warn" step from
   the old plan (unreachable, and it would contradict the rank-monotonicity
   invariant). Keep a cheap defensive `debug_assert!` that no cycle is present.

6. **CLI (factor the shared path).** Extract the `parse_dir → handle Err(report)
   → optional --json emit` preamble from `validate()` (`main.rs:46`) into a
   shared helper. Then:
   - `ara validate --layout` runs layout and reports node/edge counts + bounds
     (same exit-code semantics as `validate`).
   - `ara layout <dir> --json` dumps the positioned `Manifest` (the JSON the
     Stage 3 client consumes).

7. **Freeze the GEOMETRY shape (not the whole model).** Document the `Manifest`
   JSON in `docs/manifest-schema.md`. Freeze the **geometry** wire shape
   (`Point`, `Rect`, `pos`, `bounds`) as stable. **Explicitly document that the
   logical model stays additively extensible** — new node kinds via
   `NodeKind::Other`, extra fields via the existing `extra` capture, gated by a
   `schema_version` field — so `T-REAL-CORPUS` (pivot kind, ~12 fields, `ara-2.0`
   streams) can land without a breaking bump. Only a geometry change needs a
   coordinated `ara-core` + client bump.

## Tests / verification (enumerated — 100% branch coverage)

**Unit (`ara-core`, `layout.rs`):**
- All nodes get **finite** positions; assert no `NaN`, no `inf`.
- Layered ranks **monotonic** along `Child` edges (topological sanity).
- Equal-rank **tie-break stable**: shuffle input order, assert identical output.
- **Empty** manifest (`tree: []`) → empty layout, valid/zero bounds, no panic.
- **Single node** → finite pos, bounds encloses it.
- **Bounds** == union of all node rects.
- **Coordinate canonicalization**: `-0.0 → 0.0`, round-to-N, byte-stable.
- **`parse_and_layout`**: error-free parse → `Ok(positioned)`; errored parse
  (cycle / broken ref) → `Err(report)`, layout NOT run.

**Regression (CRITICAL):** Stage 1 `insta` snapshots (`minimal_manifest`,
`resnet_manifest`) and the parse-twice determinism test are **byte-identical
with layout OFF** — proves the `Option` geometry fields + `skip_serializing_if`
don't perturb existing JSON.

**Determinism:**
- In-process: layout the corpus twice → byte-identical positioned JSON.
- **Cross-target (step-1 gate):** golden positioned-`Manifest` byte-identical
  native vs `wasm32` in CI.
- `insta` snapshot of the positioned `Manifest` on the corpus.

**CLI (`ara-cli/tests/cli.rs`, `assert_cmd`):**
- `ara layout <dir> --json` on official fixture → valid `Manifest` JSON
  (roundtrip-parseable) with `pos`/`bounds`.
- `ara layout` on missing / non-dir path → clean error, non-zero exit, no panic.
- `ara layout` on parse-error fixture → error surfaced, layout skipped.
- `ara validate --layout` → prints node/edge counts + bounds; error path matches
  `validate`.

**Scale probe (with a budget, not just an observation):** run on the largest
corpus tree (`rebench-rust_codecontests`, 383 lines) and the largest node count;
record time + node count and assert a **max node-count / time bound** (define the
threshold, esp. for wasm). Feeds the Stage 3 SVG-vs-canvas decision.

## Milestone / acceptance

Positioned `Manifest` is deterministic (in-process **and** native≡wasm) and
snapshot-stable; `ara layout --json` emits the frozen geometry wire format;
geometry schema documented in `docs/`; logical model documented as still
additive. Step-1 spike passed (or the client-side fallback was taken).

## Out of scope (deferred)

- **Rendering** (Stage 3); **canvas fast path** (decided empirically in Stage 3).
- **Edge routing** — `Link.route` bend points; the Stage 3 client routes edges
  straight/orthogonal from endpoints (`T-EDGE-ROUTING`).
- **Real-corpus schema widening** — `pivot` kind, extra fields, `ara-2.0`
  streams (`T-REAL-CORPUS`); this stage deliberately keeps the logical model
  additive rather than freezing it.
- **Client-side dagre.js fallback** — only if the step-1 spike fails
  (`T-LAYOUT-SPIKE-FALLBACK`).

## What already exists (reused, not rebuilt)

- `parse_sources` / `parse_dir` (Stage 1) produce the logical `Manifest` —
  layout consumes it directly.
- Cycle detection already runs in parse (`parse.rs:361` `detect_cycles`,
  cycles → ERROR), so layout never needs to break cycles.
- `NodeId` is `Ord` — the tie-break sort key already exists, no new type needed.
- `assert_cmd`/`predicates`/`tempfile` CLI test harness (Stage 1) — new CLI
  tests follow the same convention.

## CHANGELOG (Unreleased → Added)

- Deterministic layered DAG **node** layout in `ara-core`; positions + bounds
  added to `Manifest`; `ara layout <dir> --json` and `ara validate --layout`.
  Edge routing deferred to the client; geometry wire shape frozen, logical model
  kept additive.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR (PLAN) | 7 issues, 0 critical gaps, all folded |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | — |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

- **CODEX:** outside voice ran via Claude subagent (Codex installed but not authenticated). Raised 4 findings the review missed: node-sizing dependency, native≡wasm (vs in-process) determinism, crate-spike-with-no-fallback, and the strategic "who consumes native layout" question. All 4 folded plus overlap with review findings 2/3/5.
- **CROSS-MODEL:** Strong overlap — both flagged the schema freeze (Issue 2 / #1), the cycle contradiction (Issue 3 / #7), and determinism (Issue 5 / #3). Six cross-model tensions resolved: routing deferred to client, fixed node sizing, cross-target golden gate, Layout trait dropped, layout stays in core (rationale documented), and a routing reconcile (defer, positions-only) after an initial conflicting answer.
- **VERDICT:** ENG CLEARED — ready to implement. Plan rewritten to reflect all accepted decisions; version bump corrected to `0.0.3 → 0.0.4`; T-EDGE-ROUTING and T-LAYOUT-SPIKE-FALLBACK added to `TODOS.md`.

NO UNRESOLVED DECISIONS
