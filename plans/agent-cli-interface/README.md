# Agent CLI rollout and remaining acceptance

The non-experiment implementation has permanent [design records](../../docs/agent-cli-interface/README.md)
and an [actual frozen-binary verification report](../../docs/verification/agent-cli-2026-10-02/README.md).
Workspace version is 0.1.23 after reconciling main's annotation release. The non-experiment
features are implemented; fixed performance measurement and final delivery are being
recorded. Historical reproduction and human protocol approval remain distinct gates.

| Plans | Current state |
|---|---|
| 01–05, 07, 09–11 | Feature proof complete; plans retired into design records |
| [00](00-protocol-contracts.md) | Local schemas/proposal implemented; human upstream approval pending |
| [06](06-batch-apply.md) | 107-row native operation proof accounted; unchanged historical replay blocked |
| [08](08-directory-merge.md) | Directory/Git/replay engineering proofs and fixed 10k timing pass; unchanged historical creation replay blocked |
| [12](12-pin-skill-contracts.md) | Verified live pin/archive; historical subset and baseline runtime gate unresolved |
| [13](13-cli-backed-skills.md) | Independent static access review and installed reader/PM/compiler packaging smokes pass |
| [14](14-shared-frontier-intentions.md) | Final-release two-process channel engineering smoke passes; scientific agent conditions deferred |
| [15](15-experiment-harness.md) | Harness/collection remain deferred; revised external-repo, three-submodule, and sharing-disabled-control plan pending review; no E0–E6 experiment claimed |
| [16](16-local-semantic-search.md), [17](17-cli-write-enforcement.md) | Closed conditionals for this delivery; evidence gates have not fired |

Actual browser and installed-agent smokes are recorded in the verification report.
The final CLI PR targets main and must use a merge commit, preserving the integration
branch history. Protocol [PR #38](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/pull/38)
stays draft; experiments use its `feat/agent-cli-interface` branch pinned to an
exact commit in the external harness submodule. Protocol merge and upstream F1–F7
approval are not prerequisites for these experimental conditions. Original baseline
archives, historical-source checks and scored-registration requirements still apply.
No PR is merged, tagged, or released by this delivery task.
Public Rust source changes and additive optional JSON compatibility are distinguished;
the minor/major integration release decision stays pending.
