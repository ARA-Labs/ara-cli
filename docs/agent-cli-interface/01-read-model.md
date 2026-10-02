# Complete optional layers and pure parsing

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.22. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

Manifest adds optional observations, sessions, heuristics, plans, taste and complete claim prose. Native detailed loads distinguish missing files, unreadable files and incomplete normalization, retaining exact source documents. Pure tree parsing uses bounded iterative extraction and destruction.

## Boundaries and remaining gates

Missing optional fields remain absent in JSON. Published experiment fields.status remains typed; non-experiment node status is optional. Public Rust constructors changed, so the integration release decision remains pending.

## Code and proof boundaries

Implementation: `crates/ara-core/src/{manifest,parse,agent_layers,flat_yaml}.rs`.

Permanent consumer regressions: `parse_fixtures.rs, deep_tree_parse.rs, claim_redirect_parse.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
