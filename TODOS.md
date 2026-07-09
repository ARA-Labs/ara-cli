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
  ara-wasm) to CI.
- **Why:** Native clippy skips `#[cfg(target_arch = "wasm32")]` branches, so
  wasm-only code can rot while CI stays green.
- **Context:** No cfg-gated code exists today (no-op now). Becomes relevant when
  Stage 3 adds `ara-wasm` bindings.
- **Depends on:** First `#[cfg(target_arch = "wasm32")]` usage in the codebase.

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
