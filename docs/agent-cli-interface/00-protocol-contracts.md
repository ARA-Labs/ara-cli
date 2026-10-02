# Protocol contracts and local implementation boundary

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.22. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

The local code implements five creation kinds, typed identities, mutability exceptions, complete audit ownership and portable conflict records. The owning protocol checkout contains the F1–F7 proposal and three live schema updates. Fourteen archived baseline pages retain their pinned bytes.

## Boundaries and remaining gates

Protocol approval remains pending. Local executable proof does not approve F1–F7 or settle baseline source contradictions.

## Code and proof boundaries

Implementation: `../Agent-Native-Research-Artifact/docs/agent-cli-contracts.md; evaluation/agent-cli/protocol-decisions.json`.

Permanent consumer regressions: `merge_identity.rs, merge_yaml_layers.rs, write_logic_documents.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
