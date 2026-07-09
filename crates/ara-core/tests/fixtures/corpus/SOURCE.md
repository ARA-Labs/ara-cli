# Real-ARA corpus subset — source & attribution

The artifacts under this directory are a **curated, pinned subset** of the
`ara-paperbench` corpus. Unlike the canonical examples under `official/`, these
real artifacts exercise a **wider schema** than `ara-core` models today (extra
node fields, transition fields, real cycles, broken evidence refs, and an
`ara-2.0` streams document). They back the always-on `corpus_no_panic`
regression test (`crates/ara-core/tests/corpus_no_panic.rs`), which asserts the
parser **never unwind-panics and always produces a `ParseReport`** on real data
— it does **not** assert a clean parse. Warnings and errors here are expected.

Do not hand-edit these files. Re-vendor at a new pin with
`scripts/vendor-corpus.sh` instead.

- **Upstream repo:** https://github.com/AmberLJC/ara-paperbench
- **Pinned commit:** `3fe7ab4d08f68555d8c4661fa2b4fbfd4d597fd8`
- **License:** CC-BY-4.0 (see the upstream `LICENSE`). Attribution:
  "Amber Liu and the ARA project contributors."

Only `trace/exploration_tree.yaml` and `logic/claims.md` are copied — the parser
consumes only those two files.

## Files

| Fixture | Copied from (upstream path) |
|---------|-----------------------------|
| `extra/andes/` | `artifacts/extra/andes/` |
| `extra/expbench/` | `artifacts/extra/expbench/` |
| `paperbench/sample-specific-masks/` | `artifacts/paperbench/sample-specific-masks/` |
| `speedrun/nanogpt-speedrun/` | `artifacts/speedrun/nanogpt-speedrun/` |
| `rebench/rebench-rust_codecontests/` | `artifacts/rebench/rebench-rust_codecontests/` |
| `rebench/rebench-restricted_mlm/` | `artifacts/rebench/rebench-restricted_mlm/` |

Each fixture keeps both `trace/exploration_tree.yaml` and `logic/claims.md`.

## Verified drift outcomes

The subset was chosen to span the drift dimensions the real schema exercises.
The outcomes below were **reproduced against `parse_dir`** at the pinned commit
(`cargo run --bin ara -- validate <dir>`), not restated from an earlier sweep.
No artifact panicked. Outcome is the `ParseReport` result: `PASS` = `Ok` (no
errors, warnings allowed), `FAIL` = `Err` (has errors). Both outcomes pass the
no-panic test.

| artifact | drift dimension | outcome | errors | warnings |
|----------|-----------------|---------|-------:|---------:|
| `extra/andes` | warnings-only: unknown node fields `failure_mode` / `hypothesis` / `lesson` | PASS (`Ok`) | 0 | 3 |
| `extra/expbench` | real cycle + transition fields `from` / `to` / `trigger` | FAIL (`Err`) | 1 | 9 |
| `paperbench/sample-specific-masks` | multiple real cycles | FAIL (`Err`) | 2 | 6 |
| `speedrun/nanogpt-speedrun` | broken `evidence:` claim refs — stresses the error path | FAIL (`Err`) | 29 | 13 |
| `rebench/rebench-rust_codecontests` | large; many unknown-field warnings | PASS (`Ok`) | 0 | 35 |
| `rebench/rebench-restricted_mlm` | **`ara-2.0`** streams format (no `tree:`/`root:`) | FAIL (`Err`) | 1 | 8 |

Notes on outcomes observed during verification (step 1.5):

- `extra/expbench` error is `cycle detected: edge to N09 closes a cycle`; its
  warnings include the transition fields `from` / `to` / `trigger`.
- `paperbench/sample-specific-masks` errors are two `cycle detected` diagnostics
  (`N04`, `N08`) — this artifact turned out to cover the **real-cycle** dimension
  rather than broken evidence refs. The broken `evidence:` claim-ref dimension is
  instead covered by `speedrun/nanogpt-speedrun`, whose 29 errors are all
  `evidence references unknown claim`.
- `rebench/rebench-restricted_mlm` is the `ara-2.0` streams document: it has no
  `tree:` or `root:`, so the single error is `neither tree: nor root: is
  present`, with warnings for the `ara-2.0` fields (`schema_version`, `anchors`,
  `official_stream`, `malt_stream`, `score_direction`).

## `ARA-Demo` is intentionally not vendored

The `ARA-Labs/ARA-Demo` corpus (which uniquely exercises the `pivot` node type)
is **not** vendored here: it has no LICENSE upstream (all rights reserved), so
redistributing its files would be improper. It is reachable only via the opt-in
submodule sweep (`corpus-external/ara-demo`, `RUN_CORPUS_SWEEP=1`). When a
license lands upstream, `ARA-Demo/nanogpt_ara` can be vendored here to pull the
`pivot` dimension into this hermetic always-on check.
