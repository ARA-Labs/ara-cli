# 02: Contributions, publication, frontier, and integration
**Date:** 2026-10-03 (content from the 2026-10-02 draft)

Status: **approved** by the human developer on 2026-10-03 after design review. Implementation pending. This plan remains staged in `ara-cli` until the upstream route is decided (README decision D1). Target repositories: `Agent-Native-Research-Artifact` for portable contracts (envelope, verification, roles, visibility), and `ara-eval` for the runner (capture, publication, briefing, receipts, recovery). Parent: [collaborative research plan series](README.md). CLI dependencies are [01: snapshot](01-ara-snapshot.md) and [04: peer-feedback merges](04-peer-feedback-merge.md). Approval covers this design; upstream contract adoption, paid runs, commits, and releases remain separate actions.

## TL;DR

A finished piece of work becomes a *contribution*: a frozen native snapshot from `ara snapshot`, the declared code and evidence, and an external envelope. The runner's coordinator publishes contributions one at a time. It announces them on the existing plan-14 channel and builds a bounded briefing from CLI reads over the exact snapshots. The integration PM later imports contributions with `ara merge --source-key`, and the runner records an integration receipt. Publication, verification, and integration are separate recorded events; none of them rewrites a published package.

## Background: what exists

- **Reads.** `status`, `ls`, `show`, `path`, `refs`, `open`, and `find` inspect one local artifact ([guide](../../docs/agent-cli.md)).
- **Writes.** `apply` and the convenience commands allocate IDs, validate deltas, keep audit history, and commit through recoverable transactions.
- **Merges.** Directory and local-Git `merge` preserve sources, relocate colliding identities, and keep conflicts along a source's linear history. Repeated imports are tracked per `--source-key`. Peer-feedback histories currently fail in the case recorded in [04](04-peer-feedback-merge.md#problem); that fix is required before enabling native peer imports in the runner.
- **Shared channel.** The plan-14 collective extension has a same-host channel with sequencing, expected-revision checks, acknowledgments, logical-round expiry, and recovery.
- **Runner.** `ara-eval` has pinned dependencies and a draft runner/community design. Its README claims no provider-backed harness yet.

## What identifies a contribution

A package binds:
- a community/run identity;
- a publisher identity;
- an idempotent publication request identity;
- a stable fork/source identity;
- an exact native snapshot;
- explicit parent contribution versions.

Imports use the merger's stable source-key contract. Fork identity never comes from `--as`, a moving branch, a directory name, or a local node number.

Three identities serve different purposes:
- The **native revision** is the `ara.artifact/v1` fingerprint that `ara snapshot` records. It covers nonprivate source and evidence bodies and portable provenance, but not modes.
- The **payload digest** binds the complete frozen inventory: native snapshot and manifest, modes, execution metadata, external inputs, and any initial argument/check attachments. It identifies bytes and execution permissions, not a publisher or a scientific claim.
- The **contribution ID** binds the payload digest and the immutable envelope, including community, publisher, request, source identity, lineage, and native pointers. Parent edges and verification targets use contribution IDs, never a payload digest alone.

A `show --source` digest isn't an artifact revision, and neither is a knowledge-only inventory. Identical payloads may have different contribution IDs when their attribution or lineage differs.

| Envelope field group | Meaning |
|---|---|
| Identity | Schema revision, community/run, publisher, request, source identity, payload digest, contribution ID. |
| Lineage | Starting native revision, parent contribution IDs, source-qualified native references the work uses. |
| Research payload | Pointers to the native experiment, question, observation, declared change, pre-execution prediction, measured outcome, and follow-up. Pointers only, never copied bodies. |
| Package inventory | Sorted relative paths, file digests and modes, external object digests, entry point, configuration, environment identity. |
| Verification | Target contribution/version, question or claim tested, method, evaluator/configuration, reproduced observations, scoped verdict. |
| Argument check | Lara coverage and verdicts; see [03](03-lara-integration.md). Kept separate from reproduction. |
| Publication receipt | Coordinator sequence, acknowledged request and contribution ID, visibility snapshot identity. Stored outside the immutable contribution. |

Illustrative envelope (digests abbreviated; the versioned schema is frozen in phase 1):

```json
{
  "schema": "ara.contribution/v1",
  "community": "run-2026-10-a",
  "publisher": "worker-a",
  "request": "req-a-0007",
  "source_key": "fork-a",
  "native": {"snapshot": "snapshot.json", "fingerprint": "3b9c…e41a"},
  "payload_digest": "sha256:9d10…",
  "contribution_id": "sha256:80b2…",
  "lineage": {
    "start_fingerprint": "0e77…",
    "parents": ["sha256:51aa…"],
    "uses": [{"source_key": "fork-a", "native_revision": "0e77…", "selector": "N04"}]
  },
  "payload": {
    "experiment": "N12",
    "question": "N04",
    "change": {"selector": "N12", "field": "description"},
    "prediction": {"native_revision": "0e77…", "selector": "O03"},
    "outcome": {"selector": "N12", "field": "result"}
  },
  "execution": {"intention_receipt": "sha256:24ad…", "input_inventory": "sha256:16fe…"},
  "inventory": {"manifest": "payload-inventory.json", "entry": "ara/src/train.py", "config": "ara/src/conf/ablation.yaml", "environment": "sha256:…", "external": []}
}
```

Rules:
- The protocol schema defines canonical JSON using RFC 8785. Inventory entries are sorted by root and relative path; parent IDs are sorted and unique. Duplicate paths or ambiguous roots reject. Schema revisions fix all remaining array ordering and digest input fields.
- Compute `payload_digest = SHA-256("ara.payload/v1\0" || canonical_inventory)`. The inventory binds every required file's bytes by SHA-256, size, and mode, plus every external object's digest and pinned retrieval descriptor. It includes the native snapshot manifest and fingerprint. The inventory excludes itself and the envelope; the coordinator verifies every inventory entry before accepting it.
- Compute `contribution_id = SHA-256("ara.contribution/v1\0" || canonical_envelope_without_contribution_id)`. Include `payload_digest` and all other immutable envelope fields. Store digests as `sha256:<lowercase hex>`. Neither hash proves authorship; runner actor permissions bind publisher identity.
- The envelope sits outside the payload inventory. Publication receipts and later attachments sit outside the immutable contribution. No field or included file may depend on the contribution ID being computed.
- An accepted `(community, publisher, request)` is bound to one contribution ID. An exact retry returns its original receipt; different metadata or payload under the same request rejects. Changing attribution or lineage changes the contribution ID even if the payload is unchanged.
- Paths stay within declared roots. Publication rejects unsafe traversal, unsupported links, missing objects, and credentials.
- Large inputs can live outside Git with a digest and a resolvable pinned location. A required object that can't be fetched blocks publication.
- Corrections, later checks, audit attestations, and superseding verdicts are new immutable records targeting contribution IDs. They use the same canonical encoding, inventory verification, request idempotency, and publication/recovery mechanism, with a schema-specific hash domain. They never rewrite an earlier envelope.

## How publication works

A result moves through `draft -> frozen -> published`. Canonical integration is a separate state: `pending`, `imported-with-conflicts`, `integrated`, or `rejected-for-integration`. These are external workflow states, not native claim statuses, and each change is recorded without rewriting the package. Publication means the package is durably available and passed the pinned structural policy. It is not scientific certification.

Publication steps:

1. Quiesce the worker and its child processes before capturing any native or external input. Keep them stopped until the complete payload is frozen. The CLI lock coordinates CLI writes only; the runner enforces this wider boundary with workspace/tool access.
2. Run `ara snapshot` ([01](01-ara-snapshot.md)) and verify its manifest. Don't re-implement capture.
3. Freeze declared inputs outside the native root, inventory referenced code and evidence without running them, and verify their pinned digests. Required inputs must belong to the recorded execution, not a later mutable workspace.
4. Check diagnostics against publication policy. Resolve research pointers at their declared revisions and bind the execution record to the acknowledged pre-execution intention and input inventory. A prediction first recorded after execution cannot be labeled a pre-execution prediction.
5. Compute and verify the payload digest and contribution ID. The coordinator durably records the immutable contribution and request binding, assigns a publication sequence, and reconciles the channel announcement. Only then does it return a visibility acknowledgment.

Private `.ara/transactions` and lock files never enter a payload; portable identity and conflict files do. The announcement carries the contribution ID and sequence. It's a view of the durable record, not a second source of facts. Publication and native knowledge writes are separate transactions.

Git can store frozen packages and small append-only envelopes in the community repository. Workers don't push to one shared mutable knowledge tree. The record keeps publisher attribution independent of the Git committer. Transport and Git publication are runner operations, not new meanings for `ara merge --git`.

The plan-14 intention schema needs a reviewed revision before it can carry this:
- Its fields are closed.
- Completion keeps the pre-work artifact revision.
- It verifies other intentions, not immutable packages.

The revision adds result contribution IDs and contribution-target verification, reusing the existing compare-and-swap checks, request digest, journal, and recovery code. It also binds a prediction's native revision and selector before execution, with the acknowledged input signature. Completion retains that pre-work binding and adds the result contribution ID separately. An intention-completion receipt is not a publication certificate.

## How peers find work before a merge

The community snapshot lists published contribution identities and visibility sequences next to the intention snapshot. The runner builds the briefing from CLI reads over exact snapshots plus validated envelopes. It doesn't parse YAML or Markdown itself.

The briefing separates five kinds of material:
- canonical facts;
- the worker's own findings;
- published but unintegrated peer findings;
- active intentions;
- stale intentions.

Cross-artifact reading and native peer imports belong to the approved collective scope; the interface-only research-foresight skill stays limited to its own ARA. The runner enforces access through exact published snapshots, not unrestricted sibling-directory access. Native imports are enabled only after [04](04-peer-feedback-merge.md) passes its acceptance checks.

| Briefing section | Based on | Lets a worker |
|---|---|---|
| Open questions and hypotheses | Native status, dependencies, staging state. | Pick a question with context. |
| Recent failed attempts | Published dead-end and experiment records with configuration and evidence. | Avoid accidental repetition, or test a different condition. |
| Verification gaps and contested outcomes | Reproduction records, preserved observations, Lara gaps and attacks with their scope. | Reproduce, challenge comparability, add evidence, or test an explanation. |
| Neglected alternatives | Branches with no follow-on attempts, linked to their research parents. | Explore without a popularity score. |
| Current investigations | Acknowledged intentions with owner, signature, sequence, expiry. | Coordinate overlap, or declare deliberate replication. |
| Integration gaps | Contributions awaiting import, known conflicts, receipts. | Help with synthesis or conflict resolution. |

Rules for the briefing:
- A documented selection and continuation policy bounds it. It reports coverage, truncation, and the sequence used.
- A missing or unreadable snapshot makes the briefing visibly incomplete; it never shows a silent empty frontier.
- References let workers fetch full native bodies and evidence from the same revision.
- Not seeing a contribution isn't proof nobody tried it.
- Follow-up suggestions use the proposed change, the affected question, and the experiment signature, not just the top metric.
- Refinement, exploration, and verification stay distinct choices. No evidence score changes claim status.
- Branches are grouped by explicit lineage and tags, not embeddings. The grouping policy is versioned and can't be retuned after outcomes are seen.

## How intentions avoid accidental duplication

Reuse the plan-14 channel outside the private forks. An intention names:
- the starting snapshot;
- the question;
- the proposed action;
- a code/configuration signature;
- for verification, the target.

Workers refresh before choosing work, after results, and before budget-consuming execution or publication.

Conflicting intentions show both actors and signatures. They don't grant ownership or block independent verification. Expiry means a reservation is stale, not that an experiment failed. If the channel is corrupt or unavailable, follow the pinned pause/closure policy; local CLI operations keep working. The logical-round clock is a same-host policy, not a multi-host guarantee.

## How integration preserves source and meaning

The integration PM imports a frozen snapshot with the source-aware merger, including the peer-feedback support in [04](04-peer-feedback-merge.md):

- **First import** from a seed-derived source verifies the shared seed as the base. A source forked from a later canonical or peer snapshot declares and preserves that exact starting snapshot; the runner verifies its parent contribution and native identity history instead of substituting the original seed.
- **Later directory imports** under the same `--source-key` use the last imported source revision as the base, so every predecessor snapshot must be kept. Local-Git imports require the enrolled source's verified ancestry.
- **Repeated import** of the latest source revision must not duplicate entries. For an older contribution already imported into this destination, the runner returns the retained receipt instead of asking the merger to regress the source. An unimported older revision follows the explicit source-order policy and must not silently replace the latest revision.
- **Receipts** bind contribution ID, source key, source/base revisions, identity mapping, destination revisions before and after integration, merge report, and owning audit session. Later resolution receipts append to the import receipt. The contribution stays readable whether or not it was accepted.

Workers may import exact peer or canonical knowledge into their own forks and publish again under the same stable fork identity. The runner retains every merge input and receipt. [04](04-peer-feedback-merge.md) must reconcile the same original entry arriving through different routes without duplicating it or dropping history. Package-only reading remains available, but is not a substitute for this required feedback loop.

A clean structural merge isn't scientific agreement. An incoming clean edit can be applied without producing a conflict. If the integration PM disagrees, it records an explicit audited logic revision and the competing interpretation; it must not assume every disagreement has a `merge resolve` choice. Mutable conflicts keep their candidates. Protected-history violations block mutation and use the separate repair path. The envelope never overrides the merger.

The runner checks the merge exit status, `unresolved_count`, and review flags: `committed: true` can mean a partial import. A dry run isn't approval for a later destination revision. The integration PM has exclusive control of the canonical workspace across review and commit, and the runner rechecks contribution identity before committing. An expected-revision CLI flag remains outside this series under README decision D5.

### How code and evidence conflicts are closed

The current merger reports changed or new `src/` and `evidence/` files as `external_read_only` conflicts and permits only `ours`. This is an intentional file-placement boundary, not scientific disagreement. An observed 0.1.23 import containing only a new `src/worker.py` returned exit 1, `committed: true`, and one unresolved conflict without installing that file; replay retained the conflict.

The integration PM acknowledges each such local conflict through `merge resolve --take ours` with the required owning session, turn, signal, and provenance. Its audit reason and external receipt state that the incoming bytes remain in the named contribution. This does not approve the experiment, discard its evidence, or copy the file into canonical. An inherited `imported_unresolved` conflict has no local resolution choice: its owning fork must resolve it and publish a later revision, which the PM then imports.

For every retained external reference, the runner records `(contribution_id, source_key, native_revision, path, digest)` in the integration receipt. Reads use that frozen contribution's original path and verify its digest. A merge report's identity mapping does not prove the file exists in canonical or has a native alias there. The runner must not redirect an absent external alias to an unrelated local file. Retained predecessor packages and source-qualified receipt chains keep these references readable after further imports or coordinator rebuilds.

Mark a contribution `integrated` only after native unresolved conflicts are closed, required review decisions are recorded, and all external references resolve to retained packages. Track external-file acknowledgments separately from scientific disagreements in process measures. Missing evidence or inherited unresolved conflicts keep integration incomplete.

Knowledge merge doesn't install incoming code or combine executables. To build on a contribution, the runner materializes its payload into an isolated workspace with original paths and modes. A synthesis contribution names parent contribution IDs, edits code in a fresh workspace, runs its own evaluator, and publishes new evidence. Repository ancestry, scientific dependency, contribution parentage, and verification targets stay distinct.

## What verification must establish

A verification record says which kind of check it performs and names its exact target and conditions. The kinds are:
- certified numerical rechecking ([03](03-lara-integration.md));
- score reproduction;
- directional hypothesis testing;
- argument review.

A reproduction record binds the actual execution and raw outputs.

Further rules:
- Where the source Seal procedure withholds expected evidence, the runner keeps that boundary.
- A held-out final evaluator stays outside worker access.
- Register the independence policy; separate accounts don't imply independent priors.
- Keep confirmed, partial, failed, interrupted, and unassessable outcomes with their observations.
- A superseding verdict references the prior one and doesn't erase it.
- The current view selects the latest verdict per verifier, exact target contribution, method/configuration, and scope using coordinator sequence rather than wall time. Different targets or scopes coexist. Lara map scope and later attachment identities follow [03](03-lara-integration.md); a smaller map cannot silently replace a verdict about a larger population.
- Claim promotion still follows the native closure procedure. Reuse counts, endorsements, and verification totals are process measures.

## What happens when a step fails

| Failure | Required behavior |
|---|---|
| Worker crashes before freezing | Keep attempt inputs and logs under runner policy; announce nothing. |
| Inputs change during capture | If a snapshot recheck or runner inventory detects a change, reject the package and retain the failed request. A successful recheck alone does not prove quiescence; lost isolation also blocks publication. |
| Publication commits but the acknowledgment is lost | Recover the request's contribution and receipt before retrying; never create a second contribution. Reusing the request with different content rejects. |
| Durable contribution, but the announcement fails | Mark it published but not yet visible; reconcile the channel from the record before acknowledging. |
| Cache or index deleted or corrupt | Rebuild from immutable envelopes, snapshots, and correction records; never show an empty frontier on failure. |
| Imported interpretation conflicts | Keep the incoming package and candidates; expose unresolved integration state. |
| Code or evidence needed for reuse is missing | Reject reuse or publication; never fetch a mutable substitute or run partial code. |
| Budget exhausted | Apply the pinned accounting/closure policy; keep the interrupted record; grant no hidden extra allocation. |

Receipt, cache, and channel recovery never edits a frozen contribution. The coordinator's accepted-record boundary and rebuild procedure are part of the contract; tests must show that a rebuild yields the same contributions, corrections, and verdicts. Agora's observed split between immutable commits and unrecorded database corrections is not copied.

## Repository ownership

| Repository | Reuses | Approved additions |
|---|---|---|
| `ara-cli` | `agent.rs`, `write.rs`, `merge.rs`, `merge/git.rs`; core `write/source.rs`, `merge/identity.rs`. | `ara snapshot` ([01](01-ara-snapshot.md)) and peer-feedback identity/provenance integration ([04](04-peer-feedback-merge.md)). No community or network commands. |
| `Agent-Native-Research-Artifact` | `skills/collective-research-cli/`, `collective-frontier-cli/`, `collective-intentions-cli/`, `evaluation/agent-cli/collective-contract.json`. | Contribution, verification, attribution, visibility, and role contracts; the intention schema revision; the Lara handoff contract. Interface-only skills and archived baselines unchanged. |
| `ara-eval` | Draft runner/community, manifest, policy, accounting, and schema modules from its plan (proposed, not implemented). | Capture, publication, announcements, briefing, receipts, recovery, and the Lara adapter ([03](03-lara-integration.md)). |
| Viewer/Obsidian | Artifact rendering, read-only notes. | Nothing in this series. A later view can consume the briefing; frontmatter edits must not bypass guarded writes. |

Evaluation infrastructure doesn't become a production collaboration service in this work. A reusable runner package needs its own owner and deployment decision.

## Engineering checks

- **Unit, integration, and functional tests** target lost contributions, bad identity binding, stale visibility, wrong verdict precedence, malformed inventories, unsafe paths, and interrupted publication or recovery. Changing envelope attribution, lineage, or a native pointer must change the contribution ID; changing payload bytes or modes must change both payload and contribution identity. Retrying an accepted request with changed content rejects.
- **Real CLI.** Runner tests invoke the actual CLI for knowledge reads and writes. Reuse native merge regressions, including the new feedback regressions in [04](04-peer-feedback-merge.md), rather than replacing them with mock results.
- **Smoke fixture**: one seed; two forks with colliding local identities; disjoint experiments; changed executable code and evidence; an incompatible shared-claim edit; an intentional verification; and a synthesis candidate.
- **Smoke steps**, in order:
  1. Publish A1 and B1 and read both before integration.
  2. Recover a lost acknowledgment without changing contribution identity.
  3. Import both into canonical and record scientific conflicts separately from external-file acknowledgments.
  4. Have B import A1, resolve B-owned external-file conflicts through its audit session, consume A's pinned evidence, and publish B2.
  5. Import B2 into canonical using exact B1 as base. Resolve only permitted local conflicts. Confirm that A's original entries have one canonical identity and all histories remain available.
  6. Exercise a canonical-to-worker-to-canonical round trip and replay both the latest contribution and an older already-receipted contribution.
  7. Restart the coordinator, rebuild its derived view, and retrieve source-qualified evidence through the restored receipts.
  8. Evaluate and publish a synthesis as a new contribution with its own evidence.
- **What to assert.** Exact input/output identities, original histories, external-object resolution, and receipt recovery, not just exit codes. An inherited unresolved conflict blocks completion until its source publishes the resolution. Local CLI reads must work while the channel is down.

## Which claims the evaluation can support

Use the [ara-eval evaluation plan](../../../ara-eval/plans/cli-interface-and-collective-research-evaluation.md), including its whole-stack comparison against the paper-based Agora reimplementation. This series adds no experimental arms or thresholds and claims no component-level causal effect.

- **Primary outcome:** independently executed held-out quality at a fixed total community budget.
- **Costs:** report time and cost for reasoning, authoring, publication, integration, verification, repair, and failed calls.
- **Process measures:** actual consumption of peer evidence; accidentally repeated experiment signatures, excluding declared verification; hypothesis coverage; verification completeness; branch concentration; unresolved conflicts.
- **What doesn't follow:** equal scores don't prove duplicate experiments, and Git reachability or account diversity doesn't prove reproducibility or independence.

Agora's single community shows sustained contribution and reuse, not better discovery per unit of compute. Its v4 discloses a view-plus-prompt intervention, mutable or incomplete indexing, and unpinned evaluators and environments; comparisons must keep these limits. Archive complete model and session identities, evaluator and dataset versions, executable inputs, correction records, and intervention logs. Don't claim an exact reproduction of the unreleased Agora run.

## Next Steps

1. Freeze contribution, intention, receipt, and role schemas with the protocol owner, including canonical encoding and exact digest inputs.
2. Implement capture and publication against plan 01. Enable native peer imports only after plan 04's acceptance checks pass.
3. Exercise the full peer-feedback and recovery smoke, then join the Lara track at phase 5. Obtain separate approval for paid runs and scored registration.
