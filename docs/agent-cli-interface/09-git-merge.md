# Local Git captures through the shared merge engine

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.23. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

Git mode pins local refs and merge base, captures exact filter-neutral tree/blob bytes into temporary snapshots, and supplies the same native merger. Dirty destination files remain the source for ours; index, HEAD, refs and merge-state files stay unchanged. Git setup clocks are separate from native clocks.

## Boundaries and remaining gates

No checkout, fetch, shell interpretation, merge, tag or release is part of this helper. Capability/source errors are explicit. Full pipeline timing differs from directory setup cost.

## Code and proof boundaries

Implementation: `crates/ara-cli/src/merge/git.rs; crates/ara-cli/src/merge.rs`.

Permanent consumer regressions: `agent_git_merge.rs`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
