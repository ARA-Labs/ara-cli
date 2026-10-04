# 04b: Canonical-feedback merge defects found by the ara-eval runner
**Date:** 2026-10-04

Status: fix implemented on `collab/phase-4b-feedback-fixes` (0.1.26), pending
review. Parent: [04: peer-feedback merge](04-peer-feedback-merge.md). The
contract changes are recorded in the
[provenance contract](../../docs/collaborative-research/provenance-contract.md#amendments-plan-04b).

## Problem background

The ara-eval phase-4 runner drives real `ara merge --self-key` imports between
canonical and worker forks, acknowledging every external-file conflict with
`merge resolve --take ours` in its own session. With ara 0.1.25 it pinned four
merge failures as known limits (`ara-eval` `docs/community-integration.md`,
`tests/test_integration_conflicts.py`). Each blocks plan 04 acceptance because
canonical feedback must round-trip without bypasses.

| # | Symptom | Trigger |
|---|---|---|
| 1 | `merge.ambiguous_origin`: "incoming `evidence/a.json` (external …) is not the `historical_identity` entry its origin `canonical:evidence/a.json` proves" | Canonical → worker → canonical for a fork that owns a `src/` or `evidence/` file. |
| 2 | `merge.identity`: "session import collision <id>" | The incoming snapshot has two or more sessions on a day where the destination also has one. |
| 3a | `merge.protected_content` (`protected_inherited_entry`, and `protected_field`/`protected_delete` on replay) | Positional `trace/pm_reasoning_log.yaml` entries written on both sides by `merge resolve` sessions. |
| 3b | `merge.alias_dangling` "alias `canonical:.gitignore` targets missing `.gitignore`" | A never-written fork (no `.gitignore`) absorbs canonical. |

## Root causes

1. Canonical's ledger maps the fork's `evidence/a.json` but never installs it,
   so in canonical's published snapshot that path is a `historical_identity`.
   The fork holds the file, so its import of canonical writes the alias
   `canonical:evidence/a.json`. On the return import, `origin::reconcile`
   treated that historical mapping as a self-origin proof and its layer check
   rejected the fork's live external entry. A historical identity is not live
   content and external paths are never relocated, so neither may prove an
   origin.
2. `identity::allocation` relocates an incoming session that collides with a
   destination session to `max(reserved) + 1`, without reserving the IDs that
   other incoming sessions keep. Canonical `_001`, `_002` into a fork with
   `_001`: `_001 → _002` collides with the kept `_002`.
3. a. Positional rows (`#entries/N`) were relocated only when the destination
      already used the same position, and otherwise kept their source position.
      The rows are appended in source order, so recorded targets could differ
      from their real positions (an incoming `entries/2` recorded as `entries/4`
      but appended at `entries/3`). Replays then compared the wrong rows.
   b. Protected YAML comparison (`yaml::protected` for base records,
      `inherited_record` for inherited ones) relocates only typed reference
      fields. The import that produced the copy relocated every identity token
      of the record (`rewrite_source`), including the reasoning entry's
      `session` and `turn`. Unchanged history therefore looked changed.
4. Inherited aliases are copied unless their target is an absent `src/` or
   `evidence/` path. `.gitignore` is an opaque file that is reported
   (`opaque_file`) but never installed, so its inherited alias dangled.

## Fix

1. `origin::reconcile`: a proving mapping whose layer is `historical_identity`
   or `external` proves nothing (contract proof rule 5). Allocation then keeps
   the external path, as for any import.
2. `identity::allocation`: a relocated session skips IDs held by incoming
   sessions. When nothing collides the targets are unchanged.
3. a. `identity::allocation`: every new incoming positional row (not in the
      base, not previously or provably mapped) takes the next position after the
      destination's rows, in source order, matching where it is appended.
   b. `yaml.rs`: when typed field comparison of a protected record fails, the
      record is still accepted if its exact bytes, relocated by the same
      `rewrite_source` the import uses, equal ours (and, for base records, the
      base equals theirs). Only leading indentation is normalized, so quoting
      or other byte changes remain protected. Logic-mutation rows and whole
      sessions keep their dedicated checks.
4. `mod.rs`: do not copy an inherited alias whose target is any whole incoming
   file absent here (and not redirected), matching the source's own mapping
   aliases.

No bypass flag is added and no error is ignored. Ledgers written by 0.1.25
with misplaced positional targets are not rewritten; their recorded mappings
still apply on replay.

## Steps

1. Regression tests reproducing each defect with the exact 0.1.25 errors:
   `crates/ara-core/tests/merge_feedback_defects.rs` and the real-binary
   `crates/ara-cli/tests/agent_feedback_defects.rs` (the runner's flow).
2. Fixes above in `merge/origin.rs`, `merge/identity.rs`, `merge/yaml.rs`,
   `merge/mod.rs`.
3. Contract amendment (plan 04b section), `docs/agent-cli.md`, CHANGELOG, patch
   version 0.1.26.
4. Gates, then the full ara-eval suite against the new binary: the two pinned
   limit tests flip, all others pass.
5. After review, fold this record into `docs/` and remove it from `plans/`.
