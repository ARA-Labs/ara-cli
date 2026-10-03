# 02: Contributions, publication, frontier, and integration
**Date:** 2026-10-03 (content from the 2026-10-02 draft)

Status: draft for human review, staged in `ara-cli` until the upstream route is decided (README decision D1). Target repositories: `Agent-Native-Research-Artifact` for portable contracts (envelope, verification, roles, visibility), and `ara-eval` for the runner (capture, publication, briefing, receipts, recovery). Parent: [collaborative research plan series](README.md). This plan requires no `ara-cli` change beyond [01](01-ara-snapshot.md).

## TL;DR

A finished piece of work becomes a *contribution*: a frozen native snapshot from `ara snapshot`, the declared code and evidence, and an external envelope. The runner's coordinator publishes contributions one at a time. It announces them on the existing plan-14 channel and builds a bounded briefing from CLI reads over the exact snapshots. The integration PM later imports contributions with `ara merge --source-key`, and the runner records an integration receipt. Publication, verification, and integration are separate recorded events; none of them rewrites a published package.

## Background: what exists

- **Reads.** `status`, `ls`, `show`, `path`, `refs`, `open`, and `find` inspect one local artifact ([guide](../../docs/agent-cli.md)).
- **Writes.** `apply` and the convenience commands allocate IDs, validate deltas, keep audit history, and commit through recoverable transactions.
- **Merges.** Directory and local-Git `merge` preserve sources, relocate colliding identities, and keep conflicts. Repeated imports are tracked per `--source-key`.
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

There are two digests:
- The **native revision** is the `ara.artifact/v1` fingerprint that `ara snapshot` records. It covers nonprivate source and evidence bodies and portable provenance, not just knowledge documents.
- The **package digest** covers everything else the runner freezes: execution metadata, declared inputs outside the native root, and modes.

A `show --source` digest isn't an artifact revision, and neither is a knowledge-only inventory.

| Envelope field group | Meaning |
|---|---|
| Identity | Schema revision, community/run, publisher, request, source identity, package digest. |
| Lineage | Starting native revision, parent contribution digests, native references the work uses. |
| Research payload | Pointers to the native experiment, question, or observation; the declared change; the prediction recorded before execution; the measured outcome; the follow-up. Pointers only, never copied bodies. |
| Package inventory | Sorted relative paths, file digests and modes, external object digests, entry point, configuration, environment identity. |
| Verification | Target contribution/version, question or claim tested, method, evaluator/configuration, reproduced observations, scoped verdict. |
| Argument check | Lara coverage and verdicts; see [03](03-lara-integration.md). Kept separate from reproduction. |
| Publication receipt | Coordinator sequence, acknowledged request/package identity, visibility snapshot identity. Stored outside the frozen package. |

Illustrative envelope (field names are unreviewed):

```json
{
  "schema": "ara.contribution/v0-draft",
  "community": "run-2026-10-a",
  "publisher": "worker-a",
  "request": "req-a-0007",
  "source_key": "fork-a",
  "native": {"snapshot": "packages/a-0007/snapshot.json", "fingerprint": "3b9c…e41a"},
  "package_digest": "sha256:9d10…",
  "lineage": {
    "start_fingerprint": "0e77…",
    "parents": ["sha256:51aa…"],
    "uses": ["N04", "C02"]
  },
  "payload": {"experiment": "N12", "question": "N04", "change": "remove gating", "prediction": "…", "outcome": "…"},
  "inventory": {"entry": "src/train.py", "config": "src/conf/ablation.yaml", "environment": "sha256:…", "external": []}
}
```

Rules:
- The schema owner defines deterministic encoding and hashing.
- The envelope sits outside its hashed payload, and the receipt sits outside the frozen package, so no digest refers to itself.
- Paths stay within declared roots. Publication rejects unsafe traversal, unsupported links, missing objects, and credentials.
- Large inputs can live outside Git with a digest and a resolvable pinned location. A required object that can't be fetched blocks publication.
- Corrections and superseding verdicts are new records that target prior versions.

## How publication works

A result moves through `draft -> frozen -> published`. Canonical integration is a separate state: `pending`, `imported-with-conflicts`, `integrated`, or `rejected-for-integration`. These are external workflow states, not native claim statuses, and each change is recorded without rewriting the package. Publication means the package is durably available and passed the pinned structural policy. It is not scientific certification.

Publication steps:

1. Quiesce the worker. The CLI lock doesn't make direct code or evidence writes atomic.
2. Run `ara snapshot` ([01](01-ara-snapshot.md)) and verify its manifest. Don't re-implement capture.
3. Freeze declared inputs outside the native root, and inventory referenced code and evidence without running them.
4. Check the snapshot's diagnostics against the publication policy.
5. The coordinator writes the package into the authoritative record, assigns a publication sequence, and reconciles the channel announcement. Only then does it return a visibility acknowledgment.

Private `.ara/transactions` and lock files never enter a package; portable identity and conflict files do. The announcement carries the package identity and sequence. It's a view of the durable record, not a second source of facts. Publication and native knowledge writes are separate transactions.

Git can store frozen packages and small append-only envelopes in the community repository. Workers don't push to one shared mutable knowledge tree. The record keeps publisher attribution independent of the Git committer. Transport and Git publication are runner operations, not new meanings for `ara merge --git`.

The plan-14 intention schema needs a reviewed revision before it can carry this:
- Its fields are closed.
- Completion keeps the pre-work artifact revision.
- It verifies other intentions, not immutable packages.

The revision should add result-version binding and contribution-target verification, reusing the existing CAS, request digest, journal, and recovery code. An intention-completion receipt is not a publication certificate.

## How peers find work before a merge

The community snapshot lists published contribution identities and visibility sequences next to the intention snapshot. The runner builds the briefing from CLI reads over exact snapshots plus validated envelopes. It doesn't parse YAML or Markdown itself.

The briefing separates five kinds of material:
- canonical facts;
- the worker's own findings;
- published but unintegrated peer findings;
- active intentions;
- stale intentions.

Cross-artifact reading is a separately approved collective scope: the interface-only research-foresight skill stays limited to its own ARA. The runner enforces access; readers don't get unrestricted sibling-directory access.

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

The integration PM imports a frozen snapshot with the existing source-aware merge:

- **First import** from a source verifies the shared seed as the base.
- **Later directory imports** under the same `--source-key` use the last imported source revision as the base, so every predecessor snapshot must be kept. Local-Git imports require the enrolled source's verified ancestry.
- **Repeated import** of the same snapshot must not duplicate entries.
- **Receipts** bind the identity mapping, destination revision, merge report, and owning audit session. The contribution stays readable whether or not it was accepted.

A clean structural merge isn't scientific agreement. If the integration PM disagrees with a contributor's change to a shared claim, the PM records the competing interpretation through the native audit contract. Mutable conflicts keep their candidates. Protected-history violations block mutation and go through the separate repair path. The envelope never overrides the merger.

The runner checks the merge exit status, `unresolved_count`, and review flags: `committed: true` can still mean a partial import with conflicts. A dry run isn't approval for a later destination revision. The initial runner gives the integration PM exclusive control of the canonical workspace and rechecks the package identity before committing. README decision D5 covers adding a CLI expected-revision guard later.

Knowledge merge doesn't install incoming code or combine executables. To build on a package, the runner materializes it into an isolated workspace with its original paths and modes. A synthesis contribution names its parent packages, edits code in a fresh workspace, runs its own evaluator, and publishes new evidence. Repository ancestry, scientific dependency, contribution parentage, and verification targets stay distinct.

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
- The current view selects the latest verdict per verifier/target/method scope and exposes conflicts across scopes.
- Claim promotion still follows the native closure procedure. Reuse counts, endorsements, and verification totals are process measures.

## What happens when a step fails

| Failure | Required behavior |
|---|---|
| Worker crashes before freezing | Keep attempt inputs and logs under runner policy; announce nothing. |
| Inputs change during capture | `ara snapshot` returns `stale_snapshot_input`, or the runner's own inventory differs. Reject the package, keep the failed request, and recapture. |
| Publication commits but the acknowledgment is lost | Recover the request's package and receipt before retrying; never create a second contribution. |
| Durable contribution, but the announcement fails | Mark it published but not yet visible; reconcile the channel from the record before acknowledging. |
| Cache or index deleted or corrupt | Rebuild from immutable envelopes, snapshots, and correction records; never show an empty frontier on failure. |
| Imported interpretation conflicts | Keep the incoming package and candidates; expose unresolved integration state. |
| Code or evidence needed for reuse is missing | Reject reuse or publication; never fetch a mutable substitute or run partial code. |
| Budget exhausted | Apply the pinned accounting/closure policy; keep the interrupted record; grant no hidden extra allocation. |

Receipt, cache, and channel recovery never edits a frozen contribution. The coordinator's accepted-record boundary and rebuild procedure are part of the contract; tests must show that a rebuild yields the same contributions, corrections, and verdicts. Agora's observed split between immutable commits and unrecorded database corrections is not copied.

## Repository ownership

| Repository | Reuses | Adds after review |
|---|---|---|
| `ara-cli` | `agent.rs`, `write.rs`, `merge.rs`, `merge/git.rs`; core `write/source.rs`, `merge/identity.rs`. | `ara snapshot` ([01](01-ara-snapshot.md)) only. No community or network commands. |
| `Agent-Native-Research-Artifact` | `skills/collective-research-cli/`, `collective-frontier-cli/`, `collective-intentions-cli/`, `evaluation/agent-cli/collective-contract.json`. | Contribution, verification, attribution, visibility, and role contracts; the intention schema revision; the Lara handoff contract. Interface-only skills and archived baselines unchanged. |
| `ara-eval` | Draft runner/community, manifest, policy, accounting, and schema modules from its plan (proposed, not implemented). | Capture, publication, announcements, briefing, receipts, recovery, and the Lara adapter ([03](03-lara-integration.md)). |
| Viewer/Obsidian | Artifact rendering, read-only notes. | Nothing in this series. A later view can consume the briefing; frontmatter edits must not bypass guarded writes. |

Evaluation infrastructure doesn't become a production collaboration service in this work. A reusable runner package needs its own owner and deployment decision.

## Engineering checks

- **Unit, integration, and functional tests** target: lost contributions, bad identity binding, stale visibility, wrong verdict precedence, malformed inventories, unsafe paths, and interrupted publication or recovery.
- **Real CLI.** Runner tests invoke the actual CLI for knowledge reads and writes. Existing native merge regressions are reused rather than re-tested.
- **Smoke fixture**: one seed; two forks with colliding local node IDs; one disjoint experiment per fork; an incompatible edit to a shared claim; one intentional verification; one synthesis candidate.
- **Smoke steps**, in order:
  1. See both publications before integration.
  2. Recover a lost acknowledgment.
  3. Restart the coordinator.
  4. Rebuild its derived view.
  5. Import both forks.
  6. Resolve only the permitted mutable conflict through a complete session.
  7. Evaluate the synthesis.
- **What to assert.** Exact input and output identities and preserved histories, not just exit codes. Local CLI reads must work while the channel is down.

## Which claims the evaluation can support

Use the [ara-eval evaluation plan](../../../ara-eval/plans/cli-interface-and-collective-research-evaluation.md), including its whole-stack comparison against the paper-based Agora reimplementation. This series adds no experimental arms or thresholds and claims no component-level causal effect.

- **Primary outcome:** independently executed held-out quality at a fixed total community budget.
- **Costs:** report time and cost for reasoning, authoring, publication, integration, verification, repair, and failed calls.
- **Process measures:** actual consumption of peer evidence; accidentally repeated experiment signatures, excluding declared verification; hypothesis coverage; verification completeness; branch concentration; unresolved conflicts.
- **What doesn't follow:** equal scores don't prove duplicate experiments, and Git reachability or account diversity doesn't prove reproducibility or independence.

Agora's single community shows sustained contribution and reuse, not better discovery per unit of compute. Its v4 discloses a view-plus-prompt intervention, mutable or incomplete indexing, and unpinned evaluators and environments; comparisons must keep these limits. Archive complete model and session identities, evaluator and dataset versions, executable inputs, correction records, and intervention logs. Don't claim an exact reproduction of the unreleased Agora run.
