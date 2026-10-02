# Guarded native node and edge appends

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.23. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

The native engine snapshots inputs under a persistent checkout lock, allocates IDs including retired history, plans exact source spans and validates complete declared changes. Flow and block parents support bounded targeted appends; aliases and tagged targets remain guarded. Additions require all source payloads for the five kinds, and dead ends remain leaves.

## Boundaries and remaining gates

Explicit IDs do not bypass creation validation. Original scientific dialects are not silently converted. Locks coordinate cooperating CLI processes in one checkout.

## Code and proof boundaries

Implementation: `crates/ara-core/src/write/{node,source,positions,intent,transaction}.rs`.

Permanent consumer regressions: `write_node_engine.rs, write_transaction_engine.rs, agent_writes.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
