# 23: read real ARAs despite optional-layer warnings

Shipped in workspace 0.1.34 for [#128](https://github.com/ARA-Labs/ara-cli/issues/128).
Command behavior is documented in [agent-cli.md](../agent-cli.md#incomplete-optional-layers);
heading rules in [stage-1](../stage-1-core-parse-validate.md#entry-headings).

## Problem

`ara` 0.1.33 refused `ls`, `show` and `find` on every research ARA over 40
nodes in the #128 audit, and on `ara-paperbench/nanogpt-speedrun`, although
plain file reads worked. The agent-CLI scaling evaluation (E2) pads benchmarks
with these ARAs, so its CLI condition measured refusals. Two causes concerned
only optional agent layers:

- `markdown_entries` split experiment and heuristic headings on the first `:`
  only. `## E01 — Baseline` dropped the entry and warned `ARA217`, while claims
  already accepted spaced dashes.
- `validity::read_refusal` turned any `ARA217`/`ARA218` warning into an
  `incomplete_artifact` refusal of every structural read. One duplicate
  observation ID hid a 900-entry tree.

`ARA100`, `ARA105` and `ARA112` in the same audit are real record errors and
keep refusing.

## Design

### One entry-heading parser

`ara_core::entry_heading(heading, prefix)` accepts a canonical `{prefix}##`
ID, then `:` or a dash (`-`, U+2013, U+2014) with whitespace on both sides,
then a nonempty title. `claim_heading` is its `C` case.
`typed_entry_heading` also accepts a bracketed status before the separator
(`## E02 [in progress]: Title`); the reader stores it as the entry's `status`
unless a `**Status**` field exists. Claims take no bracket.
`native_entry_heading` picks the prefix from the heading's first character.

The reader (`markdown_entries`), merge identity (`heading_entry`,
`identified_title`) and the CLI heading lookup (`agent::headings::heading_id`)
all use it, so no layer sees an ID the others do not. Writes needed no change:
`write::logic::heading_id` splits on `:` or whitespace and already returned
`E01` for both new forms, and merge's entry inventory comes from the same
reader. A heading that starts like an entry ID but does not parse still drops
the entry; its `ARA217` now carries `path:line` and the heading text.

### Optional layers read through

`validity::optional_layer` classifies an `ARA217`/`ARA218` warning by its path
(without `:line`): `staging/observations.yaml`, `trace/taste_log.yaml`,
`trace/sessions/*`, `logic/experiments.md`, `logic/solution/heuristics.md` and
`PAPER.md.knowledge_paths`. These no longer refuse. Node-scoped warnings
(`ARA222`, `ARA226`, `ARA217` on `nodes[…]`) still refuse, because they change
what a tree node says.

`validity::with_coverage` adds `complete: false` and
`incomplete: [{path, codes, reason}]` to `ls`, `show`, `find` and `status`
results when any optional-layer file is affected, and nothing otherwise. Brief
text prints `incomplete: <path> (<codes>)` after the diagnostics summary.

`ls`, `show` and `find` exit 0. An implementation first exited 1, reusing
the `status` rule that `complete: false` exits 1; that rule existed because
only `status` emitted `complete`. Exit 1 on a read means the request failed:
partial multi-address `show` exits 1 because an address the caller asked for
did not resolve, and the reader harness counts those exits as agent failures.
Here the request succeeds (`show N01` returns N01 in full), the gap is often
in a file the caller never asked about, and `ARA218` loses nothing at all, so
exit 1 would report artifact defects as agent errors. Reads already return
other warnings with exit 0; the `incomplete` report keeps the gap explicit, so
it is not reported as plain success. `status` still exits 1 when
`complete` is false, because reporting completeness is its job. The shared
skill access guide and the reader skill say that `incomplete` names files
that were not fully read while the rest of the output is valid.

`status` keeps `complete` all-or-nothing, but `counts` and `next_ids` stay
available unless an error or node-scoped warning exists. The next ID of an
affected prefix (`O`, `T`, `E`, `H`) is null with `next_id.incomplete_layer`,
because a dropped entry may already hold that number. Writes validate in
`ara-core` and never used `read_refusal`, so they are unchanged.

## Verification

- `crates/ara-cli/tests/agent_layer_tolerance.rs`: the three #128
  reproductions (failing on 0.1.33), bracket status, line-numbered warnings,
  duplicate records, non-text `promoted_to`, brief output, `status`, and a
  node-scoped refusal.
- `agent_tolerant_reads.rs`: the old observations-shape refusal case became
  node-scoped `ARA222` and pivot `ARA217` refusal cases.
- `entry_heading` unit tests; merge helper unit tests; a merge test showing
  dash and bracket headings merge exactly like colon headings. That merge test
  also passes on 0.1.33, because merge identity already came from the reader
  and write heading lookups; it guards the shared behavior.
- Local real-ARA rerun, 0.1.33 → 0.1.34 (`ara ls --json`):

| ARA | 0.1.33 | 0.1.34 |
|---|---|---|
| `ara-paperbench/.../nanogpt-speedrun` | `incomplete_artifact` ARA217 | readable, exit 0 |
| `Lara/ara` | `incomplete_artifact` ARA217, ARA218 | readable; `staging/observations.yaml` incomplete |
| `DissClaimer/ara`, `MSI/ara` | `invalid_artifact` ARA100 | unchanged (out of scope) |
| `mandate/ara`, `mandate-eval/ara` | `invalid_artifact` ARA105 | unchanged (out of scope) |
| `rsi-bench`, `rsi-chip-bench`, `sierra` | readable | readable |

The four other ARAs from the #128 audit were not available locally.
