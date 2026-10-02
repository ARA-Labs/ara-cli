# Native logic and bounded document editing

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.22. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

Claim and heuristic creation/setters, typed document selectors, guarded structural edits and whole-body revisions share source/digest validation. Long CLI values support @file, @-, and @@ escaping; JSONL values remain literal. Claims retain audited withdrawal/merge entries.

## Boundaries and remaining gates

Unknown fields, arbitrary source prose and unrelated spans survive. Native concept names keep their own namespace. Compiler heuristics preserve singular Source and full Bounds without invented PM fields.

## Code and proof boundaries

Implementation: `crates/ara-core/src/write/{logic,fields,documents}.rs; crates/ara-cli/src/write.rs`.

Permanent consumer regressions: `write_logic_documents.rs, agent_writes.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
