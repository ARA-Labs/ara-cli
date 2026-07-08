# ARA Format — Feedback for the Maintainer

Discussion notes for the Agent-Native Research Artifact (ARA) maintainer,
collected while building `bara`'s Stage-1 parser (`ara-core`: parse
`trace/exploration_tree.yaml` + `logic/claims.md` into one normalized
`Manifest`). These are format-design issues that make a **typed, deterministic,
tolerant** parser harder to define than it should be. Each item lists the
concrete pain, an example from the official corpus (or a nearby real artifact),
and a requested resolution.

Corpus inspected: `Agent-Native-Research-Artifact/examples/minimal-artifact`,
`.../resnet-ara-example` (canonical), plus two real hand-authored artifacts
(`SOULFuzz`, `LoongDoc`) that surfaced additional drift.

---

## 1. No published schema or spec

The format is defined only by examples. There is no JSON Schema, no prose spec,
no `SCHEMA.md`. A consumer must reverse-engineer field names, types, and
cross-reference rules from a handful of `.yaml`/`.md` files, and has no way to
tell an intentional field from an incidental one, or a required field from an
optional one.

**Requested:** publish a versioned schema (JSON Schema or equivalent) for
`exploration_tree.yaml` and `claims.md`, with a `schema_version` field on the
artifact so consumers can pin and migrate.

## 2. Two root dialects (`tree:` vs `root:`)

Roots appear as either a list under `tree:` or a single node under `root:`.
Both canonical examples use `tree:`; `SOULFuzz` uses `root:`. A parser must
model both and normalize. There is no stated reason for two forms.

```yaml
# minimal-artifact, resnet
tree:
  - id: N01
    ...
# SOULFuzz
root:
  id: RQ
  ...
```

**Requested:** pick one canonical root form (recommend `tree:` always, with a
single-element list for a single root). If both must exist, document that
exactly one of `tree:`/`root:` is present, and that both-present / neither is
invalid.

## 3. The node display label has no single, guaranteed key

Most nodes carry `title:`, but the label is sometimes under a different key.
`SOULFuzz`'s root node has `question:` and no `title:`. Decision nodes carry
`choice:` alongside (or instead of) a title. A parser can't assume `title` is
present, so it must invent a fallback precedence (`title` → `question` →
`choice` → `id`) that is nowhere specified.

**Requested:** guarantee a single label key (`title:`) on **every** node,
regardless of type.

## 4. Type-specific body fields are unbounded and undocumented

Each node `type:` carries different body keys, and the set is neither closed nor
documented:

| type | body keys seen |
|------|----------------|
| question | `description` |
| experiment | `description`, `result` |
| dead_end | `description`, `why_failed` |
| decision | `description`, `choice`, `alternatives`, `rationale` |
| insight | `description` |

Plus cross-cutting metadata: `support_level`, `source_refs`. Without a per-type
field list, a typed parser either (a) models a guessed set and silently drops
anything new, or (b) captures everything opaquely and loses type structure.

**Requested:** document the field set per node type, and mark which are
required vs optional.

## 5. `dead_end` failure-reason key drifts (`why_failed` vs `reason`)

The same semantic field is spelled `why_failed` in the canonical resnet example
and `reason` in `SOULFuzz`. Even if only canonical output is "supported," this
shows the format allows drift that a consumer can't reconcile without an alias
table.

```yaml
# resnet dead_end
why_failed: >- ...
# SOULFuzz dead_end
reason: "Same max-safety and lift-rate; ..."
```

**Requested:** one canonical key per semantic field; if aliases are permitted,
publish the alias table.

## 6. `evidence:` mixes typed references and free prose in one list

`evidence:` is a heterogeneous list whose elements are sometimes claim ids and
sometimes free-text strings:

```yaml
evidence: [C01, "Table 2"]        # minimal-artifact: a claim id + a prose note
evidence: "Table 3 ablation ..."   # elsewhere: a bare string
```

A parser must inspect each element to decide whether it is a resolvable
reference (`C\d+`) or opaque prose, and handle the scalar-vs-list shape. This
conflates a machine-checkable reference with a human note.

**Requested:** separate the fields — e.g. `claims: [C01]` (typed refs) and
`evidence_notes: ["Table 2"]` (prose) — and always use list form.

## 7. Claims live in Markdown with semi-structured bullets

`logic/claims.md` is prose Markdown, not machine-readable YAML/JSON. Claim data
is encoded as `## C01: ...` headers plus `- **Key**: value` bullets. Parsing it
reliably requires header-regex + bullet heuristics, and the header style itself
drifts:

```markdown
## C01: Attention-only architecture achieves SOTA   # canonical (colon)
## C01 — Expert rubric + per-scenario context ...    # SOULFuzz (em dash)
```

Bullet keys (`Statement`, `Status`, `Falsification criteria`, `Proof`,
`Dependencies`, `Tags`, plus `Evidence basis`, `Interpretation`) are not
guaranteed present or consistently named.

**Requested:** provide claims as structured data (`claims.yaml`/`claims.json`)
alongside or instead of the Markdown, or fix a strict Markdown grammar (single
header style, closed bullet-key set).

## 8. `E##` evidence ids are referenced but never defined

Claims reference evidence artifacts via `Proof: [E01]`, but no `evidence/`
registry defines what `E01` is. In the canonical corpus, `minimal-artifact` has
no `evidence/` directory at all, and `resnet-ara-example` has `figures/` +
`tables/` but no `E##`-keyed index. Every `E##` reference is dangling by
construction — a consumer cannot resolve or validate them.

**Requested:** define an evidence registry (`evidence/index.yaml` keyed by
`E##`) so proof references resolve, or drop the `E##` layer until it does.

## 9. ID conventions are unspecified

Ids appear as `N01`, `C01`, `E01` in canonical, but nothing states the grammar:
is `C01` distinct from `C1` or `c01`? Is leading/trailing whitespace
significant? Is the namespace prefix (`N`/`C`/`E`) mandatory? Without a rule,
duplicate-detection and broken-reference behavior become accidental.

**Requested:** specify the id grammar (recommend `^[NCE]\d+$`, case-sensitive,
trimmed, no zero-padding ambiguity) and that ids are unique within their
namespace.

## 10. Source-order significance is unspecified

`children:` ordering and sibling ordering appear to encode narrative reading
order (in resnet, `N06 insight → N07 decision → ...` reads as a story). But the
format never states whether array order is semantically meaningful or
incidental. A consumer that reorders (e.g. sorts by id for determinism) may
silently destroy the intended narrative sequence.

**Requested:** state explicitly whether `tree:`/`children:` order is
significant. `bara` currently **preserves source order** on the assumption that
it is — please confirm or correct.

## 11. Cross-references create a DAG, but acyclicity is not guaranteed

`children:` alone is a tree, but `also_depends_on:` (and `verifies:` in some
artifacts) add cross-edges that can point back up and create cycles. The format
calls the structure a DAG but provides no acyclicity guarantee, so a layout
consumer (layered/dagre) must defensively detect cycles.

**Requested:** guarantee (and ideally validate upstream) that the combined
`children` + `also_depends_on` graph is acyclic, or document that cycles are
permitted so consumers plan for them.

## 12. Three id namespaces, no documented cross-reference rules

Nodes (`N##`), claims (`C##`), and evidence (`E##`) form three id namespaces,
referenced across files: nodes → claims (`evidence:`, `verifies:`), claims →
claims (`Dependencies:`), claims → evidence (`Proof:`). The legal cross-reference
directions and which file owns each namespace are never documented.

**Requested:** document the reference graph — which namespace each field points
into, and which file is the authority for each id space.

---

## Summary of asks (priority order)

1. Publish a versioned schema + `schema_version` (items 1, 4).
2. Move claims to structured data, or fix a strict grammar (item 7).
3. Define or drop the `E##` evidence registry (item 8).
4. Guarantee a single label key and one canonical key per field (items 3, 5).
5. Specify id grammar, namespaces, and cross-reference rules (items 9, 12).
6. State root-form and source-order contracts (items 2, 10, 11).
7. Split typed refs from prose in `evidence:` (item 6).

`bara` ships tolerant workarounds for all of the above (canonical-only scope,
opaque capture of unknown fields, lenient Markdown claim parsing, source-order
preservation, cycle detection). None of the workarounds are free, and each is a
place the parser can silently diverge from author intent. A published schema
would let the parser be strict where it currently must guess.
