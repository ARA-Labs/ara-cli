# PR 14: define shared frontier and intention protocols across forks
**Date:** 2026-10-01

Status: **approved** 2026-10-01 (frontier and intentions made separately installable at approval, to match the four-arm collective study). Target repository: `ARA-Labs/Agent-Native-Research-Artifact`. Parent: [agent CLI interface plan](../agent-cli-interface.md). Series: [PR index and shared gates](README.md). Dependencies: [08-directory-merge.md](08-directory-merge.md) and [13-cli-backed-skills.md](13-cli-backed-skills.md). Shared-channel execution is implemented by the external [15-experiment-harness.md](15-experiment-harness.md) consumer, not by `ara`.

## TL;DR

Add frontier and shared-intention instructions as a separately versioned collective extension to the CLI copies. The two are independent components: an agent can receive frontier views alone, intentions alone, or both, so PR 15's E6 can measure each against the CLI arm and the Files arm, as the Agora comparison note proposes. Publish intentions through one shared channel outside all private ARA forks and refresh it before choosing work. Keep `ara` offline; an external runner handles publication, synchronization and budgeting. Changes to writer authority require explicit approval and must never enter the interface-only CLI condition.

## Problem

A private intention file in each fork does not inform other agents until after they may have repeated the work. The parent requires publication and refresh across forks but does not name a transport or staleness rule. A usable protocol must identify one shared source, say when agents read it, and record when unavailable or outdated intentions influenced a choice.

Current `skills/research-foresight/references/CONTRACT.md` is read-only and research-manager, the live project manager (PM), declares a single writer. Allowing workers to edit knowledge layers therefore changes the research roles as well as access. Local `ara open` output describes only the selected artifact revision; it cannot claim to include unseen work from other forks.

## Constraints

This PR defines protocol and skill instructions; it adds no network call, LLM call or distributed service to an `ara` command. Source/evidence editing and full history requirements remain unchanged. No new public frontier command is assumed: build a frontier view from the reviewed `open`, `status`, `ls`, `show`, `refs` and merge reports. Follow the [shared gates](README.md), including draft review and explicit non-goals.

The proposed initial transport supports community processes on one host with a shared writable runner directory. This is a real shared channel across independent fork workspaces, not private replicated files. Multi-host deployment is out of scope until a separate transport review; do not label the one-host design a distributed deployment. The same compute budget covers all coordination reasoning and transport work, with no hidden additional agent or merge advisor.

## Proposed approach

Create `skills/collective-research-cli/` and its `references/frontier.md`, `intentions.md`, `roles.md`, and `failure-policy.md` (all new) in the protocol repository. This extension composes with pinned CLI skills from PR 13 and leaves their trees unchanged. `frontier.md` and `intentions.md` install independently; `roles.md` and `failure-policy.md` are shared by both and are part of every collective arm, including the plain C+S collective control, so role changes do not differ between arms. The contract records which components each arm loads. Add `evaluation/agent-cli/collective-contract.json` and `community-smoke-scenarios/` (new). The contract records approved roles, intention schema, transport requirements, logical staleness parameters and the exact extra prompts supplied to agents. PR 15 implements the runner against this contract.

The proposed shared channel is `<run-root>/shared/community/<community-id>/intentions/`, outside every `<run-root>/forks/<agent-id>/ara/`. All agent runners use that exact common path. Only the external coordinator writes `events.jsonl` and atomically replaces `snapshot.json` (new channel files). Agents submit publication requests to their runner, which serializes them through the coordinator before acknowledging success. The coordinator assigns an increasing sequence, validates actor ownership and expected previous intention revision, appends an event durably, then replaces the snapshot. Acknowledgment includes the durable sequence. If acknowledgment is lost, the actor treats publication as uncertain and must recover its request identity before execution. The append log reconstructs the snapshot after interruption, and duplicate request identities return the original acknowledgment without appending a second event.

| Intention field | Proposed meaning |
|---|---|
| `community_id`, `actor_id`, `intention_id`, `revision` | Stable run-scoped identity and increasing revision; no ARA node ID is invented. |
| `source_identity`, `artifact_revision`, `native_refs` | Fork identity and exact knowledge revision used to select the work; native refs stay source-qualified where needed. |
| `action`, `question`, `experiment_signature`, `verification_of` | Planned research action, motivation and reproducible configuration/source digest used to identify accidental duplication; deliberate verification is explicit. |
| `state` | planned, active, completed, abandoned or expired; each change is a new event preserving earlier states. |
| `published_sequence`, `refresh_round`, `expires_after_round` | Coordinator sequence and logical scheduling rounds; no unreliable local wall clock decides freshness. |
| `budget_reserved`, `budget_used`, `result_refs` | Remaining allocation and outputs for completed work; reservations are accounting, not extra compute. |

Use scheduling rounds as the initial freshness clock: a round ends when every admitted actor completes its allotted action/turn or the registered failure policy closes it. The reviewed run configuration supplies a refresh cadence and an expiry interval in whole rounds before data collection. These are design parameters requiring approval, not empirical thresholds. An active intention expires when the coordinator round exceeds `expires_after_round` without a refresh. Retain expired records in history; agents see them as stale context, not active reservations. An unavailable actor cannot keep work reserved forever. The coordinator is the only authority for round and sequence advancement.

The refresh protocol is mandatory: the runner reads the latest committed snapshot before each work-selection boundary, after every action result, and before publication or budget-consuming execution. It supplies the snapshot's sequence and round to the agent and records them with the choice. The agent publishes or refreshes its intended action and receives acknowledgment before starting execution. The coordinator checks that the supplied observed sequence still matches current state; a changed snapshot requires refresh and reconsideration, not an invisible overwrite. A publisher owns its intention revisions; another actor may flag a likely overlap but cannot complete or abandon the owner's work.

A shared intention is advisory research context, not proof that a question is answered or an experiment succeeded. Intentional replication remains allowed and must carry `verification_of` or a recorded rationale. Conflicting fresh intentions return both owners and signatures for explicit judgment. On an unavailable or corrupt shared channel, the default collective protocol pauses budget-consuming work until refresh succeeds or the pre-registered failure policy closes the run; it never silently behaves as if no other work exists. Record failed refresh time and interrupted runs. Offline `ara` reads, writes, validation and merges remain usable independently of channel availability.

The frontier view contains unresolved questions, unpromoted/stale observations, pending bindings, unfinished claims and merge conflicts with native refs, artifact revision and intention snapshot sequence. It separates current-local facts, imported facts and active/stale remote intentions. It does not auto-promote, choose conclusions or suppress uncertainty. Agents may reason about priority under the shared budget; all resulting rationale and decisions use the normal trace/history contract.

Implementation steps for the future PR:

1. Review the channel path, durability, sequencing, logical freshness and failure policy as a protocol contract. Record the parameters PR 15 must pin.
2. Review writer authority explicitly. Proposed collective roles allow designated contributor writers to edit their own fork's logic through CLI transactions; readers remain read-only. Each fork retains one authorized PM for continuity/history. A canonical integration PM owns merges and conflict resolution. This changes current global single-writer authority only if approved; shared transport does not grant it automatically.
3. Define how contributor proposals reach the integration PM and how revisions, provenance and complete history survive PR 08 merges. Leave unresolved conflicts visible and never resolve them by role privilege alone.
4. Write the frontier and intention instructions as an extension, with every additional prompt clause recorded for experiment review.
5. Specify duplicate-intention, expired-owner, concurrent-publication, offline-channel and merge-conflict scenarios for PR 15's external runner.
6. Review the E6 arm separation (F, C+S, C+S+frontier, C+S+intentions) with PR 15 and publish a contract revision. Mark runtime integration pending until the shared channel is actually exercised across independent forks.

## Alternatives considered

Private fork files fail the publication requirement because other actors cannot refresh them before acting. A shared git branch could support multi-host publication, but would require a transport policy for fetch/push races and staleness. The initial shared runner directory gives all fork runners a single visible source without putting network behavior inside the binary.

Hard exclusive experiment locks would prevent some duplication but could suppress useful verification. Advisory intentions with explicit freshness preserve researcher judgment; the coordinator serializes publication, not scientific decisions.

## Tradeoffs

Coordination can consume tokens and delay work, so its overhead belongs in the same measured budget as research. The external coordinator is a failure point and performs no unbudgeted reasoning. Logical rounds make freshness reproducible; a live asynchronous community may need a separately reviewed clock policy.

Allowing contributor writers broadens the single-writer rule and can generate more merge conflicts. The canonical integration PM remains responsible for adjudication, and the intervention is measured separately from CLI access.

## Migration

Keep Files and CLI conditions unchanged. Install this extension only for the collective condition and record its revision alongside the PR 13 skill pins. Channel files are run metadata outside the artifact, while research decisions and results retain their normal artifact provenance. A new community begins with an empty shared log and new run identities; old active intentions are never copied into a new run.

Protocol revisions create new run configurations. Do not change expiry, role policy or failure handling within a scored run. Existing forks without the extension remain valid ARAs and can use the offline CLI normally.

## Verification and acceptance

This drafting task performs no runtime checks. Future proposed protocol-consumer scenarios exercise concurrent publication, stale expected revision, expired owners, interrupted append/snapshot replacement, duplicate request acknowledgments, unavailable transport, deliberate verification and budget exhaustion. PR 15 implements and runs these scenarios rather than creating mocks that merely echo the requested intention.

The actual-surface smoke uses two independent fork processes and one shared directory: actor A publishes a planned experiment, actor B refreshes and sees A's acknowledged sequence before choosing work, A refreshes it, and an unrefreshed intention later expires by the configured logical-round rule. Run actual `ara open --json` and `ara show --full --json` in both forks, and merge their completed knowledge with the PR 08 binary. Observe source-qualified frontier context, retained histories, visible conflicts and channel records showing every publish/refresh. Disable external synchronization and verify `ara` commands still work offline while collective execution follows the registered pause/failure policy. These are required future smoke results, not claimed observations.

Acceptance requires an approved shared-channel schema and real publication/refresh proof across forks, explicit staleness and outage rules, budget accounting, and reviewed writer-role changes. A private intention file or an unimplemented transport does not satisfy this plan. The extension cannot appear in the interface-only CLI prompt bundle.

## Next Steps

1. Approve shared-directory transport, logical freshness, contributor authority and outage handling.
2. Freeze the collective contract and implement its external runner in PR 15.
3. Exercise cross-fork publication and merge before collecting collective-research measurements.
