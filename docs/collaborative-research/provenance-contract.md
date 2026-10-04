# Peer-feedback provenance contract

**Date:** 2026-10-04
**Status:** frozen representation for plan 04 ([peer-feedback merge](peer-feedback-merge.md)), proposed for adoption by the protocol repository. This file is reviewed before the implementation that follows it.
**Amended in phase 4 review (PR #104):** self facts prove identity only, never
content; unshared foreign revisions cannot remap a held origin
(`merge.unshared_origin_revision`); `revision` mappings are authoritative;
replay never re-proves origins; proofs must match the entry's layer and path;
stricter self-key checks. See [Amendments](#amendments-phase-4-review).
**Amended in plan 04b (0.1.26):** external code and evidence never prove an
origin, and a retired native ID proves only the retired identity; inherited
aliases to any uninstalled whole file are not copied; YAML history compares
exact relocated bytes; positional rows and same-day sessions relocate without
collisions. See [Amendments (plan 04b)](#amendments-plan-04b).

## Problem in one paragraph

A source revision can reach a destination by more than one route. In the
demonstrated diamond, canonical imports fork A directly and also receives the
same A revision inside fork B's later publication. The 0.1.23 merger fails on
that second route for two reasons. It allocates a fresh destination ID for A's
claim because it only consults mappings recorded under the current source key.
It also compares the foreign `revision` record as a whole, so the receiving
fork's import time, base, predecessor and Git context count as a history
conflict. This contract separates the immutable *source fact* from each
destination's *import event* and states when an incoming entry provably has an
origin that the destination already holds. It also covers canonical feedback,
where a fork receives its own entries back inside another snapshot (worker B
absorbing a canonical that integrated B, or canonical receiving its own claims
back from B).

## Terms

| Term | Meaning |
|---|---|
| Source key | Stable fork identity supplied with `--source-key`. Display labels (`--as`) are separate. |
| Transport source | The source key of the current `ara merge`. |
| Foreign source | Any other source key named in the incoming ledger or aliases. |
| Source fact | `(source_key, fingerprint, files, mappings[].{original, layer, path})`. Identical wherever that source revision was imported. |
| Import event | One destination's receipt of a source fact: `time`, `base`, `predecessor`, `git`, and the destination-local `mappings[].target`. |
| Origin | `(source_key, original)`: a native identity in its source's own namespace. |
| Inherited origin | An incoming entry whose origin belongs to a foreign source. |
| Self key | This destination's own stable source key, given explicitly with `ara merge --self-key <key>`. Never inferred from labels, directory names or `--as`. |
| Self origin | An incoming entry whose origin is this destination's own self key. |

## Where each part is stored

Both files keep their existing envelope (`format: ara.merge-log/v1` with
`records:`, `format: ara.aliases/v1` with `aliases:`). Records stay append-only
and are never rewritten.

### `trace/merge_log.yaml` record kinds

| Kind | Status | Fields | Meaning |
|---|---|---|---|
| `enrollment` | existing | `source_key`, `label`, `time` | First time a key is known here, directly or through a route. |
| `label` | existing | `source_key`, `label`, `time` | Later display-label change. |
| `revision` | existing, meaning narrowed | `source_key`, `fingerprint`, `base`, `predecessor`, `time`, `git`, `files`, `mappings[]` | **This destination's own import event** of the transport source, carrying the full source fact. Forms the per-key predecessor chain. |
| `inherited_revision` | **new** | `source_key`, `fingerprint`, `files`, `mappings[]`, `via_source_key`, `via_revision` | A foreign source fact received through the transport source `via_source_key` at its revision `via_revision`. `mappings[].target` are this destination's identities. It has no time, base, predecessor or Git fields because those belong to another destination's event. |
| `transport` | existing | `source_key`, `revision`, `path`, `bytes` | Exact bytes of the source's own `merge_log.yaml` / `aliases.yaml` at that revision. This is where every other destination's import events survive verbatim. |
| `self_identity` | **new** | `source_key`, `time` | This destination's own self key, written once, on the first merge that passes `--self-key`. |
| `conflict`, `resolution`, `protected_decision`, `imported_resolution` | existing | unchanged | Unchanged. Imported unresolved conflicts keep `allowed: []`. |

`mappings[]` entries keep the existing `ImportMapping` shape:
`source_key`, `original`, `target`, `layer`, `path`.

### `trace/aliases.yaml`

Unchanged: `source_key`, `label`, `original`, `target`, `revision`. Incoming
aliases keep their original `source_key`, `label`, `original` and `revision`;
only `target` is relocated to this destination's identity.

### What each consumer reads

| Consumer | `revision` | `inherited_revision` |
|---|---|---|
| Source history, replay detection, `--base` proof, Git predecessor | yes | **no** |
| Per-key predecessor-chain validation | yes | no |
| Stable mapping for a later import of the same key | yes, authoritative | only for originals no `revision` maps |
| Reserved IDs and historical identities | yes | yes |
| Origin proof and content base for inherited entries | yes | yes |
| Association target for `transport` records | yes | yes |

Facts of the destination's own self key are never stored as either kind: the
destination is the source of truth for its own history. Their bytes remain only
inside the `transport` record of the snapshot that carried them.

Keeping inherited facts out of the predecessor chain means a destination always
advances a source against the last revision it imported itself. Received facts
cannot regress, fork or re-order that chain, and a later Git import is not
confused by a received fact that has no Git provenance.

## Proof rules for an inherited origin

An incoming entry with local ID `L` reuses destination ID `D` only when all of
the following hold:

1. The incoming aliases contain `(K, O, T)` with `K` not equal to the transport
   key, and `T` resolves through the incoming audited redirects to `L`.
2. The incoming ledger holds a fact for `(K, F)` (`revision` or
   `inherited_revision`) whose mapping for `O` targets `T`. An alias without
   this backing proves nothing.
3. The destination ledger holds a fact for the same `(K, F)` with
   byte-identical `files`, the same set of `(original, layer, path)`, and a
   mapping for `O`. Its target is `D`.
4. When several shared revisions of `K` qualify, the latest one in the incoming
   ledger order supplies the content base. All of them must agree on `D`.
5. Code and evidence prove nothing: a mapping whose layer is `external`, or
   `historical_identity` for a `src/` or `evidence/` path (an external file the
   source mapped but never installed), is skipped, for self origins too. Their
   identity is their path and they are never relocated.
6. A `historical_identity` mapping of a native ID (an ID the source retired, for
   example by an audited rename) proves the retired identity itself: the
   incoming retired ID maps to the retired original here (for a self origin,
   the original must still be known here, live or retired), not to either
   side's redirect target. Live successors are proven by their own aliases.

Everything else is unproven. Unproven entries allocate destination IDs exactly
as before, so two unrelated `C77` entries stay distinct. Equal text, equal
local IDs, labels, or package parentage are never evidence.

The current source's own previous mapping is applied first. A proven inherited
destination must agree with it.

## Self key and self-origin proof

The self key is resolved before any identity work:

| Situation | Result |
|---|---|
| `--self-key K`, none recorded | Use `K`; append `self_identity {source_key: K}`. |
| `--self-key K`, `K` recorded | Use `K`; append nothing. |
| No `--self-key`, `K` recorded | Use `K`. |
| `--self-key K`, a different key recorded | Reject `merge.self_identity_conflict`. |
| Self key equals the transport `--source-key` | Reject `merge.self_identity_conflict`. |
| This destination holds an `enrollment`, `revision` or `inherited_revision` for its self key | Reject `merge.self_identity_conflict`. |
| No self key at all | Self origins are unproven; entries behave as before. |

An incoming entry with local ID `L` maps to this destination's own `O` when:

1. The incoming aliases contain `(S, O, T)`, `S` being the self key, with `T`
   resolving through the incoming audited redirects to `L`.
2. The incoming ledger holds a fact for `(S, F)` whose mapping for `O` targets
   `T`. This proves identity only: the fact is asserted by the peer, so it is
   never used as a content base.
3. `O`, followed through this destination's audited redirects, still exists
   here as a live entry with the same layer and path as the fact's mapping.
   Otherwise the merge rejects with `merge.self_origin_missing`; a duplicate is
   never allocated.

Self and inherited proofs of one `L` must agree. The current source's previous
mapping must also agree. For every proof, the live incoming entry must have the
proving mapping's layer and path (session files and documents relocate by
identity); otherwise `merge.ambiguous_origin`.

Replay of the latest transport revision re-applies recorded mappings only and
never re-runs origin proofs.

## Content of an inherited entry

Identity and content are reconciled separately.

- Markdown: the shared fact's bytes, rewritten into the incoming namespace with
  the incoming fact's mappings, are the 3-way base. The normal field merge then
  applies: unchanged routes produce nothing, one-sided edits apply, and edits
  on both routes produce a `mutable_field` conflict with base/ours/theirs
  candidates. Conflict evidence stays in the transport source's namespace, so
  `merge resolve` relocates choices the same way as any other conflict.
- YAML (tree, staging, sessions, ledgers): records are compared in the
  destination namespace. Equal records, or a destination-only change, keep
  ours. An incoming change relative to the shared base rejects the whole merge
  as `protected_inherited_entry`, because these layers are immutable history.
  A record also counts as equal when its exact bytes, relocated exactly as an
  import relocates a copied record, equal ours (only leading indentation is
  normalized). Without a rebuildable base, any difference rejects the same way. New incoming
  descendants of an inherited record are still imported under ours' record.
- When an entry's origin is proven but there is no trusted base, an entry that
  is equal after relocation is a no-op. Any Markdown field difference is a
  `mutable_field` conflict with the base absent; ours is kept and nothing is
  taken from theirs, including fields only theirs has. YAML differences reject
  as above.

A self-origin entry never has a trusted base from the incoming side: a peer
could assert our own current bytes as the base and silently overwrite an
unpublished edit. Its only trusted base is the transport `--base` snapshot
(or, in Git mode, the recorded previous revision) when that already contains
the entry, in which case it is merged through the ordinary base path. The
first time an own entry comes back changed, it is therefore a conflict that
keeps ours; later changes merge normally against the previous transport
revision.

The same base rule applies to a transport-source entry that the destination
received earlier through a route: the latest `inherited_revision` of the
transport key that contains it is its base.

## Comparing repeated foreign provenance

For each incoming fact `(K, F)` with `K` not equal to the transport key:

| Destination already holds `(K, F)`? | Rule |
|---|---|
| Yes, as `revision` or `inherited_revision` | Require identical `files` and identical sorted `(original, layer, path)`. For each original whose incoming target maps into this destination, the mapped target must equal the destination's target. Ignore `time`, `base`, `predecessor` and `git`. Append nothing. |
| No | Append one `inherited_revision` with targets mapped into this destination. Every target must map, except `external` (code/evidence) paths, which keep their source path; otherwise reject. |

Incoming `enrollment`, `label`, `revision`, `inherited_revision` and
`transport` records of this destination's own self key are skipped.

Incoming `transport` records for foreign keys are appended once and must be
byte-identical when repeated. `enrollment` and `label` records for foreign keys
are appended when new, as before. A replay of the latest transport revision
appends nothing.

## Error codes

| Code | Status | When |
|---|---|---|
| `merge.alias_conflict` | existing | One alias address resolves to two destinations after relocation (for example, an unbacked or forged incoming alias for an origin this destination already holds). |
| `merge.foreign_mapping_conflict` | existing, narrowed | The same foreign source revision has different source bytes, a different identity set, or a mapping that disagrees with this destination; or a new foreign fact names a target with no provable identity. Import-event fields no longer trigger it. |
| `merge.ambiguous_origin` | **new** | One incoming entry is proven to two destination identities; two incoming entries are proven to one destination identity; or a proven origin disagrees with the transport source's previous mapping. |
| `merge.self_identity_conflict` | **new** | `--self-key` differs from the recorded self key, equals the transport source key, or names a key this destination imported as a source. |
| `merge.self_origin_missing` | **new** | A live incoming entry is proven to originate here, but its original no longer exists here with the same layer and path. |
| `merge.unshared_origin_revision` | **new (review)** | An incoming foreign fact this destination does not hold would map an origin it already holds to a different identity. Import that source revision here first. Nothing is written, so later imports of the source are not wedged. |
| `merge.protected_content` | existing | Protected history changed, including the new conflict kind `protected_inherited_entry`. |
| `merge.corrupt_ledger` | existing | A malformed `inherited_revision` (unenrolled key, unknown `via` revision, duplicate `(K, F)`, unsafe paths, duplicate originals), or one that disagrees with a `revision` for the same `(K, F)`; more than one `self_identity`; or an `enrollment`, `revision` or `inherited_revision` of the recorded self key. |
| `merge.source_regression`, `merge.unproven_source_revision` | existing | Unchanged; they consider `revision` records only. |

All of these reject before the destination is mutated and carry conflict
evidence with the complete base/ours/theirs bytes of the affected file.

## Migration and unknown fields

- Existing ledgers stay valid and are never rewritten. Foreign facts that older
  binaries appended as ordinary `revision` records keep their old meaning and
  still take part in the comparison rules above.
- `inherited_revision` is written only when a merge receives a foreign source
  fact the destination does not already hold. `self_identity` is written only
  on the first merge that passes `--self-key`. Artifacts without peer imports
  contain neither.
- Every record kind and alias row keeps `deny_unknown_fields`. A reader that
  does not know `inherited_revision` or `self_identity` (ara 0.1.23 and
  earlier) rejects the ledger with `merge.corrupt_ledger`. That is the intended fail-closed gate:
  runners must upgrade every reader before enabling native peer imports.
- The only new input is the optional `self_key` merge option
  (`--self-key`). Unknown fields remain errors.

## What stays fail-closed

- Protected-history edits, ledger or alias rewrites, and source regressions.
- Forged or unbacked aliases for an origin the destination already holds.
- Divergent copies of one source revision.
- Self origins without a self key. A destination that never received
  `--self-key` cannot know which incoming entries are its own; they stay
  unproven and import as new entries or collide. The runner must pass
  `--self-key` on every merge into a fork or canonical.
- A self origin whose original was deleted here, or moved to another layer or
  path (`merge.self_origin_missing`).
- Source facts the destination has not imported itself and that no shared
  revision proves. They are transported as `inherited_revision` evidence only.
- Incoming code and evidence under `src/` and `evidence/` are never installed;
  they stay `external_read_only` with `allowed: [ours]`. Opaque files such as
  `.gitignore` likewise stay `opaque_file`. An incoming alias whose target is
  such a whole file, absent here, is not copied into `aliases.yaml`; it remains
  in the transported alias bytes.
- Inherited unresolved conflicts arrive as `imported_unresolved` with
  `allowed: []`. Only the owning fork resolves them; the destination receives
  that resolution as `imported_resolution` on a later import.

## Cost when unused

Origin reconciliation runs only when the incoming ledger holds a fact for a key
other than the transport key. Otherwise the merger takes the 0.1.23 path:
no extra inventories, no base reconstruction, no new records.

## Amendments (phase 4 review)

The independent review of PR #104 changed these rules before merge:

1. Self facts are peer assertions. They may prove that an incoming entry is our
   own (so no duplicate is allocated) but never supply a 3-way base. Without a
   trusted base, differences conflict and ours is kept.
2. A peer can hold a source revision this destination never imported. If such a
   fact would give an origin that this destination already holds a second
   identity, the merge rejects with `merge.unshared_origin_revision` before
   writing. `revision` mappings are authoritative for later imports;
   `inherited_revision` mappings only fill originals no `revision` maps. The
   0.1.23 path would otherwise have written a ledger on which every later
   import of that source fails `merge.corrupt_ledger`.
3. Replay never re-proves origins.
4. `--self-key` is refused when the key is enrolled or inherited here, and
   ledger validation rejects such records for the recorded self key.
5. A proof whose mapping layer or path differs from the incoming entry rejects
   with `merge.ambiguous_origin`.

## Amendments (plan 04b)

The ara-eval phase-4 runner found four defects in 0.1.25
([plan 04b](peer-feedback-merge.md#defects-found-by-the-integration-runner-plan-04b-fixed-in-0126)).
The fixes change these rules; every fail-closed guarantee above still holds.

1. Proof rules 5 and 6: external code and evidence, including an external
   file the source mapped but never installed (carried as a historical
   identity), never prove an origin. Before, a fork's own `evidence/` file was
   "proven" by canonical's historical mapping and rejected as
   `merge.ambiguous_origin`. A retired native ID still proves its origin, now
   as the retired identity itself; 0.1.25 followed both sides' redirects and
   rejected a self-key round trip of a renamed claim as
   `merge.ambiguous_origin`.
2. Not copying inherited aliases to uninstalled targets now covers every whole
   file (for example `.gitignore`), not only `src/` and `evidence/`.
3. Protected YAML records, inherited or in the base, also compare equal when
   their exact relocated bytes agree. Typed field comparison leaves opaque
   values unrelocated, while the import relocated every identity token of the
   copied record (a reasoning entry's `session` and `turn`), so unchanged
   history was rejected. Logic-mutation rows and whole sessions keep their
   dedicated checks.
4. Allocation (not a portable record): a new positional row
   (`<path>#entries/N`, `#mutations/N`, ...) takes the next position after this
   destination's rows, in source order, which is where it is appended. A
   relocated session ID skips IDs that other incoming sessions keep.
