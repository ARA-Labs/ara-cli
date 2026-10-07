# Source-preserving CLI skill variants

This record describes the implementation on `feat/agent-cli-interface`, workspace
0.1.23. Its command/source contract is [agent-cli.md](../agent-cli.md).
Observed proof and unresolved gates are recorded in the delivery verification
[report](../verification/agent-cli-2026-10-02/README.md); engineering execution does not imply upstream approval or release.

## Implemented behavior

Twenty-one variant files preserve thirteen source pages and their procedures, while access clauses route knowledge/root operations through ara. Adapter pages use the actual research-manager initialization profile and complete native source/history contracts. Access hunks reconstruct complete pages and all file digests are regenerated from actual bytes.

## Command simplification

The live agent routes are `status`, `ls`, `show`, `find`, `edit`, `claim set`, `heuristic set`, `apply` and `merge`. Creation, staging, promotion and session setup/logging use existing typed JSONL operations. `show --with path,refs`, `ls --unfinished` and `show --identity` retain ancestry, citation, inactivity and exact imported-identity behavior. Tooling remains unchanged. See the [migration guide](../agent-cli.md#command-simplification-migration) for inputs and result mappings; old verification reports remain frozen historical evidence.

## Boundaries and remaining gates

Independent static access review and three actual installed-agent packaging tasks
passed; the source-bounded compiler also emitted a complete artifact with no
invented code or empirical results. Exact fixture/readback evidence is in the
protocol repository's `evaluation/agent-cli/delivery-proof/runtime-smoke.json`.
Human protocol approval and historical reproduction remain pending. These smokes
are not E0–E6 experiments or evidence of interface-only reasoning equivalence.

## Code and proof boundaries

Implementation: `Protocol skills/{research-foresight-cli,research-manager-cli,compiler-cli}; evaluation/agent-cli/{access-diff,variant-lock}.json`.
The skills now live in this repository's [`skills/`](../../skills), imported
originally from protocol commit `03f19c7`, then migrated with the live binary; see [agent-cli-skills.md](../agent-cli-skills.md).

Permanent consumer regressions: `verify-variants.py; native 107-row proof`. Final locked workspace, Clippy, wasm
and actual-release checks are linked from the delivery verification report.
