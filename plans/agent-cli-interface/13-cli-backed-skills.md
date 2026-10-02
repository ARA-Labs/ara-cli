# PR 13: add CLI-only copies of the pinned research skills
**Date:** 2026-10-01

Status: **approved** 2026-10-01 (split at approval into 13a and 13b). Target repository: `ARA-Labs/Agent-Native-Research-Artifact`. Parent: [agent CLI interface plan](../agent-cli-interface.md). Series: [PR index and shared gates](README.md). Dependencies:

- **13a (reader):** [02-read-commands.md](02-read-commands.md), [10-keyword-search.md](10-keyword-search.md), the reader rows of [12-pin-skill-contracts.md](12-pin-skill-contracts.md), and approved F6/F7 in [00-protocol-contracts.md](00-protocol-contracts.md).
- **13b (research-manager and compiler):** 13a, [06-batch-apply.md](06-batch-apply.md), the complete PR 12 inventory, and approved F3/F4 in PR 00.

Each deliverable ships when its own required operations are covered.

## TL;DR

Copy the pinned reader, research-manager and compiler skills and replace their knowledge-layer access instructions with `ara` commands. Ship in two deliverables: 13a copies the reader (research-foresight) as soon as reads and search exist, so the read experiments in PR 15 can start; 13b copies research-manager and the compiler after PR 06. Research-manager remains the live project manager (PM). Keep research procedures, evidence standards, roles and stopping criteria unchanged. Full prose, unknown source fields, provenance and every required history record must survive the change. A missing operation blocks integration and must be completed in PR 06 or approved separately; direct file fallback is not allowed.

## Why two deliverables

The research goal is cheaper and faster agent–ARA interaction without losing quality, and the research note runs the read experiments (understanding benchmark pilot and scaling curve) right after the deterministic merge tests. The reader skill needs only the read commands and search. Tying it to the write chain (PRs 03–06) and its protocol approvals would delay the headline read result by the whole write rollout. Each deliverable keeps the full coverage rule for its own skills: 13a cannot ship with an uncovered reader operation, and 13b cannot ship with an uncovered manager or compiler operation.

## Problem

The parent proposes an interface-only comparison, so shorter or different research instructions would confound the result. Current protocol reader contracts explicitly read native files and forbid an index layer. The inspected compiler writes arbitrary method prose and complete source-grounded artifacts, while PM records full revisions and continuity. A short list of node, claim and session commands is insufficient evidence that these skills can run through the CLI.

The current reader contract also declares that only research-manager writes canonical logic. CLI access does not grant readers or workers new authority. Collective roles and frontier instructions belong to [14-shared-frontier-intentions.md](14-shared-frontier-intentions.md), not to this PR.

## Constraints

Use unchanged baselines and source/reference closure from PR 12, never the installed skill version. Preserve reasoning steps, closure decisions, conservatism, contradiction adjudication, evidence extraction, complete source coverage and native grounding refs. Count command documentation and every supplied page in experiment token cost. The CLI remains offline and LLM-free; agent reasoning still chooses what to read or conclude.

Knowledge-layer reads and writes must use the CLI. Direct access to input sources, code and evidence bodies remains allowed and retains the source skill's evidence rules. `PAPER.md`, initialization and any knowledge document outside the three standard directories require the explicit inventory and approved bounded operations in PR 06. This PR neither implements those operations nor quietly exempts them. Refer to the [shared gates](README.md) for the cross-repository integration and draft approval requirements.

## Proposed approach

Create `skills/research-foresight-cli/`, `skills/research-manager-cli/`, and `skills/compiler-cli/` (new proposed paths) in the protocol repository by copying their corresponding pinned trees. Preserve every loaded reference page and its precedence rule. Create `evaluation/agent-cli/variant-lock.json`, `access-diff.json`, `command-reference.md`, and `smoke-tasks/` (new). The lock records baseline pin, CLI commit/version, protocol decision revision, variant file digests and the reviewed access diff. The diff maps each changed source clause to an operation row in `evaluation/agent-cli/operation-coverage.json` from PR 12, using `ara.skill-operations/v1`.

| Existing access step | Proposed replacement | Required behavior |
|---|---|---|
| Read/grep structural entries and relations | `ara status`, `ls`, `show`, `path`, `refs`, `open`, and `find`, with `--json` when parsed. | Select complete required objects and distinguish structured references from possible prose mentions. |
| Read cited bodies | `ara show --full` with the reviewed native entry or path/section selector. | Preserve original grounding anchors and complete text, including arbitrary logic bodies and unknown fields. If the selector/body is unavailable, stop integration. |
| Pick a free ID and append entries | Corresponding `ara add`, claim/heuristic operations or `ara apply`. | CLI assigns IDs and provisional batch bindings resolve; the skill never scans files to choose an ID. |
| Stage or promote | `ara stage`, `ara promote` or equivalent batch operations. | Preserve original closure signal, raw observation, provenance and linked trace/history; no new maturity judgment. |
| Reconcile logic and session history | Reviewed edit/set operations and one batch for related changes. | Current logic and verbatim `before`/`after`, signals, revision pointers, session arrays/index and PM reasoning stay consistent. |
| Compile and repair | Bounded initialization/document operations through `ara apply`. | Cover every required logic body, root content, source-supported DAG and repeated coverage repair; no loss of appendix, citation or evidence content. |
| Read/write source and evidence bodies | Original direct tools. | Retain screenshots with descriptions, full raw tables/figures, logs, implementation pointers and source quotes. |

The reader's copied `references/CONTRACT.md`, `RETRIEVE.md`, and `PREDICT.md` must agree on the new access path. Amend the copied ban on an index only enough to permit `ara find` as a retrieval aid. Keep the agent's relevance judgment, native-ref honesty, uncertainty disclosure and read-only output contract. A search excerpt is not sufficient proof for a cited assertion; the reader retrieves and verifies its full source body before answering.

The PM copy keeps end-of-turn cadence and single-writer authority. It must write all session fields, including `ai_actions`, `claims_touched`, `logic_revisions`, `key_context`, `open_threads`, `ai_suggestions_pending`, rolling metadata and index counts. It must log rejected signals and preserve taste records. A split/merge/generalization retains original and new identities, revised references and complete prior values. Approved F4 exceptions control stale metadata and session mutation, not a catch-all editable field option.

The compiler copy keeps the original input reading, epistemic reasoning, evidence ledger, visual extraction, citation coverage, validation and repair loop. CLI transactions cover knowledge documents; source and evidence files remain direct. This boundary is not a transaction across source/evidence files, and a failed knowledge batch must not be reported as a complete compiled artifact. Run the source skill's full validation after successful generation and report any real failures under its original rules.

Implementation steps for the future PR:

Run steps 1–7 for the reader skill as deliverable 13a, then again for research-manager and the compiler as deliverable 13b. Each run produces its own variant lock.

1. Read the verified source lock and task map; copy each selected skill tree without altering procedures.
2. Freeze the operation coverage artifact used for this variant revision. Review every required row against the pinned CLI version and fail integration on proposed/blocked coverage.
3. Replace only artifact access clauses and the minimum command documentation needed to execute them. Label every changed clause with its source and operation references in the access diff.
4. Review native selectors, full-body recovery, errors and role boundaries. Match exit status 1 for rejected/unknown operations and 2 for command setup failure; the skill must report failure and follow its original stopping/repair rule without direct fallback or automatic semantic retries.
5. Execute representative reader, writer and compiler tasks against disposable artifacts through the actual CLI. Capture complete artifacts and access logs for fidelity review.
6. Review the semantic instruction diff with a second reviewer. Reject any change to research procedure, evidence standards, coordination or authority from the CLI-only copy.
7. Publish the variant lock and command pages for [15-experiment-harness.md](15-experiment-harness.md), preserving Files separately.

## Alternatives considered

Editing the original skills would erase the evaluation baseline. A mixed CLI/file-access variant would obscure whether missing operations or the interface caused the outcome. Simplifying PM history or compiler generation would make the variant runnable earlier while changing the task being evaluated.

Shipping all three copies together was the draft design. It made the reader wait for write coverage it never uses. The split keeps the coverage rule per skill: no skill is labeled integrated until its own paths work end to end.

## Tradeoffs

Command pages add context cost, which the harness must count. Research tasks that require complete source material can still need large full-body reads; truncating them to achieve a token target would change the task. Integration waits for missing initialization or arbitrary-body operations so the experiment preserves its source contracts.

## Migration

Original skills and immutable baseline archives remain available with their existing names. CLI variants use separate names and explicit pin/version locks. A CLI wire-format change requires a new reviewed variant lock and command reference. Do not rewrite already collected evaluation logs or repin a source after observing results.

The copied skills do not introduce shared intentions, frontier prioritization or multiwriter roles (Q5 keeps the single writer for these copies). Those are separately versioned instructions in PR 14 and a separate experimental condition.

## Verification and acceptance

This drafting task runs no tests or binary commands. Future proposed contract tests under `evaluation/agent-cli/` check consumer-visible fidelity: full arbitrary-body recovery, native grounding, provenance, session continuity, logic revision before/after records, promotion links, unknown source fields and rejection without partial mutation. Tests must compare required semantic content and history, not assert that a prompt contains a particular command string.

Actual-binary smoke scenarios are: answer a mapped reader question using `ara find` then `ara show --full --json`; execute a PM turn that stages, later promotes and revises an entry with complete history; compile a source requiring a non-template solution file and figures, then repair a coverage gap. Run the actual source validation including `ara check <artifact>` where appropriate. Observe correct native refs, complete files and history, unchanged read-only/single-writer roles, and no direct knowledge-layer file access. Inject an unsupported operation and verify the task reports the missing capability, leaves the knowledge transaction unchanged and does not fall back to Edit/Write. All these scenarios are future integration proof, not executions during this drafting task.

13a acceptance requires a reviewed access-only diff for every loaded reader page, covered reader inventory rows with pinned proof, and a successful representative reading task. 13b acceptance requires the same for research-manager and compiler pages, plus successful representative PM writing and compilation. Required history and artifact fidelity must be audited against the source skill. Performance equivalence remains unproven until PR 15 measures both conditions anew.

## Next Steps

1. Start 13a once PRs 02 and 10 are merged and PR 12's reader rows are pinned; freeze its variant lock for PR 15's read experiments.
2. Resolve the manager and compiler coverage gaps in PR 06 and protocol contradictions in PR 00, then build 13b.
3. Run and review the representative tasks for each deliverable before freezing its variant pins for the external harness.
