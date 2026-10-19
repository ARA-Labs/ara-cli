# Plan 20 command-simplification verification
**Date:** 2026-10-07

## TL;DR

Plan 20's required engineering checks pass on the coordinated 0.1.27 binary and live-skill cutover. The complete acceptance run finished, but its overall result is **false**: nine report-only 100k merge cases and one frozen historical creation replay still fail. Upstream protocol/skill approval and independent frozen search criteria remain separate blockers; no model-driven experiment, release, merge, or upstream adoption is claimed.

## Which binary and sources were exercised?

The final executable was `target/release/ara`, version `ara 0.1.27`, SHA-256 `93bb9f76839bd51fccac6eaeb6b3d6b165036c167f61d7b02ec264fb6e1eecb3`. The runner used Rust 1.94.1 (`e408947bf`, release), macOS 27.0.1 arm64, Apple M5 Pro, 18 logical CPUs, and 64 GiB RAM. [Source digests](build-source-digests.json) bind the CLI/core implementation, live skill files, manifests, lockfile, and current runner to this executable; the [integrity manifest](evidence-integrity.json) binds the retained evidence files.

The [completed design](../../agent-cli-interface/20-command-simplification.md) and [migration guide](../../agent-cli.md#command-simplification-migration) describe the new contracts. Historical inputs, archived baseline skills, and prior verification reports were not rewritten. The intermediate patch bump records integration work, not compatibility or a release-version decision.

## What runtime behavior was checked?

The [actual-binary smoke receipt](smoke.json) records 54 invocations and 48 checks or workflow markers. It exercises complete owned turns, literal `@` and list inputs, provisional identities, dry runs, failing physical-line diagnostics, full rollback, successful and failed promotions, audited revision, standalone setters, ordered combined relations, projections, complete bounds, whole-document refs, unfinished filters and unknown history, malformed/ambiguous identities, directory merge/replay, and installed-skill pairing. Read-only calls leave knowledge-layer bytes unchanged. The final runner additionally proves all 88 CLI-required operations; its remaining 19 rows are explicit direct-access, output-only, or skill-reference exemptions, not fabricated native proofs.

[Six baseline comparisons](read-parity.json) pass against the preserved 0.1.26 capture: ordinary brief and JSON listing are byte-identical; ancestry steps, entry refs, whole-document refs, and unfinished rows retain exact payloads and ordering. The compressed [baseline capture](baseline-0.1.26.json.xz) remains available. Official minimal and ResNet fixtures exercised the unchanged `validate`, `check`, and `layout` contracts.

The final executable served the official ResNet artifact in a real browser. Opening question N01 displayed its detail panel; the proof records no runtime error or horizontal overflow. [Browser state](viewer.json) and the [screenshot](viewer.png) refer to the same final executable. The tab and server were closed after verification; this is a viewer smoke, not a browser-suite or bundle-size claim.

## Which final build gates passed?

[The complete gate log](workspace-gates.log) records 1,132 Rust tests passing across 54 suite results, with one ignored test, plus 18 Python runner tests. Locked all-target Clippy passed without warnings; the pinned formatter passed. The applicable wasm build and embedded-viewer freshness check passed; the latter reported hash `ec6e9e51dc50bc8c1215dec85c0fda912beb4aa71519a18d9978ae93a125fdaf`. Native-only citation API changes do not change the viewer wire contract; no embedded bundle regeneration or browser-specific wasm suite result is claimed.

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
cargo build -p ara-core -p ara-wasm --target wasm32-unknown-unknown --locked
scripts/embed-viewer.sh --check
cargo build --release
python3 -m unittest scripts/test_agent_cli_acceptance.py
```

## What did the fixed-budget runner measure?

The final run completed all selected sections, recording 361 section results and 729 invocations. All current read, corpus, required merge/identity, and native skill-coverage checks passed. The runner kept five fresh-process samples per timed case and the existing thresholds; neither thresholds nor failed samples were relaxed or discarded. These are local measurements with OS caches, not cold-machine results, a speedup estimate, or research-accuracy evidence.

| Measured group | Cases | Fresh-process samples | Worst sample (ms) | Existing budget (ms) | Result |
|---|---:|---:|---:|---:|---|
| Real-artifact reads | 7 | 35 | 26.039 | 100 | Every sample passes |
| Generated-artifact reads | 42 | 210 | 298.007 | 1,000 | Every sample passes |
| 10k directory merge | 1 | 5 | 881.458 | 1,000 | Every sample passes |
| 10k Git merge, both tree modes | 2 | 10 | 1,113.813 | None | Timing report only; behavior passes |

Git timing is report-only in the existing runner; the fixed 10k merge budget applies to the directory case. The [compact final summary](acceptance-summary.json) lists coverage statuses, exact maxima, blockers, and every retained failure. The full [final acceptance evidence](final-acceptance.json.xz) preserves invocations, sources, output, and individual samples, including failures.

```bash
python3 scripts/agent-cli-acceptance.py \
  --binary target/release/ara \
  --toolchain 'rustc 1.94.1 (e408947bf 2026-03-25), release' \
  --output /tmp/ara-plan20-final-acceptance.json
```

## Why is overall acceptance still false?

The nine 100k directory/Git failures comprise six 30-second timeouts and three native YAML resource-budget rejections (`Nodes { nodes: 250001 }`). These stress cases were already designated report-only in the [previous engineering report](../agent-cli-2026-10-02/README.md) and [directory-merge record](../../agent-cli-interface/08-directory-merge.md). They remain visible, not reclassified as successes. The frozen creation replay also fails at physical line 55 because dependency on ancestor `N65` is redundant; its historical inputs are preserved rather than rewritten to satisfy the current writer.

Pinned protocol/skill contracts still await upstream review, and independent frozen search criteria are missing. The runner therefore claims neither upstream approval nor a measured search result. These wider rollout gates do not authorize a model pilot or paid run and are not completed by this implementation PR.

The [initial probe summary](initial-probe-summary.json) and [full initial evidence](initial-probe.json.xz) retain the earlier 130.578 ms first status sample and false status/promotion-oracle failures. The final cutover corrected those oracles to consume the actual completeness and explicit-target provenance contracts, then ran the complete unchanged-budget runner on the final executable. The initial timing failure is still evidence; it is not explained away or removed, and the final run is not a claim that it cannot recur.

## How can the retained evidence be inspected?

Compact JSON receipts can be read directly. Each full runner archive expands to roughly 3.5 GB because native resource-limit failures include complete source evidence; reserve scratch space before decompressing. The integrity manifest records exact compressed sizes and SHA-256 values. No binary build products are committed.

```bash
xz -dc docs/verification/plan-20-command-simplification/final-acceptance.json.xz \
  > /tmp/plan20-final-evidence.json
```

## Next Steps

1. Review the coordinated PR against `feat/agent-cli-interface`, using the completed design and these evidence files.
2. Resolve frozen historical reproduction, upstream approval, and independent search registration separately before wider rollout claims or experimental execution.
3. Decide the breaking public-interface release version before publication; do not treat the intermediate 0.1.27 patch as a compatibility promise.
