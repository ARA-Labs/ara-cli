# Observed integration verification, 2026-10-02

Functional engineering checks and every mandatory performance budget pass for
the non-experiment interface; historical compatibility and approval gates remain.
Overall acceptance is **not PASS**. The frozen binary, exact invocations, source/final-state proofs
and observed metrics are recorded here; no E0–E6 experiment is claimed.

The table and `acceptance.json.gz` below retain the original 0.1.22 binary's
attribution. They are not relabeled as proof of the later 0.1.23 optimizations.

| Check | Observed result |
|---|---|
| Locked workspace suite | PASS; all targets after integration fixes |
| Strict workspace/all-target Clippy | PASS under Rust 1.94.1 |
| Native+wasm and pure wasm checks | PASS |
| Manual embedded viewer regeneration/freshness | PASS with wasm-bindgen 0.2.126 |
| Release build | PASS, 0.1.22; hash/path in binary.json |
| 107-row operation inventory | 88 required CLI operations PASS; 19 explicitly permitted direct/reference/output cases |
| Broad/deep 100/1k/10k reads | Functional PASS; every 10k read sample below 1 s |
| Pinned reads | Functional PASS; first status invocation misses 100 ms; OS cache state not controlled |
| True corpus | 32 read sweeps and 32 layout calls; defined errors retained, source bytes unchanged |
| Native writes/resources | Broad/deep 100/1k/10k source/readback checks PASS; actual per-child RSS in layout-rss-writes.json |
| Same-checkout concurrency | 8 processes × 50 appends: 400 unique nodes and exact payloads PASS |
| Directory/Git merge 100/1k/10k | Source/reference/replay and Git-state proofs PASS; 10k directory timing FAIL |
| 100k merge/Git | Report-only; actual timeouts, failed invocations and setup error retained |
| Keyword controls | Recall@10 1.0 on each four-query split, measured after frozen review |
| Duplicate controls | Precision/recall 1.0 on each bounded labeled split; one crafted positive per split |
| Shared channel | Real two-process CAS/ack/outage/recovery/idempotency and CLI reads PASS |
| Packaging/variants | 14 archived pages/3 pins/107 operations; 21 files/13 pages/13 collective files PASS |
| Original-node replay | BLOCKED by strict creation/ancestor/leaf dialect incompatibilities; no source invention or skipped nodes |
| Human protocol approval | Pending F1–F7 |
| Revised independent access review | PASS for 21 primary paths and 9 selected consumers; not human/scientific approval |
| Actual browser/installed-agent runtime | Browser thinking/result/glossary PASS; installed reader, PM revision, and source-bounded compiler PASS |

`acceptance.json.gz` is the lossless complete JSON proof, including source and
final-state data. `evidence-integrity.json` records its uncompressed and compressed
digests. `observed-checks.json` is the concise derived summary. Decompress the full
proof to inspect individual invocations and operation rows; the summary does not
replace raw evidence. `race.json`, `layout-rss-writes.json` and `community.json`
record independent actual-process smokes with the same frozen binary.

## Final source-bound runtime

`final-release-binary.json` pins 0.1.23 source commit
`4f70972cb68aaec122148cf98421dda29f3061e4` and executable SHA-256
`edb181c62e4a275bfaae4e464aea83d54cafb125739b0fb8fbd2385531912734`.
Its locked workspace suite passes 835 tests across 36 suites (one ignored).
Strict all-target Clippy, native+wasm/pure wasm checks, manual viewer regeneration,
freshness, release build and the acceptance harness's 16 tests pass.

The final complete runner is losslessly archived as `final-acceptance.json.xz`,
with digests in `final-acceptance-integrity.json` and the bounded derived result in
`complete-acceptance-edb181c62e4a275b.json`. All mandatory budgets pass, including
pinned status at 16.05–16.87 ms and five 10k directory merges at
885.38, 881.46, 868.62, 888.90 and 894.49 ms against the unchanged 1,000 ms limit.
`acceptance_complete` is true but `passed` is false: nine report-only 100k
merge/Git failures and the strict historical creation replay remain recorded,
alongside the pending human contract approval. No failed measurements are discarded.
Use `xz -dk final-acceptance.json.xz` to recover the complete JSON source/final-state
proof; the small summary is not a substitute for it.

`final-release-installed-agent-replay.json` records complete PM before/after,
session/index/reasoning readback equal to the earlier actual installed-agent run,
plus a clean check of the source-bounded compiler output. It is a binary replay,
not a newly claimed model experiment. `final-release-browser.json` and its
screenshots record actual thinking, display-only artifact pointer, focused
glossary, experiment result and bound claim/falsification surfaces, with no browser
errors or external pointer fetch. The protocol repository's
`delivery-proof/community-smoke.json` records two distinct final-release processes,
26 acknowledged channel transitions, expiry/outage/recovery and unchanged knowledge.

`merge-optimization-observations.json` retains every focused optimization sample,
including failures; those probes do not replace the full preservation/replay runner.


Search evidence is bounded functional acceptance. It does not establish natural
paraphrase/duplicate quality, population recall, agent reasoning equivalence or
historical paper reproduction. The historical source lock still lists 465 native
questions versus 450 reported published questions. Archived baselines are immutable;
three live protocol pages intentionally carry the approved local schema changes.

Public Rust constructor/type changes are separate from additive optional JSON
compatibility. The integration minor/major release decision is pending. The final
CLI PR must merge into main with a merge commit; the protocol repository remains
separate. Protocol implementation [PR #38](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/pull/38)
stays draft alongside [CLI implementation PR #99](https://github.com/ARA-Labs/ara-cli/pull/99).
The researcher authorized experiments on the protocol branch pinned by exact
submodule commit; upstream approval and protocol merge do not block those runs.
The recorded engineering acceptance result remains unchanged, and this permission
does not claim historical reproduction, scored results or release approval.
No PR is merged, tagged or released by this delivery.
