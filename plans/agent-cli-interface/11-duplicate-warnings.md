# PR 11: warn about likely duplicate nodes
**Date:** 2026-10-01

Status: **approved** 2026-10-01. Repository: `ARA-Labs/ara-cli`. Depends on [PR 03](03-guarded-node-writes.md), [PR 08](08-directory-merge.md), and [PR 10](10-keyword-search.md). Parent: [agent CLI interface](../agent-cli-interface.md). Shared checks: [PR index](README.md).

## TL;DR

Use the offline search code to warn about likely duplicates after a node addition or while planning a merge. Warnings never reject a valid write, delete nodes, or mark findings equivalent. Populate the merge report's existing `duplicate_candidates` field and give node-add callers bounded candidate identities. Keep relevance scores separate from calibrated duplicate similarity.

## Problem

ID collision repair prevents duplicate identifiers but cannot detect two different IDs describing the same finding. The parent requires nonblocking candidates for add and merge. Raw BM25 scores are unbounded and query-dependent, so the illustrative merge score of 0.91 cannot be treated as a probability or copied as a universal threshold.

## Constraints

Only the agent judges whether findings are the same. A warning does not change successful exit codes. `same_as` remains an explicit PR 07 operation; warning support must not depend on that operation being available. No embeddings, stored index, network, or automatic deduplication enters this PR.

## Proposed approach

Extend `crates/ara-cli/src/search.rs`, the PR 03 write adapter, and the PR 08 merge adapter. Keep core writer and merger APIs independent of search dependencies. Use BM25 to retrieve a bounded candidate set, then a deterministic token-overlap similarity to rank suspected duplicates. Proposed initial similarity is weighted Jaccard over normalized title and substantive body tokens, with corpus-derived term weights; review the formula, field weights, threshold, and maximum candidates using labeled pairs before publication. State explicitly that the resulting bounded score is similarity, not confidence.

1. Build a labeled set of duplicate and distinct node pairs at the pinned artifact revision, including shared-topic but different-outcome experiments and deliberate verification runs. Tune only on a designated development split, then freeze the threshold before evaluating held-out pairs.
2. Score a new node against the locked pre-write snapshot. Do not include the node itself. For batches, compare each newly created node with prior committed nodes and earlier nodes in the same staged batch so duplicates are visible without changing transaction results.
3. For merge, compare imported nodes with pre-existing ours nodes and with other imported nodes. Exclude already mapped identical entries and repeated-import identities; do not confuse idempotent import with a new semantic duplicate. Emit each unordered pair once in deterministic order.
4. Add a `--no-duplicate-check` flag and an `ARA_NO_DUPLICATE_CHECK=1` environment switch that skip scoring entirely, so PR 15's E4 can compare duplicate-node rates with and without warnings under one pinned binary. Extend node-add JSON with a documented `duplicate_candidates` array and emit human warnings on stderr, leaving the assigned ID on stdout. Populate the existing `ara.merge/v1` array with pairs and similarity scores. Respect the shared field projection without hiding transaction outcome identity.
5. Keep detection advisory if scoring cannot run: commit valid writes under the existing transaction contract and report a specific advisory-unavailable warning, not fabricated empty evidence of no duplicates. Measure advisory overhead in add and merge timings. Do not hold the exclusive lock for an unbounded all-pairs scan; reuse indexed retrieval over the guarded snapshot.

## Alternatives considered

A global BM25 cutoff is simpler but does not have a stable meaning across query lengths or corpus sizes. Candidate retrieval plus a defined similarity measure supports an interpretable threshold. Automatic merging violates the parent's requirement that agents decide.

## Tradeoffs

Keyword similarity misses paraphrases and may flag intentional repetition. The warning contract must preserve that uncertainty and retain both nodes. Threshold changes affect agent output, so require a reviewed calibration report rather than silently retuning between experiments.

## Migration

Before this PR, merge's reserved candidate array remains empty. This PR fills it without changing conflict semantics. Apply functional versioning and document scoring, threshold selection, advisory failures, and JSON additions in `docs/agent-cli.md` and the changelog. Neither warnings nor search alter the source artifact.

## Verification and acceptance

Test that the off switch removes warnings and their cost without changing writes. Test held-out duplicate versus distinct pairs, unrelated text, same title with contradictory result, intentionally repeated verification, empty content, equal-score tie order, batch duplicates, and imported-to-imported pairs. Tests assert candidate identities and absence of mutation; formula tests cover score bounds and finite serialization. Do not assert that warnings force equivalence.

Smoke add twice with near-identical findings into a disposable artifact: both nodes must exist with distinct assigned IDs, the second result must identify the first candidate, and `ara check` must still pass. Smoke a directory merge containing similar new findings: inspect `duplicate_candidates`, confirm both nodes survive, and confirm a second import does not repeat them. Run the reviewed pair-set evaluation and record both missed duplicates and false warnings. Approval requires agreed precision/recall criteria; a fabricated probability or a count-only test cannot establish accuracy.

## Next Steps

Review the similarity formula and calibration dataset before choosing the threshold. Merge this PR only after the full directory-merge report is available; it does not block CLI-only skill integration if search and write commands already meet their contracts.
