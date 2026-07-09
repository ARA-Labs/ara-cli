# Issue #3 — Real-ARA no-panic regression coverage

**Issue:** https://github.com/EYH0602/bara/issues/3
**PR target:** `issue-3-real-corpus` → `main`. **Depends on:** Stage 1 (#2, merged).
**Version bump:** `0.0.2 → 0.0.3`.

## Problem background

Stage 1 (#2) validated the parser against the **two canonical** examples only
(`minimal-artifact`, `resnet-ara-example`), asserting a *clean* parse (zero
warnings, zero errors). But the real corpora exercise a much wider schema:

- `AmberLJC/ara-paperbench` — 32 artifacts, **CC-BY-4.0**
- `ARA-Labs/ARA-Demo` — 2 artifacts, **no LICENSE** (all rights reserved)

Running `ara validate` over all 34 shows the parser is **panic-free**, but
every artifact emits diagnostics because the real schema is a superset (extra
node fields like `failure_mode`/`hypothesis`/`lesson`, transition fields
`from`/`to`/`trigger`, real cycles, broken `evidence:` claim refs, and one
`ara-2.0` streams document with no `tree:`). See `docs/ara-format-feedback.md`
§13 and `TODOS.md` → `T-REAL-CORPUS`.

We want CI to keep guaranteeing **"runs to completion without panicking on real
data"** — a robustness check, permanently.

### What this is NOT

- **Not** a clean-parse check. Real artifacts legitimately produce
  warnings/errors; the test must **not** assert zero diagnostics.
- **Not** T-REAL-CORPUS (widening the model to parse real artifacts *cleanly*).
  That stays deferred. This issue only adds the **regression net** that proves
  no-panic + report-produced, and locks in the drift knowledge before Stage 2
  freezes the wire schema.

## Assertion semantics (the contract)

For each corpus artifact, the test asserts:

1. `parse_dir` / `parse_sources` **does not unwind-panic**, and
2. it **produces a `ParseReport`** — i.e. returns either
   `Ok((manifest, report))` or `Err(report)`. **Both outcomes pass.** A broken
   artifact that returns `Err(report)` is a *success* for this test; only a
   panic is a failure.

Practically, in Rust this means: call `parse_dir` inside `catch_unwind`; if the
closure returns `Err` (a panic unwound), fail with a message naming **which**
artifact panicked; otherwise match the inner `Ok`/`Err` and confirm a report
exists in both arms.

**Precise scope of the guarantee (corrected in eng review).** `catch_unwind`
catches *unwinding* panics only. It does **not** catch a stack-overflow
`SIGABRT`, and there is **no per-test timeout**, so a hang is not detected
either. The parser recurses per-child (`Normalizer::dfs`, `parse.rs:233`) and
per-edge (`visit`, `parse.rs:386`) with no depth guard, so a pathologically
deep artifact would abort rather than surface a clean error. The real corpus is
not thousands-deep, so the vendored/swept artifacts will not trip this — but the
contract this test enforces is **"no unwinding panic + a report is produced,"**
not "cannot crash on any input." Hardening the parser against deep recursion is
tracked separately as `T-PARSE-DEPTH` (see TODOS.md), out of scope here.

> Note (eng review): step 2's Ok/Err match is close to a type-level tautology —
> `parse_dir`'s return type already carries a `ParseReport` in both arms. The
> load-bearing assertion is the `catch_unwind` no-panic check plus the
> discovery-count guard (below); keep both, they are what actually does work.

## Proposed solution: **Both** (per the issue's decided approach)

### Part 1 — Vendored subset (required, hermetic CI check)

Copy a curated subset of `ara-paperbench` artifacts into
`crates/ara-core/tests/fixtures/corpus/`, each keeping only the two files the
parser reads (`trace/exploration_tree.yaml` + `logic/claims.md`), plus a
`SOURCE.md` recording origin, pinned commit, and CC-BY attribution (mirroring
`official/SOURCE.md`). An **always-on** test iterates them and asserts
no-panic + report-produced. Stays offline and `--locked`-friendly.

**Subset** (from the issue — chosen to span the drift dimensions):

| artifact | drift dimension it covers |
|----------|---------------------------|
| `extra/andes` | small; warnings-only (`failure_mode`/`hypothesis`/`lesson`) |
| `extra/expbench` | real cycle + transition fields (`from`/`to`/`trigger`) |
| `paperbench/sample-specific-masks` | broken `evidence:` claim refs (errors) |
| `speedrun/nanogpt-speedrun` | many errors (29) — stresses the error path |
| `rebench/rebench-rust_codecontests` | large; many warnings (35) |
| `rebench/rebench-restricted_mlm` | **`ara-2.0`** streams format (no `tree:`) |

Pinned commit: `3fe7ab4d08f68555d8c4661fa2b4fbfd4d597fd8`.

> Note: the `pivot` node type appears only in `ARA-Demo/nanogpt_ara`, which is
> **unlicensed today** → it can only be covered by the submodule sweep, not
> vendored, **until** the ARA-Demo license lands (expected — the bara author
> knows the ARA founder). See "Fast-follow" below.

### Part 2 — Submodule full sweep (opt-in, maintainer-run)

Add git submodule(s) for the full corpora behind an env-gated / `#[ignore]`
test so maintainers can sweep **all 34** artifacts locally. This is where the
broad coverage — and the unlicensed `ARA-Demo` (pointer only, not vendored) —
lives. Required CI does **not** check out submodules.

- Submodules under e.g. `corpus-external/ara-paperbench` and
  `corpus-external/ara-demo`, pinned to the commits above (`ara-demo` pin:
  `8f184717bb7827fe59d47a1ac44a00f66c6375ee`).
- The sweep test walks each submodule for any directory containing
  `trace/exploration_tree.yaml`, runs `parse_dir`, and asserts no-panic +
  report. Gate: `RUN_CORPUS_SWEEP=1` (skips cleanly, logging why, when unset or
  when the submodule dir is absent — so a fresh clone without submodules still
  passes).

## Implementation steps

1. **Vendor the subset.** Fetch `ara-paperbench` at the pinned commit; copy the
   6 artifacts' `trace/exploration_tree.yaml` (+ `logic/claims.md` where
   present) into `crates/ara-core/tests/fixtures/corpus/<artifact>/`. Do **not**
   hand-edit them. Prefer a small documented `scripts/vendor-corpus.sh` (Q2
   resolved: script over by-hand — re-vendoring at a new pin becomes one command
   and the provenance is executable) that copies from a checkout at the pin.
1.5. **Verify each candidate BEFORE it becomes the acceptance bar** (added in eng
   review — sequencing fix). For each of the 6 vendored artifacts, run
   `cargo run -- validate <dir>` (or a scratch `parse_dir` call) and confirm: (a)
   no panic, and (b) capture the **observed** outcome (Ok + N warnings, or Err +
   N errors). Fold the observed outcomes into `corpus/SOURCE.md`'s drift table so
   the numbers in that table are reproduced-and-verified, not restated from the
   original `ara validate` sweep. This de-risks the always-on test's first run —
   if a chosen artifact actually panics under `parse_dir`, we learn it here, not
   in CI.
2. **Write `corpus/SOURCE.md`** mirroring `official/SOURCE.md`: upstream repo,
   pinned commit, CC-BY-4.0 attribution, the artifact→upstream-path table, and
   (from step 1.5) the verified per-artifact drift outcome.
3. **Always-on test** in a new `crates/ara-core/tests/corpus_no_panic.rs`, built
   on **two shared helpers** (Q from eng review — DRY: the sweep in step 5 reuses
   the same helpers, no copy-paste):
   - `fn discover_artifacts(root: &Path) -> Vec<PathBuf>` — recurse with
     **`std::fs`** (no `walkdir` dependency — avoids a new crate + `Cargo.lock`
     churn), collect every dir containing `trace/exploration_tree.yaml`, and
     **`sort()` the result** so a failure message lists artifacts deterministically.
   - `fn assert_parses_without_panic(dir: &Path)` — call `parse_dir` inside
     `catch_unwind`; on the outer `Err` (panic unwound) `panic!("PANIC parsing
     {}", dir.display())`; otherwise the inner `Ok`/`Err` both pass (report
     present by type).
   The always-on test does `let dirs = discover_artifacts(fixtures/corpus)` then
   **`assert!(dirs.len() >= 6, ...)`** *before* iterating — this closes the
   vacuous-pass hole (added in eng review): if the fixtures dir is ever moved or
   emptied, the walk finds zero dirs and the test would otherwise pass while
   guarding nothing. The `>= 6` guard makes that fail loudly with a clear message.
   - Gate on `#[cfg(feature = "native")]` (matches existing `parse_dir` tests).
   - **Panic-hook note:** `catch_unwind` catches the unwind, but Rust's default
     panic hook still prints the message/backtrace to stderr *before* it returns.
     That is expected and harmless on a green run; the per-artifact
     `panic!("PANIC parsing {dir}")` is the actionable signal that names which
     artifact broke.
4. **Add submodule(s).** `git submodule add` the two upstream repos under
   `corpus-external/` (Q1 resolved: **one submodule per repo** —
   `corpus-external/ara-paperbench` and `corpus-external/ara-demo` — for cleaner
   independent pinning), pin to the commits, and document each in `.gitmodules`.
5. **Opt-in sweep test** (same file): `#[ignore]` **and** env-gated
   (`RUN_CORPUS_SWEEP=1`); reuses `discover_artifacts` + `assert_parses_without_panic`
   from step 3 over the submodule paths. Gate decision factored into a **pure,
   always-on-testable** helper (added in eng review):
   `fn should_run_sweep(env_set: bool, dir_exists: bool) -> bool { env_set && dir_exists }`.
   The sweep test calls it; when it returns false the test **skips with a log
   line** (env unset or submodule dir absent) so a fresh clone still passes. When
   it runs, `assert!(swept.len() >= 1, ...)` guards against a vacuous sweep.
5.5. **Always-on skip-gate unit test** (added in eng review — closes the one new
   codepath with no CI coverage). A plain `#[test] fn sweep_gate_logic()` asserts
   all four cases of `should_run_sweep` (`true/true`→run; the other three→skip).
   This guards the **fresh-clone-passes** invariant — the most important property,
   since every CI run and every new contributor hits it — inside CI itself,
   without ever checking out the 34 real artifacts.
6. **Docs:**
   - `CONTRIBUTING.md`: a "Running the full corpus sweep" section — how to
     `git submodule update --init` and run `RUN_CORPUS_SWEEP=1 cargo test`.
   - Note in `corpus/SOURCE.md` that `ARA-Demo` is intentionally **not** vendored
     (no license) and is reachable only via submodule.
7. **CI:** confirm the default `test` job does **not** init submodules (current
   `actions/checkout` (`ci.yml:40`) has no `submodules:` key → already correct;
   add an explicit comment so nobody adds it later). No new required job.
8. **Publish hygiene (added in eng review — licensing).** Add
   `exclude = ["tests/fixtures/**"]` to `crates/ara-core/Cargo.toml`. `publish`
   defaults to `true` and `cargo publish` bundles `tests/` by default, so without
   this the CC-BY-4.0 corpus (and the MIT `official/` set) would ship inside the
   MPL-2.0 crate tarball — redistributing attribution-required third-party data.
   Excluding all fixtures keeps the published artifact free of third-party data
   entirely; tests still run from a git checkout / workspace. Not live today
   (crates reserved at `0.0.0`, nothing auto-publishes) but belongs in **this** PR
   because this is the PR that introduces the CC-BY files.
9. **Version + changelog:** bump `0.0.2 → 0.0.3` in `Cargo.toml`; refresh
   `Cargo.lock` with a plain `cargo build`; add a CHANGELOG `Added` entry.
   - Note: a version-only bump should **not** move `Cargo.lock` (no deps changed).
     If `cargo build` does move it, inspect the diff — that's unrelated dependency
     churn riding along, not part of this change.
10. **Close the loop on `TODOS.md`:** `T-REAL-CORPUS` stays open (widening is
   still deferred), but add a line noting the no-panic regression net now exists
   (this issue) so the two aren't conflated. Add **`T-PARSE-DEPTH`** (new — see
   below): harden `Normalizer::dfs`/`visit` with a recursion depth guard so a
   pathologically deep artifact yields a clean error instead of a stack-overflow
   abort.

## Tests / verification

- `cargo test --workspace --locked` green, including: the new always-on
  `corpus_no_panic` test over the 6 vendored artifacts, and the always-on
  `sweep_gate_logic` unit test (4 cases of `should_run_sweep`).
- The `assert!(dirs.len() >= 6)` count guard trips (test fails loudly) if the
  vendored fixtures are ever moved/emptied — verify by temporarily pointing the
  walk at an empty dir during development.
- Existing `official_fixtures_are_clean` and snapshot tests **unchanged** and
  still green (this issue adds fixtures under a new `corpus/` subtree; it must
  not touch `official/`). Confirmed: `official_fixtures_are_clean`
  (`parse_fixtures.rs:20`) uses a **hardcoded** dir list, so a new `corpus/`
  subtree cannot perturb it.
- Manual: `RUN_CORPUS_SWEEP=1 git submodule update --init && cargo test -- --ignored`
  sweeps all 34 and passes (no panics), locally.
- Manual: with submodules **absent**, `cargo test --workspace --locked` still
  passes (sweep test skips cleanly). This invariant is now *also* guarded in CI
  by `sweep_gate_logic`.
- `--locked` + wasm build unaffected (fixtures are test-only, `native`-gated).
- Confirm no build profile sets `panic = "abort"` (checked: none today) — if one
  is ever added, `catch_unwind` silently becomes a no-op and this net stops
  catching anything. Worth a one-line comment in the test file noting the
  dependency on unwinding panics.

## Acceptance (maps to issue checklist)

- [ ] `crates/ara-core/tests/fixtures/corpus/` holds the vendored subset +
      `SOURCE.md` (CC-BY attribution, pinned commit).
- [ ] Always-on test: each corpus artifact parses without panic and returns a
      report; runs in default `cargo test` and CI.
- [ ] Submodule(s) added; opt-in/`#[ignore]` full-sweep test that only runs when
      explicitly enabled (documented in `CONTRIBUTING.md`).
- [ ] `.gitmodules` documented; CI does **not** require submodule checkout for
      the default job.
- [x] Follow-up filed upstream asking `ARA-Labs/ARA-Demo` to add a license.
      *(filed — paste URL here)*

## Fast-follow (once ARA-Demo's LICENSE actually merges upstream)

The ARA-Demo license is **expected** but not present yet. We do **not** vendor
its files on assumption — redistributing all-rights-reserved files before the
license exists is exactly what the issue warns against, and is hard to scrub
from git history. When the LICENSE actually merges upstream:

- Vendor `ARA-Demo/nanogpt_ara` into `fixtures/corpus/` (two files + attribution
  in `corpus/SOURCE.md`, pinned commit `8f184717…`), which pulls the **`pivot`
  node type into the hermetic always-on check** — the one drift dimension
  currently reachable only via the opt-in submodule sweep.
- Until then, `pivot` coverage lives in the submodule sweep only.

## Out of scope (explicitly deferred)

- Widening the model to parse real artifacts *cleanly* (`T-REAL-CORPUS`): new
  node fields, `pivot` kind, `ara-2.0` streams. This issue only proves no-panic.
- `E##` evidence-proof resolution (`T-EVIDENCE`).
- **Parser recursion depth hardening (`T-PARSE-DEPTH`, new — eng review):**
  `Normalizer::dfs` (`parse.rs:233`) and `visit` (`parse.rs:386`) recurse with no
  depth guard, so a pathologically deep artifact would stack-overflow (abort),
  which `catch_unwind` cannot catch. The real corpus is not deep enough to trip
  this, so it does not block the regression net; tracked as a separate parser fix.
- Per-test timeout / hang detection: no watchdog is added. A hang would wedge the
  CI job until the runner timeout, with no per-artifact attribution. Not worth the
  machinery for a corpus that parses in milliseconds; noted so the contract is honest.

## Open questions for review — RESOLVED (eng review 2026-07-09)

1. **Submodule layout:** ✅ **one submodule per repo** under `corpus-external/`
   (`ara-paperbench`, `ara-demo`) — cleaner independent pinning.
2. **Vendoring mechanics:** ✅ **small documented `scripts/vendor-corpus.sh`** —
   re-vendoring at a new pin is one command; provenance is executable.
3. **Gate style:** ✅ **both** `#[ignore]` + `RUN_CORPUS_SWEEP=1` — `cargo test`
   never runs the sweep and `--ignored` still respects the env gate. The gate
   decision is additionally factored into the pure `should_run_sweep(env, dir)`
   helper so it can be unit-tested always-on (step 5.5).

## CHANGELOG (Unreleased → Added)

- Real-ARA no-panic regression coverage: vendored `ara-paperbench` subset under
  `crates/ara-core/tests/fixtures/corpus/` with an always-on test asserting the
  parser never panics and always produces a `ParseReport`; opt-in submodule
  full-sweep test (`RUN_CORPUS_SWEEP=1`) over all 34 real artifacts (#3).

## Implementation Tasks
Synthesized from this review's findings. Each task derives from a specific
finding above. Run with Claude Code or Codex; checkbox as you ship.

- [ ] **T1 (P1, human: ~2h / CC: ~20min)** — test-harness — Shared `discover_artifacts` + `assert_parses_without_panic` helpers with `>= 6` count guard
  - Surfaced by: Code Quality — DRY across always-on + sweep; vacuous-pass hole on an empty walk
  - Files: `crates/ara-core/tests/corpus_no_panic.rs`
  - Verify: `cargo test --workspace --locked`; temporarily point the walk at an empty dir → test must fail on the count guard
- [ ] **T2 (P1, human: ~30min / CC: ~10min)** — test-harness — `should_run_sweep` pure gate fn + always-on `sweep_gate_logic` unit test
  - Surfaced by: Test review — fresh-clone-passes invariant had no CI coverage
  - Files: `crates/ara-core/tests/corpus_no_panic.rs`
  - Verify: `cargo test sweep_gate_logic` covers all 4 cases
- [ ] **T3 (P1, human: ~10min / CC: ~2min)** — packaging — Add `exclude = ["tests/fixtures/**"]` to `ara-core` Cargo.toml
  - Surfaced by: Outside voice — `cargo publish` bundles CC-BY corpus into MPL-2.0 tarball
  - Files: `crates/ara-core/Cargo.toml`
  - Verify: `cargo package -p ara-core --list` shows no `tests/fixtures/` entries
- [ ] **T4 (P1, human: ~1h / CC: ~15min)** — fixtures — Verify each of the 6 candidates under `parse_dir` before commit; record observed drift in SOURCE.md
  - Surfaced by: Outside voice — vendoring precedes verification; drift table restated from memory
  - Files: `crates/ara-core/tests/fixtures/corpus/SOURCE.md`, `scripts/vendor-corpus.sh`
  - Verify: `cargo run -- validate <dir>` per artifact; no panic, outcomes match SOURCE.md
- [ ] **T5 (P2, human: ~15min / CC: ~5min)** — docs — Correct contract wording to "no-unwinding-panic"; note `panic = "abort"` dependency in the test file
  - Surfaced by: Outside voice — contract overstates (SIGABRT + hang not covered)
  - Files: `plans/issue-3-real-corpus-no-panic.md` (done), `crates/ara-core/tests/corpus_no_panic.rs`
  - Verify: test-file comment present; plan contract reads "no unwinding panic"
- [ ] **T6 (P3, human: ~2h / CC: ~30min)** — parser — `T-PARSE-DEPTH`: recursion depth guard in `dfs`/`visit` + deep-nesting fixture (follow-up TODO)
  - Surfaced by: Outside voice — unguarded recursion → stack-overflow abort `catch_unwind` misses
  - Files: `crates/ara-core/src/parse.rs`
  - Verify: 10k-deep synthetic fixture yields a clean `Err`, no abort

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | ISSUES FOLDED | 6 issues, 0 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | — |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

- **CODEX:** not installed — outside voice ran as a Claude subagent. Found 7 items; 3 new decisions folded (guarantee wording + T-PARSE-DEPTH, publish exclude, verify-before-vendor), 2 overlapped existing findings, 1 minor folded as notes (panic=abort / Cargo.lock churn), 1 rebutted (claim that vendored fixtures duplicate `broken/*` coverage — false; they cover real drift dimensions nothing else exercises).
- **CROSS-MODEL:** no tension on scope — outside voice's "always-on half is near-worthless" verdict was rebutted against source, so the "Both" scope decision stands unchallenged.
- **VERDICT:** ENG CLEARED — ready to implement. All 6 findings folded into the plan; scope accepted as Both.

NO UNRESOLVED DECISIONS
