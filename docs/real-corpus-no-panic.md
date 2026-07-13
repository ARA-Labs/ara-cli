# Real-ARA no-panic regression coverage (issue #3)

Design record for the corpus robustness net shipped for [issue #3](https://github.com/ARA-Labs/ara-cli/issues/3)
(PR #4, workspace `0.0.3`). It permanently guarantees the parser **runs to
completion without panicking on real data**, and locks in the schema-drift
knowledge before Stage 2 froze the wire schema.

Companion doc: [`stage-1-core-parse-validate.md`](stage-1-core-parse-validate.md)
(the parser this exercises).

## Problem

Stage 1 validated the parser against the **two canonical** examples only
(`minimal-artifact`, `resnet-ara-example`), asserting a *clean* parse. But the
real corpora exercise a much wider schema — extra node fields
(`failure_mode`/`hypothesis`/`lesson`), transition fields (`from`/`to`/`trigger`),
real cycles, broken `evidence:` claim refs, and one `ara-2.0` streams document
with no `tree:`. Running `ara validate` over all 34 real artifacts shows the
parser is panic-free but every artifact emits diagnostics, because the real
schema is a superset (catalogued in [`ara-format-feedback.md`](ara-format-feedback.md)
§13, `T-REAL-CORPUS`).

This is **not** a clean-parse check (real artifacts legitimately warn/error) and
**not** `T-REAL-CORPUS` (widening the model to parse them cleanly, still
deferred). It is only the regression net that proves no-panic + report-produced.

## The contract

For each corpus artifact the test asserts: `parse_dir`/`parse_sources` does not
**unwind-panic**, and it produces a `ParseReport` (either `Ok((manifest, report))`
or `Err(report)` — both pass; a broken artifact returning `Err(report)` is a
success). In Rust: call `parse_dir` inside `catch_unwind`; on the outer `Err` (a
panic unwound), fail naming *which* artifact panicked; otherwise both inner arms
pass.

**Precise scope:** `catch_unwind` catches *unwinding* panics only. It does not
catch a stack-overflow `SIGABRT`, and there is no per-test timeout, so a hang is
not detected. The parser recurses per-child/per-edge with no depth guard, so a
pathologically deep artifact would abort rather than surface a clean error. The
real corpus is not deep enough to trip this; hardening is tracked as
`T-PARSE-DEPTH`. The net also depends on unwinding panics — if any build profile
ever sets `panic = "abort"`, `catch_unwind` becomes a no-op and the net stops
catching anything (noted in the test file).

## Two-part coverage

**Part 1 — vendored subset (always-on, hermetic).** A curated subset of
`ara-paperbench` artifacts lives in `crates/ara-core/tests/fixtures/corpus/`, each
keeping only the two files the parser reads (`trace/exploration_tree.yaml` +
`logic/claims.md`), plus a `SOURCE.md` recording origin, pinned commit
(`3fe7ab4d08f68555d8c4661fa2b4fbfd4d597fd8`), and CC-BY-4.0 attribution. The
subset spans the drift dimensions:

| artifact | drift dimension |
|----------|-----------------|
| `extra/andes` | small; warnings-only |
| `extra/expbench` | real cycle + transition fields |
| `paperbench/sample-specific-masks` | broken `evidence:` claim refs (errors) |
| `speedrun/nanogpt-speedrun` | many errors (stresses the error path) |
| `rebench/rebench-rust_codecontests` | large; many warnings |
| `rebench/rebench-restricted_mlm` | `ara-2.0` streams format (no `tree:`) |

An always-on test iterates them, staying offline and `--locked`-friendly. It
computes `discover_artifacts(fixtures/corpus)` then asserts `dirs.len() >= 6`
**before** iterating — closing the vacuous-pass hole where an emptied/moved
fixtures dir would pass while guarding nothing. Re-vendoring at a new pin is one
command via `scripts/vendor-corpus.sh` (executable provenance).

**Part 2 — submodule full sweep (opt-in, maintainer-run).** Git submodules under
`corpus-external/ara-paperbench` and `corpus-external/ara-demo` (one per repo for
independent pinning) hold the full corpora. The sweep test is both `#[ignore]`d
and env-gated (`RUN_CORPUS_SWEEP=1`), reusing the same
`discover_artifacts` + `assert_parses_without_panic` helpers over all 34
artifacts. Required CI does **not** check out submodules — a fresh clone without
them still passes green.

The gate decision is factored into a pure helper
`should_run_sweep(env_set, dir_exists) -> bool` so the fresh-clone-passes
invariant (the most important property, hit by every CI run and new contributor)
is unit-tested always-on via `sweep_gate_logic`, without ever checking out the
real artifacts.

## Licensing hygiene

`crates/ara-core/Cargo.toml` sets `exclude = ["tests/fixtures/**"]`. `cargo
publish` bundles `tests/` by default, so without this the CC-BY-4.0 corpus (and
the MIT `official/` set) would ship inside the MPL-2.0 crate tarball, redistributing
attribution-required third-party data. Excluding all fixtures keeps the published
artifact free of third-party data; tests still run from a git checkout.

`ARA-Demo/nanogpt_ara` (the only source of the `pivot` node type) is **not**
vendored — it is unlicensed today — and is reachable only via the submodule
sweep. When the ARA-Demo LICENSE merges upstream, vendoring it will pull `pivot`
into the hermetic always-on check.

## Acceptance (met)

`cargo test --workspace --locked` is green including the always-on
`corpus_no_panic` test (6 vendored artifacts) and the `sweep_gate_logic` unit
test. The `>= 6` count guard fails loudly if the fixtures are moved/emptied. The
Stage-1 `official_fixtures_are_clean` and snapshot tests are unchanged (they use
a hardcoded dir list, so a new `corpus/` subtree cannot perturb them). With
submodules absent, the default suite still passes.

## Deferred

Widening the model to parse real artifacts cleanly (`T-REAL-CORPUS`); `E##`
evidence-proof resolution (`T-EVIDENCE`); parser recursion-depth hardening
(`T-PARSE-DEPTH`); per-test timeout / hang detection (not worth the machinery for
a corpus that parses in milliseconds).
