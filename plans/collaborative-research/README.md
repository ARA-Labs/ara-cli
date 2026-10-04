# CLI-mediated parallel and collaborative ARA research
**Date:** 2026-10-02 (revised and split 2026-10-03)

Status: **approved** by the human developer on 2026-10-03, with the review revisions below. Implementation pending. The approval adopts this design and its implementation scope; this turn changes documentation only. It does not create a commit, start a paid run, deploy a service, approve an upstream protocol release, or waive owning-repository contract and evaluation gates. This series replaces `plans/collaborative-research-cli.md`.

## TL;DR

Researchers (human or agent) work in private ARA forks. When a result is ready, an external runner freezes the fork and its declared inputs, publishes an immutable *contribution*, and shows it to peers before canonical integration. Peers can read, reproduce, challenge, or import its knowledge and publish their own follow-on results. One integration project manager merges selected contributions into canonical `logic/`, keeping competing interpretations visible. Lara optionally rechecks the numbers and arguments behind selected claims.

`ara-cli` supplies offline snapshots and native merges. This series adds `ara snapshot` and fixes identity/provenance handling for a peer result that reaches canonical through more than one fork. Publication, participant authority, community views, and Lara invocation remain outside the CLI.

## Plan series

| Plan | Implementation target | Status |
|---|---|---|
| [01-ara-snapshot](01-ara-snapshot.md) | `ara-cli`: snapshot command, shared capture rules, and an internal jj-lib version store hidden from agents | Directory-only `ara snapshot --output` delivered (#101). Spike evidence recorded (#102); the optional store waits on re-approval of D-S4, D-S6, and D-S8. |
| [02-contribution-workflow](02-contribution-workflow.md) | `Agent-Native-Research-Artifact` (contracts) and `ara-eval` (runner) | Approved design; staged here pending upstream routing |
| [03-lara-integration](03-lara-integration.md) | `Agent-Native-Research-Artifact` (bindings), `ara-eval` (adapter), `Lara` (docs only) | Approved design; staged here pending upstream routing |
| [04-peer-feedback-merge](04-peer-feedback-merge.md) | `ara-cli`: identity reconciliation and provenance transport | Delivered in `ara-cli` (#103 contract, #104 implementation). Adoption of the contract in the protocol repository is pending. |

Plans 02 and 03 remain staged here, following the precedent of [plan 14](../agent-cli-interface/14-shared-frontier-intentions.md). D1 leaves their eventual repository placement deferred. Plan 04's portable provenance contract belongs to the protocol repository even though its implementation lives in `ara-cli`.

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
- Workers can incorporate peer or canonical knowledge into their forks and republish without duplicating original identities or losing history.

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
| Contribution | An immutable record binding a frozen native ARA, declared code/evidence, and its attribution and lineage. Its contribution ID covers the envelope and payload digest. |
| Envelope | Metadata outside the native ARA: attribution, lineage, inventory, and verification relations. It is bound into the contribution ID, not an alternate knowledge format. |
| Native revision | The existing `ara.artifact/v1` fingerprint of captured paths and bytes, used by merge. It excludes file modes. |
| Capture ID | Identity of the exact native snapshot manifest, including file digests and full modes. Store lookup/export uses this ID; it does not identify attribution or external inputs. |
| Payload digest | Identity of frozen bytes and modes, including the snapshot and declared external inputs. It does not identify attribution or parentage. |
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
| Argument producer / reviewer | Producer writes arguments and bindings; an authorized reviewer separately attests their faithfulness. | Gain knowledge-write privileges or count producer self-review as audited coverage. |
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
  -> peers: read/reproduce/challenge; optionally import into their fork [04]
  -> peers: publish a follow-on contribution with retained parent identities
  -> integration PM: merge exact versions; acknowledge external files; resolve
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

### Zero-cost when collaboration is unused

Collaboration is pay-for-use. Existing commands must not initialize or access the snapshot store, contact the runner or Lara, capture extra artifact bodies, create collaboration state, or perform collaboration maintenance. This applies even when `.ara/vcs/` exists, is large, or is corrupt. Explicit snapshot operations may pay for capture, storage, and the existing artifact lock; ordinary reads and writes must not inherit that work.

Keep ordinary parser loads, guarded-write snapshots, and lazy identity resolution separate from complete publication capture. Plan 04 may add work to imports that need origin reconciliation, but must preserve lightweight paths for artifacts without imported provenance and avoid rebuilding the full origin graph for unrelated reads. Required provenance validation stays fail-closed.

Build and installation costs are part of the requirement. The default CLI build and default release distribution must exclude the optional version-store crate and jj-lib dependency graph. Plan 01 uses a non-default `snapshot-store` feature and a separately selected store-enabled distribution. `ara-core`, `ara-wasm`, and the viewer gain no store dependency. The runner, collective skills, and Lara remain separately installed and explicitly invoked.

### How the zero-cost requirement is checked

Before implementation measurements, freeze the fixtures, commands, toolchain, build flags, repetition policy, and noise tolerances in the stage's verification record. Compare each CLI stage against its parent on `feat/collaborative-ara`, and compare the complete series against the recorded pre-series revision. Do not relax tolerances after seeing a regression. A repeatable slowdown or resource increase outside the predeclared measurement noise blocks the stage until fixed or the developer explicitly revises the requirement.

| Surface | Required evidence |
|---|---|
| `status`, `ls`, ordinary `show`, `find`, and existing validation/viewer commands | No new store access, collaboration initialization, or extra evidence-body capture. Preserve results and failure behavior. |
| `add`, `edit`, `apply`, and guarded fixes | No automatic capture after a write or at the start of the next command. Existing transaction and audit guarantees remain intact. |
| Existing directory and local-Git merge | Compare simple imports and long linear histories as well as the new feedback cases. Correctness alone does not establish unchanged cost. |
| Missing, corrupt, unreadable, or large `.ara/vcs/` | Ordinary commands remain independent of store contents and availability. Exercise both default and store-enabled binaries. |
| Runtime | Measure cold and warm command latency, peak memory, and filesystem I/O on a small artifact, large external evidence, and long imported histories. Use access tracing or equivalent instrumentation to prove absence of store work; timings alone are insufficient. |
| Build and distribution | Inspect default dependency trees and release artifacts; measure clean/incremental build time and binary/download size separately for default and store-enabled builds. Run native and Wasm checks. |

The planned implementation has no performance measurements yet. Shared privacy-filter corrections remain required, but ordinary loads must not be replaced with `load_complete`.

## Sources

- Current CLI behavior: [agent CLI guide](../../docs/agent-cli.md) and [delivery evidence](../../docs/verification/agent-cli-2026-10-02/README.md).
- Collective protocol: [plan 14](../agent-cli-interface/14-shared-frontier-intentions.md) keeps frontier and intentions separately installable and puts transport outside `ara`.
- Runner and evaluation: the [ara-eval design](../../../ara-eval/plans/cli-interface-and-collective-research-evaluation.md), which separates the interface study from the collective-stack comparison. The older [harness plan](../agent-cli-interface/15-experiment-harness.md) is historical context and is not edited by this series. Relative `ara-eval` links assume the sibling checkout.
- Research motivation: Obsidian notes `Ideas/CLI-Mediated ARA` and `Analyses/Agora vs ARA as Research Records`. They are not implementation authority.
- Agora: [v4 paper](https://arxiv.org/html/2609.18094v4). Its [public repository](https://github.com/yifanzhang-pro/Agora) currently holds the description, website, and figures, not the service. No integration with a live Agora service is assumed.
- Lara: see [03](03-lara-integration.md#sources).

## Phases

The core track proves publication, visibility, and integration. Lara runs in parallel and joins the complete worker loop at phase 5; it does not gate core phases 3 or 4. Plans 01 and 04 can be developed independently, but the runner cannot enable native peer imports until plan 04 passes. Approval does not claim that these capabilities already exist.

| Phase | Deliverable | Acceptance before proceeding |
|---|---|---|
| 1. Freeze contracts | Snapshot contract ([01](01-ara-snapshot.md)), contribution identity, roles, publication and external-evidence receipts ([02](02-contribution-workflow.md)), origin/import-history representation ([04](04-peer-feedback-merge.md)). | Covers negative results, missing evidence, envelope tampering, source advancement, feedback histories, and repetition. Owning repositories adopt versioned contracts separately from the interface-only skills. |
| 2. Capture and publish | `ara snapshot`; runner quiescence, inventories, immutable contributions, recoverable announcements. | Two processes publish complete snapshots. Exercise unsafe output and output races, source mutation, pending transactions, missing objects, changed requests, lost acknowledgment, and restart. |
| 3. Shared frontier | Briefing from CLI reads, intentions, contribution and integration receipts. | A peer sees a result before integration and retrieves its exact evidence. Incomplete views report truncation. Peer native imports remain gated on plan 04. |
| 4. Integrate and verify | Peer-feedback merge support; audited integration; external-file acknowledgments; reproduction records; separately evaluated synthesis. | Publish, peer import, republish, canonical import, and canonical feedback all preserve original identities and histories. Latest replay and older receipt lookup do not duplicate entries. Protected edits reject; external evidence remains retrievable. |
| L1. Lara contracts | Binding and reviewer authority, setting registry, attachment and map-scope identities ([03](03-lara-integration.md)). | Freeze schemas and actual checker/policy/vocabulary pins with their owners; review the `ara-eval` stack revision. |
| L2. Lara checks in the frontier | Adapter, review attestations, initial and later checks, individual and composite verdict views. | Shows rejected certificates, same-setting contests, different-setting separation, unreviewed/disputed bindings, incompatible policies, and incomplete map coverage without changing native maturity. |
| 5. Complete worker loop | Provider-backed runner smoke over the adopted contracts, with all costs recorded. | Approved smoke covers intention → experiment → record → (argument) → publication → peer reuse/import → republication → integration → restart/rebuild. Lara joins here. No paid call without separate budget approval. |
| 6. Evaluate | The reviewed `ara-eval` collective-stack study. | Dependencies, prompts, policies, tasks, budgets, and held-out evaluation frozen before collection. |

### Stage PR instructions

Each stage in the table above (1, 2, 3, 4, L1, L2, 5, and 6) must be delivered as a separate PR. In this repository, every stage PR targets **`feat/collaborative-ara`**, not `main`. Branch each stage from the updated integration branch after its prerequisites land. If a stage needs smaller reviewable parts, name those substages before implementation and give each its own PR to the same base; do not combine independent stages into one PR.

Cross-repository ownership remains unchanged. Protocol, runner, and Lara work goes through linked PRs in its owning repository, with that repository's base branch stated explicitly rather than assuming this branch exists there. The corresponding stage PR here records adopted contracts, exact dependency revisions, and acceptance evidence. Documentation/evidence-only stage PRs are valid; do not move external runtime code into `ara-cli` to satisfy the workflow.

Every stage PR must identify its phase/substage, prerequisite PRs, changed contracts, tests and actual-program smoke evidence, and applicable zero-cost measurements. A stage is complete only when its required owning-repository changes are adopted and its acceptance checks pass. Functional CLI PRs include the workspace patch bump, local lockfile updates, changelog entry, and required viewer review; documentation-only PRs do not. Keep dependent stages blocked on unmet contracts, while Lara's independent track may proceed in parallel.

This instruction defines the implementation workflow. It does not authorize creating commits or PRs during this documentation revision, merging the integration branch into `main`, choosing a release procedure, or starting paid runs.

## Alternatives considered

- **Many agents writing one checkout through the lock.** This works for cooperating writers, but it doesn't isolate experiment code or preserve separate interpretations.
- **Plain Git merges.** They keep history, but they can't do native ID reconciliation, immutable-history checks, or audited conflict resolution.
- **A central planner that assigns experiments.** Rejected: participants choose work from shared evidence; the coordinator serializes publication, not research.
- **A hosted Agora-style service.** It adds deployment, admission, storage, and network-failure contracts before the same-host loop is proven.
- **Making every contribution canonical.** That hides alternatives and delays visibility until integration.
- **Package-only peer reuse.** It avoids the current diamond-merge failure but prevents workers from maintaining native knowledge that incorporates peers. Read-only reuse remains available; it does not replace the required feedback path.
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

## Approved decisions and remaining gates

The developer approved these choices on 2026-10-03. Implementation evidence, upstream adoption, and experimental registration remain required before the corresponding capabilities or studies run.

| ID | Decision | Adopted choice or remaining gate |
|---|---|---|
| D1 | Where do plans 02 and 03 live long-term? | Placement remains deferred. Keep them staged here; each repository retains semantic ownership. |
| D2 | Canonical writer | Many private contributors, one integration PM. Revisit only with evidence of a bottleneck. |
| D3 | Lara on the core critical path | No. Develop Lara in parallel and join at phase 5. Lara remains in scope. |
| D4 | `ara snapshot` | Adopt [01's decisions](01-ara-snapshot.md#approved-decisions): public command, lock, no source-key option, report-only diagnostics, caller-enforced direct-writer quiescence. Revision pending re-approval: an internal jj-lib store under `.ara/vcs/`, hidden from agents, and the `snapshot create`/`list`/`export` command shape ([01 pending decisions](01-ara-snapshot.md#decisions-pending-re-approval-revision)). |
| D5 | Merge expected-revision flag | Rely on the integration PM's exclusive workspace ownership across review and commit. A new CLI flag is outside this series. |
| D6 | Peer and canonical feedback | Support native imports and republication. Include [04](04-peer-feedback-merge.md), not a package-only restriction or provenance bypass. |
| D7 | Contribution identity | Distinguish native merge revision, exact capture ID, complete payload digest, and envelope-bound contribution ID. Parents and verification target contribution IDs; store export targets capture IDs. |
| D8 | Lara audit and scope | Authorized review separate from producer assertions; pinned setting identities; immutable later attachments and map revisions. Freeze concrete schemas in L1. |
| D9 | Cost for nonusers | Require runtime and build/install pay-for-use, explicit snapshots only, and the zero-cost acceptance gates above. |
| D10 | Stage delivery | Separate PR per stage or declared substage; all `ara-cli` PRs target `feat/collaborative-ara`. Cross-repository work uses linked owning-repository PRs. |

## Review record (2026-10-03)

The initial review of the single draft focused on organization. A subsequent source and real-CLI review found an integration blocker and missing contracts. The original restructuring changes were:

| Finding | Change |
|---|---|
| The problem was abstract and buried; the clearest motivation (the A/B/C/D example) sat mid-document. | The Problem section now opens with the example and names four concrete failures. |
| It didn't say what ARA adds over Agora's contribution graph. | Stated in Problem. |
| The "What exists and what this plan adds" table mixed problem and proposal. | Removed; current capabilities are cited in each plan's own background. |
| The initial draft concentrated runner, protocol, and Lara design in the CLI repository. | Split capture, contribution workflow, and Lara into separate plans. The later behavioral review adds a second CLI/core plan, 04. |
| Snapshot capture had the thinnest specification. | 01 now defines the interface, manifest, errors, shared capture rules, isolation boundary, and tests. |
| Too many negative statements hid the positive design. | Positive behavior comes first; non-guarantees are collected into per-plan lists. |
| Terms were undefined. | Glossary added. |
| There were no concrete examples of the envelope or manifest. | Illustrative examples added in 01 and 02. |
| "Next steps" lacked concrete decisions. | Added decision tables; the adopted choices are now recorded above. |
| Lara sat on the critical path of phases 1 through 4. | Moved Lara to a parallel track joining at phase 5. |

The behavioral review ran temporary fixtures with the actual `ara 0.1.23` binary. Canonical imports of A1 and B1 and B's import of A1 succeeded. Canonical's import of B2 using exact B1 as base then rejected with `merge.alias_conflict` for `fork-a:C77`. A separate import containing a new `src/worker.py` committed knowledge metadata but returned exit 1 and an unresolved `external_read_only` conflict; replay retained it. These observations establish current limits, not passing acceptance evidence for the planned fixes.

| Finding | Approved revision |
|---|---|
| A peer's original identity can arrive through multiple forks, which the current merger cannot reconcile. | Added [04](04-peer-feedback-merge.md), including source-fact/import-event separation, diamond and canonical-feedback tests, and explicit CLI/core scope. |
| Normal code/evidence changes produce unresolved conflicts without scientific disagreement. | 02 defines audited external-file acknowledgments, source-owned inherited resolution, package-backed evidence reads, and completion criteria. |
| A payload digest alone does not bind attribution, lineage, or verification targets. | 02 adds a canonical envelope-bound contribution ID and request-to-record idempotency. |
| A checker verdict does not authenticate formalization review or establish shared experimental settings. | 03 defines reviewer authority, setting descriptors, audit coverage, and immutable attachment/map scopes. |
| A lock and double-read cannot isolate unrestricted direct writers. | 01 states the quiescence precondition, shared privacy filtering, and verified no-replace output publication. |

The developer approved the revised design after the behavioral review. This revision changes documentation only; it performs no implementation, commit, paid run, or upstream release.

The later cost review found that `ara.artifact/v1` excludes modes but the proposed store deduplicated and exported by that fingerprint. The current binary reported the same source revision after a mode-only change. Plan 01 now separates exact capture identity from native merge revision, preserves the manifest and full captured modes, and makes storage optional. The developer requested these revisions and the stage-PR workflow. Backend selection and command-shape re-approval still depend on the spike; no performance result for the planned implementation is claimed.

## Delivery record

Stage PRs merged into `feat/collaborative-ara`:

| Stage | PR | Content |
|---|---|---|
| 1a | #100 | `ara.snapshot/v1` contract; zero-cost measurement policy and harness (`scripts/collab-zero-cost.py`). |
| 1b | #103 | Peer-feedback provenance contract (`docs/collaborative-research/provenance-contract.md`), amended in #104. |
| 1c | this PR | Plan 02 contribution contracts adopted from `Agent-Native-Research-Artifact` [#39](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/pull/39) at `36e6f89` (`evaluation/collaborative/`). The capture ID reproduces `ara.capture/v1` byte for byte on three real snapshots. |
| L1 | this PR | Lara contracts adopted from `Agent-Native-Research-Artifact` [#40](https://github.com/ARA-Labs/Agent-Native-Research-Artifact/pull/40) at `e5179d9` (`evaluation/collaborative/lara/`). The checker pin is Lara `a31299f`, a local build with sha256 `dfbc5966…`, provisional until a release executable is pinned. |
| L2 | this PR | Lara adapter, review attestations, composite maps, and verdict views in `ara-eval` [#3](https://github.com/ARA-Labs/ara-eval/pull/3) at `6e6d4e5`. It pins the protocol at `e5179d9` and Lara `a31299f`. 80 tests pass against the real `lara` and `ara`. |
| 2a | #101 | `ara snapshot --output`, 0.1.24. Zero-cost PASS (`docs/verification/collaborative-research/phase-2a/`). |
| 2 (runner) | this PR | Capture, freeze, and publication in `ara-eval` [#2](https://github.com/ARA-Labs/ara-eval/pull/2) at `71da192`. It pins ara-cli `da43bf6` and the protocol at `36e6f89`. 48 tests pass against the real binary, covering concurrent publishers, stale input, lost acknowledgment, announcement recovery, index rebuild, and mode-only identity. |
| 2b | #102 | jj-lib store spike evidence. The spike source sits on the unmerged branch `spike/jj-snapshot-store`. |
| 4 | #104 | Peer-feedback identity reconciliation and `--self-key`, 0.1.25. Zero-cost PASS (`docs/verification/collaborative-research/phase-4/`). |

Not delivered, with the blocking decision for each:

| Stage | Blocked on |
|---|---|
| 2c optional store | The developer's re-approval of D-S4, D-S6, and D-S8 against the phase-2b numbers. |
| 3, 4 runner parts | In progress in `ara-eval` on `feat/collaborative-ara`, authorized by the developer on 2026-10-04. |
| 5 | Stages 2–4 and L2 on the runner side, plus separate budget approval for the provider-backed smoke. |
| 6 | Stage 5, the reviewed `ara-eval` stack registration, and budget approval for collection. |

## Next Steps

1. Freeze the versioned snapshot, contribution, and provenance contracts with their owning repositories, including exact capture IDs and the zero-cost measurement policy.
2. Implement 01 and 04 with their regressions and real-binary acceptance checks; enable the runner's native peer imports only after 04 passes.
3. Build 02's complete feedback/recovery loop and 03's parallel Lara track. Keep upstream placement D1 deferred until directed.
4. Follow the stage PR instructions above, targeting `feat/collaborative-ara` for this repository. Obtain separate paid-smoke and scored-study approvals before provider-backed execution or collection. Do not create commits or PRs during this documentation revision.
