# Immutable baseline packaging and operation inventory

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.22. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

The protocol package pins fourteen complete pages across reader, research-manager and compiler, including reference closure and task/source pointers. Packaging verifies archived bytes against the actual full Git object and enumerates all 107 required operation rows.

## Boundaries and remaining gates

The live pin is not a historical paper reproduction. 465 available native questions versus 450 reported published questions remains unresolved. Three live protocol pages intentionally differ from archived baselines.

## Code and proof boundaries

Implementation: `Protocol evaluation/agent-cli/{source-lock,task-skill-map,operation-coverage}.json; verify-packaging.py`.

Permanent consumer regressions: `Packaging checks with --git-source`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
