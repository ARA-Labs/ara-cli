# PR 19: deterministic bookkeeping in the CLI, not the agent
**Date:** 2026-10-04

Status: **draft, pending review**. Repository: `ARA-Labs/ara-cli` (binary and
[skills](../../docs/agent-cli-skills.md)). Parent: [agent CLI interface](../agent-cli-interface.md).
Shared checks: [PR index](README.md). Related: [plan 18](18-failed-blocked-calls.md).

## TL;DR

Work that needs only parsing and rewriting belongs in `ara`, not in the LLM. An
audit of the six CLI skills found the research-manager agent still computes, on every
turn, values `ara` could derive from the batch it is already writing: the turn number,
timestamps, today's session id, `events_logged` rows and `claims_touched` rows. Four
skill steps ask the agent to do what `ara` already does or rejects. Reference
rewriting and staleness counts are also left to the agent. This plan moves all of
these into `ara` (groups A, C, D), deletes the redundant skill steps (B), and fixes a
bug where `claim add` writes fields in alphabetical order with a layout no other
claim uses. Compiler and collective bookkeeping (E, F) are recorded as follow-ups.

## Problem

### Principle

An agent should supply judgement: what happened, what a claim says, what to record.
Everything derivable from the artifact, the batch, or the clock should come from `ara`,
which does it the same way every time and cannot drift.

### What `ara` already does

IDs (N, O, C, H, T, session); session `turn_count`, `last_turn`, per-record `turn` and
every session-index row (`write/sessions.rs:601`, rewritten from the session file so
past rows never change); `Last revised` (caller values are rejected, `logic.rs:425`);
promotion provenance; dangling-reference and dependency-loop checks at write time;
taste formatting.

### What the agent still does (audit, 2026-10-04)

Line numbers refer to `skills/research-manager-cli/` unless stated.

| # | Chore | Skill instruction | `ara` today | Hit |
|---|---|---|---|---|
| A1 | Next turn number for `logic.revise`, audits, the reasoning-log `turn`, `merge --turn` | `references/cli-access.md:65` | `turn: u64` is a literal (`write/mod.rs:197`); `$s#N` binds only when `session.start` is in the same batch (`batch.rs:311`) | Every turn with a revision |
| A2 | Timestamps on nodes, observations, `session.log`, taste records | `references/schema-and-initialization.md:57,138,156,246` | `apply` rejects a `session.log` without one; `node.add` never stamps one (`node.rs:611`). Convenience `stage` and `session log` default to now (`ara-cli/src/write.rs:562,623`) | Every turn |
| A3 | Today's session id, or starting a session | `SKILL.md:78` | JSONL requires `session`; convenience `session log` picks today's (`write.rs:625`) | Every turn |
| A4 | `events_logged` rows repeating id, type, routing, provenance of each creation | `references/schema-and-initialization.md:161` | Built by `session log --node`, but only for nodes and only outside `apply` (`write.rs:668`) | Every turn |
| A5 | `claims_touched` rows after `promote` or `revise` | `references/schema-and-initialization.md:175` | Not derived; the agent writes the row | Every claim change |
| B1 | Update `Last revised` | `SKILL.md:319` | Computed; caller values rejected (`logic.rs:425`) | Every revision |
| B2 | Copy before/after into `logic_revisions` | `SKILL.md:330` | `logic.revise` appends exact before/after (`logic.rs:450`) | Every revision |
| B3 | Set `promoted: true`, `promoted_to` on the observation | `SKILL.md:165` | Set by `observation.promote` (`staging.rs:305`) | Every promotion |
| B4 | Add `Crystallized via`, `From staging` to the claim | `SKILL.md:162` | Not claim fields; writes are rejected (`fields.rs:46`) | Every promotion |
| C1 | Rewrite citing entries on split, merge, rename | `SKILL.md:325` | `refs` lists citers; `entry.rename` needs caller-written before/after rows per reference (`logic.rs:875`); merge redirection is one manual `logic.revise` per citer | Each restructure |
| D1 | Count turns since an observation was last referenced (k = 5) | `SKILL.md:130` | `open` reports staleness by session-days only (`ara-cli/src/agent.rs:935`) | Each staleness check |
| D2 | Build the `session_days` list for `observation.mark_stale` | `SKILL.md:233`, `references/cli-access.md:72` | `mark_stale` verifies the list against evidence it already computes (`staging.rs:379`) but will not build it | Each stale mark |

### Bug: `claim add` writes a layout no other claim uses

Reproduced on a copy of `crates/ara-core/tests/fixtures/agent-cli`:

```
ara claim add --title "Order probe" --set "Statement=S text" --set "Conditions=C text" \
  --set "Status=supported" --set "Falsification criteria=F text" --set 'Proof=[]' \
  --set 'Dependencies=[]' --set "Provenance=user" --set 'Tags=["x"]'
```

writes fields in alphabetical order (`fields.rs:185` builds a `BTreeMap`), each value on
its own indented line, `Falsification criteria` renamed to `Falsification`, and lists as
JSON (`[]`, `["x"]`):

```
## C17: Order probe
- **Conditions**:
  C text
- **Dependencies**:
  []
- **Falsification**:
  F text
...
```

Every existing claim in the same file reads `- **Statement**: …` on one line, in schema
order, with `none` for an empty list and comma-separated tags. Deterministic writes
must follow the document's convention, or the agent has to clean up after `ara`.

## Goals and acceptance

1. A research-manager turn's batch carries only judgement: what happened, the content of
   each creation or revision, and the summary. Session, turn, timestamps,
   `events_logged` and `claims_touched` come from `ara`.
2. No skill step asks the agent for a value `ara` computes or rejects (B1–B4 gone).
3. Restructuring an entry rewrites structured references in one operation (C1).
4. `open` reports both staleness measures, and `mark_stale` needs no caller-built list (D1, D2).
5. `claim add` output is indistinguishable in layout from the file's existing claims.
6. Re-running the audit finds no A–D item and no "skill still asks" item.

## Proposed changes

### A. Batch-level derivation

| # | Change | Contract impact |
|---|---|---|
| A0 | Each `apply` batch reads the clock once (`batch_time`); every derived timestamp in the batch uses it, so a batch is internally consistent and testable with an injected clock | Internal |
| A1 | `turn` may be omitted, meaning the turn owned by the batch's `session.log`; a batch that needs a turn and has no `session.log` gets one with `summary` required | Optional field; old batches unchanged |
| A2 | `timestamp` may be omitted on `node.add`, `observation.stage`, `session.log` and taste records; defaults to `batch_time`. `node.add` stamps nodes it creates | Optional field; nodes gain timestamps |
| A3 | `session` may be omitted on `session.log`: use the single open session for `batch_time`'s date, or start one if none exists; several open sessions remain an error | Optional field |
| A4 | `session.log` derives `events_logged` rows from the batch's `node.add`, `observation.stage` and `promote` operations (id, type, routing, provenance); the agent may add extra rows | Additive; derived rows marked so they are not duplicated |
| A5 | `session.log` derives `claims_touched` rows from `claim.add` (`created`), `promote` to a claim (`crystallized`) and `logic.revise` on a claim (`revised`, or the Status change as the action) | Additive |

The convenience commands (`stage`, `session log`) already default A2 and A3; they move
onto the same code path so both entry points behave the same.

### B. Skill deletions (`research-manager-cli`)

Delete B1–B4. Replace B3 with "`observation.promote` sets these". Rewrite the batch
examples in `SKILL.md` and `references/schema-and-initialization.md` to omit the values A
derives, and say which values `ara` fills.

### C. Reference rewriting

| # | Change | Contract impact |
|---|---|---|
| C1 | `entry.rename` and a merge redirect accept `rewrite_references: true`: `ara` replaces the old id token in structured reference fields (`Dependencies`, `Proof`, tree `evidence`, `bound_to`, …) of every current citer and records each edit in the audit, as it records caller-written edits today. Prose mentions are listed (as `refs` lists them) but never rewritten; historical layers stay byte-identical | Additive option |

### D. Staleness

| # | Change | Contract impact |
|---|---|---|
| D1 | `open` reports `turns_since_reference` for each observation alongside the session-day measure | Additive field |
| D2 | `observation.mark_stale` makes `session_days` optional and fills it from the evidence it already computes; a supplied list is still verified | Optional field |

### Bug fix

`claim add` (and `heuristic add`) write fields in the schema's order, keep the caller's
canonical field names (`Falsification criteria`), and render values inline in the
document's existing style: single values on the bullet line, empty lists as `none`, lists
comma-separated. Existing documents are never reformatted.

## Implementation steps

Per the bug process, each PR starts with tests that reproduce the current behavior.

1. **PR 19a — claim layout bug.** Reproducer in `crates/ara-cli/tests/agent_writes.rs`:
   the `claim add` above against the `agent-cli` fixture; assert the new block matches
   the layout of the file's existing claims and that the rest of the file is
   byte-identical. Fix in `ara-core/src/write/fields.rs` and the claim renderer.
2. **PR 19b — batch derivation (A0–A5) and skill deletions (B).** Tests with an
   injected clock: a batch with no session, turn or timestamps produces the same files
   as the fully specified batch; omitted `turn` resolves to the batch's own `session.log`
   turn; a batch on a day with no session starts one; two open sessions still refuse;
   `events_logged` and `claims_touched` match what the agent writes today, with no
   duplicates when the agent also supplies a row; old fully specified batches produce
   byte-identical output. Update `research-manager-cli` in the same change.
3. **PR 19c — reference rewriting (C1).** Tests: rename `C03` with citers in
   `Dependencies`, `Proof` and tree `evidence`; every structured citation updated, prose
   mentions untouched and listed, historical layers byte-identical, audit rows equal to
   today's caller-written rows; merge redirect the same.
4. **PR 19d — staleness (D1, D2).** Tests: `turns_since_reference` against a fixture
   with known mention turns; `mark_stale` without `session_days` equals the call with
   the correct list; a wrong supplied list still refuses.
5. Each PR bumps the patch version, adds a `CHANGELOG.md` entry, updates
   `docs/agent-cli.md`, and keeps `tests/skills.rs` passing.
6. Re-run the skill audit (the same prompt, recorded in this plan's PR) and attach
   the result.
7. After the PRs land, rewrite this plan as a design record in
   `docs/agent-cli-interface/` and remove it from `plans/`.

## Follow-ups (not in this plan)

- **E. Compiler bookkeeping** (`compiler-cli`): check that each quote appears at its cited
  `path:line`; Seal L1 structural checks as an `ara check` profile; generated `PAPER.md`
  Layer Index rows and counts; generated `evidence/README.md` index rows; compiler-mode
  `claim.add`/`experiment.add`/`rw.add` without PM-only fields; automatic
  `knowledge_paths` registration on `document.create`.
- **F. Collective bookkeeping** (`collective-*`): a `knowledge_revision` digest in
  `status` (which `references/cli-access.md:34` already claims exists), and
  `open --frontier` emitting frontier facts with digests.

## Open questions

- **Q1.** A3: should `apply` start a session automatically when the day has none, or
  refuse and require an explicit `session.start`?
- **Q2.** Bug fix: when a document's existing claims disagree on layout, follow the
  schema's order and style, or the majority of the file?
