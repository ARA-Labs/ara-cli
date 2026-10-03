# PR 15: build the external experiment harness
**Date:** 2026-10-01

Status: **approved** 2026-10-01 (arms, metrics and experiments aligned at approval with the CLI-Mediated ARA research note and the Agora comparison note). Target repository: a proposed new external repository, `ARA-Labs/ara-agent-interface-eval`, with proposed local checkout `../ara-agent-interface-eval/`; neither is claimed to exist. Parent: [agent CLI interface plan](../agent-cli-interface.md). Series: [PR index and shared gates](README.md). Dependencies: [12-pin-skill-contracts.md](12-pin-skill-contracts.md) for source inventory; [13-cli-backed-skills.md](13-cli-backed-skills.md) deliverable 13a for E1 and E2 and 13b for E3; [08-directory-merge.md](08-directory-merge.md), [11-duplicate-warnings.md](../../docs/agent-cli-interface/11-duplicate-warnings.md) and 13b for the E0 agent suite and E4; [14-shared-frontier-intentions.md](14-shared-frontier-intentions.md) for E6. Inventory and experiment design can start before runnable variants; scored collection for each experiment starts when its own dependencies are met.

## TL;DR

Build a separate harness that pins source skills, tasks, artifacts, prompts, CLI binaries and graders. Test the research claim that the CLI makes agent–ARA interaction cheaper (tokens, dollars) and faster (wall clock) without losing quality. Compare Files with CLI on reading, scaling, replay of the live project manager (PM), compilation, merge conflicts and multiple writers; ablate search and the CLI's sufficiency; then compare CLI with CLI plus frontier views and CLI plus shared intentions on collective work. Run the read experiments first, as soon as the reader skill copy exists. Pre-register quality margins, compute limits, repetition counts and success rules before collecting scored data. Repeat entire communities and analyze each community run as the independent unit, preserving failed runs and coordination costs.

## Problem

Deterministic CLI tests establish command behavior, not cheaper or better agent research. The research note (Obsidian `Ideas/CLI-Mediated ARA`) defines experiments E0–E5: merge microbenchmarks and a merge conflict suite, the 450-question understanding benchmark, a scaling curve, PM session replay with a compiler variant, multiple writers, and an end-to-end RE-Bench extension. The comparison note (`Analyses/Agora vs ARA as Research Records`) adds a collective study with separate frontier and intention arms. The parent also requires compilation fidelity. Existing paper results do not measure performance after replacing artifact access.

The accessible `../ara-paperbench/README.md` confirms an artifact collection and its structure, not the historical question/grading harness or skill revision. The external harness destination is therefore proposed rather than inferred from that collection. Historical benchmark collection remains blocked until PR 12 verifies its corresponding sources; other fully specified live experiments can proceed independently after their own prerequisites are met.

## Constraints

Keep experiment code outside ara-cli. This plan changes no shipped binary or viewer and supplies no fabricated measurement, sample size or claimed gain. The original skills remain baselines and all source/reference pages supplied to an agent count toward token cost. Preserve complete content, evidence, provenance and required history under every applicable task contract.

Within each comparison, fix model build, tools, tasks, starting artifacts, grader, total compute budget, deadlines and source revisions. Only the reviewed intervention differs. Enforce condition policy in the runner and record violations; do not silently convert CLI-only tasks to file access. The CLI remains offline and LLM-free. Model calls, shared-channel publication and any transport are external harness responsibilities, with costs attributed to the run. Follow the [shared gates](README.md) and review the external destination before implementation.

### Which protocol revision experiments use

Keep protocol [PR #38](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/pull/38) in draft. Experiments consume `feat/agent-cli-interface` directly, without waiting for a protocol merge or upstream F1–F7 approval. This permission applies to experimental conditions and does not approve a protocol release. Keep the unchanged Files baselines and their archived source pin; using the draft treatment must not replace them.

The current protocol pin is `03f19c7767ec993ae53a0698417b8b68040d7fee` from `https://github.com/ARA-Labs/Agent-Native-Research-Artifact.git`. When the external harness repository is created, add its protocol submodule at `vendor/ara-protocol` and record both the branch and this exact commit:

```sh
git submodule add -b feat/agent-cli-interface \
  https://github.com/ARA-Labs/Agent-Native-Research-Artifact.git vendor/ara-protocol
git -C vendor/ara-protocol fetch --depth 1 origin 03f19c7767ec993ae53a0698417b8b68040d7fee
git -C vendor/ara-protocol checkout --detach 03f19c7767ec993ae53a0698417b8b68040d7fee
git add .gitmodules vendor/ara-protocol
```

The superproject gitlink pins the source; `.gitmodules` records the branch for deliberate updates. Ordinary runs use the recorded gitlink, never `git submodule update --remote`. Record the protocol commit, lock-file digests and CLI binary checksum in each run manifest. A branch update creates a new run identity before collection; completed runs retain their old pin. Historical skill/task/grader availability and numeric registration still govern their own experiments.

## Proposed approach

All paths in this paragraph are new proposed files in the proposed external repository. Add `pyproject.toml` with a reviewed locked runtime environment; `src/ara_agent_interface_eval/` with `manifest.py`, `runner.py`, `policy.py`, `metrics.py`, `grading.py`, `analysis.py`, and `shared_channel.py`; `configs/conditions.json`; `configs/experiments/`; `preregistration/`; `schemas/`; and `tests/`. Add `README.md` documenting the accepted procedure. The implementation first inspects any existing harness provided with the paper and reuses its task/grading loader where verified; these proposed modules do not imply that uninspected external APIs already exist.

Each run manifest records source-lock and operation-inventory digests from PR 12, variant lock and CLI commit/binary checksum from PR 13, collective-contract revision where used, corpus/task/grader hashes, complete prompt bundles, model/provider versions, pricing schedule, sampling settings, seed, paired starting artifact, run identity, budget and environment. The runner captures tool requests/results, command exit codes, exact input/output tokens, cached-token accounting, cost components, wall times, output artifacts and grading records. Secrets stay outside exported logs. A missing pin or capability produces a setup failure before consuming scored research budget.

Arms. Every arm uses the same model and the same system prompt; only the tool surface and its documentation differ, and documentation tokens are counted.

| Arm | Skills and access | Role |
|---|---|---|
| PDF | Paper PDF and repository with Bash/Read/Glob/Grep. | The paper's baseline; run only on E1 where comparability with published numbers is needed. |
| F (Files) | Unchanged archived source skills with their original file tools. | Status quo baseline in every Files-versus-CLI comparison. |
| C | PR 13 CLI copies with structural commands; `ara find` withheld. | Search ablation on E1 and E2. |
| C+S | C plus `ara find`. | Main CLI arm. Knowledge-layer reads and writes go through `ara` by instruction; direct Read stays allowed for code and evidence bodies. Violations are logged for PR 17. |
| C-only | C+S with no Read/Grep/Glob at all. | Ablation: is the CLI sufficient on its own? E1 only. Expect losses on questions answered in `src/` or `evidence/` prose and report them by category. |
| C+S+E | C+S with local embeddings. | Run only if PR 16 ships. |
| C+S+frontier | C+S plus PR 14's frontier instructions. | Collective study (E6). |
| C+S+intentions | C+S plus PR 14's shared-intention instructions and channel. | Collective study (E6). |

The note's arm C allowed Read on every file. This plan's C and C+S route knowledge-layer access through `ara`, so the measured difference is the interface; C-only removes direct reads entirely. To separate tool familiarity from the interface, run E1's pilot for C+S twice: a "cold" run with only `ara --help`, and a run with the command reference page. Run the main model as Claude Sonnet 4.6 for comparability with the ARA paper, plus one current model pinned in the registration record.

Comparisons: F versus C+S is the interface comparison on every Files-versus-CLI experiment. C versus C+S isolates search. C-only and the cold run are E1 diagnostics. E6 compares F, C+S, C+S+frontier and C+S+intentions at matched total community compute, following the Agora comparison note; a combined frontier-plus-intentions arm is a diagnostic only. Do not run every arm on every experiment. For E6, use the same outer task/merge schedule and approved contributor roles in every collective arm; record any departure from the source single-writer rule as a common collective-task protocol, never as a change to the interface-only CLI arm. Each treated arm receives only its own component's prompts. If reviewer-approved role changes cannot be held equal across collective arms, label the result as a bundled coordination-and-role intervention.

| Experiment | Required design | Outcomes and checks |
|---|---|---|
| E0 merge conflict suite (agent part) | About 50 cases per conflict type, each with a known correct result: disjoint appends, same-parent appends, one claim edited in different fields, one claim edited in the same field, renumbering with prose references. F runs `git merge`, then the agent repairs with Read/Edit; C+S runs `ara merge`, then the agent works through the report. The deterministic part of E0 (timing, property tests) lives in PR 08's CI. | Correctness against the known result, agent tokens and wall time per case, integrity after repair. |
| E1 understanding | Verified paper question set and task/skill/grading map; paired runs from identical artifacts. The parent specifies 450 questions; the authoritative source must confirm this count. Pilot first: 105 RE-Bench questions plus 105 sampled PaperBench questions, F versus C+S, one seed, including the cold C+S run. Full run: 3 seeds and the arms above. | Held-out answer quality, native-ref correctness, tokens, dollars and wall time, stratified by category A/B/C, tier T1/T2/T3, and trace size (PaperBench versus RE-Bench). Grade new answers with the paper's procedure; the paper's 61K/96K/153K tokens per question by tier are context, not a control arm. |
| E2 scaling | Fix questions whose answers exist at every size and grow the ARA underneath them so only distractors increase. Use naturally grown snapshots first: the-ara-of-ara cut at each session, and RE-Bench ARAs recompiled from more MALT runs to 1,000 or more nodes. Do not concatenate unrelated ARAs. Synthetic growth is a labeled supplement only. F versus C+S, with C as the search ablation. | Tokens per question and accuracy against node count, with size levels frozen in advance; report missing full-body content and timeouts. |
| E3 PM replay | Replay the-ara-of-ara session by session: at step k, the PM gets snapshot k−1 plus session k's inputs and must produce snapshot k; the real snapshot k is gold. F versus C+S. | Tokens and wall time per turn as functions of k, `ara check` errors after each turn, ID collisions, node-level fidelity against gold (type, parent, title matched by a blinded judge), routing, closure decisions, provenance, full sessions/index/reasoning, before/after revisions and stale handling. |
| E3 compiler variant | Paired source bundles requiring arbitrary solution prose, complete citations/appendices and tables/figures; compile by writing files (F) versus through `ara apply` (C+S), with the original compiler validation and repair procedure. | In-loop Seal L1 validation iterations, tokens and time to a passing `ara check`, source content/evidence coverage, native bindings and exact evidence fidelity. |
| E4 multiple writers | K ∈ {2, 4, 8} agents work on one RE-Bench extension task, each in its own fork, merging into the main copy on a fixed schedule (the note proposes every 30 minutes). F merges with Git and repairs by hand-editing files; C+S uses `ara merge`. Run C+S with and without PR 11's duplicate warnings. | Share of merges completed without a human, agent tokens and time per merge, integrity after merge (duplicate IDs, dangling references, nodes lost against the logged union of all writes), and duplicate-node rate with and without warnings. |
| E5 RE-Bench extension (exploratory) | The paper's protocol: 5 tasks, 8 h wall clock, $50 cap, 3 seeds, F versus C+S. Run last and only if budget remains. | Score versus cumulative cost and versus wall time, cost and time to reach the reference score, share of tokens spent on tool calls touching `ara/`, and whether `ara open` and search reduce the anchoring the paper reports. Report as exploratory. |
| E6 collective research | Independent communities start from paired identical ARAs; agents work in separate forks under a fixed integration schedule using directory merge. Arms F, C+S, C+S+frontier and C+S+intentions at matched total community compute. | Held-out research quality per dollar and time, accidental duplicate experiments excluding deliberate verification (judged by experiment contents such as configuration and source digests, not by equal scores), hypothesis-testing coverage, verification completeness, branch concentration, artifact integrity, merge/conflict outcomes and full run resources. |

For PM replay, acquire authoritative session-by-session input transcripts and validate that reconstructed prompts do not contain future knowledge. The session records themselves are the PM's output, so feeding session k's logged events as input would leak the answer. If transcripts are unavailable, run the approved fallback, E3-prospective: record new live research sessions with their full inputs captured, then replay those inputs in both arms. Report it under that name, not as historical replay.

For E1, if the paper's historical skill revision cannot be verified, pin the current reader skill for both F and C+S and label the run "not a reproduction". Both arms are newly collected either way, so the interface comparison stays valid; only the comparison with the paper's published numbers is lost. For scaling, preserve original reference identities and verify that added material neither answers the held-out question nor deletes relevant content. Compiler tasks preserve original evidence ledgers, screenshots and source provenance, with source/evidence direct access allowed equally in both conditions.

The collective runner implements PR 14's actual shared directory at `<run-root>/shared/community/<community-id>/intentions/`. Only the external coordinator appends durable events and replaces snapshots; each fork runner publishes through it and refreshes before work selection and execution. Persist sequence, logical round, acknowledgments, expiry decisions and failure events. A common budget ledger accounts for agent reasoning, publication/refresh overhead, failed calls, merge work, graders where included and deliberate verification. Reservations do not enlarge the total allocation. Fresh-run identities and directories prevent leakage between communities.

### What every run measures

- **Cost:** input tokens split into uncached, cache-read and cache-write; output tokens; dollars with cache reads priced as in the ARA paper (10%); tool calls and turns. Report raw and cache-adjusted numbers together, because caching narrows the dollar gap.
- **Time:** wall time per item, split into model time and tool time. Report `ara` command latency separately.
- **Interface share:** the share of tokens and tool calls that touch `ara/`.
- **Quality:** the paper's measures (blinded judge accuracy, rubric score, best task score), `ara check` errors, and fidelity against gold for writes. Keep judges blind to the arm.
- **Headline ratio:** cost per correct answer (tokens divided by accuracy).

Analyze paired by item across arms with paired bootstrap confidence intervals on the token ratio and the accuracy difference. Report medians alongside means because token counts are heavy-tailed. Report every result per stratum (category, tier, trace size): the claim is that the saving grows with artifact size, so a pooled average alone would hide it. PaperBench ARAs (12–16 nodes) act as the control. If the CLI saves as much there as on RE-Bench ARAs (132–236 nodes), attribute the effect to novelty rather than size and say so.

Guard against overfitting the CLI to the test questions: tune output format, verbosity and search parameters on a dev split (the-ara-of-ara plus 5 artifacts), freeze the CLI binary in the variant lock, then run the test split. Dev artifacts and questions are excluded from scored analysis.

### What must be registered before scored collection

Create an immutable registration record under `preregistration/<study-id>.json` (new) with reviewer approval and a digest published before scored runs. It must contain the research note's predictions (file-based cost grows roughly linearly with artifact size while C+S stays roughly flat; the largest E1 saving is Category C over RE-Bench; little or no saving on PaperBench Category A; F's PM cost per turn grows with k while C+S stays flat; F merges break silently while C+S merge cost tracks the report size). It must also contain numeric task-quality noninferiority margins separately for understanding, scaling, PM, compilation, merge and collective quality (the note's starting proposal is −2 percentage points of accuracy for E1), with each margin's units, rubric range and scientific justification. It must also contain confidence level, uncertainty method, minimum whole-run repetitions, community size, budgets, deadlines, fixed stopping rule, exclusion/failure policy, cost/time primary endpoint, multiplicity policy and duplicate-experiment adjudication rule. No margin or sample count is approved by this draft. The runner rejects an unapproved record, nonnumeric margins or a record changed after collection begins.

For each quality endpoint, let `D` be treated minus control on the registered scale and `delta` the approved allowable loss. Quality passes only when the registered lower confidence bound for `D` is at least `-delta`. Artifact integrity and mandatory content/history/provenance fidelity are hard requirements with no tolerance for silently lost or rewritten records. Efficiency is assessed only after the corresponding quality requirement passes. A claimed cost or time reduction requires the registered uncertainty bound for its treated/control ratio to be below one; measured point estimates alone do not establish improvement. Collective duplication and research quality use their own registered criteria, not an invented expected gain.

Select numeric margins with reviewers using the verified rubric's meaningful-loss interpretation before viewing scored condition outcomes. Determine repetitions using a reviewed power/precision calculation or a disjoint feasibility pilot; archive its inputs, exclude pilot tasks/runs from scored analysis and freeze the calculation. No convenient value is inserted merely to make the experiment runnable. If a justified margin or repetition count is still absent, finish the reproducible harness and smoke proof but keep scored collection disabled.

Whole community runs are the collective sampling unit. Pair control and treated runs by task/start artifact and budget block, randomize execution order with registered seeds, and use the registered run-level paired analysis or community-level bootstrap. Contributions, turns, experiments and merges inside a community are correlated and cannot be counted as independent samples. Repeat communities from fresh state and independent registered seeds until the fixed run count, never until a favorable result. Other experiments retain their task/run hierarchy and account for repeated questions on the same artifact.

Implementation steps for the future PR:

Collect in this order, as the research note sets it: E1 pilot and E2 after 13a; E3 after 13b; the E0 agent suite and E4 after PRs 08 and 11; E6 after PR 14; E5 last. The deterministic E0 checks run in ara-cli CI from PR 08 onward.

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

1. Create the external repository and verify historical benchmark and replay sources; apply the E1 and E3 fallbacks where they are missing.
2. Fix numeric quality margins, repetition counts and the dev/test split, then freeze registration.
3. Implement and smoke the harness against 13a, then run the E1 pilot and E2 before the write-side experiments.
