# Real-ARA corpus subset — source & attribution

The artifacts under this directory are a **curated, pinned subset** of the
`ara-paperbench` corpus. Unlike the canonical examples under `official/`, these
real artifacts exercise a **wider schema** than `ara-core` models today (extra
node fields, redundant ancestor back-edges, broken evidence refs, and an
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

For the six subset fixtures below, only `trace/exploration_tree.yaml` and
`logic/claims.md` are copied. (`paperbench/self-composing-policies/` is the
exception — a full artifact copy; see its section below — because `parse_dir`
now also reads `PAPER.md` and the `logic/` + `evidence/` layers.)

## Files

| Fixture | Copied from (upstream path) |
|---------|-----------------------------|
| `extra/andes/` | `artifacts/extra/andes/` |
| `extra/expbench/` | `artifacts/extra/expbench/` |
| `paperbench/sample-specific-masks/` | `artifacts/paperbench/sample-specific-masks/` |
| `speedrun/nanogpt-speedrun/` | `artifacts/speedrun/nanogpt-speedrun/` |
| `rebench/rebench-rust_codecontests/` | `artifacts/rebench/rebench-rust_codecontests/` |
| `rebench/rebench-restricted_mlm/` | `artifacts/rebench/rebench-restricted_mlm/` |
| `paperbench/self-composing-policies/` | `artifacts/paperbench/self-composing-policies/` |

Each fixture keeps both `trace/exploration_tree.yaml` and `logic/claims.md`.

## Exception: `paperbench/self-composing-policies/` (full artifact)

Unlike the other fixtures, this one is a **full, byte-verbatim artifact copy**
(`PAPER.md`, `logic/**` including `problem.md` / `concepts.md` /
`related_work.md` / `solution/*.md`, `trace/`, `evidence/`, `src/`, `rubric/`).
It backs the logic-layer snapshot test `self_composing_policies_snapshot`
(`crates/ara-core/tests/parse_fixtures.rs`), which locks the parsed
`paper` / `problem` / `concepts` / `related_work` / `recipes` fields.

No hand-edits. The trace's `N10` and `N11` each carry `also_depends_on: [N09]`
on their own parent `N09`; the parser treats a dependency on an ancestor as a
**redundant back-edge** (the child nesting already encodes it) and drops it with
a *warning* rather than flagging a fatal cycle, so `parse_dir` returns `Ok` on
the unmodified upstream file (2 warnings). Real published artifacts do this, so
tolerating it is required to open them at all.

## Verified drift outcomes

The subset was chosen to span the drift dimensions the real schema exercises.
The outcomes below were **reproduced against `parse_dir`** at the pinned commit
(`cargo run --bin ara -- validate <dir>`), not restated from an earlier sweep.
No artifact panicked. Outcome is the `ParseReport` result: `PASS` = `Ok` (no
errors, warnings allowed), `FAIL` = `Err` (has errors). Both outcomes pass the
no-panic test.

| artifact | drift dimension | outcome | errors | warnings |
|----------|-----------------|---------|-------:|---------:|
| `extra/andes` | node fields `failure_mode` / `hypothesis` / `lesson` — now modeled (clean) | PASS (`Ok`) | 0 | 0 |
| `extra/expbench` | redundant ancestor back-edge (tolerated) + `pivot` transition fields `from`/`to`/`trigger` — now modeled | PASS (`Ok`) | 0 | 1 |
| `paperbench/sample-specific-masks` | multiple redundant ancestor back-edges (tolerated as warnings) | PASS (`Ok`) | 0 | 2 |
| `speedrun/nanogpt-speedrun` | broken `evidence:` claim refs — stresses the error path | FAIL (`Err`) | 29 | 2 |
| `rebench/rebench-rust_codecontests` | large; many unknown-field warnings | PASS (`Ok`) | 0 | 29 |
| `rebench/rebench-restricted_mlm` | **`ara-2.0`** streams format (no `tree:`/`root:`) | FAIL (`Err`) | 1 | 8 |

Notes on outcomes observed during verification:

- `extra/andes` is now fully clean: the `failure_mode` / `hypothesis` / `lesson`
  dead-end fields it carries are modeled (no longer unknown-field warnings).
- `extra/expbench` and `paperbench/sample-specific-masks` were previously `FAIL`
  on `cycle detected` diagnostics. Those "cycles" are all **redundant
  `also_depends_on` edges pointing at an ancestor** (`N10→N09`; `N12→N04`,
  `N12→N08`) — the child nesting already encodes the dependency. The parser now
  drops such edges with a `redundant ... ancestor` **warning** instead of a fatal
  cycle error, so both artifacts `PASS`. Genuine cross-cycles (a dependency on a
  sibling/descendant that closes a loop) remain fatal — covered by the synthetic
  `broken/cycle.yaml` and `crates/ara-cli/tests/fixtures/cycle-dir`.
- `speedrun/nanogpt-speedrun` still exercises the broken `evidence:` claim-ref
  error path: its 29 errors are all `evidence references unknown claim`.
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
