# PR 23: read real ARAs despite optional-layer warnings (#128)
**Date:** 2026-10-18

Status: **draft, awaiting review**; D1 and D2 decided 2026-10-18 (both option a). Issue: [#128](https://github.com/ARA-Labs/ara-cli/issues/128).
Branch: `fix/128-readable-real-aras`, targeting `feat/agent-cli-interface`.
Parent: [agent CLI interface](../agent-cli-interface.md). Shared checks: [PR index](README.md).

## TL;DR

Two changes. (1) Typed entries in `logic/experiments.md` and
`logic/solution/heuristics.md` accept the same `ID: Title`, `ID — Title`,
`ID – Title` and `ID - Title` headings that claims already accept, through one
shared parser that the reader, merge identity and CLI heading lookup all use.
(2) An `ARA217`/`ARA218` warning about one optional-layer file stops refusing
every structural read. Reads go ahead and say which files were not fully
represented. Writes keep their existing validation.

## Problem

`ara` 0.1.33 refuses `ls`, `show` and `find` on every real research ARA over 40
nodes in the #128 audit, and on `ara-paperbench/nanogpt-speedrun`. Plain file
reads work. Two causes concern only optional agent layers:

- **Entry headings.** `markdown_entries` (`crates/ara-core/src/agent_layers.rs`)
  splits on the first `:` only. `## E01 — Baseline` and `## H03 – Track …` drop
  the entry and warn `ARA217 malformed entry heading` (with no line number).
  `claims::claim_heading` already accepts spaced dashes, but only for `C` IDs.
- **All-or-nothing refusal.** `validity::read_refusal` turns any `ARA217`,
  `ARA218`, `ARA222` or `ARA226` warning into `incomplete_artifact` for all
  structural reads. One duplicate observation ID or one session file with a
  duplicate YAML key hides a valid 500-node tree, its claims and experiments.

The agent-CLI evaluation E2 pads benchmarks with these ARAs, so today the CLI
condition measures refusals, not efficiency.

Out of scope: `ARA100`, `ARA105`, `ARA112` in the audit. They are real record
errors and keep refusing with `invalid_artifact`.

### Reproduction

`crates/ara-cli/tests/agent_layer_tolerance.rs` (new). On 0.1.33:

| Test | Result |
|---|---|
| `dash_separated_typed_entry_headings_parse` | fails: `incomplete_artifact`, `ARA217`×3 |
| `duplicate_observation_id_does_not_hide_the_tree` | fails: `incomplete_artifact`, `ARA218` |
| `malformed_session_file_does_not_hide_the_tree` | fails: `incomplete_artifact`, `ARA217` |
| `hyphenated_title_word_is_not_a_separator` | passes (guard for the fix) |

## What else reads these headings

| Site | Today | Change |
|---|---|---|
| `agent_layers::markdown_entries` (reader) | `:` only | shared parser |
| `merge::identity::heading_entry`, `merge::markdown::identified_title` | dash only for `C` | shared parser for any native prefix |
| `ara-cli` `agent::headings::heading_id` | dash only for `C` | shared parser |
| `write::logic::heading_id` (writes, ID allocation) | splits on `:`/whitespace | none: already returns `E01` for `E01 — X` and `E02 [x]: X` |

If the reader accepted dashes but merge did not, merge would treat `E01 — X` as
a different entry from the reader's `E01`. All three sites change together.

## Proposed approach

### Part 1: shared entry-heading parser

1. In `ara-core`, generalize `claims::claim_heading` into
   `entry_heading(heading, prefix) -> Option<(&str, &str)>` with the same rules:
   canonical `{prefix}##` ID, then `:` or a dash (`-`, `–`, `—`) with whitespace
   on both sides, then a nonempty title. Keep `claim_heading(h)` as
   `entry_heading(h, 'C')` so existing callers are unchanged.
2. `markdown_entries` uses `entry_heading(section.heading, prefix)`.
3. Merge (`heading_entry`, `identified_title`) and the CLI `heading_id` try
   `entry_heading` with the prefix the ID starts with (`N C H E O T`).
4. The `malformed entry heading` warning gains the line and heading text
   (`logic/experiments.md:12: malformed entry heading `E02 …``), as
   `unparsed_claim_headings` already reports for claims.
5. **D1 (decided: accept).** `## E02 [in progress]: Reception` parses as ID
   `E02`, title `Reception`. The bracket text is the entry's `status` when it
   has no `**Status**` field; a `**Status**` field wins. The heading source is
   kept verbatim. Only `entry_heading` callers that read typed entries use the
   bracket; claims do not gain bracket syntax.

### Part 2: optional-layer warnings read through

6. Split `UNREPRESENTED` by what the warning covers, using its `path`:
   - **Optional layer file** (read through): `ARA217`/`ARA218` whose path is
     `staging/observations.yaml`, `trace/taste_log.yaml`, `trace/sessions/*`,
     `logic/experiments.md`, `logic/solution/heuristics.md`, or
     `PAPER.md.knowledge_paths`.
   - **Node-scoped** (still refuse): `ARA222`, `ARA226`, and `ARA217` on a
     `nodes[…]` path. These change what a tree node says, which is outside
     #128.
7. `read_refusal` refuses `incomplete_artifact` only for node-scoped warnings.
   Optional-layer warnings no longer refuse.
8. **D2 (decided: explicit marker).** When an optional layer is not fully
   represented, `ls`/`show`/`find` results add `"complete": false` and
   `"incomplete": [{"path", "codes", "reason"}]`; both are absent otherwise.
   Brief text adds one line, `incomplete: staging/observations.yaml (ARA218)`.
   `status.complete` keeps its all-or-nothing meaning.
9. `status`: `counts` and `next_ids` are no longer nulled for an optional-layer
   warning alone. The next ID for an affected prefix (`E` after a dropped
   experiment heading) is null with a `next_id_errors` entry, because the
   dropped entry may already hold the next number.
10. `show O01` with two `O01` records (`ARA218`, "all entries retained"):
    returns both records. A test pins this.
11. Writes are unchanged. They validate in `ara-core` (`write/source.rs`) and
    reject only diagnostics the write introduces; they never called
    `read_refusal`.

## Decisions (2026-10-18)

**D1. `## E02 [planned]: Title`.**
- (a) **Chosen:** accept it. The ID is `E02`, the title `Title`. The bracket
  text becomes the entry's `status` when there is no `**Status**` field, and the
  source heading stays verbatim. Writes already accept it.
- (b) Keep refusing, with a clearer message (`status brackets are not
  supported; use a **Status** field`).

**D2. What a structural read returns for partial coverage.**
- (a) **Chosen:** add `"complete": false` and
  `"incomplete": [{"path", "codes", "reason"}]` to `ls`/`show`/`find` results
  only when something is unrepresented. Brief text adds one line
  (`incomplete: staging/observations.yaml (ARA218)`). `status.complete` keeps
  its all-or-nothing meaning.
- (b) Rely on the existing `diagnostics.warnings` and add nothing. Smaller, but
  an agent must know which codes mean lost data.

## Implementation steps

1. Part 1 steps 1–4 with core unit tests for `entry_heading` (all separators,
   `E01-x` not split, empty title, wrong prefix) and merge identity tests for a
   dash-separated `E`/`H` heading.
2. D1 as decided, with a test.
3. Part 2 steps 6–10. Update `agent_tolerant_reads.rs`: its `ARA217`
   observations-shape case becomes a read-through case, and a node-scoped
   `ARA226` case keeps refusing.
4. Remove the `#128` reproduction header once all four tests pass.
5. Docs: `docs/agent-cli.md` (refusal table and the `ARA217`… paragraph),
   ARA format notes for accepted heading forms.
6. Bump to 0.1.34, `CHANGELOG.md` entries under Fixed/Changed.
7. Rerun the #128 audit ARAs and `nanogpt-speedrun` with `ara ls`; record which
   remain refused and why (expected: only the ARA100/105/112 ones).

## Verification and acceptance

- `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` pass.
- The four reproduction tests pass.
- A merge test shows a dash-headed `E01` matches the reader's `E01`.
- Step 7: no audited ARA is refused for `ARA217`/`ARA218` alone.
