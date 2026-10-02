# Nonblocking lexical duplicate advice

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.22. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

Node additions and merge report bounded candidate identities through weighted set Jaccard. Threshold 0.8, 64 retrieval candidates and ten output candidates remain fixed. --no-duplicate-check removes advisory work while preserving mutation decisions.

## Boundaries and remaining gates

Similarity is not a probability. Both identities survive; equivalence remains explicit. The held-out crafted controls do not prove natural duplicate or paraphrase quality.

## Code and proof boundaries

Implementation: `crates/ara-cli/src/search.rs; crates/ara-cli/src/{write,merge}.rs`.

Permanent consumer regressions: `agent_writes.rs, search.rs unit tests`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
