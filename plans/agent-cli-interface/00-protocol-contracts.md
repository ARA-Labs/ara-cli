# PR 00: approve the protocol contracts for CLI writes and merges
**Date:** 2026-10-01

Implementation record: [00-protocol-contracts](../../docs/agent-cli-interface/00-protocol-contracts.md). Remaining acceptance: F1–F7 upstream approval remains pending.

Observed proof: [delivery verification](../../docs/verification/agent-cli-2026-10-02/README.md).


Status: **approved** 2026-10-01. Target repository: `ARA-Labs/Agent-Native-Research-Artifact`. Parent: [agent CLI interface plan](../agent-cli-interface.md). Series: [PR index and shared gates](README.md). Dependencies: none; this proposal can proceed alongside [01-read-model.md](../../docs/agent-cli-interface/01-read-model.md) and [02-read-commands.md](../../docs/agent-cli-interface/02-read-commands.md).

## TL;DR

Publish one protocol proposal covering F1 through F7 before shipping writes or directory merges. Keep current local IDs and native references, preserve historical content, and document every permitted metadata change. Resolve the source skills' session and staleness requirements instead of treating the parent's short pointer whitelist as complete. Alias identity, conflict resolution, and changes to writer roles remain approval decisions.

## Problem

The parent recommends seven format decisions, but those recommendations are not approved contracts. Its F4 list permits `events_logged` appends while the current protocol checkout's `skills/research-manager/SKILL.md` also requires complete session histories, rolling metadata, reasoning records, taste comments, and stale flags. Research-manager acts as the live project manager (PM). The same skill declares trace and staging immutable except for forward pointers. CLI writes cannot satisfy both instructions until the protocol states the precise exceptions.

Merge identity also needs more than renumbering. A source entry imported without changing its ID must still be recognized on the next merge. Reused display labels must not cause unrelated forks to share an identity. Persistent unresolved conflicts must survive repeated merges without silently becoming settled work.

## Constraints

This PR is a protocol proposal and documentation change, not an implementation or approval of its recommendations. Existing files remain authoritative and existing IDs remain valid. No viewer changes, hosted services, new global ID scheme, automatic semantic deduplication, or changes to the Files baseline belong here. Every `ara` command must continue to run offline without an LLM.

The inspected current protocol sources are `skills/research-manager/SKILL.md`, its `references/event-taxonomy.md`, `skills/compiler/SKILL.md`, `skills/compiler/references/ara-schema.md`, and `skills/research-foresight/references/CONTRACT.md`. They establish present requirements, not the paper benchmark's historical skill revision. [PR 12](12-pin-skill-contracts.md) supplies that separate revision and operation inventory. Follow the shared review gates in the [series index](README.md); protocol repository contribution rules must be refreshed before implementation.

## Proposed approach

Add `docs/agent-cli-contracts.md` (new) in the protocol repository as the decision record. Update the inspected `skills/compiler/references/ara-schema.md` with approved additive fields. Add an approved mutability table to `skills/research-manager/SKILL.md` and its event taxonomy reference without changing the pinned baseline copies created by PR 12. Include examples under `examples/cli-contracts/` (new) only after the contracts are approved. A proposed `evaluation/agent-cli/protocol-decisions.json` (new) records each decision's status, approved revision, consumers, and unresolved choices; consumers must fail their integration review when the required decision is still a proposal.

| Decision | Proposed contract | Approval needed before consumption |
|---|---|---|
| F1 | Add append-only `trace/aliases.yaml` for source-qualified identity mappings, including identity mappings for entries whose local IDs did not change. Record a stable source identity, display label, source revision or content identity, destination identity, date, and merge identity. | Approve record shape, identity creation, label reuse rules, import ledger coverage, alias chaining and cycle rejection. A renumber-only log is insufficient for merge idempotence. |
| F2 | Forks allocate ordinary local IDs. Source-qualified references disambiguate imports at the CLI boundary; do not introduce `N124~bob` or rewrite every source skill's native references. | Approve how a stable fork identity is established without relying on `--as`, how unchanged shared ancestry is recognized, and how revision advancement is distinguished from an unrelated fork. |
| F3 | Preserve in-place promotion pointers. Unpromoted versus promoted converges to promoted only when the target and closure information agree. | Two promoted observations with different targets remain a conflict; `true` must not discard either target or evidence. Approve validation and resolution of target/closure disagreements. |
| F4 | Default trace/staging content to immutable; append new history records and permit only specified metadata transitions. Logic remains mutable with full before/after history. | Approve the detailed table below, deletion policy, merge-resolution exceptions, and consistency across skill/reference pages. |
| F5 | Add optional `same_as: [N131]` on a later node while retaining both nodes and their original provenance. | Approve direction, self-link and cycle behavior, reference validity and mutability exception. [07-same-as-links.md](../../docs/agent-cli-interface/07-same-as-links.md) consumes this decision independently. |
| F6 | Carry existing claim fields, complete opaque content, artifact pointers and concept links through the agent read model. | Approve the additive schemas from issues 61, 62 and 63, including concept identity and reference resolution. Viewer acceptance remains in those issues. |
| F7 | CLI-backed copies use `ara` for every knowledge-layer access operation; direct source and evidence body access stays allowed. | Approve the command coverage prerequisite and reviewed access-only diff. This does not approve checker enforcement or coordination changes. |

The following F4 details need individual reviewer decisions. Keep each unresolved item visible in the decision record rather than publishing a permissive wildcard.

| Object | Proposed allowed transition | Required preserved material |
|---|---|---|
| Observation promotion | Set `promoted`, `promoted_to`, `crystallized_via` together after the skill's closure signal. | Original content, context, provenance and bindings; conflicting promotions retain both proposals. |
| Trace relation append | Permit `add edge` to append `also_depends_on`, and F5 to append `same_as`, only with validated targets and approved cycle semantics. | Existing content and relations stay unchanged. F4 must name these exceptions explicitly; generic content edits remain forbidden. |
| Observation staleness | Permit `stale` metadata changes according to the pinned skill's session-day rule, or approve a derived stale view and explicitly amend the skill. | No automatic deletion, promotion, or content rewrite. The recommendation is explicit metadata permission, pending review. |
| Active session | Append `events_logged`, `ai_actions`, `claims_touched`, `logic_revisions`, `key_context`, `open_threads`, and `ai_suggestions_pending` using their defined shapes. | Earlier records remain unchanged, including verbatim `before`/`after`, signals and provenance. Decide append versus replacement for any field described as a rolling list. |
| Session summaries/index | Update `last_turn`, `turn_count`, `summary`, and corresponding session-index counts and references. | Stable session identity/date/start; previous history remains recoverable. Approve same-date fork session identity and metadata reconciliation rules. |
| Reasoning/taste | Append reasoning entries and taste records; approved logic taste fields follow the source skill. | Near-miss signals, target refs, tags and verbatim researcher comments. |
| Immutable merge conflict | Record both competing values and their provenance in persistent conflict state; resolve through an explicit audited resolution operation. | Original competing historical records, a resolution event and the chosen view. Generic `ara edit` must not bypass immutability. Approve the storage and resolution schema. |

For F6, [issue 61](https://github.com/ARA-Labs/ara-cli/issues/61) asks for claim falsification plus proof/dependency rendering. Read existing `Provenance`, `Falsification criteria` or the documented `Falsification` spelling, `Tags`, `Conditions`, `Sources`, and additional claim prose without dropping unknown fields. [Issue 62](https://github.com/ARA-Labs/ara-cli/issues/62) proposes node `artifacts` with `name`, `pointer`, `what`. [Issue 63](https://github.com/ARA-Labs/ara-cli/issues/63) proposes node `concepts` referring to existing concept identities. Do not mint numeric IDs for concepts or related work merely to simplify the CLI. Define path/section addressing for arbitrary logic bodies, root `PAPER.md`, and any compiler-generated knowledge document required by PR 12; approve scope extensions before PR 06 implements them.

Review source-contract contradictions before publishing the write whitelist, including promotion instructions that request `From staging` and `Crystallized via` in logic while the later snapshot contract places them in trace history. Review source dialects for `parent` pointers, pivot `from`/`to`/`trigger` fields and all type-specific payloads required by the compiler and PM. Unknown or unsupported incoming values must retain their complete source text in conflict evidence. Identical concurrent logic changes should converge without conflict. These are proposed decisions requiring explicit disposition, not permission to rewrite pinned source instructions.

Implementation steps for the future PR:

1. Refresh the current protocol and issue sources and compare each recommendation with the pinned skill contracts from PR 12 when available.
2. Publish F1 through F7 with separately recorded approval status and examples of accepted and rejected transitions.
3. Define the merge identity and conflict contracts consumed by [08-directory-merge.md](08-directory-merge.md): all imported identities, revisions, reused labels, persistent unresolved conflicts, identical concurrent changes, deletions, and session reconciliation.
4. Review immutable-edit resolution and simultaneous promotions with differing targets. Specify atomic failure semantics: a rejected transition leaves artifact files and import/conflict records unchanged.
5. Amend current protocol reference pages only for approved choices. Link the unchanged baseline inventory and the CLI variant coverage gates.
6. Record the approval revision and notify dependent plans. F1 through F4 unblock [03-guarded-node-writes.md](../../docs/agent-cli-interface/03-guarded-node-writes.md), [05-staging-and-sessions.md](../../docs/agent-cli-interface/05-staging-and-sessions.md), and PR 08; F5 unblocks PR 07; relevant F6/F7 decisions unblock PR 13.

## Alternatives considered

Globally unique fork IDs would reduce numeric collisions, but change the native reference grammar and every reader. A separate append-only promotion event would avoid in-place promotion changes, but changes the pinned writer workflow and still requires a target-disagreement rule. Accepting the parent's short F4 whitelist without comparison would leave necessary PM operations unsupported.

A private cache for import identity is removable and cannot establish repeatable merge behavior after checkout or transfer. An append-only import ledger is the recommended durable alternative; its exact placement within the alias format remains a review choice.

## Tradeoffs

Explicit exceptions let the CLI reject writes without interpreting the skill's intent. Additive metadata helps older readers continue to work, while complete writers must guard fields they do not model. Preserving unresolved historical conflicts costs space and requires a reviewed resolution path; overwriting them would weaken the audit trail.

Q5 is decided in the parent: CLI-only copies retain the single-writer role, so F7 changes access only. [14-shared-frontier-intentions.md](14-shared-frontier-intentions.md) proposes any collective role changes separately.

## Migration

No existing artifact needs a format rewrite. New alias/import and conflict files are created only when a merge needs them. Missing additive fields remain absent. Existing source skills are kept as baselines, and approved protocol clarifications are revisioned so the experiment can distinguish a changed contract from a changed interface.

Do not apply a newly approved metadata exception retroactively to rewrite old history. Old artifacts with unsupported historical edits surface review conflicts rather than being normalized silently.

## Verification and acceptance

This drafting task runs no code or checks. The future proposal is accepted when each F decision has a reviewer-recorded disposition, the mutability tables agree across schema and skills, and every requirement from PR 12 either has an approved operation or a named blocked decision. Examples must cover unchanged-ID imports, alias cycles, label reuse, repeat conflicting merges, stale metadata, complete session history and divergent promotion targets.

No new binary is shipped by this protocol PR. Once dependent commands exist, use disposable fixtures with the actual binary: promote an observation, inspect it and its complete session with `ara show --full --json`, and run `ara check <fixture>`. The allowed update must retain original content and all revision fields; an attempted immutable content edit must reject without changing bytes. These are future smoke scenarios using proposed commands, not results observed while drafting. CLI integration remains blocked until those dependent PRs prove them.

## Next Steps

1. Review F1 through F7 and the additional identity, mutability and conflict decisions before implementation.
2. Start PR 12's baseline inventory in parallel, then reconcile any missing operations with the approved contract.
3. Publish the approved protocol revision to the numbered write, merge and skill plans; keep unresolved decisions marked blocked.
