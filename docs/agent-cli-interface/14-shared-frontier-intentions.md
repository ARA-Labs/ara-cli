# Separate shared-channel coordination extensions

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.22. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

Frontier and intentions are separately installable from interface-only skills. Two independent processes use one same-host shared channel with revision CAS, acknowledgments, logical-round leases, outage/recovery and idempotency. Their native reads invoke the actual CLI on unchanged copied fixtures.

## Boundaries and remaining gates

No multi-host, model-budget experiment, writer-authority approval or E6 result is claimed. Collective files remain separate from the interface-only variants.

## Code and proof boundaries

Implementation: `Protocol skills/collective-*-cli/; evaluation/agent-cli/community-smoke-scenarios/`.

Permanent consumer regressions: `test_community_channel.py, two_process_smoke.py`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
