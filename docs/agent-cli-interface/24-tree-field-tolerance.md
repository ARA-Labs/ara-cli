# 24: one mistyped tree value no longer fails the whole tree

Shipped in workspace 0.1.35 for [#130](https://github.com/ARA-Labs/ara-cli/issues/130),
stacked on [23](23-optional-layer-tolerance.md). Parse rules are in
[stage-1](../stage-1-core-parse-validate.md#validation-severity); the rule
table is in [stage-5](../stage-5-check.md).

## Problem

After #128, two research ARAs still failed to load their whole exploration
tree (`ARA100`), so every structural read refused them:

| ARA | Source | YAML value | Error |
|---|---|---|---|
| DissClaimer | `evidence:` item `- 7258910` (a short git SHA) | integer | `did not match any variant of untagged enum Evidence` |
| MSI | `alternatives:` item `- Build on Debian 12 … (rejected: its glibc is above the floor).` | one-key mapping | `expected string scalar` |

About 4% of 7-character SHAs are all digits, so any ARA citing commits in
`evidence:` eventually hits the first case.

Plain `String` fields already took a numeric or boolean scalar's source
spelling (`1e5`, `0x1F`, `true`). Only `evidence:`, an untagged enum, did not:
untagged matching buffers the parsed number, not its text.

## Design

### `evidence` scalars

`Evidence` has a hand-written `Deserialize`. List items deserialize as
`String`, so `[be13e96, 7258910, 0123456]` keeps every spelling with no
warning, like any text field. A bare numeric scalar (`evidence: 0123456`) has
lost its spelling by the time a visitor sees it, so it is refused and the tree
is re-read on the recovery path below.

### Recovery path

`flat_yaml::parse` first parses a shallow tree with serde-saphyr as before.
Only if that fails does it retry through the `flat_yaml` arena, and it takes
the arena result only when the arena recovered at least one value; any other
failure keeps the original `ARA100`. Deep trees (over 64 levels) always use
the arena. Trees that loaded before load byte-identically.

In the arena, a value of a node text field (`TEXT_FIELDS`: `title`,
`description`, body fields, `alternatives`, `evidence`, `source_refs`, …) that
YAML did not type as a string is read as its exact source text: a plain
scalar, a mapping such as `a (rejected: b)`, or a list in a scalar field
(`choice: [a, {b: c}]`). The text runs from the value's start to its last
descendant, plus a closing flow bracket; comments are excluded. Each
recovery warns `ARA230 non-text-tree-field` at
`trace/exploration_tree.yaml:line`. Identity fields (`id`, `type`, `parent`,
`also_depends_on`) and non-text fields (`isolated`, `same_as`, `artifacts`,
`concepts`) are not in `TEXT_FIELDS` and stay strict.

The plan placed recovery in `flat_yaml` spans for every tree. Shallow trees
never reach that code (serde-saphyr parses them directly), so recovery is a
failure-only fallback instead, which also guarantees no change for trees that
already loaded.

### Decisions

- **D1 (a):** `ARA230` does not refuse reads or mark them `incomplete`. The
  recovered text is exactly what the author wrote, so the node is fully
  represented; reads exit 0 with the warning in `diagnostics`.
- **D2:** `ARA008` covers the tree only.

### `ARA008 unquoted-tree-scalar`

The format rule reports each `ARA230` recovery. A one-line value gets a
`ReplaceInLine` fix that single-quotes it and doubles inner `'`; a multi-line
value is reported without a fix. The fix guard accepts a candidate only if it
adds no error, leaves the normalized manifest equal, and removes an `ARA230`.
The plan also proposed quoting numeric `evidence:` list items; those now read
correctly without a warning, so `ARA008` follows the parser's recoveries
instead of a separate scan.

## Verification

- `crates/ara-core/tests/tree_field_tolerance.rs`: list and bare evidence
  spellings, nulls, mappings and flow lists, multi-line values, strict
  identity fields, the deep-tree arena path (plain text not reported), and
  `ARA008` lint plus `fix_dir` with manifest equality.
- `crates/ara-cli/tests/agent_tree_field_tolerance.rs`: the two #130
  reproductions (failing on 0.1.34) and a spelling guard.
- `agent_reads.rs`: the identity-lookup fixture that relied on a mapping in
  `description` failing now uses `isolated`, which stays strict.
- Real ARAs, 0.1.34 → 0.1.35: DissClaimer and MSI go from `invalid_artifact`
  (`ARA100`) to readable. DissClaimer reports no `ARA230`; MSI reports one at
  line 5859, and `ara check --fix` on a copy quotes exactly that line.
