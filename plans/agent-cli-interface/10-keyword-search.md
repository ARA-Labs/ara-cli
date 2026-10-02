# PR 10: add offline keyword search
**Date:** 2026-10-01

Status: **approved** 2026-10-01. Repository: `ARA-Labs/ara-cli`. Depends on [PR 02](02-read-commands.md), not on writes or merge. Parent: [agent CLI interface](../agent-cli-interface.md). Shared checks: [PR index](README.md).

## TL;DR

Ship `ara find` with in-memory BM25 keyword ranking over the agent read model. Return entry identities, scores, source locations, and short excerpts using the shared discovery and JSON rules. Start with no persistent index or network dependency. Evaluate ranking on reviewed real-artifact questions and keep search available independently of later duplicate warnings.

## Problem

Structural queries find known IDs and relations, but agents also need to locate entries by topic. Reading the entire tree for each topic wastes output. The parent requires a local keyword ranker and permits a cache only if the 10,000-node timing gate proves one is needed.

## Constraints

Search stays in `ara-cli`, including its ranking dependency, so wasm does not inherit it. Search has no LLM, embeddings, model downloads, or hosted API. Rank every supported entry kind, including staging and experiment plans, while never indexing arbitrary `src/` or evidence body contents. Index exhibit metadata and typed evidence pointers already loaded by the model.

Use exact current IDs and existing scoped document/term identities; no synthetic protocol IDs. Search emits `format: ara.find/v1`. `--type`, `--limit`, `--fields`, `--full`, `-C`, and `ARA_DIR` follow PR 02. Invalid limits, unknown kinds, empty queries, and malformed artifacts produce defined errors. A valid query with no match returns an empty result and exit 0.

## Proposed approach

Add `crates/ara-cli/src/search.rs` (new) and export its native ranking entry point through the thin CLI `lib.rs` introduced in PR 02. Command handlers and duplicate detection call that same entry point. Keep clap types and printing outside the ranking API. Extend `main.rs` and shared output handling. A proposed API accepts an iterable of borrowed entry views, query text, kind filter, and result limit; it returns identities, finite scores, match offsets, and snippets.

1. Define one searchable document per entry, with title and supported full prose fields plus source location. Avoid indexing `heuristics.md` both as a raw recipe and as typed H entries; retain non-entry introductory prose as a separate scoped document only if relevant to the published read contract.
2. Implement Unicode-aware token boundaries and deterministic case normalization. Preserve ID tokens and document the punctuation rules. Use BM25 with proposed initial `k1 = 1.2` and `b = 0.75`; these are design parameters, not measured results. Score only matching documents and keep score computation deterministic. Review tokenizer and parameters before freezing the ranking fixture.
3. Rank using corpus-wide statistics computed before type filtering so the same document's score does not change merely because a kind filter changes. Bound top-k storage instead of sorting copied full bodies. Break equal-score ties by stable kind and source identity, with a final exact-ID comparison. Do not serialize NaN or infinite scores.
4. Generate snippets around matching terms, using the common one-line prose bound. Preserve offsets into the original UTF-8 text so snippets cannot cut a character. Return `{"format":"ara.find/v1","results":[{"id":"N12","kind":"question","score":1.5,"source":"trace/exploration_tree.yaml","excerpt":"..."}]}` as an illustrative schema, not a fixture expectation.
5. Add a reviewed relevance set to the pinned artifact fixtures. Record query, acceptable result IDs, rationale, and corpus revision. Split tuning questions from held-out questions and report recall at 10, the fraction of labeled relevant entries returned in the first ten results. Agree a minimum on the reviewed dataset before tuning; do not invent a quality result or use the same questions to tune and claim generalization.
6. Run the read-command timing procedure at 100, 1,000, and 10,000 nodes. If in-memory ranking misses the parent budget, document the measured cause before proposing a stored index. A cache implementation is a separately reviewed conditional follow-up, not an unconditional dependency of this PR.

## Alternatives considered

Tantivy provides persistent indexing but adds cache invalidation and storage before there is evidence those costs help. Start with a small in-memory ranker or a focused BM25 library after inspecting its allocation and dependency behavior. Local semantic search belongs to [PR 16](16-local-semantic-search.md) only after experiments establish keyword misses.

## Tradeoffs

BM25 matches terms and can miss paraphrases. Keep this limit visible in relevance reports; it does not justify changing agent reasoning or silently calling an external model. Building statistics each invocation trades repeated local computation for a cache-free source-of-truth contract.

## Migration

Search is additive and artifacts remain unchanged. Add CLI crate library code only as needed for direct reuse; do not move unrelated serve/check code. Apply the functional patch, Cargo.lock, changelog, and `docs/agent-cli.md` rules. Search-only dependencies must not appear in core's no-default-features wasm graph.

## Verification and acceptance

Unit tests cover a rare-term ranking above a common-term match, length normalization, repeated terms, Unicode, punctuation, type filtering, equal-score ordering, empty corpora, and no-match results. Consumer-level tests check exact ranked identities for reviewed examples; do not pin incidental floating-point formatting. Add subprocess tests in `crates/ara-cli/tests/agent_search.rs` (new, proposed target) for limits, kinds, projection, JSON error placement, and discovery precedence.

Smoke the prebuilt binary with `ara -C <fixture> find "<reviewed query>" --limit 10 --json`, then pass returned IDs to `show --full` and confirm the matched prose and source entry. Repeat from a nested directory without `-C`. Run held-out relevance and process-inclusive timing gates; acceptance requires the approved recall threshold, no network use, and the parent under-100-ms real / under-1-s 10,000-node limits.

## Next Steps

Review document inclusion, tokenizer rules, ranking parameters, and the relevance dataset. This PR can merge while writes and directory merge are under development. [PR 11](11-duplicate-warnings.md) reuses its ranking code without changing the `find` contract.
