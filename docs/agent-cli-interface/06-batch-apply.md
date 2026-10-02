# One-snapshot typed batches and operation coverage

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.23. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

Strict JSONL decoding preserves physical line numbers and rejects duplicate keys. Ordered operations share typed provisional bindings and one working snapshot. References substitute in declared fields while prose stays literal. The compiler/research-manager union includes bounded initialization, complete documents, root edits, structural transitions and coupled audit history.

## Boundaries and remaining gates

Native operation proof covers the 107-row inventory: 88 required CLI operations plus 19 explicit permitted access/output cases. Unchanged historical replay is blocked by recorded dialect/ancestor/leaf incompatibilities and missing session-index files; no historical-import API is invented.

## Code and proof boundaries

Implementation: `crates/ara-core/src/write/{mod,batch,intent}.rs; scripts/agent-cli-acceptance.py`.

Permanent consumer regressions: `write_batch_engine.rs, agent_writes.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
