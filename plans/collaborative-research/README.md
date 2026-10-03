# CLI-mediated parallel and collaborative ARA research
**Date:** 2026-10-02 (revised and split 2026-10-03)

Status: draft for human review. This plan series authorizes no implementation, role change, network service, paid model run, commit, or release. It replaces the single draft `plans/collaborative-research-cli.md`; see [Review record](#review-record-2026-10-03) for what changed and why.

## TL;DR

Researchers (human or agent) work in private ARA forks. When a result is ready, an external runner freezes an exact copy of the fork, publishes it as an immutable *contribution*, and shows it to peers before anyone merges it into the canonical ARA. Peers can reuse, reproduce, or challenge it right away. One integration project manager later merges selected contributions into canonical `logic/` with the existing `ara merge`, keeping competing interpretations visible. Lara optionally rechecks the numbers and arguments behind selected claims.

`ara-cli` has one job in this design: an offline `ara snapshot` command that captures a fork exactly, so publication uses the same capture rules as `ara merge`. Everything else lives in other repositories.

## Plan series

| Plan | Implementation target | Status |
|---|---|---|
| [01-ara-snapshot](01-ara-snapshot.md) | `ara-cli`: the only CLI change | Draft; next to review |
| [02-contribution-workflow](02-contribution-workflow.md) | `Agent-Native-Research-Artifact` (contracts) and `ara-eval` (runner) | Draft; staged here until the upstream route is decided |
| [03-lara-integration](03-lara-integration.md) | `ara-eval` (adapter) and `Lara` (docs only) | Draft; staged here until the upstream route is decided |

Plans 02 and 03 live in this repository for now, following the precedent of [plan 14](../agent-cli-interface/14-shared-frontier-intentions.md), which targets the protocol repository but is staged here. How and when they move to `Agent-Native-Research-Artifact` or `ara-eval` is decision D1.

## Problem

Take one research question, "why does method M improve task T?", and four workers who fork the same seed ARA:

- A tests an ablation of M's proposed mechanism.
- B tries an alternative implementation.
- C wants to reproduce A's result.
- D looks for counterevidence against A's interpretation.

Today this breaks in four places:

1. **Duplicate work.** B can't see that A already ran an ablation until someone merges A's fork. A safe merge afterwards doesn't stop the duplicate run beforehand.
2. **No exact version to reuse.** C has nothing stable to reproduce. A's fork keeps changing, and nothing captures its knowledge, code, and evidence together under one identity.
3. **Lost alternatives.** If A and D disagree, the canonical `logic/` can hold only one current interpretation. Today the other one survives only in a private fork.
4. **Unclear authority.** The artifact lock serializes CLI writes inside one checkout. It doesn't sync separate checkouts or decide who may change canonical `logic/`.

Agora (the related system, [v4](https://arxiv.org/html/2609.18094v4)) solves the visibility part with a durable contribution graph that workers inspect before choosing work. This design offers what a graph of commits doesn't:

- **Typed knowledge.** Claims carry conditions, falsification criteria, and dependencies. Exploration nodes record questions, experiments, dead ends, and pivots.
- **Explicit source references.** Merged entries remember which fork and revision they came from.
- **Audited mutable logic.** Every change to canonical `logic/` keeps before/after history and an owning session.

That lets a worker fetch what a hypothesis depends on, rather than reading a whole log.

## Goals and non-goals

Goals:

- Peers see published results, negative results included, before canonical integration.
- Every published result points to an exact, complete, re-readable package of knowledge, code, and evidence.
- Peers see each other's current work, so duplicate runs are visible and any repetition is a choice.
- Canonical integration preserves sources, history, and competing interpretations.
- Numerical and argument checks (Lara) are recorded with their exact scope.

Non-goals for this series:

- Network transport across hosts, a hosted publication API, or cryptographic participant admission.
- Live co-editing of canonical `logic/`, or more than one canonical writer.
- Model-generated ranking, semantic embeddings, or automatic research scheduling.
- New native ARA node kinds, or changes to native claim statuses.
- Network access, model calls, or community commands inside `ara`.

## Glossary

| Term | Meaning |
|---|---|
| Fork | A private copy of the seed ARA owned by one participant, with its own code workspace. |
| Continuity writer | The participant's authorized knowledge writer (the research-manager skill), which records into its own fork through CLI writes. |
| Integration PM | The single project manager that owns canonical `logic/` and merges contributions into it. |
| Contribution | An immutable, published package: a frozen native ARA snapshot, the declared code and evidence, and an external envelope describing it. |
| Envelope | Metadata stored outside the native ARA: attribution, lineage, inventory, verification relations. It never becomes an alternate knowledge format. |
| Runner | The external process (`ara-eval`) that isolates workspaces, freezes packages, publishes, and builds community views. |
| Coordinator | The runner's single same-host component that serializes publication and owns the shared channel. |
| Intention | An advisory, expiring record of what an actor plans or is attempting. |
| Briefing | A bounded, snapshot-bound view of canonical facts, peer contributions, and intentions, built from CLI reads. |
| Closure signal | The research-manager trigger (abandonment, affirmation, empirical resolution, commitment) that promotes staged observations. Publication is not one. |
| Interface-only skills | The CLI-backed skills from [plan 13](../agent-cli-interface/13-cli-backed-skills.md). They keep their original single-writer roles; collective authority is a separate, reviewed extension. |
| Logical round | The coordinator's reproducible clock for intention expiry, from plan 14. |

## Architecture

### How the ARA layers support different writers

| Layer or object | Meaning | Collective rule |
|---|---|---|
| `logic/` | Current best understanding. | A fork's writer may record a proposed interpretation; only the integration PM changes canonical `logic/`. |
| `trace/` | Decisions, experiments, failures, pivots, continuity. | Each participant's history is preserved through native merges. Scientific dependencies are explicit native references. |
| `staging/` | Observations awaiting closure signals. | Published promptly, left unpromoted. |
| `src/`, `evidence/` | Code, configuration, environments, raw proof. | Publication pins their bytes; knowledge merge never installs incoming code. |
| Envelope | Attribution, lineage, inventory, publication and verification state. | Stored in the external community record ([02](02-contribution-workflow.md)). |
| Lara argument and verdict | Numerical certificates and support/attack status for selected claims. | Stored beside the frozen ARA with a source binding ([03](03-lara-integration.md)). |
| Intention | What an actor plans or is doing. | Advisory, expiring, never exclusive. |

The canonical ARA is one reviewed synthesis of contributions, not the only place evidence is published.

### Who does what

| Owner | Does | Does not |
|---|---|---|
| Participant | Chooses a research action; states hypothesis, change, and result. | Claim work is verified because someone reused it. |
| Continuity writer | Records knowledge into its own fork with CLI writes. | Change frozen history or touch another fork. |
| `ara` CLI | Reads, validates, allocates IDs, commits guarded writes, merges, and (new) captures snapshots. | Fetch peers, authenticate actors, choose experiments, or run code. |
| Runner / coordinator | Isolates workspaces, freezes and publishes packages, serves views, meters budget. | Decide scientific truth or edit published packages. |
| Integration PM | Merges contributions and resolves mutable conflicts through audited operations. | Override protected history or silently accept a disputed conclusion. |
| Verifier | Tests an exact published version and publishes a scoped verdict. | Imply independence from account separation alone. |
| Argument producer | Writes Lara arguments and source bindings for selected claims. | Gain knowledge-write privileges. |
| Lara checker | Rechecks certificates, bridges, and compatible argument maps. | Fetch evidence, reproduce experiments, or change claim status. |

### End-to-end flow

```text
worker (private fork)
  -> ara reads: question, dependencies, claims, failed attempts
  -> runner: refresh briefing; post intention
  -> worker tools: run experiment in isolated code workspace
  -> continuity writer: ara apply + session history
  -> runner: ara snapshot                                  [01]
  -> argument producer + runner: optional Lara check       [03]
  -> runner: freeze package, publish contribution, announce [02]
  -> peers: read exact snapshot; reuse, reproduce, or challenge
  -> integration PM: ara merge --source-key ...; resolve; record synthesis
  -> runner: publish integration receipt; refresh briefing
```

Private forks are the recommended path. Same-checkout concurrency stays supported for cooperating writers, but the lock is not access control. File-system permissions and runner tool policy enforce fork isolation.

## Constraints

- The CLI stays deterministic, offline, and model-free.
- Knowledge files stay authoritative. No database, cache, briefing, or intention snapshot replaces them.
- Native IDs and the existing import/alias records are kept. Display labels (`--as`), provenance categories (`ai-executed`), and participant identity are distinct.
- Git ancestry is repository history. Scientific dependencies are native references or contribution relations.
- Each repository owns its own semantics. Portable contribution, source-binding, and role contracts belong to `Agent-Native-Research-Artifact`. Certificate and support/attack semantics belong to `Lara`. `ara-eval` is the first runtime, not the owner of either.
- Lara runs as a separate pinned process. No Haskell dependency enters the Rust workspace.

## Sources

- Current CLI behavior: [agent CLI guide](../../docs/agent-cli.md) and [delivery evidence](../../docs/verification/agent-cli-2026-10-02/README.md).
- Collective protocol: [plan 14](../agent-cli-interface/14-shared-frontier-intentions.md) keeps frontier and intentions separately installable and puts transport outside `ara`.
- Runner and evaluation: the [ara-eval design](../../../ara-eval/plans/cli-interface-and-collective-research-evaluation.md), which separates the interface study from the collective-stack comparison. The older [harness plan](../agent-cli-interface/15-experiment-harness.md) is historical context and is not edited by this series. Relative `ara-eval` links assume the sibling checkout.
- Research motivation: Obsidian notes `Ideas/CLI-Mediated ARA` and `Analyses/Agora vs ARA as Research Records`. They are not implementation authority.
- Agora: [v4 paper](https://arxiv.org/html/2609.18094v4). Its [public repository](https://github.com/yifanzhang-pro/Agora) currently holds the description, website, and figures, not the service. No integration with a live Agora service is assumed.
- Lara: see [03](03-lara-integration.md#sources).

## Phases

The core track proves publication, visibility, and integration. The Lara track adds checking. Whether the Lara track gates core phases 3–4 is decision D3.

| Phase | Deliverable | Acceptance before proceeding |
|---|---|---|
| 1. Freeze contracts | Snapshot contract ([01](01-ara-snapshot.md)), contribution/verification envelope, role policy, publication boundary ([02](02-contribution-workflow.md)). | Covers negative results, missing evidence, competing interpretations, source advancement, and repetition. Collective roles reviewed separately from the interface-only skills. |
| 2. Capture and publish | `ara snapshot`; runner freezing, inventories, contribution records, recoverable announcements. | Two processes publish complete snapshots. Exercise unsafe output, source mutation, pending transactions, missing objects, lost acknowledgment, and restart. |
| 3. Shared frontier | Briefing from CLI reads, intentions, integration receipts. | A peer sees a result before integration and retrieves its exact evidence. Incomplete views report truncation. |
| 4. Integrate and verify | Audited merges, source-qualified receipts, reproduction records, separately evaluated synthesis. | Colliding identities and histories survive; repeated import is a no-op; protected edits reject; a synthesis is evaluated anew. |
| L1. Lara contracts | ARA-to-Lara binding schema; selected policy and vocabulary ([03](03-lara-integration.md)). | Reviewed with the `ara-eval` stack revision. |
| L2. Lara checks in the frontier | Adapter, packaged arguments, individual and composite verdict views. | Shows gaps, rejected certificates, same-setting contests, non-conflicting different settings, incompatible policies, and incomplete map coverage without changing native maturity. |
| 5. Complete worker loop | Provider-backed runner smoke over the reviewed contracts, with all costs recorded. | Approved smoke covers intention → experiment → record → (argument) → publication → peer reproduction → integration → recovery. No paid call without approval. |
| 6. Evaluate | The reviewed `ara-eval` collective-stack study. | Dependencies, prompts, policies, tasks, budgets, and held-out evaluation frozen before collection. |

## Alternatives considered

- **Many agents writing one checkout through the lock.** This works for cooperating writers, but it doesn't isolate experiment code or preserve separate interpretations.
- **Plain Git merges.** They keep history, but they can't do native ID reconciliation, immutable-history checks, or audited conflict resolution.
- **A central planner that assigns experiments.** Rejected: participants choose work from shared evidence; the coordinator serializes publication, not research.
- **A hosted Agora-style service.** It adds deployment, admission, storage, and network-failure contracts before the same-host loop is proven.
- **Making every contribution canonical.** That hides alternatives and delays visibility until integration.
- **A new native verification node kind.** It would change the protocol, parser, writers, and viewer. An external envelope pointing at native records avoids that cutover.

## Tradeoffs

- **Storage and authoring cost.** Snapshots and envelopes take space and effort. Large evidence belongs in pinned object storage, and its retention cost should be reported.
- **Read volume.** Publishing every attempt keeps useful failures but adds reading; bounded briefings help only if full follow-up stays possible.
- **A bottleneck at integration.** One integration PM can slow synthesis, though raw contributions stay visible without waiting.
- **Anchoring.** Shared visibility can make everyone follow the current leader. Report branch concentration and neglected branches.
- **A single point of failure.** The same-host coordinator is one, and the pause policy can delay work. Explicit failure beats showing stale peer data as fresh.

## Migration

- **Existing ARAs** keep working without a community record.
- **A collective run** starts from an immutable seed, creates distinct fork identities, and pins its role and channel contracts.
- **No carry-over.** Active intentions, private journals, credentials, and caches never carry into a new run.
- **Historical contributions** are imported only with their original attribution. Unknown fields stay unknown.
- **Repository rules still apply.** Interface-only procedures stay unchanged. Adding Lara to the evaluated stack needs a reviewed `ara-eval` update. Functional CLI PRs follow the patch-version, changelog, and viewer rules; this docs-only revision needs no version bump.

## Decisions needed

| ID | Decision | Options | Recommendation |
|---|---|---|---|
| D1 | Where do plans 02 and 03 live long-term? | (a) Stay here as the umbrella; (b) move 02 contracts to `Agent-Native-Research-Artifact` and 02 runner + 03 to `ara-eval`; (c) split differently. | Deferred by the human developer; keep them staged here. |
| D2 | One canonical writer? | (a) Many private contributors, one integration PM; (b) partitioned claim ownership; (c) concurrent canonical writers. | (a). Revisit only with evidence of a bottleneck. |
| D3 | Does Lara gate core phases 3–4? | (a) Yes, as in the original draft; (b) no, Lara is a parallel track that joins at phase 5. | (b). The core loop can be proven without Lara, and failures stay easier to isolate. Lara stays in scope. |
| D4 | `ara snapshot` design choices | See [01 decisions](01-ara-snapshot.md#decisions-needed). | As recommended there. |
| D5 | Merge expected-revision check | (a) Rely on the integration PM's exclusive control of the canonical workspace; (b) add an `--expected-revision` guard under the CLI lock now. | (a). Add (b) through a separate review if concurrent canonical advancement becomes a requirement. |

## Review record (2026-10-03)

A review of the original single draft found the technical design sound but hard to review. The findings and how this revision addresses them:

| Finding | Change |
|---|---|
| The problem was abstract and buried; the clearest motivation (the A/B/C/D example) sat mid-document. | The Problem section now opens with the example and names four concrete failures. |
| It didn't say what ARA adds over Agora's contribution graph. | Stated in Problem. |
| The "What exists and what this plan adds" table mixed problem and proposal. | Removed; current capabilities are cited in each plan's own background. |
| Scope didn't match the repository: the only `ara-cli` change was `ara snapshot`, but most of the 6.9k words were runner, protocol, and Lara design. | Split into 01 (CLI), 02 (contracts + runner), and 03 (Lara). |
| The one CLI change had the thinnest spec and hedged on whether it was needed. | 01 gives the interface, an example manifest, error codes, a rationale grounded in existing code, tests, and explicit decisions. |
| Too many negative statements hid the positive design. | Positive behavior comes first; non-guarantees are collected into per-plan lists. |
| Terms were undefined. | Glossary added. |
| There were no concrete examples of the envelope or manifest. | Illustrative examples added in 01 and 02. |
| "Next steps" said "review X" but listed no decisions. | Replaced with a decisions table with recommendations. |
| Lara sat on the critical path of phases 1–4. | Lara moved to a parallel track; whether it gates the core phases is D3. |

The reviewer checked that every linked plan, doc, and source file exists, and that `ara merge` already requires `--source-key` and treats `--as` as a display label only (`crates/ara-cli/src/merge.rs`).

## Next steps

1. Review [01-ara-snapshot](01-ara-snapshot.md); it is the only `ara-cli` implementation in the series.
2. Decide D2–D5.
3. Review 02 and 03 as cross-repository designs; decide D1 later.
4. Do not commit this draft or start implementation without human approval.
