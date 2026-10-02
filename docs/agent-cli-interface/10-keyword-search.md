# Stateless keyword retrieval

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.23. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

find tokenizes maximal Unicode alphanumeric runs and lowercases Unicode. BM25 k1=1.2/b=0.75 ranks loaded knowledge once per invocation. In-memory posting lists preserve the original score formula, lexical summation order and tie ordering. Raw source/evidence bodies are excluded.

Corpus-local scoring cells reuse document normalizations and term weights while a
snapshot is unchanged. Appending a document invalidates corpus-dependent values;
stable term positions retain lexical accumulation order without per-candidate
weight allocation. Exact-reference regressions cover repeated queries and append
transitions. No scoring state survives the invocation.

## Boundaries and remaining gates

Frozen labels/criteria precede measurement. Four singleton queries per split establish bounded functional recall only; no model, network or persistent index is introduced.

## Code and proof boundaries

Implementation: `crates/ara-cli/src/search.rs; crates/ara-core/tests/fixtures/agent-cli/search/`.

Permanent consumer regressions: `search.rs unit tests; scripts/test_agent_cli_acceptance.py`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
