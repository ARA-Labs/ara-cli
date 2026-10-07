# Agent CLI rollout and remaining acceptance

The non-experiment implementation has permanent [design records](../../docs/agent-cli-interface/README.md)
and an [actual frozen-binary verification report](../../docs/verification/agent-cli-2026-10-02/README.md).
Workspace version is 0.1.26 after plan 18's pilot follow-ups. The non-experiment
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
| [18](18-failed-blocked-calls.md) | Approved 2026-10-04; CLI changes B1–B10 and skills S1–S5 implemented in 0.1.24; H1 landed in ara-eval; 120-session dev pilot recorded 2026-10-06, with skill and `show --source` follow-ups in 0.1.26; H2/C1 and viewer embed rebuild pending |
| [19](19-deterministic-bookkeeping.md) | Approved 2026-10-04; revised and re-approved 2026-10-05 (bug reproduced, clock and day-count rules tightened, split into sub-PRs 19a–19e): 19a–19e implemented in 0.1.25 with [CLI smokes](../../docs/verification/plan-19-bookkeeping/README.md); repeated six-skill audit (acceptance item 6) and upstream protocol review of the event `target` field pending |

Plan 18's CLI and skill changes are implemented; its external changes and pilot evidence are not, so it stays a plan. Plan 18 supersedes the parent plan's default-JSON agent workflow with brief text while retaining programmatic JSON access, and [the command reference](../../docs/agent-cli.md) documents the shipped behavior. Plan 19 is an approved design revision, not a completed feature; it moves deterministic recording work into the CLI without delegating research judgment to it.

Their functional sub-PRs target `feat/agent-cli-interface` and are squash-merged there under the [parent rollout policy](../agent-cli-interface.md#order-of-work). Harness and corpus changes stay in their owning repositories. These approvals do not alter frozen experiment conditions, approve upstream protocol changes, authorize a paid run, or create a commit or PR.

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
