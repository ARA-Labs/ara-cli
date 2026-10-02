# Observed integration verification, 2026-10-02

Functional engineering checks pass for the non-experiment interface, with
remaining timing, historical compatibility and approval gates. Overall acceptance
is **not PASS**. The frozen binary, exact invocations, source/final-state proofs
and observed metrics are recorded here; no E0–E6 experiment is claimed.

| Check | Observed result |
|---|---|
| Locked workspace suite | PASS; all targets after integration fixes |
| Strict workspace/all-target Clippy | PASS under Rust 1.94.1 |
| Native+wasm and pure wasm checks | PASS |
| Manual embedded viewer regeneration/freshness | PASS with wasm-bindgen 0.2.126 |
| Release build | PASS, 0.1.22; hash/path in binary.json |
| 107-row operation inventory | 88 required CLI operations PASS; 19 explicitly permitted direct/reference/output cases |
| Broad/deep 100/1k/10k reads | Functional PASS; every 10k read sample below 1 s |
| Pinned reads | Functional PASS; one cold status timing sample misses 100 ms |
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
| Revised independent access review | Pending; prior byte review is not approval of revised bytes |
| Actual browser/installed-agent runtime | Owner-run gates pending; final PR creation also belongs to owner |

`acceptance.json.gz` is the lossless complete JSON proof, including source and
final-state data. `evidence-integrity.json` records its uncompressed and compressed
digests. `observed-checks.json` is the concise derived summary. Decompress the full
proof to inspect individual invocations and operation rows; the summary does not
replace raw evidence. `race.json`, `layout-rss-writes.json` and `community.json`
record independent actual-process smokes with the same frozen binary.

Search evidence is bounded functional acceptance. It does not establish natural
paraphrase/duplicate quality, population recall, agent reasoning equivalence or
historical paper reproduction. The historical source lock still lists 465 native
questions versus 450 reported published questions. Archived baselines are immutable;
three live protocol pages intentionally carry the approved local schema changes.

Public Rust constructor/type changes are separate from additive optional JSON
compatibility. The integration minor/major release decision is pending. The final
CLI PR must merge into main with a merge commit; the protocol repository remains
separate. This preparation performs no merge, tag, release or final PR creation.
