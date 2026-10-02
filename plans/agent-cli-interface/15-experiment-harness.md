# PR 15: build the external three-condition experiment harness
**Date:** 2026-10-01

Status: draft for review. Target repository: a proposed new external repository, `ARA-Labs/ara-agent-interface-eval`, with proposed local checkout `../ara-agent-interface-eval/`; neither is claimed to exist. Parent: [agent CLI interface plan](../agent-cli-interface.md). Series: [PR index and shared gates](README.md). Dependencies: [12-pin-skill-contracts.md](12-pin-skill-contracts.md) for source inventory, [13-cli-backed-skills.md](13-cli-backed-skills.md) for Files versus CLI, and [14-shared-frontier-intentions.md](14-shared-frontier-intentions.md) for the collective comparison. Inventory and experiment design can start before runnable variants; scored collection cannot.

## TL;DR

Build a separate harness that pins source skills, tasks, artifacts, prompts, CLI binaries and graders. Compare Files with CLI on reading, replay of the live project manager (PM), and compilation, then compare CLI with CLI plus frontier and intentions on collective work. Pre-register quality margins, compute limits, repetition counts and success rules before collecting scored data. Repeat entire communities and analyze each community run as the independent unit, preserving failed runs and coordination costs.

## Problem

Deterministic CLI tests establish command behavior, not cheaper or better agent research. The parent names a 450-question understanding benchmark, a scaling curve, PM session replay and parallel-fork work. It also requires compilation fidelity and a separate coordination condition. Existing paper results do not measure performance after replacing artifact access.

The accessible `../ara-paperbench/README.md` confirms an artifact collection and its structure, not the historical question/grading harness or skill revision. The external harness destination is therefore proposed rather than inferred from that collection. Historical benchmark collection remains blocked until PR 12 verifies its corresponding sources; other fully specified live experiments can proceed independently after their own prerequisites are met.

## Constraints

Keep experiment code outside ara-cli. This plan changes no shipped binary or viewer and supplies no fabricated measurement, sample size or claimed gain. The original skills remain baselines and all source/reference pages supplied to an agent count toward token cost. Preserve complete content, evidence, provenance and required history under every applicable task contract.

Within each comparison, fix model build, tools, tasks, starting artifacts, grader, total compute budget, deadlines and source revisions. Only the reviewed intervention differs. Enforce condition policy in the runner and record violations; do not silently convert CLI-only tasks to file access. The CLI remains offline and LLM-free. Model calls, shared-channel publication and any transport are external harness responsibilities, with costs attributed to the run. Follow the [shared gates](README.md) and review the external destination before implementation.

## Proposed approach

All paths in this paragraph are new proposed files in the proposed external repository. Add `pyproject.toml` with a reviewed locked runtime environment; `src/ara_agent_interface_eval/` with `manifest.py`, `runner.py`, `policy.py`, `metrics.py`, `grading.py`, `analysis.py`, and `shared_channel.py`; `configs/conditions.json`; `configs/experiments/`; `preregistration/`; `schemas/`; and `tests/`. Add `README.md` documenting the accepted procedure. The implementation first inspects any existing harness provided with the paper and reuses its task/grading loader where verified; these proposed modules do not imply that uninspected external APIs already exist.

Each run manifest records source-lock and operation-inventory digests from PR 12, variant lock and CLI commit/binary checksum from PR 13, collective-contract revision where used, corpus/task/grader hashes, complete prompt bundles, model/provider versions, pricing schedule, sampling settings, seed, paired starting artifact, run identity, budget and environment. The runner captures tool requests/results, command exit codes, exact input/output tokens, cached-token accounting, cost components, wall times, output artifacts and grading records. Secrets stay outside exported logs. A missing pin or capability produces a setup failure before consuming scored research budget.

| Condition | Supplied skills and access | Role and coordination rules |
|---|---|---|
| Files | Unchanged archived source skills and their original file tools. | Pinned source roles, with no added frontier or intention instructions. |
| CLI | Copies from the same source pins with reviewed CLI access substitutions and command pages. | Same procedures and roles as Files; no shared channel or collective extension. |
| CLI + frontier + intentions | CLI variants plus the separately pinned PR 14 extension. | Approved collective roles, shared publication/refresh and frontier reasoning; all additions are part of the intervention. |

Files versus CLI is the interface comparison. CLI versus CLI plus frontier and intentions is the collective comparison. Do not run all conditions against every benchmark. For the collective control, use the same outer task/merge schedule and approved contributor roles in both arms when necessary to make separate-fork work possible; record any departure from the source single-writer rule as a common collective-task protocol, never as a change to the interface-only CLI condition. The treated arm alone receives frontier and shared-intention prompts. If reviewer-approved role changes cannot be held equal across the collective arms, label the result as a bundled coordination-and-role intervention rather than attributing it only to intentions.

| Experiment | Required design | Outcomes and checks |
|---|---|---|
| Paper understanding | Verified paper question set and historical task/skill/grading map; paired Files/CLI runs from identical artifacts. The parent specifies 450 questions; the authoritative source must confirm this count. | Held-out answer quality, native-ref correctness, tokens, dollars and wall time. Reuse the verified paper grading procedure; grade new CLI answers, not historical results. |
| Scaling | Hold questions and answer-bearing content fixed while adding registered distractor/trace growth to 1,000 or more nodes, with size levels frozen in advance. | Quality, retrieval coverage and cost/time curves by size; report missing full-body content and timeout failures. |
| PM replay | Replay the-ara-of-ara's session inputs in chronological order with future-session evidence hidden. Both arms see the same accumulated allowed history. | Routing, closure decisions, provenance, full sessions/index/reasoning, before/after revisions, stale handling, integrity and resources. A final artifact is not a substitute for historical per-turn inputs. |
| Compilation | Paired source bundles requiring arbitrary solution prose, complete citations/appendices and tables/figures; retain original compiler validation and repair procedure. | Source content/evidence coverage, native bindings, full history where required, exact evidence fidelity and total resource cost including visual extraction. |
| Collective research | Independent communities begin from paired identical ARAs; agents work in separate forks under a fixed integration schedule using directory merge. Compare collective CLI control with treated collective extension. | Held-out research quality, accidental duplicate experiments excluding deliberate verification, artifact integrity, merge/conflict outcomes and full run resources. |

For PM replay, acquire authoritative session-by-session input transcripts and validate that reconstructed prompts do not contain future knowledge. If unavailable, block historical replay and propose a separately named prospective recording experiment for review; do not report that substitute as replay. For scaling, preserve original reference identities and verify that added material neither answers the held-out question nor deletes relevant content. Compiler tasks preserve original evidence ledgers, screenshots and source provenance, with source/evidence direct access allowed equally in both conditions.

The collective runner implements PR 14's actual shared directory at `<run-root>/shared/community/<community-id>/intentions/`. Only the external coordinator appends durable events and replaces snapshots; each fork runner publishes through it and refreshes before work selection and execution. Persist sequence, logical round, acknowledgments, expiry decisions and failure events. A common budget ledger accounts for agent reasoning, publication/refresh overhead, failed calls, merge work, graders where included and deliberate verification. Reservations do not enlarge the total allocation. Fresh-run identities and directories prevent leakage between communities.

### What must be registered before scored collection

Create an immutable registration record under `preregistration/<study-id>.json` (new) with reviewer approval and a digest published before scored runs. It must contain numeric task-quality noninferiority margins separately for understanding, scaling, PM, compilation and collective quality, with each margin's units, rubric range and scientific justification. It must also contain confidence level, uncertainty method, minimum whole-run repetitions, community size, budgets, deadlines, fixed stopping rule, exclusion/failure policy, cost/time primary endpoint, multiplicity policy and duplicate-experiment adjudication rule. No margin or sample count is approved by this draft. The runner rejects an unapproved record, nonnumeric margins or a record changed after collection begins.

For each quality endpoint, let `D` be treated minus control on the registered scale and `delta` the approved allowable loss. Quality passes only when the registered lower confidence bound for `D` is at least `-delta`. Artifact integrity and mandatory content/history/provenance fidelity are hard requirements with no tolerance for silently lost or rewritten records. Efficiency is assessed only after the corresponding quality requirement passes. A claimed cost or time reduction requires the registered uncertainty bound for its treated/control ratio to be below one; measured point estimates alone do not establish improvement. Collective duplication and research quality use their own registered criteria, not an invented expected gain.

Select numeric margins with reviewers using the verified rubric's meaningful-loss interpretation before viewing scored condition outcomes. Determine repetitions using a reviewed power/precision calculation or a disjoint feasibility pilot; archive its inputs, exclude pilot tasks/runs from scored analysis and freeze the calculation. No convenient value is inserted merely to make the experiment runnable. If a justified margin or repetition count is still absent, finish the reproducible harness and smoke proof but keep scored collection disabled.

Whole community runs are the collective sampling unit. Pair control and treated runs by task/start artifact and budget block, randomize execution order with registered seeds, and use the registered run-level paired analysis or community-level bootstrap. Contributions, turns, experiments and merges inside a community are correlated and cannot be counted as independent samples. Repeat communities from fresh state and independent registered seeds until the fixed run count, never until a favorable result. Other experiments retain their task/run hierarchy and account for repeated questions on the same artifact.

Implementation steps for the future PR:

1. Confirm or approve the external repository destination and inspect verified historical harness sources. Lock tasks, prompts, graders and corpus identities with PR 12's task map.
2. Obtain review of the condition differences, role controls, quality endpoints, numeric margins, repetition design and failure rules. Publish the immutable registration before scored collection.
3. Implement condition loading, CLI capability checks and access-policy logging. Keep all extra documentation in the token accounting.
4. Implement isolated run workspaces and deterministic budget/seed scheduling. Archive raw traces and full output artifacts without importing state between runs.
5. Implement PM replay input boundaries, scaling generator, compilation source coverage audit, and collective fork/merge workflow using the actual CLI contracts.
6. Implement the real shared-channel coordinator with durable publication/refresh, expected sequence, ownership, logical expiry, interrupted-write recovery and budget accounting from PR 14.
7. Run the smoke scenarios below, including missing capability, channel outage and budget exhaustion. Repair harness errors before freezing the scored environment.
8. Collect only registered runnable comparisons. Grade blinded outputs where compatible with the verified procedure, export all failures, and run the registered analysis without changing margins or selecting only favorable runs.
9. Publish reproducibility instructions, raw-data hashes and qualified conclusions. Supply held-out keyword-search and policy-violation evidence to [16-local-semantic-search.md](16-local-semantic-search.md) and [17-cli-write-enforcement.md](17-cli-write-enforcement.md); those remain evidence-gated follow-ups.

## Alternatives considered

Putting the harness in ara-cli would couple model/provider experimentation to an offline binary's release cycle. Comparing historical Files results with newly collected CLI results would confound the interface with model, environment and grading changes. Counting every contribution as an independent community sample would understate uncertainty.

A pilot-only cost comparison can establish feasibility but cannot answer held-out quality or collective effectiveness. The proposed harness separates feasibility smoke from pre-registered scored collection.

## Tradeoffs

Full artifacts and tool logs are larger than aggregate tables but permit fidelity and policy audits. Missing source pins may block historical reproduction while live experiments remain runnable. Under an equal total budget, coordination can reduce the treated arm's research time; include that cost in the measured intervention.

Shared-channel failures may interrupt a community. Keeping failed runs under the registered policy avoids making the treated arm appear cheaper or better by excluding costly failures after observing them.

## Migration

Import existing paper harness tasks and graders only after verifying their revisions and interfaces. Do not alter `ara-paperbench` artifacts or protocol baselines. Store new run records separately with immutable condition and registration locks. Results collected under a changed margin, prompt, role, binary or source contract belong to a new study identity, with exploratory status when the change follows outcome inspection.

## Verification and acceptance

This drafting task runs no checks. Future new unit/contract targets in the external `tests/` cover policy violations, budget accounting, run isolation, future-session leakage, pricing/version capture, channel recovery and run-level statistical units. They must check visible outcomes rather than mock command forwarding. Existing paper graders remain unchanged unless an explicitly reviewed new study requires different grading.

The actual-program smoke launches the proposed harness entrypoint `python -m ara_agent_interface_eval smoke --config <approved-smoke-config>` (new interface to implement, not an existing command) with the actual pinned CLI binary. Execute a paired reader task, a PM session/revision task and a compiler task with figures and non-template logic prose. Run a community with two separate fork processes to observe publication, refresh, expiry and final `ara merge`/`ara check` outcomes. Verify actual artifacts, valid refs, full history, measured accounting, role/access policies and fresh run isolation; smoke values are feasibility output, not scored empirical findings. Inject missing CLI coverage, corrupted channel state and exhausted budget and observe explicit setup/run failure without direct knowledge-file fallback or extra allocation.

Acceptance requires reproducible source/prompt/task/grader locks, approved numeric registration, implemented cross-fork shared-channel behavior, all applicable experiments, complete resource/fidelity output and analysis at the correct sampling unit. Historical understanding or replay prerequisites that cannot be sourced stay visibly blocked; no smaller benchmark or prospective task is silently substituted. Scored results and any scientific success claim require newly collected registered data and its actual uncertainty analysis.

## Next Steps

1. Approve the proposed external destination and verify historical benchmark and replay sources.
2. Review condition roles, numeric quality margins and repeated-community design, then freeze registration.
3. Implement and smoke the harness against completed skill/CLI contracts before collecting scored data.
