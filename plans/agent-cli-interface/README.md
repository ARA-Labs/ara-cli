# Agent CLI rollout and remaining acceptance

The non-experiment implementation has permanent [design records](../../docs/agent-cli-interface/README.md)
and an [actual frozen-binary verification report](../../docs/verification/agent-cli-2026-10-02/README.md).
Workspace version remains 0.1.22. Functional code/tests/docs are prepared, with
performance, historical compatibility and external review/runtime gates still open.
No merge, tag, release or final PR is performed by this delivery task.

| Plans | Current state |
|---|---|
| 01, 03–05, 07, 09–11 | Feature proof complete; plans retired into design records |
| [00](00-protocol-contracts.md) | Local schemas/proposal implemented; human upstream approval pending |
| [02](02-read-commands.md) | Reads/corpus/source proof complete; first-sample timing gate pending |
| [06](06-batch-apply.md) | 107-row native operation proof accounted; unchanged historical replay blocked |
| [08](08-directory-merge.md) | Functional directory/Git/replay proof complete; 10k timing gate fails locally |
| [12](12-pin-skill-contracts.md) | Verified live pin/archive; historical subset and baseline runtime gate unresolved |
| [13](13-cli-backed-skills.md) | Byte/access adapters prepared; fresh independent review and installed runtime pending |
| [14](14-shared-frontier-intentions.md) | Same-host channel engineering smoke passes; approval/agent condition pending |
| [15](15-experiment-harness.md) | Deferred by user; no E0–E6 experiment claimed |
| [16](16-local-semantic-search.md), [17](17-cli-write-enforcement.md) | Closed conditionals for this delivery; evidence gates have not fired |

The integration owner runs actual browser UI and installed-agent smokes and creates
the final PR. The final CLI PR targets main and must use a merge commit, preserving
the integration branch history. The owning protocol repository has its own branch/PR.
Public Rust source changes and additive optional JSON compatibility are distinguished;
the minor/major integration release decision stays pending.
