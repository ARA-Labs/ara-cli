# PR 15: build the external experiment harness
**Date:** 2026-10-02

The original E0–E6 design was approved on 2026-10-01. This revision adds explicit baseline/treatment submodules, related-work lessons, a sharing-disabled collective control, and staged repository delivery. These additions are **pending review**; requesting this plan does not authorize repository creation, harness implementation, paid model calls, or scored collection. Target: proposed `ARA-Labs/ara-agent-interface-eval`, with local checkout `../ara-agent-interface-eval/`; neither is claimed to exist. Parent: [agent CLI interface plan](../agent-cli-interface.md). Series: [PR index and shared gates](README.md). Dependencies: [12-pin-skill-contracts.md](12-pin-skill-contracts.md) for source inventory; [13-cli-backed-skills.md](13-cli-backed-skills.md) deliverable 13a for E1/E2 and 13b for E3; [08-directory-merge.md](08-directory-merge.md), [11-duplicate-warnings.md](../../docs/agent-cli-interface/11-duplicate-warnings.md), and 13b for E0/E4; [14-shared-frontier-intentions.md](14-shared-frontier-intentions.md) for E6. Each experiment needs its own verified sources and registration before scored collection.

## TL;DR

Create a separate experiment repository that pins unchanged upstream ARA, our draft protocol/skills, and our CLI as separate Git submodules. First compare file access with CLI access while preserving the research procedures; then test frontier views, shared intentions, and access to peer findings as separate collective interventions. Keep E0–E6, including understanding, scaling, project-manager replay, compilation, merge repair, multiple writers, and the exploratory RE-Bench extension. Register quality margins, total community budgets, repetition counts, and failure rules before scored collection. Report held-out quality, complete resource costs, artifact fidelity, and uncertainty, with whole communities as the collective sampling unit.

## Problem

Deterministic CLI tests establish command behavior, not cheaper or better agent research. The research note (Obsidian `Ideas/CLI-Mediated ARA`) defines experiments E0–E5: merge microbenchmarks and a merge conflict suite, the 450-question understanding benchmark, a scaling curve, PM session replay with a compiler variant, multiple writers, and an end-to-end RE-Bench extension. The comparison note (`Analyses/Agora vs ARA as Research Records`) adds a collective study with separate frontier and intention arms. The parent also requires compilation fidelity. Existing paper results do not measure performance after replacing artifact access.

The accessible `../ara-paperbench/README.md` confirms an artifact collection and its structure, not the historical question/grading harness or skill revision. The external harness destination is therefore proposed rather than inferred from that collection. Historical benchmark collection remains blocked until PR 12 verifies its corresponding sources; other fully specified live experiments can proceed independently after their own prerequisites are met.

### What related work establishes

The closest research-memory systems use different evaluation designs. Their published results guide the controls below, but their reported gains are not expected gains for ARA and do not replace newly collected baseline runs.

| Work and primary source | How the experiments were conducted | What this plan adopts or avoids |
|---|---|---|
| [Agora, sections 4–5](https://arxiv.org/html/2609.18094v3#S4) | One nearly 12-day weight-transfer run with 13 independent coding-agent workers. Workers read shared frontier views, check out parent contributions, evaluate code, and publish a Git-backed contribution graph. Diversity views were introduced midway; results include lineage, negative results, and independent reproductions. | Adopt exact result/code lineage and verification. Do not treat a single community or the mid-run intervention as a controlled estimate of discovery efficiency; the paper explicitly calls for matched comparisons. |
| [AgentRxiv, sections 3.1–3.2 and 4.1](https://arxiv.org/html/2503.18102v1#S3) | A laboratory generated 40 papers seeking better MATH-500 prompting methods, with a prior-paper-access ablation. Three parallel laboratories shared reports asynchronously. Parallel runs generated 120 papers versus 40 sequentially and consumed more total resources. Authors manually inspected code/results because generated reports could contain invented measurements. | Add a sharing-disabled control at the same worker count and total budget. Grade independently executed code, not claimed paper scores. Separate wall-clock speed from efficiency per dollar. |
| [ScienceClaw + Infinite, section 3](https://arxiv.org/html/2603.14312v1#S3) | Four scientific case studies use persistent memories, typed artifact lineage, shared needs, and synthesis across agents. The evaluation reports scientific outputs, participation, tools, artifacts, synthesis, and dependency depth. | Measure actual use of peer evidence and multi-agent synthesis. Treat these case studies as capability demonstrations, not a matched memory/no-memory estimate. |
| [MemCollab, section 4](https://arxiv.org/html/2603.23234v1#S4) | Math/code tasks compare no memory, self-memory, transferred memory, and contrast-derived memory across model families, with retrieval ablations and reasoning-turn measurements. | Keep memory construction, access, and coordination effects separate. Its task-level memory-transfer evaluation is not an online collective-research benchmark. |

## Constraints

Keep experiment code outside ara-cli. This plan changes no shipped binary or viewer and supplies no fabricated measurement, sample size or claimed gain. The original skills remain baselines and all source/reference pages supplied to an agent count toward token cost. Preserve complete content, evidence, provenance and required history under every applicable task contract.

Within each comparison, fix model build, tools, tasks, starting artifacts, grader, total compute budget, deadlines and source revisions. Only the reviewed intervention differs. Enforce condition policy in the runner and record violations; do not silently convert CLI-only tasks to file access. The CLI remains offline and LLM-free. Model calls, shared-channel publication and any transport are external harness responsibilities, with costs attributed to the run. Follow the [shared gates](README.md) and review the external destination before implementation.

This plan does not implement new CLI commands, merge or release the draft protocol, or reproduce another system's headline result. The repository first delivers a working harness with engineering smoke evidence; scientific conclusions require a separate approved registration and scored runs.

### Which protocol revision experiments use

Keep protocol [PR #38](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/pull/38) in draft. Experiments consume `feat/agent-cli-interface` directly, without waiting for a protocol merge or upstream F1–F7 approval. This permission applies to experimental conditions and does not approve a protocol release. Keep the unchanged Files baselines and their archived source pin; using the draft treatment must not replace them.

### Which repositories and commits are pinned

Use three submodules because the treatment changes both the executable and the skill package. The two protocol paths intentionally use the same remote at different immutable commits. The baseline remains independent of treatment updates.

| Path in the new repository | Remote | Initial proposed commit and purpose |
|---|---|---|
| `vendor/ara-upstream` | `https://github.com/ARA-Labs/Agent-Native-Research-Artifact.git` | `e52a925e9d03b4ada3008653e72f99b04116fca2`, the unchanged live baseline in `source-lock.json`; not a verified paper-corresponding revision. |
| `vendor/ara-protocol` | `https://github.com/ARA-Labs/Agent-Native-Research-Artifact.git` | `03f19c7767ec993ae53a0698417b8b68040d7fee`, draft PR #38's protocol contracts, CLI-backed skills, variant locks, and collective package. |
| `vendor/ara-cli` | `https://github.com/ARA-Labs/ara-cli.git` | `4f70972cb68aaec122148cf98421dda29f3061e4`, the source-bound 0.1.23 binary in the [delivery evidence](../../docs/verification/agent-cli-2026-10-02/README.md). |

Before the harness source freeze, verify that all commits remain fetchable and that source/variant locks agree with the selected executable. Later documentation-only branch commits need not change the executable pin. Any functional update requires new build and smoke evidence, updated locks, and a new study identity if scored collection has started.

The following commands are for implementation after review, not commands executed by this planning task. Add the unchanged baseline and executable pins alongside the draft protocol submodule:

```sh
git submodule add \
  https://github.com/ARA-Labs/Agent-Native-Research-Artifact.git vendor/ara-upstream
git -C vendor/ara-upstream fetch origin e52a925e9d03b4ada3008653e72f99b04116fca2
git -C vendor/ara-upstream checkout --detach e52a925e9d03b4ada3008653e72f99b04116fca2
git submodule add -b feat/agent-cli-interface \
  https://github.com/ARA-Labs/ara-cli.git vendor/ara-cli
git -C vendor/ara-cli fetch origin 4f70972cb68aaec122148cf98421dda29f3061e4
git -C vendor/ara-cli checkout --detach 4f70972cb68aaec122148cf98421dda29f3061e4
git add .gitmodules vendor/ara-upstream vendor/ara-cli
```

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

### What lives in the experiment repository

The experiment repository owns scheduling, provider adapters, access controls, resource accounting, task loading, graders, registration, and analysis. Reuse the pinned protocol's packaging checks, skill variants, and channel implementation when their contracts match, rather than maintain copied implementations. Build the CLI from its pinned source with the pinned Rust toolchain and locked dependencies, then record the executable checksum. A Git commit identifies source, but does not identify the executable produced by a build.

```text
ara-agent-interface-eval/
  vendor/
    ara-upstream/
    ara-protocol/
    ara-cli/
  src/ara_agent_interface_eval/
  configs/
    conditions.json
    experiments/
  locks/
  tasks/
  graders/
  preregistration/
  schemas/
  tests/
  analysis/
  README.md
  pyproject.toml
```

Keep source submodules read-only during runs. Copy each starting artifact into a fresh workspace outside `vendor/`; knowledge, intentions, caches, and credentials from one run must not leak into another. Dataset locks record immutable source locations, licenses, and content hashes. Store large traces, checkpoints, and output artifacts in durable external storage with a hash manifest. Keep small fixtures and run summaries in Git. Never commit credentials or use a source submodule as an agent's working directory.

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
| C+S-private (proposed) | Same CLI skill, initial ARA, and own evolving history as C+S; peer findings and intentions are inaccessible. | E6 isolation control at the same worker count and total community budget; pending review. |

The note's arm C allowed Read on every file. This plan's C and C+S route knowledge-layer access through `ara`, so the measured difference is the interface; C-only removes direct reads entirely. To separate tool familiarity from the interface, run E1's pilot for C+S twice: a "cold" run with only `ara --help`, and a run with the command reference page. Run the main model as Claude Sonnet 4.6 for comparability with the ARA paper, plus one current model pinned in the registration record.

F versus C+S is the interface comparison on every Files-versus-CLI experiment. C versus C+S isolates search. C-only and the cold run are E1 diagnostics. E6 retains F, C+S, C+S+frontier, and C+S+intentions at matched total community compute and proposes C+S-private as a sharing-disabled control. A combined frontier-plus-intentions arm is a diagnostic only. Do not run every arm on every experiment. For E6, use the same outer task/merge schedule and approved contributor roles in every collective arm; record any departure from the source single-writer rule as a common collective-task protocol, never as a change to the interface-only CLI arm. Each treated arm receives only its own component's prompts. If reviewer-approved role changes cannot be held equal across collective arms, label the result as a bundled coordination-and-role intervention.

| Experiment | Required design | Outcomes and checks |
|---|---|---|
| E0 merge conflict suite (agent part) | About 50 cases per conflict type, each with a known correct result: disjoint appends, same-parent appends, one claim edited in different fields, one claim edited in the same field, renumbering with prose references. F runs `git merge`, then the agent repairs with Read/Edit; C+S runs `ara merge`, then the agent works through the report. The deterministic part of E0 (timing, property tests) lives in PR 08's CI. | Correctness against the known result, agent tokens and wall time per case, integrity after repair. |
| E1 understanding | Verified paper question set and task/skill/grading map; paired runs from identical artifacts. The parent specifies 450 questions; the authoritative source must confirm this count. Pilot first: 105 RE-Bench questions plus 105 sampled PaperBench questions, F versus C+S, one seed, including the cold C+S run. Full run: 3 seeds and the arms above. | Held-out answer quality, native-ref correctness, tokens, dollars and wall time, stratified by category A/B/C, tier T1/T2/T3, and trace size (PaperBench versus RE-Bench). Grade new answers with the paper's procedure; the paper's 61K/96K/153K tokens per question by tier are context, not a control arm. |
| E2 scaling | Fix questions whose answers exist at every size and grow the ARA underneath them so only distractors increase. Use naturally grown snapshots first: the-ara-of-ara cut at each session, and RE-Bench ARAs recompiled from more MALT runs to 1,000 or more nodes. Do not concatenate unrelated ARAs. Synthetic growth is a labeled supplement only. F versus C+S, with C as the search ablation. | Tokens per question and accuracy against node count, with size levels frozen in advance; report missing full-body content and timeouts. |
| E3 PM replay | Replay the-ara-of-ara session by session: at step k, the PM gets snapshot k−1 plus session k's inputs and must produce snapshot k; the real snapshot k is gold. F versus C+S. | Tokens and wall time per turn as functions of k, `ara check` errors after each turn, ID collisions, node-level fidelity against gold (type, parent, title matched by a blinded judge), routing, closure decisions, provenance, full sessions/index/reasoning, before/after revisions and stale handling. |
| E3 compiler variant | Paired source bundles requiring arbitrary solution prose, complete citations/appendices and tables/figures; compile by writing files (F) versus through `ara apply` (C+S), with the original compiler validation and repair procedure. | In-loop Seal L1 validation iterations, tokens and time to a passing `ara check`, source content/evidence coverage, native bindings and exact evidence fidelity. |
| E4 multiple writers | K ∈ {2, 4, 8} agents work on one RE-Bench extension task, each in its own fork, merging into the main copy on a fixed schedule (the note proposes every 30 minutes). F merges with Git and repairs by hand-editing files; C+S uses `ara merge`. Run C+S with and without PR 11's duplicate warnings. | Share of merges completed without a human, agent tokens and time per merge, integrity after merge (duplicate IDs, dangling references, nodes lost against the logged union of all writes), and duplicate-node rate with and without warnings. |
| E5 RE-Bench extension (exploratory) | The paper's protocol: 5 tasks, 8 h wall clock, $50 cap, 3 seeds, F versus C+S. Run last and only if budget remains. | Score versus cumulative cost and versus wall time, cost and time to reach the reference score, share of tokens spent on tool calls touching `ara/`, and whether `ara open` and search reduce the anchoring the paper reports. Report as exploratory. |
| E6 collective research | Independent communities start from paired identical ARAs; agents work in separate forks under a fixed integration schedule using directory merge. Retain F, C+S, C+S+frontier, and C+S+intentions; add proposed C+S-private after review. Match worker count and total community compute. Private workers do not receive merged peer findings. | Held-out research quality at the fixed total budget, score versus dollars/time, accidental duplicate experiments excluding deliberate verification (judged by configuration and source digests, not equal scores), hypothesis-testing coverage, verification completeness, peer-evidence use, synthesis, branch concentration, artifact integrity, merge/conflict outcomes, and full run resources. |

### How the two studies separate interface and shared memory

The interface study compares F with C+S without frontier prompts, shared intentions, extra contributor roles, or a changed research procedure. Search and CLI-only comparisons remain separate ablations. Both conditions use newly collected runs on paired inputs; previously published Files measurements are context, not the control group.

The collective study retains F, C+S, C+S+frontier, and C+S+intentions and proposes C+S-private as a separate sharing control. C+S-private versus C+S measures access to accumulating peer findings; C+S versus each extension measures its added coordination component. F versus C+S measures the interface within the common collective contributor protocol. A single-worker reference may be exploratory, but it cannot replace the same-worker-count sharing-disabled control.

In C+S-private, each worker sees the identical starting ARA and only its own later findings. The runner denies peer workspaces, shared knowledge snapshots, peer messages, intention channels, and alternative access through Bash, network tools, or source paths. The external coordinator still performs the registered integration schedule for auditing, but withholds merged peer content from private workers until the run ends. Include that integration cost. Register the withheld integrated view as part of the sharing intervention; worker roles and budgets remain unchanged. Test denied access and exact visibility using two real worker processes before collecting data.

Keep worker count, models, contributor roles, research tools, task inputs, integration times, and total community allocation fixed within each collective comparison. Record the resolved system prompt, complete skill/reference bundle, allowed tools, and peer-visibility rules for every arm. Use a development evaluator during search and freeze a candidate-selection rule before collection; independently evaluate the selected output on held-out data after the run. Workers cannot read held-out scores or use them to choose the winner. Count actual peer-artifact consumption and evidence-backed synthesis as process measures, not substitutes for held-out research quality.

For PM replay, acquire authoritative session-by-session input transcripts and validate that reconstructed prompts do not contain future knowledge. The session records themselves are the PM's output, so feeding session k's logged events as input would leak the answer. If transcripts are unavailable, run the approved fallback, E3-prospective: record new live research sessions with their full inputs captured, then replay those inputs in both arms. Report it under that name, not as historical replay.

For E1, if the paper's historical skill revision cannot be verified, pin the current reader skill for both F and C+S and label the run "not a reproduction". Both arms are newly collected either way, so the interface comparison stays valid; only the comparison with the paper's published numbers is lost. For scaling, preserve original reference identities and verify that added material neither answers the held-out question nor deletes relevant content. Compiler tasks preserve original evidence ledgers, screenshots and source provenance, with source/evidence direct access allowed equally in both conditions.

The collective runner implements PR 14's actual shared directory at `<run-root>/shared/community/<community-id>/intentions/`. Only the external coordinator appends durable events and replaces snapshots; each fork runner publishes through it and refreshes before work selection and execution. Persist sequence, logical round, acknowledgments, expiry decisions and failure events. A common budget ledger accounts for agent reasoning, publication/refresh overhead, failed calls, merge work, graders where included and deliberate verification. Reservations do not enlarge the total allocation. Fresh-run identities and directories prevent leakage between communities.

### What every run measures

- **Cost:** input tokens split into uncached, cache-read and cache-write; output tokens; dollars with cache reads priced as in the ARA paper (10%); tool calls and turns. Report raw and cache-adjusted numbers together, because caching narrows the dollar gap.
- **Time:** wall time per item, split into model time and tool time. Report `ara` command latency separately.
- **Interface share:** the share of tokens and tool calls that touch `ara/`.
- **Quality:** the paper's measures (blinded judge accuracy, rubric score, best task score), `ara check` errors, and fidelity against gold for writes. Keep judges blind to the arm.
- **Headline ratio:** cost per correct answer (tokens divided by accuracy).

Analyze paired by item across arms with paired bootstrap confidence intervals on the token ratio and the accuracy difference. Report medians alongside means because token counts are heavy-tailed. Report every result per stratum (category, tier, trace size): the claim is that the saving grows with artifact size, so a pooled average alone would hide it. PaperBench ARAs (12–16 nodes) act as the small-artifact control against RE-Bench ARAs (132–236 nodes). Similar savings in both groups would not support a larger benefit from artifact size; they would not establish novelty as the cause either.

Guard against overfitting the CLI to the test questions: tune output format, verbosity and search parameters on a dev split (the-ara-of-ara plus 5 artifacts), freeze the CLI binary in the variant lock, then run the test split. Dev artifacts and questions are excluded from scored analysis.

### What must be registered before scored collection

Create an immutable registration record under `preregistration/<study-id>.json` (new) with reviewer approval and a digest published before scored runs. It must contain the research note's predictions (file-based cost grows roughly linearly with artifact size while C+S stays roughly flat; the largest E1 saving is Category C over RE-Bench; little or no saving on PaperBench Category A; F's PM cost per turn grows with k while C+S stays flat; F merges break silently while C+S merge cost tracks the report size). It must also contain numeric task-quality noninferiority margins separately for understanding, scaling, PM, compilation, merge and collective quality (the note's starting proposal is −2 percentage points of accuracy for E1), with each margin's units, rubric range and scientific justification. It must also contain confidence level, uncertainty method, minimum whole-run repetitions, community size, budgets, deadlines, fixed stopping rule, exclusion/failure policy, cost/time primary endpoint, multiplicity policy and duplicate-experiment adjudication rule. No margin or sample count is approved by this draft. The runner rejects an unapproved record, nonnumeric margins or a record changed after collection begins.

For each quality endpoint, let `D` be treated minus control on the registered scale and `delta` the approved allowable loss. Quality passes only when the registered lower confidence bound for `D` is at least `-delta`. Artifact integrity and mandatory content/history/provenance fidelity are hard requirements with no tolerance for silently lost or rewritten records. Efficiency is assessed only after the corresponding quality requirement passes. A claimed cost or time reduction requires the registered uncertainty bound for its treated/control ratio to be below one; measured point estimates alone do not establish improvement. Collective duplication and research quality use their own registered criteria, not an invented expected gain.

Select numeric margins with reviewers using the verified rubric's meaningful-loss interpretation before viewing scored condition outcomes. Determine repetitions using a reviewed power/precision calculation or a disjoint feasibility pilot; archive its inputs, exclude pilot tasks/runs from scored analysis and freeze the calculation. No convenient value is inserted merely to make the experiment runnable. If a justified margin or repetition count is still absent, finish the reproducible harness and smoke proof but keep scored collection disabled.

Whole community runs are the collective sampling unit. Pair control and treated runs by task/start artifact and budget block, randomize execution order with registered seeds, and use the registered run-level paired analysis or community-level bootstrap. Contributions, turns, experiments and merges inside a community are correlated and cannot be counted as independent samples. Repeat communities from fresh state and independent registered seeds until the fixed run count, never until a favorable result. Other experiments retain their task/run hierarchy and account for repeated questions on the same artifact.

The revised registration also names the sharing-disabled control and its information restrictions, the common contributor protocol, the development/held-out split, the candidate-selection rule, and independent execution checks. Register how zero correct answers, no valid candidate, exhausted budgets, interrupted workers, and unavailable evaluators enter the results. Do not drop these runs or compute a finite cost-per-correct value when correctness is zero. Report research quality at the fixed total budget as the primary collective outcome, with score-versus-cost/time curves as supporting outcomes.

### How implementation and collection are delivered

These phases describe future work in the external repository. Approval of repository creation and harness engineering is separate from approval of numeric scientific registration; neither requires merging the draft protocol. Human reviewers retain the unresolved release and historical-reproduction decisions.

| Phase | Deliverable | Required evidence before proceeding |
|---|---|---|
| 1. Review and repository creation | Approve this revision, create `ARA-Labs/ara-agent-interface-eval`, add all three pinned submodules, and document their licenses and update policy. | A clean clone with `git submodule update --init --recursive` resolves the recorded commits; locks and archived baseline bytes verify. No paid calls or scored runs. |
| 2. Task and condition contracts | Reuse verified task/grader sources; implement manifests, provider accounting, access policy, fresh workspaces, and the Files/CLI variants. | Missing pins/capabilities fail before model calls. Unit tests prove run isolation, token/cache/dollar accounting, policy enforcement, and withheld future-session inputs. Historical-source failures stay named and blocked. |
| 3. Actual harness smoke | Execute paired reader, PM revision, compiler, merge repair, and two-process collective tasks against the pinned CLI and real provider integration. | Review the paid-smoke budget first. Archive complete requests/results, actual source/readback fidelity, independent score checks, denied peer access in C+S-private, channel expiry/recovery, and budget exhaustion. Smoke proves operation, not scientific benefit. |
| 4. Disjoint feasibility pilot and registration | Use pilot data only to establish feasibility and justify fixed budgets, numeric quality margins, and repetition counts. | Pilot tasks/runs are excluded from scored analysis. Publish the approved registration digest, source/environment locks, held-out split, and candidate-selection rule before scored runs. |
| 5. Interface experiments | Collect E1 pilot/full understanding, E2 scaling, E3 replay/compiler, E0 agent repairs, and E4 multiple writers when each is runnable. | Paired fresh-state runs, blinded quality grading, complete source/history checks, costs/time, failures, and registered uncertainty analysis. Label live/prospective fallbacks correctly. |
| 6. Collective experiments | Collect E6 shared, private, frontier, and intention communities after the corresponding registration is approved. | Repeat whole communities with registered independent seeds and paired starting states. Analyze at community level and include coordination/verification costs and failed communities. |
| 7. Publication and exploratory extension | Publish reproducibility instructions, raw-data hash manifests, analysis, and qualified conclusions; run E5 last if its separately registered budget remains available. | A second operator reconstructs a selected run and reruns its candidate evaluation from archived artifacts. No causal, historical-reproduction, or efficiency claim extends beyond measured comparisons. |

Each engineering PR records its runnable acceptance proof. After review, implementation may commit and submit PRs in the new repository under that repository's rules. This planning request creates no remote repository, commit, PR, experiment, or release.

Implementation steps for the future PR:

Collect in this order, as the research note sets it: E1 pilot and E2 after 13a; E3 after 13b; the E0 agent suite and E4 after PRs 08 and 11; E6 after PR 14; E5 last. The deterministic E0 checks run in ara-cli CI from PR 08 onward.

1. Review this revision's external destination, three source pins, and sharing-disabled control. Then create the repository, verify clean-clone reproduction, and inspect available historical harness sources without inventing their APIs.
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

This planning task checks document style and local links only. Future unit/contract targets in the external `tests/` cover policy violations through every exposed tool, private/shared visibility, budget accounting, run isolation, future-session leakage, pricing/version capture, channel recovery, independent evaluator execution, candidate selection without held-out leakage, and community-level statistical units. They must check visible outcomes rather than mock command forwarding. Existing paper graders remain unchanged unless an explicitly reviewed new study requires different grading.

The actual-program smoke launches the proposed harness entrypoint `python -m ara_agent_interface_eval smoke --config <approved-smoke-config>` (new interface to implement, not an existing command) with the actual pinned CLI binary. Execute a paired reader task, a PM session/revision task and a compiler task with figures and non-template logic prose. Run a community with two separate fork processes to observe publication, refresh, expiry and final `ara merge`/`ara check` outcomes. Verify actual artifacts, valid refs, full history, measured accounting, role/access policies and fresh run isolation; smoke values are feasibility output, not scored empirical findings. Inject missing CLI coverage, corrupted channel state and exhausted budget and observe explicit setup/run failure without direct knowledge-file fallback or extra allocation.

Acceptance requires reproducible source/prompt/task/grader locks, approved numeric registration, implemented cross-fork shared-channel behavior, all applicable experiments, complete resource/fidelity output and analysis at the correct sampling unit. Historical understanding or replay prerequisites that cannot be sourced stay visibly blocked; no smaller benchmark or prospective task is silently substituted. Scored results and any scientific success claim require newly collected registered data and its actual uncertainty analysis.

Engineering acceptance also requires source submodules to stay unchanged after smoke, a fresh clone to rebuild the executable, and every run manifest to bind source commits to the actual binary, prompt/task/grader hashes, provider/model settings, budget, seed, and output hashes. Scientific acceptance additionally requires held-out evaluation outside the worker environment, executable verification of claimed results, and complete reporting of registered failures. Historical skill mapping, the 465-versus-450 question discrepancy, and unavailable replay inputs cannot be fixed by renaming a live comparison as reproduction.

## Next Steps

1. Review the new submodule boundaries, sharing-disabled control, information restrictions, and staged delivery. Keep earlier E0–E6 approval distinct from these proposed additions.
2. After creation/engineering approval, create the external repository, pin the sources, implement and smoke the real harness, and resolve each task's source requirements or apply its explicitly labeled fallback.
3. Review pilot resources and numeric registration, freeze sources/conditions, then collect interface experiments before the collective study. Keep E5 exploratory and conditional on remaining approved budget.
