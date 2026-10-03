# CLI-mediated parallel and collaborative ARA research
**Date:** 2026-10-02

Status: draft for human review. This plan authorizes no implementation, role change, network service, paid model run, commit, or release. Target owners are `ara-cli` for local artifact operations, `Agent-Native-Research-Artifact` for portable contribution and research-role contracts, `Lara` for numerical and argument checking, and the existing `ara-eval` repository for initial community execution. The user requested Lara's inclusion; the integration contracts and implementation remain subject to review.

## TL;DR

Each researcher works in a private workspace with an authorized knowledge writer, using the CLI for grounded reads, guarded writes, and source-aware integration. Add one offline snapshot export operation so publication reuses native capture rules. Publish immutable contributions before their conclusions enter canonical `logic/`. Reuse Lara's certified numerical comparisons, defeasible empirical bridges, and cross-artifact argument checking to expose gaps and contested interpretations. The external runner publishes contributions, exposes intentions and bounded frontier views, and invokes the pinned Lara checker; the CLI remains offline and does not schedule research.

## Problem

ARA separates a project's current understanding from the history and evidence that produced it. That design helps a later reader, but a parallel community also needs to know what peers are doing, what they just learned, which exact version can be reused, and which results need verification. A safe merge after an experiment cannot prevent two workers from unknowingly running the same experiment beforehand. A shared artifact lock protects local writes, but does not assign authority or synchronize separate checkouts.

Agora's relevant design is a durable contribution graph that workers inspect before selecting their own work. The opportunity for ARA is to combine that publication loop with typed knowledge, explicit source references, and audited mutable logic. Publishing an unsuccessful attempt must be possible without certifying a conclusion. Competing explanations must remain available even when the integration project manager accepts only one into `logic/`.

### What exists and what this plan adds

| Concern | Current capability or boundary | Proposed addition |
|---|---|---|
| Read a research record | `status`, `ls`, `show`, `path`, `refs`, `open`, and `find` inspect the selected local artifact. | A community briefing composes these reads with visible published contributions; it names its exact visibility boundary. |
| Write knowledge safely | `apply` and convenience commands allocate local IDs, validate deltas, retain audit history, and use recoverable transactions. | Collective instructions authorize each fork's continuity writer; they do not relax local write guards. |
| Integrate divergent records | Directory and local-Git `merge` preserve sources, resolve incoming identity collisions, and retain conflicts. | The integration worker consumes accepted contribution snapshots and records the distinction between import and scientific acceptance. |
| Coordinate work in progress | The separate collective extension has a same-host channel with sequencing, revision checks, acknowledgments, expiry, and recovery. | Reuse that channel for intention visibility and contribution announcements under an explicitly revised collective contract. |
| Preserve code and evidence | Agents may access source and evidence bodies directly. CLI knowledge digests do not establish a complete experiment package. | Freeze and inventory all declared inputs and outputs alongside the native ARA; reject incomplete packages. |
| Check numerical claims and arguments | Lara already rechecks ordered comparisons and relative drops and composes compatible arguments through `.laramap`. | Attach source-linked argument files and pinned verdicts to selected contributions; expose their scope without automatically promoting ARA claims. |
| Execute and account for communities | `ara-eval` has pinned dependencies and a draft runner/community design. Its README does not claim a provider-backed harness. | Implement this workflow through that existing design, without creating a second runner or a new evaluation program. |

### Which sources govern the draft

Current CLI behavior is described in [the agent interface guide](../docs/agent-cli.md) and [delivery evidence](../docs/verification/agent-cli-2026-10-02/README.md). The [collective protocol plan](agent-cli-interface/14-shared-frontier-intentions.md) keeps frontier and intentions separately installable and puts transport outside `ara`. The newer [ara-eval design](../../ara-eval/plans/cli-interface-and-collective-research-evaluation.md) supplies the actual external repository destination and separates the interface study from the collective-stack comparison. The older [harness plan](agent-cli-interface/15-experiment-harness.md) remains historical planning context; this draft does not silently edit its destination or experiment arms.

The Obsidian notes `Ideas/CLI-Mediated ARA` and `Analyses/Agora vs ARA as Research Records` supplied the research motivation. Their earlier proposals and figures are not implementation authority. [Agora v4](https://arxiv.org/html/2609.18094v4) is the related-work source. Its [public repository](https://github.com/yifanzhang-pro/Agora) currently contains the project description, website, and figures rather than the described service implementation. No integration with an available Agora service is assumed. The relative `ara-eval` links assume the current sibling checkout.

Lara's existing interfaces are described in its [specification](https://github.com/ARA-Labs/Lara/blob/a31299feafb484404b62bc3b4fd313d987cdda44/docs/spec.md) and [multi-artifact composition contract](https://github.com/ARA-Labs/Lara/blob/a31299feafb484404b62bc3b4fd313d987cdda44/docs/multi-artifact-composition-decision.md). Its executable [ordered comparison](https://github.com/ARA-Labs/Lara/blob/a31299feafb484404b62bc3b4fd313d987cdda44/src/Lara/Strict/Ord.hs), [relative-drop](https://github.com/ARA-Labs/Lara/blob/a31299feafb484404b62bc3b4fd313d987cdda44/src/Lara/Strict/RA.hs), and [comparison expansion](https://github.com/ARA-Labs/Lara/blob/a31299feafb484404b62bc3b4fd313d987cdda44/src/Lara/Elaborate/Comparison.hs) implementations establish the numerical guarantee. These links pin the inspected upstream revision, not a chosen experimental dependency. Freeze the actual source and executable digests at implementation and registration.

The README clarification is tracked in [Lara issue #6](https://github.com/ARA-Labs/Lara/issues/6). It requests clearer documentation of shipped numerical checks and their measurement-assurance boundary, not new checker behavior.

## Constraints

The CLI remains deterministic, offline, and free of model calls. Knowledge files remain authoritative; a database, cached briefing, or intention snapshot cannot silently replace them. Keep native IDs and validated import/alias records. A display label, provenance category such as `ai-executed`, and participant identity are different things. Git ancestry records repository history; scientific dependencies remain explicit native references or contribution relations.

The interface-only CLI skills retain their original research procedures and writer roles. Collective contributor authority is a separately reviewed intervention. Research-foresight readers remain read-only. A contributor may direct its own authorized project manager to update its private fork, but cannot edit another participant's continuity history or the canonical artifact. All mutable knowledge changes retain the required session history and before/after evidence.

The first deployment uses private workspaces and one same-host community coordinator. Multi-host transport, a hosted publication API, cryptographic participant admission, model-generated ranking, new verification node kinds, semantic embeddings, and live co-editing of canonical `logic/` are outside this plan. They require their own review and are unnecessary to prove the initial publication and integration workflow. The coordinator performs deterministic storage, visibility, and accounting work; it does not supply an uncharged research advisor.

Portable contribution, source-binding, and verification semantics belong to the ARA protocol. Lara owns numerical certificate and support/attack semantics. `ara-eval` is the first execution environment, not the owner of either language. Invoke Lara as a separately pinned checker process; this plan adds no Haskell dependency or model-backed argument generator to the Rust CLI.

## Proposed approach

### How the ARA layers support different writers

| Layer or object | Meaning | Collective rule |
|---|---|---|
| `logic/` | Current best understanding of the selected artifact. | A private fork can hold a proposed interpretation through its authorized writer. An integration project manager owns changes to canonical understanding. |
| `trace/` | Decisions, experiments, failures, pivots, and continuity history. | Preserve each participant's history through native merges and allowed metadata transitions. Record scientific dependencies explicitly. |
| `staging/` | Observations awaiting the skill's closure signals. | Publish observations promptly while leaving them unpromoted. Public visibility is not a closure signal. |
| `src/` and `evidence/` | Executable artifacts, configurations, environments, and raw proof. | Workers create them under existing permissions. Publication pins their bytes or immutable external objects; knowledge integration does not merge arbitrary code bodies automatically. |
| Contribution envelope | Attribution, parent versions, package inventory, publication state, and verification relations. | Store outside the native ARA in the external community record. It points to complete native snapshots and never becomes an alternate knowledge grammar. |
| Lara argument and verdict | Numerical certificates, supporting arguments, attacks, and policy-relative status for selected native claims. | Store alongside the frozen ARA with an explicit source-binding record. It does not replace native history, certify measurement origin, or grant write authority. |
| Shared intention | What an actor plans or is currently attempting. | Advisory and explicitly fresh or expired; it does not assert a completed result or exclusive scientific ownership. |

The community record keeps alternative contributions readable before canonical integration. The canonical ARA is one reviewed synthesis of those contributions, not the only place where evidence may be published. Native trace and staging retain their existing mutability exceptions; this proposal does not replace them with an event-only format or require one Git commit per native node.

Native claims carry conditions, falsification criteria, dependencies, and proof references. Exploration nodes record questions, experiments, decisions, dead ends, and pivots with parent and cross-dependency relations. Sessions preserve the writer's continuity and complete logic revisions. These relations let a worker retrieve what a hypothesis depends on rather than treat the artifact as one prompt-length document. Sharing adds visibility across artifacts; it does not turn a structural check into a reasoning audit or an independent experimental test.

### What Lara checks and how it enters the workflow

Use Lara to verify numerical consequences of declared empirical evidence and check the arguments connecting them to claims. Measurement execution, source-byte verification, and independent reproduction remain separate responsibilities. Reuse its existing numerical backends and support/attack rules in the runner.

| Existing Lara surface | Checked behavior | Remaining assurance boundary |
|---|---|---|
| `ord@1` | Checks `<` and `<=` with exact rational arithmetic, binds operands to cited premise cells, and reports the consulted dependencies. | The declared cells are inputs; arithmetic acceptance does not prove how they were measured. |
| `ra@1` | Recomputes relative drop, checks the witness fraction and claimed drop, enforces the threshold, and rejects a zero full cell. | Correct arithmetic does not establish evaluator correctness or experiment comparability. |
| `comparison` | Generates a certified numerical step plus a defeasible empirical bridge; checks metric polarity and structured system, metric, dataset, and setup bindings. | The comparison-setup leaf remains declared evidence that can be attacked. |
| `.lara` | Checks support terms, declared obligations, certificates, and typed attacks; reports scoped argument status. | Prose-to-formal correspondence, omitted evidence, and policy adequacy require review. |
| `.laramap` | Rechecks independently authored members under a shared formal contract and generates cross-member attacks from declared contraries. | It does not reconcile vocabularies, assess evidence independence, or pin member bytes. |

An argument producer reads the frozen ARA through complete CLI source reads and permitted evidence access. For each selected claim, the `.lara` binding record identifies the original source, exact native revision, selectors, and evidence objects behind its claims, leaves, arguments, and attacks. Record the formalization author, rationale, audit status, and assumptions. The checker does not verify prose-to-formal correspondence or transcription from a cited CSV. An LLM-assisted producer remains untrusted and uses the existing research budget.

Raw findings remain publishable without a complete Lara argument. Record coverage and whether each check was not performed, accepted, rejected, or unavailable. An accepted file can report `gap`, `defeated`, or `contested`; an absent check supplies no status. Preserve admission-blocked diagnostics and map refusals separately. A condition requiring Lara fails explicitly when its required checker is unavailable.

Store the argument and binding files outside the native snapshot root so their header can name its content fingerprint without a self-referential hash. The contribution package binds that snapshot plus exact argument, binding, policy, executable, and verdict bytes. `.laramap` aliases map to full contribution/source identities; neither aliases nor the author-declared `artifact` digest establish publisher authentication. Native integration can renumber entries, so retain original source-qualified bindings and the native import mapping. Re-author against a new snapshot when the supporting claim or evidence changes.

The runner materializes immutable argument files, invokes `lara check`, and archives the exit status, output, and all actual checker inputs. Composite checks name the exact member roster, map manifest, policy, signature, backend/theory selection, and input digests. Lara maps read current local paths and do not verify their declared artifact digests; package pinning supplies reproducibility. Check exit status before consuming output: a failed `--out` invocation can leave an older verdict file intact.

Build bounded composite views only from members with matching policy structure, proposition vocabulary, theories, and backends. Lara map v1 is flat, rejects members requiring unsupported admission/group pruning, and has no cross-member support imports or handwritten cross-member undercuts. A native ARA can cite a peer contribution, but its Lara argument must initially be self-contained with that evidence's original pinned provenance. Report incompatible or excluded members and the reason; never silently treat a partial map as the whole community. Extending map composition requires a separate Lara contract review.

Retain individual and composite verdicts with their respective scopes. A claim can be justified alone and contested in a map when another contribution declares a contrary under the same setting. Different experimental settings do not become a disagreement merely because their prose sounds opposed. Keep Lara argument status distinct from ARA research maturity and experimental reproduction verdicts; numerical acceptance, claim promotion, and canonical integration remain different events.

### Who performs each operation

```text
Human or agent in private workspace
  -> ara reads: question, dependencies, claims, failed attempts
  -> runner: refresh community snapshot and acknowledge intention
  -> tools: execute experiment in isolated code workspace
  -> local authorized writer: ara apply plus complete session history
  -> runner: export frozen native snapshot before argument authoring
  -> argument producer: author source-linked Lara for selected claims
  -> runner: check numerical certificates and empirical arguments
  -> runner: freeze package, validate, publish durable contribution
  -> peers: inspect exact snapshot, reuse it, or verify it
  -> runner: recheck compatible Lara members for shared disagreement view
  -> integration PM: ara merge, adjudicate conflicts, record synthesis
  -> runner: publish integration receipt and refresh community briefing
```

| Owner | Responsibility | Explicit limit |
|---|---|---|
| Participant | Choose a research action and explain its hypothesis, planned change, and result. | Cannot claim work is verified because another participant reused it. |
| Local continuity writer | Record the participant's knowledge through existing CLI operations. | Cannot change frozen history or operate on another fork. |
| CLI | Parse, query, validate, allocate native IDs, commit guarded writes, and reconcile native layers. | Does not fetch peers, authenticate a community actor, select experiments, or execute published code. |
| Community runner | Isolate workspaces, freeze packages, serialize publication, expose shared snapshots, and meter resources. | Does not decide scientific truth or alter immutable contributions to fix a view. |
| Integration PM | Import contributions and resolve mutable conflicts through audited native operations. | Cannot convert a protected-history violation into a generic edit or silently accept a disputed conclusion. |
| Verifier | Test an exact published version and publish observations and a scoped verdict. | Different account identity alone does not establish methodological independence. |
| Lara checker | Recheck numerical certificates, empirical bridges, and compatible argument maps under pinned inputs. | Does not fetch evidence, reproduce experiments, establish contributor independence, or update canonical claim status. |
| Argument producer | Author the formal argument and source bindings from the permitted snapshot and evidence. | Formalization is untrusted; producer authority does not add knowledge-write privileges. |

Same-checkout CLI concurrency remains supported for cooperating writers, but private workspaces are the recommended research path. They isolate code, experiment state, session ownership, and proposed logic changes. File-system permissions and runner tool policy enforce this boundary; an artifact lock is not an access-control system.

### Which work can proceed in parallel

For a question about why a method improves a task, workers can pursue different interventions from the same pinned seed:

- A tests an ablation of the proposed mechanism.
- B explores an alternative implementation or boundary condition.
- C deliberately reproduces A's published experiment at its exact code/configuration version.
- D searches for counterevidence or audits whether the interpretation follows from the measurements.

This is an example decomposition, not a fixed worker count or new experimental arm. A and B need not wait for canonical integration. C can consume A's frozen package directly. D remains a read-only researcher unless separately authorized to run an experiment; its report is input to a writer, not an edit. The integration PM synthesizes their records and preserves disagreement. The CLI supplies bounded context, local IDs, audit-safe writes, and semantic reconciliation; participants or the human still choose the scientific division of work.

### What identifies a contribution

Each package binds a community/run identity, publisher identity, an idempotent publication request identity, a stable fork/source identity, an exact native snapshot, and explicit parent contribution versions. Native entry references carry their source identity and artifact revision. Use the merger's existing stable source-key contract when importing; do not derive fork identity from `--as`, a moving branch, a directory name, or a local node number.

Reuse the merger's full artifact content fingerprint, computed over a complete snapshot, to identify native import inputs. It includes nonprivate source/evidence bodies and portable provenance, not just knowledge documents. Separately identify the full experiment package, which also binds execution metadata and inputs outside the native root. A selected `show --source` digest is not an artifact revision; a knowledge-only administrative inventory from the existing channel smoke is not one either. The current public CLI has no complete export/inventory command. The recommended addition is a small offline snapshot operation backed by the existing core loader and fingerprint, not a duplicate inventory implementation in Python.

| Envelope field group | Required meaning |
|---|---|
| Identity | Schema revision, community/run, publisher, request, stable source identity, and package digest. |
| Lineage | Exact starting knowledge revision, parent contribution digests, and native references identifying what the work uses. |
| Research payload | Pointer to the native experiment/question/observation, declared change, prediction recorded before execution, measured outcome, and follow-up. Do not copy the native body into an independently editable record. |
| Package inventory | Sorted relative paths, exact file digests, declared external object digests, executable entry point, configuration, and environment identity. |
| Verification | Exact target contribution/version, question or claim tested, method, evaluator/configuration, reproduced observations, evidence, and scoped verdict. |
| Argument check | Coverage, source bindings, Lara input/policy/backend identities, checker executable digest, exit status, diagnostics, and scoped verdict. Keep separate from experimental reproduction. |
| Publication receipt | Durable coordinator sequence, acknowledged request/package identity, and visibility snapshot identity. Receipt metadata cannot change frozen package contents. |

Define deterministic envelope encoding and hashing in the reviewed external schema. Keep the envelope outside its hashed artifact payload and the receipt outside the frozen package to avoid self-referential digests. Record executable/file modes separately: the existing content fingerprint covers paths and bytes, not modes. Paths must remain within declared roots; reject unsafe traversal, unsupported links, missing objects, and credentials. Large immutable inputs can live outside Git with a digest and a resolvable pinned location. Publication rejects an unavailable required object rather than declaring an incomplete result reusable. Corrections and superseding verification verdicts are new records targeting prior versions.

### Proposed local snapshot interface

`ara snapshot` is a proposed command, not an existing interface. Its spelling and schema require review. The intended invocation is `ara -C <fork> snapshot --output <new-directory> --source-key <stable-key> --json`.

The command acquires the existing cooperative artifact lock, rejects unresolved prepared transactions, loads the complete nonprivate native-root inventory, validates the export policy, and emits `ara/` plus a separate `snapshot.json` into a new output directory. The manifest records the supplied source key, content fingerprint, inventory digests and modes, diagnostics, and snapshot schema version. Output must be disjoint from the source root; never overwrite an existing package. Check the complete input inventory again before atomically exposing the output. A failed capture exposes no complete package.

This is transport export, not a new knowledge authoring mechanism. It does not mutate native history, initialize a community, authenticate the source key, fetch missing dependencies, execute code, or publish remotely. A source key is caller-supplied lineage, not an authorship certificate. External dependencies and experiment metadata remain the runner's envelope responsibility. Readers inspect the exported `ara/` with existing commands; integrators consume it with existing directory merge.

The runner still quiesces direct code/evidence writers: the CLI lock does not make arbitrary file-tool writes atomic. Snapshot rechecks detect changes, not hostile or unsynchronized writer behavior. If the implementation can expose this contract through an already usable core adapter without a public command, document that evidence at review; the default design is a CLI export so agents and external runners share one supported capture interface.

### How publication differs from integration

A local result passes through `draft -> frozen -> published`; canonical integration is recorded separately as `pending`, `imported-with-conflicts`, `integrated`, or `rejected-for-integration`. These are external workflow states, not new native claim statuses. Record state changes separately without rewriting the package. Publication establishes durable availability and conformance to the pinned structural acceptance policy, not scientific certification. Explicitly check the artifact's diagnostics: a successful guarded write guarantees no new diagnostic errors, not that the artifact was error-free beforehand. Retain unresolved scientific interpretations as declared state.

The coordinator makes the frozen package reachable in the authoritative record, assigns its publication sequence, and reconciles the shared announcement before returning a visibility acknowledgment. The announcement contains the package identity and sequence. It is a view of the durable publication, not a second source of scientific facts. Publication and native knowledge writes are separate transactions; no existing command makes both atomic.

The runner quiesces its worker, invokes the proposed snapshot operation, freezes the additional declared experiment inputs, and inventories referenced code/evidence without executing it. Prepared or corrupt native transactions cannot become a valid snapshot; private `.ara/transactions` and lock payloads never enter the package. Portable native identity/conflict files remain included. The runner verifies the snapshot manifest rather than reproducing native capture rules.

Git can store frozen packages and small append-only contribution envelopes in the community repository. Workers do not push competing edits to one mutable knowledge tree. A single coordinator publishes accepted records on this host; the record preserves publisher attribution independently of the Git committer. Transport and Git publication are runner operations, not new meanings for `ara merge --git`. The durable record pins the native snapshot's base and incoming revision so the integration PM can use ordinary directory merge without trusting a moving branch.

The current intention schema has closed fields and cannot bind an exact post-experiment contribution revision: completion carries source/native result references while retaining the pre-work artifact revision. It also verifies another intention, not an immutable result package. Extend result-version binding and contribution-target verification through a reviewed schema revision; an intention-completion receipt alone is not a publication certificate. Reuse the existing CAS, request-digest, journal, and recovery implementation rather than silently adding fields or a second channel convention.

### How peers find and use work before a merge

The community snapshot lists published contribution identities and their visibility sequence alongside the existing intention snapshot. The runner builds the briefing from CLI reads over exact immutable native snapshots plus validated contribution envelopes. It must not implement another YAML/Markdown knowledge parser. A briefing distinguishes canonical facts, the worker's local findings, published but unintegrated peer findings, active intentions, and stale intentions. Cross-artifact reading is a separately approved collective scope: the interface-only research-foresight skill remains isolated to its authorized ARA. Enforce access through the runner, not by giving every reader unrestricted sibling-directory access.

| Briefing section | Structural basis | Action it makes possible |
|---|---|---|
| Open questions and hypotheses | Native status, dependencies, and staging state. | Choose a question with relevant context. |
| Recent failed attempts | Published experiment/dead-end records with exact configuration and evidence. | Avoid accidental repetition or deliberately test a different condition. |
| Verification gaps and contested outcomes | Targeted reproduction records, preserved observations, and source-bound Lara gaps/attacks with individual or composite scope. | Reproduce a result, challenge comparability, supply missing evidence, or test an explanation. |
| Neglected alternatives | Branches without follow-on attempts, linked to their actual research parents. | Explore an alternative without depending on a popularity score. |
| Current investigations | Acknowledged intentions, owner, signature, sequence, and expiry. | Coordinate overlapping work or declare deliberate replication. |
| Integration gaps | Published contributions awaiting import, known merge conflicts, and receipt refs. | Help with synthesis or conflict adjudication. |

Bound the briefing by a documented selection/continuation policy. Report coverage, truncation, and the sequence used; missing or unreadable snapshots produce a visible incomplete/error result. Preserve references that let workers fetch complete native bodies and evidence from the same revision. Failure to see a contribution is not proof that nobody has tried it. Candidate-specific follow-up uses the proposed change, affected question, and experiment signature rather than always starting from the highest reported metric.

The extension keeps refinement, exploration, and verification as distinct choices. No numerical evidence score changes claim status. Initial branch-grouping rules use explicit lineage and tags, not semantic embeddings. The runner publishes any grouping policy and later change as a versioned contract; it cannot silently tune the view after observing research outcomes.

### How intentions avoid accidental duplication

Reuse the existing common channel outside private forks, including coordinator-owned sequencing, expected-revision checks, durable acknowledgments, idempotent requests, logical-round expiry, and outage handling. An intention names the exact starting snapshot, research question, proposed action, and a code/configuration signature. A verification intention explicitly names its target. Refresh before work selection, after results, and before budget-consuming execution or publication, as the collective contract already requires.

Conflicting intentions surface both actors and signatures for judgment. They do not grant exclusive ownership of a hypothesis or suppress independent verification. Expiry means a reservation is stale, not that an experiment failed. On a corrupt or unavailable channel, follow the pinned collective pause/closure policy; local offline CLI operations remain usable. The logical-round clock remains the initial same-host policy, not a claim about asynchronous multi-host availability.

### How integration preserves source and scientific meaning

The integration PM merges a frozen incoming native ARA using the existing source-aware planner and portable identity records. For first enrollment, verify the shared seed/base; for subsequent directory imports under the same source key, use the exact last-imported source revision as the base, not always the original fork point. Preserve every required predecessor snapshot. Local-Git imports instead require the enrolled source's verified ancestry. Repeated import must not duplicate native entries. Receipts bind the native identity mapping, actual destination revision, report, and owning audit session; the incoming contribution remains readable regardless of acceptance into current logic.

A clean structural merge is not scientific agreement. If a contributor changed a shared claim and the integration PM disagrees, record the competing interpretation and decision through the native audit contract. Mutable conflicts retain candidates; protected-history violations block mutation and use the existing separate repair path where permitted. Unresolved material stays visible in the community record and briefing. The contribution envelope never grants permission to override the merger.

Check the merge exit status, `unresolved_count`, and review flags; `committed: true` can still mean safe partial import with conflicts. A dry run is not an approval token for a later destination revision. The initial runner gives the integration PM exclusive control of the canonical workspace and rechecks its complete package identity before committing. If concurrent canonical advancement becomes a requirement, add an expected full-revision check under the existing CLI lock through a separately reviewed change.

Knowledge merge does not install arbitrary incoming code/evidence or combine executable candidates. Materialize a selected frozen package into an isolated execution workspace, with its original paths and modes, while retaining source-qualified evidence access for canonical readers. A synthesis contribution identifies exact parent packages, edits code in a fresh workspace, runs its own evaluator, and publishes new evidence. Its native trace records the results it builds on. Keep repository ancestry, scientific dependency, contribution parentage, and verification targets distinct.

### What verification must establish

Separate certified numerical rechecking, score reproduction, directional hypothesis testing, and argument review. A verification record says which it performs and identifies the exact target and conditions. Reuse Lara for numerical certificates, defeasible comparison bridges, and support/attack checking; a reproduction record separately binds the actual execution and raw outputs. Where the source Seal procedure withholds expected evidence, preserve that boundary in the runner. A held-out final evaluator is outside worker access and cannot select the next experiment. Register the independence policy without claiming that separate accounts imply independent priors.

Retain confirmed, partial, failed, interrupted, and unassessable outcomes with the actual observations. A superseding verdict references the prior one and does not erase it. The current verification view selects the latest verdict for a declared verifier/target/method scope and exposes conflicts across scopes. Claim promotion still follows the original closure and evidence procedure; reuse counts, endorsements, and verification totals are process measures.

Keep strict arithmetic findings separate from disputes about their empirical interpretation. The inspected S4 example checks the numerical ordering while defeating the broader improvement claim through an attack on comparison setup. S5 checks a lower-is-better metric and its empirical bridge. These are existing illustrative checker examples, not independent validation of real experiments or proof that every empirical claim is representable.

### What happens when a step fails

| Failure boundary | Required behavior |
|---|---|
| Worker crashes before freezing | Preserve local attempt inputs/logs under runner policy. Do not announce a reusable result. |
| Inputs change during capture | Reject the frozen package and require a new stable capture; preserve the failed publication request. |
| Publication commits but acknowledgment is lost | Recover the request's package and durable receipt before retry or execution. Do not create a second contribution. |
| Contribution is durable but channel announcement fails | Retain it as published but not yet acknowledged visible. Reconcile the channel from the authoritative record before acknowledging community publication. |
| Cache/index is deleted or corrupt | Rebuild from immutable envelopes, native snapshots, and versioned correction records. Never invent an empty frontier on failure. |
| Imported interpretation conflicts | Preserve incoming package and candidates; expose unresolved integration state without certifying a conclusion. |
| Numerical certificate or formalization rejects | Retain the raw research contribution, source bindings, and diagnostic. Do not mark the argument accepted or infer the experiment failed. |
| Composite map refuses or omits required coverage | Expose unavailable/incomplete map scope and member reasons. Never reuse a prior verdict as the current community result. |
| Code/evidence required for reuse is missing | Reject reuse/publication completeness. Do not fetch a mutable substitute or execute partial code. |
| Worker or coordinator exhausts budget | Apply the pinned accounting/closure policy and retain the interrupted record; no hidden extra reasoning allocation. |

Receipt, cache, and channel recovery must not depend on editing a frozen contribution. The coordinator's accepted-record boundary and rebuild procedure are part of the contract. Do not copy Agora's observed separation between immutable commits and unrecorded database corrections. Tests must prove that rebuilding yields the same visible contributions, corrections, and verdicts.

### Which changes belong in which repository

| Repository | Existing surface to reuse | Planned changes after review |
|---|---|---|
| `ara-cli` | `crates/ara-cli/src/agent.rs`, `write.rs`, `merge.rs`, `merge/git.rs`; `crates/ara-core/src/write/source.rs` and `merge/identity.rs`. | Add the proposed offline snapshot interface using complete capture, locking, private-path, and fingerprint rules. Add expected-revision integration only if exclusive canonical ownership is insufficient. No community/network commands. |
| `Agent-Native-Research-Artifact` | `skills/collective-research-cli/`, `collective-frontier-cli/`, `collective-intentions-cli/`, and `evaluation/agent-cli/collective-contract.json`. | Own portable contribution, source-binding, attribution, verification, visibility, and role contracts, including the Lara handoff. Keep interface-only variants and archived baselines unchanged. |
| `ara-eval` | Draft runner/community, manifest, policy, accounting, schema, and test modules from its existing plan. | Implement capture, publication, announcements, briefing, integration receipts, and recovery. Add an external Lara invocation/materialization adapter and source-bound verdict views; reuse the pinned channel. |
| `Lara` | `Strict/Ord.hs`, `Strict/RA.hs`, `Elaborate/Comparison.hs`, `.lara` checking, and `.laramap` composition. | Reuse shipped numerical and argument semantics. Clarify documentation about numerical empirical verification; review any future composition extension in Lara rather than changing its frozen rules in the runner. |
| Viewer/Obsidian | Current artifact rendering and read-only note views. | No UI implementation in this plan. A later view can consume the same snapshot-bound briefing; direct frontmatter edits must not bypass guarded knowledge writes. |

The module names in the `ara-eval` row are proposed by its plan, not implemented harness claims. Do not promote evaluation-owned infrastructure into a production collaboration service during this work. A later reusable runner package requires a separate owner and deployment decision.

## Alternatives considered

Multiple agents can write one checkout through the artifact lock, but that does not isolate experiment code or preserve separate interpretations. Private workspaces plus explicit publication are the recommended path; same-checkout writes remain a supported lower-level capability. Ordinary Git merges retain version history but cannot replace native ID reconciliation, immutable-history checks, or audited conflict resolution.

A centralized planner could assign every experiment, but this design lets participants choose work from shared evidence and advisory intentions. The coordinator serializes publication, not research choices. A full Agora-style hosted service would add deployment, admission, storage, and network failure contracts before the current same-host workflow is proven. Requiring every contribution to become canonical would hide alternatives and delay visibility until integration.

A new verification native node kind would expose verification directly in the core graph, but changes the protocol, parser, writers, and viewer. An external typed verification envelope pointing to existing native experiment/decision records supplies the initial relation without that cutover. A new schema is still a protocol proposal and must be versioned; it cannot silently alter native IDs or claim semantics.

A custom numerical or argument checker would duplicate Lara's certificate, metric-polarity, empirical-bridge, and attack rules. Reuse Lara through a pinned external process. Replacing all native ARA knowledge with `.lara` would lose research continuity and require formalizing every raw observation; attach arguments to selected claims instead. Automatically equating Lara `justified` with ARA `supported` would erase the difference between checked consequences of declared evidence and independently verified measurements.

## Tradeoffs

Private snapshots and explicit envelopes cost storage and authoring work. Git can deduplicate shared file contents, but large evidence belongs in pinned object storage and its retention cost must be reported. Publishing every attempt preserves useful failures while increasing read volume. Bounded briefings reduce that volume only if full exact-revision follow-up remains available.

An integration PM can become a synthesis bottleneck, although raw contributions stay visible without waiting for it. Start with one canonical synthesis authority to preserve current ARA semantics. Partitioning claim ownership or permitting concurrent canonical writers needs a later policy review. Shared visibility can also anchor everyone on the current leader; report concentration and neglected branches without assuming that more branching means better science.

The coordinator is a same-host failure point, and the collective pause policy can delay work. Its explicit failure behavior is preferable to claiming fresh peer visibility from unavailable data. Store publication history and correction events durably, and prove rebuildability. The existing logical-round clock provides reproducible experiments but may be unsuitable for a general asynchronous community.

Argument authoring and source-binding review add research cost. A valid certificate can accompany a misleading formalization or an inappropriate policy, so the runner must retain the audited mapping and policy identity. Lara's shared-contract requirement limits which contributions compose; show that coverage boundary explicitly. Preserve unsupported domains and raw findings without pretending that every contribution has an accepted formal argument.

## Migration

Existing ARAs continue to use their native files, IDs, and CLI commands without a community record. A collective run begins from an immutable seed, creates distinct fork/source identities, and pins its role and channel contract. Do not carry active intentions, private journals, credentials, or cache authority into a fresh run. Import actual historical contributions only with their original attribution and evidence; missing fields stay visibly unknown and cannot be invented from Git committer names.

This plan complements the approved CLI implementation and collective extension. Interface-only procedures stay unchanged. Adding Lara to the collective stack requires a reviewed update to `ara-eval`'s stack definition, prompts, dependency locks, coverage policy, and registration before collection; this plan does not silently amend that repository. Implementation and protocol approval are separate from permission to run already authorized pinned experiments. Functional CLI PRs follow patch-version, lockfile, changelog, native/wasm, and viewer rules; this docs-only revision requires no version bump.

### What should be implemented first

| Phase | Deliverable after review | Acceptance before proceeding |
|---|---|---|
| 1. Freeze the collective contract | Contribution/verification and ARA-to-Lara binding schemas, offline snapshot contract, role policy, publication boundary, and the selected Lara policy/vocabulary. | Cover negative results, missing evidence/arguments, competing interpretations, source advancement, and repetition. Review collective scope/roles and the revised evaluated stack separately from access-only skills. |
| 2. Capture and publish exact work | Offline snapshot export, immutable inventories, contribution records, and recoverable announcements; package selected arguments outside the native root. | Two processes publish complete snapshots. Exercise unsafe outputs, source mutation, pending transactions, missing objects, lost acknowledgment, restart, and argument/source hash mismatch. |
| 3. Expose the shared frontier | Actual CLI reads plus external Lara checks, source bindings, compatible composite maps, intentions, and integration receipts. | A peer sees a result before integration and retrieves its exact evidence. Show argument gaps, rejected certificates, contested same-setting claims, nonconflicting different settings, incompatible policies, and incomplete map coverage without changing native maturity. |
| 4. Integrate and verify | Audited native merges, source-qualified receipts, numerical checks, independent reproduction, and separately evaluated executable synthesis. | Preserve colliding identities/history and original Lara bindings; repeated import is unchanged. Protected edits reject. Correct arithmetic survives a comparability attack while the empirical bridge can be defeated; a synthesis is evaluated anew. |
| 5. Exercise the complete worker loop | The planned provider-backed runner consumes the reviewed collective/Lara contracts and records all authoring, checking, and integration costs. | Approved smoke follows intention, experiment, native record, selected argument, publication, composite view, peer reproduction, integration, and recovery. No paid call starts without approval. |
| 6. Evaluate the reviewed system | The reviewed `ara-eval` collective-stack study includes pinned Lara access; the interface study remains unchanged. | Freeze dependencies, formalization prompts, policies, coverage rules, tasks, budgets, and held-out evaluation before collection. Report whole-community results; no component-level causal claim. |

### Which checks prove engineering correctness

Permanent unit, integration, and functional tests must target lost contributions, bad identity binding, stale visibility, incorrect verdict precedence, malformed inventories, unsafe paths, and interrupted publication/recovery. Use the existing native merge regressions rather than retesting copied output fields. New protocol/runner tests must invoke the real CLI for knowledge reads and writes. Run the owning repository's tests after implementation, then exercise the complete workflow with independent processes and archived exact source/evidence readback.

The smoke fixture has one seed, two forks, colliding local node IDs, one disjoint experiment per fork, an incompatible shared-claim edit, one intentional verification, and a synthesis candidate. Observe both publications before integration; recover one lost acknowledgment; restart the coordinator; rebuild its derived view; import both forks; resolve only the permitted mutable conflict through a complete session; and evaluate the synthesis. Assert exact input/output identities and preserved histories, not merely that commands return success. Local CLI reads must still work when the collective channel is unavailable, while collective execution follows its pause policy.

Extend engineering coverage with real Lara checks of same-setting contrary claims, different settings, metric polarity, wrong cells/false comparisons, and relative-drop witness/threshold failures. Test stale argument-to-ARA bindings, altered policy/map bytes, incompatible contracts, refused maps, and stale `--out` results. Assert that argument acceptance never silently upgrades research maturity or substitutes for a reproduction receipt. The integrated smoke must read back exact argument inputs and both individual and composite outcomes through the frontier's declared scope; retain native merge regressions and reuse Lara's own backend conformance cases.

If a CLI addition is approved, run targeted consumer tests and actual binary smoke before the final locked workspace, format, all-target Clippy, and native/wasm checks. Core behavior changes require reviewing embedded-viewer regeneration even when the source freshness hash passes. If only protocol/runner code changes, do not bump the CLI or run unrelated Rust release work. After implementation, document the accepted design in the owning `docs/` and retire this plan according to repository convention.

### Which claims the evaluation can support

Use the current [ara-eval evaluation plan](../../ara-eval/plans/cli-interface-and-collective-research-evaluation.md), including its whole-stack comparison against the paper-based Agora reimplementation. Review Lara's inclusion in that stack before registration; preserve already archived study identities and do not change a scored run retrospectively. This document adds no experimental arms or numeric thresholds and claims no component-level causal effect. The separate interface study keeps Files and CLI procedures unchanged. Charge argument production, formalization review, checker invocation, and composite views to the collective budget.

The collective outcome is independently executed held-out quality at a fixed total community budget. Report time and cost for reasoning, authoring, publication, integration, verification, repair, and failed calls. Process measures include actual consumption of peer evidence, accidentally repeated experiment signatures excluding declared verification, hypothesis coverage, verification completeness, branch concentration, and unresolved conflicts. Equal scores do not establish duplicate experiments. Git reachability and account diversity do not establish reproducibility or scientific independence.

Agora's single community demonstrates sustained contribution and reuse, not improved discovery per unit compute. Its v4 discloses a view-plus-prompt intervention, mutable/incomplete indexing, and absent evaluator/environment pins. Preserve these limits in comparisons. Archive complete model/session identities, evaluator and dataset versions, executable inputs, correction records, and intervention logs; do not claim an exact reproduction of the unreleased Agora run.

## Next Steps

1. Review the boundary between immediate shared contributions and curated canonical `logic/`; the recommendation is many private contributors with one canonical integration PM.
2. Review the contribution, verification, and ARA-to-Lara binding contracts, reusing native identities, the intention coordinator, and Lara's existing numerical/argument semantics.
3. Approve the offline snapshot contract, collective protocol changes, Lara adapter and coverage policy, and `ara-eval` stack revision separately. Keep network, model calls, and research scheduling out of the CLI.
4. Implement and prove the two-process publication, visibility, merge, and verification workflow before provider-backed or scored research runs. Do not commit this draft or start implementation without the requested human approval.
