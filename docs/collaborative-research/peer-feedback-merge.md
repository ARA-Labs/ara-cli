# Peer-feedback merges: design record

Status: implemented in ara 0.1.25 (#104) and corrected in 0.1.26 (#112). This record replaces plan 04 (peer-feedback merge) and plan 04b (feedback-merge defects), both approved on 2026-10-03. The normative rules are in [provenance-contract.md](provenance-contract.md); the command reference is in [agent-cli.md](../agent-cli.md#directory-and-git-merge).

## Problem

Workers merge published peer or canonical knowledge into their own forks and publish again. The same original entry can then reach a destination by more than one route. ara 0.1.23 rejected this even when every earlier merge had succeeded:

- **Diamond.** Canonical imported fork A directly and later received A's claim again inside fork B. The merger allocated a fresh ID for A's claim, because it consulted only mappings recorded under the current source key, and failed with `merge.alias_conflict`.
- **Import time.** Repeated foreign provenance was compared as whole records, so two imports of the same source revision at different times failed with `merge.foreign_mapping_conflict`. Whether the merge passed depended on the wall clock.

## Design

- **Source facts vs import events.** A source fact is `(source_key, fingerprint, files, mappings[].{original, layer, path})`. It is identical wherever that revision was imported. An import event adds one destination's time, base, predecessor, Git context, and local targets.
  - **Storage:** `revision` records hold only this destination's own import events and form the predecessor chain. Foreign facts the destination lacks are stored as `inherited_revision`, outside that chain. Other destinations' events survive verbatim in `transport` records.
  - **Comparison:** repeated facts compare only bytes and identities.
- **Origin reconciliation before allocation.** An incoming entry reuses an existing destination ID only when its origin is proven. Unrelated entries with equal local IDs stay distinct.
  - **Inherited origin:** an incoming alias backed by an incoming fact, where the destination holds the same fact byte for byte.
  - **Self origin:** with `--self-key`, an alias for the destination's own key whose original is still known here.
- **Content.** An entry that arrived by another route merges 3-way against a trusted base, so edits on both routes give a normal `mutable_field` conflict.
  - **No peer-supplied base:** a peer-supplied self fact proves identity only and never supplies a content base.
  - **No trusted base:** an equal entry is a no-op, and a differing entry conflicts while keeping ours.
- **Self key.** `ara merge --self-key <key>` records the destination's stable key once, as `self_identity`. It is never inferred from labels, directory names, or `--as`, and a mismatched key is rejected.
- **Pay for use.** Reconciliation runs only when the incoming ledger holds facts for a key other than the transport key. Artifacts without peer imports take the 0.1.23 path.

## Defects found by the integration runner (plan 04b, fixed in 0.1.26)

The `ara-eval` phase-4 runner (ARA-Labs/ara-eval#5) exercised canonical feedback end to end and found four defects:

| Symptom (0.1.25) | Root cause | Fix |
|---|---|---|
| `merge.ambiguous_origin` when a fork with its own `src/` or `evidence/` file returns to canonical | Canonical maps but never installs the fork's external file, and that historical mapping was used as a self-origin proof. | External code and evidence never prove an origin. A retired native ID proves only its retired identity, without following redirects. |
| `session import collision` with several same-day sessions | A relocated session took `max(reserved)+1` without reserving IDs kept by other incoming sessions. | Relocation skips every ID held by incoming sessions. |
| `protected_inherited_entry` for reasoning entries written by `merge resolve` on both sides | Positional rows kept their source position, and protected comparison relocated only typed fields. | New positional rows take the next position in source order. A protected record is also accepted when its exact relocated bytes match, and the peer side is compared unrelocated. |
| `merge.alias_dangling` in a never-written fork without `.gitignore` | An inherited alias to an uninstalled whole file was copied. | Such aliases are not copied. |

The independent review of the first 04b commit found that the external-file skip also dropped proofs for retired native IDs. The final fix narrows it, and a retired-claim round trip is tested.

## Acceptance evidence

Plan 04's acceptance table is covered by core tests (`merge_identity.rs`, `merge_peer_feedback.rs`, `merge_self_origin.rs`, `merge_feedback_defects.rs`) and real-binary tests (`agent_merge_identity.rs`, `agent_self_origin.rs`, `agent_feedback_defects.rs`):
- **Merge cases:** the diamond, independent import times, edits on both routes, canonical feedback in both directions, a non-seed fork, later revisions, and replay.
- **Rejections:** forged, ambiguous, and protected edits.
- **Files and conflicts:** external files and inherited conflicts.
- **Git and no-metadata paths:** local-Git parity, and artifacts without import metadata.

The runner-side acceptance also passes with ara 0.1.26 and the real `ara` binary (ARA-Labs/ara-eval#7 and the phase-5 smoke, ARA-Labs/ara-eval#8). That covers publish, peer import, republish, canonical import, canonical feedback, latest replay, and older-receipt lookup, with one canonical identity per original entry.

Zero-cost evidence: `docs/verification/collaborative-research/phase-4/` and `phase-4b/`, each a full gate-valid run with no regressions and no new crates.

## Known limits (fail-closed)

- **Self key:** the runner must pass `--self-key` on every merge into a fork or canonical. Without it, a destination's own returning entries are unproven.
- **Inherited YAML records:** divergent appends to an inherited record's append-only fields reject instead of union-merging.
- **Unshared revisions:** an origin seen only in a source revision that the two sides do not share stays unproven. If it would remap an origin the destination already holds, it is rejected with `merge.unshared_origin_revision`.
- **Legacy ledgers:** ledgers written by 0.1.25 with misplaced positional targets are not rewritten.
