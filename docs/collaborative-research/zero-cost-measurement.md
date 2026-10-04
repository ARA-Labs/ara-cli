# Zero-cost measurement policy

Status: frozen in phase 1a on 2026-10-03, before any collaboration code was measured. It implements [How the zero-cost requirement is checked](../../plans/collaborative-research/README.md#how-the-zero-cost-requirement-is-checked). Tolerances must not be relaxed after a result is seen; only the developer may revise them.

## What is compared

- **Per stage:** the stage's release binary against its parent commit on `feat/collaborative-ara`.
- **Whole series:** the final binary against the pre-series revision `0f51ebd` (`ara 0.1.23`).

Both binaries are built from clean checkouts with `cargo build --release --locked -p ara-cli` under the pinned toolchain in `rust-toolchain.toml` (Rust 1.94.1), on the same machine, in the same session.

## Fixtures

The harness `scripts/collab-zero-cost.py` generates every fixture into a scratch directory. Generated content uses a fixed seed (`20261003`).

| ID | Fixture | Contents |
|---|---|---|
| F1 | small | `Agent-Native-Research-Artifact/examples/the-ara-of-ara` at commit `03f19c7767ec993ae53a0698417b8b68040d7fee`. |
| F2 | large external evidence | F1 plus 256 files of 1 MiB seeded random bytes under `evidence/blobs/` and 64 files of 256 KiB under `src/blobs/`. |
| M | merge seed | A generated artifact: 20 question nodes, 10 claims, `src/train.py`, and four 64 KiB evidence files. F1 cannot serve as a merge input, because ara 0.1.23 rejects its session index and promotion records. |
| F3 | long imported history | M as the destination after 40 linear directory imports from one source key, each adding one node. Built with the parent binary. |
| S0–S3 | store state, applied to F1, F2, and M | S0 no `.ara/vcs/`; S1 `.ara/vcs/` holding 64 MiB of garbage files; S2 `.ara/vcs/` with mode `000`; S3 `.ara/vcs/` holding 4,096 small files. |

## Commands

Reads, on F1 and F2 under S0–S3 and on F3 under S0: `status --json`, `ls --json`, `show <first claim> --json`, `find "method" --json`, `validate <dir> --json`, `check <dir>`.

Writes, on a fresh copy of F1 per invocation under S0–S3: `add node` with a fixed payload; `apply` with a fixed one-line batch. The default duplicate check stays on.

Merges, on a fresh copy per invocation: the first directory import of M plus one node into M, under S0–S3; the 41st linear import into F3, under S0.

## Repetition and statistics

- Each command runs 3 warm-up invocations, then 15 measured invocations. Parent and candidate invocations alternate (ABAB…) to cancel drift.
- Wall time: `time.perf_counter_ns` around the child process. Report median and p90.
- Peak memory: a separate set of 5 invocations under `/usr/bin/time -l` (macOS) or `/usr/bin/time -v` (Linux). Report the median maximum resident set size.
- The first warm-up invocation of each side is reported separately as *first-invocation* latency. The parent always runs first, and the page cache is not dropped, so it is not a true cold-cache figure and it is not gated.

## Tolerances

A result is a **regression** only if it exceeds the tolerance in the first comparison and again in an immediate interleaved rerun of the same command.

| Metric | Tolerance |
|---|---|
| Median wall time | candidate ≤ parent × 1.10 + 3 ms |
| p90 wall time | candidate ≤ parent × 1.20 + 5 ms |
| Median peak RSS | candidate ≤ parent × 1.05 + 1 MiB |
| Output | stdout, stderr, and exit code byte-identical after removing `artifact_location`, absolute fixture paths, timestamps, merge `timings.*_ms` values, and the binary's version string |

## Absence of store and collaboration work

Timing cannot prove absence. The harness proves it as follows:

- Under S2 (`.ara/vcs/` unreadable), every ordinary command must produce the same output as under S0. Any read inside the store would fail with a permission error.
- Under S1 and S3, outputs must equal S0 outputs.
- After every ordinary command, the harness compares a listing of `.ara/` (file names, file sizes, modes; directory sizes are ignored) with the listing before the command. Only `.ara/lock` and `.ara/transactions/**` may change, and only for writes and merges.
- No ordinary command may create `snapshot.json`, a `.ara-snapshot-` staging directory, or `.ara/vcs/`.

## Build and distribution

| Check | Method | Tolerance |
|---|---|---|
| Default dependency tree | `cargo tree -p ara-cli -e normal --prefix none` sorted and deduplicated | The harness fails on any new crate name. A new crate passes review only if the stage PR names and justifies it. |
| Release binary size | Size of `target/release/ara` | ≤ parent × 1.02 + 64 KiB |
| Clean release build time | `cargo build --release --locked -p ara-cli` after removing `target/`, run by hand | Reported, with no gate, because one sample per side is too noisy to gate. |
| Wasm | `cargo build -p ara-core -p ara-wasm --target wasm32-unknown-unknown --locked`, run by hand | Passes. |

## Running the harness

```sh
python3 scripts/collab-zero-cost.py --parent-bin <parent>/target/release/ara \
  --candidate-bin <candidate>/target/release/ara \
  --anra ../Agent-Native-Research-Artifact --out <record>.json \
  [--parent-src <parent-checkout> --candidate-src <candidate-checkout>]
```

Exit 0 means every case passed. Exit 1 means a confirmed regression, a failed check, or a policy item that could not be executed; such an item fails the gate rather than being skipped. Exit 2 is a harness error. `--quick` runs too few repetitions to be gate-valid. Peak-RSS measurement needs `/usr/bin/time`, which the Claude Code sandbox blocks, so run the harness outside it.

## Records

Each stage stores the harness's JSON output and the exact binary SHA-256s under `docs/verification/collaborative-research/<stage>/`.
