# Chronological equivalence pointers

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.23. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

same_as is an explicit annotation from a later node to a provably earlier existing node. Recorded timestamps or actual node.add order establish chronology; IDs and DFS order do not. Reads expose equivalence without collapsing source records.

## Boundaries and remaining gates

Self, dangling, cyclic and unproved-order links reject. Duplicate advice never creates equivalence.

## Code and proof boundaries

Implementation: `crates/ara-core/src/write/node.rs; crates/ara-core/src/{parse,query}.rs`.

Permanent consumer regressions: `write_node_engine.rs, parse_fixtures.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
