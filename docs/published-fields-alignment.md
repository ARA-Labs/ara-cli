# Published ARA node metadata & evidence fields

Design record for the published-fields alignment shipped in `0.1.15` (issue
[#75](https://github.com/ARA-Labs/ara-cli/issues/75)). It supersedes #12
(per-node narrative field) and lands the detail-pane slice of #64. Companion
docs: [`stage-5-check.md`](stage-5-check.md) (the ARA005–ARA007 lint rules),
[`hub-parity.md`](hub-parity.md) (the viewer surfaces), and
[`manifest-schema.md`](manifest-schema.md) (the wire contract).

## Problem / background

`ara check` was scoped canonical-only against `minimal-artifact` +
`resnet-ara-example`. The published format — defined by
`ARA-Labs/Agent-Native-Research-Artifact` (`ara-schema.md`,
`exploration-tree-spec.md`) and exercised by the canonical
`examples/the-ara-of-ara` artifact — uses node metadata and narrative fields
the CLI did not model. Every one of the 116 nodes in the canonical tree
emitted `provenance` and `timestamp` unknown-field warnings, and pivot /
deep-dive experiment nodes emitted more — 244 unknown-field warnings in
total. Downstream artifacts had
to delete research provenance to satisfy the checker — a runtime/schema
mismatch, not bad data.

A frequency-verified inventory of the canonical tree drove the field
selection: `provenance` / `timestamp` are universal (116/116 nodes);
`status`, `exploration`, `outcome` appear only on experiments;
`prior_direction`, `new_direction`, `reason` only on pivots; `lesson` on both
dead ends and pivots. Notably, the spec's `exploration-tree-spec.md` documents
pivot fields `from` / `to` / `trigger` while its own canonical example uses
`prior_direction` / `new_direction` / `reason` — the published example wins.

## Data model (`ara-core`)

- `Node` gains `provenance: Option<String>` and `timestamp: Option<String>` —
  free strings, carried verbatim, `skip_serializing_if`-None so old manifests
  round-trip unchanged. **No vocabulary validation** (D2): the published tag
  set (`user | ai-suggested | ai-executed | user-revised`) is advisory and
  evolving; fail-closed applies to unknown *keys*, not values.
- `NodeFields::Pivot` is `{prior_direction, new_direction, reason, lesson}`,
  replacing the pre-0.1.15 `{from, to, trigger}` triple. This is a
  **logical wire break** (not geometry — see
  [`manifest-schema.md`](manifest-schema.md)); the CHANGELOG flags it as
  breaking for manifest consumers.
- `NodeFields::Experiment` gains `exploration`, `outcome`, `status`. `status`
  is **experiment-scoped** (T-B); `provenance` / `timestamp` stay universal
  (116/116 evidence). Note: the planning-time inventory recorded both
  `status` uses as experiments, but the post-implementation check of the
  canonical tree shows `status: resolved` on two *decision* nodes (N122,
  N123) and `lesson` on the two deep-dive *experiments* (N95, N96). The
  approved T-B/T-A contract stands: those nodes now emit wrong-kind drop
  warnings (loud, non-strict) instead of silently discarding the values.
  Widening `status` / `lesson` to more kinds is a spec-level decision for
  upstream, not something this CLI infers unilaterally.
- `NodeFields::DeadEnd` is unchanged (it already modeled
  `hypothesis` / `failure_mode` / `lesson` / `why_failed`).
- `ExhibitKind` gains `Result` and `Proof` (D3). Deliberately no
  `#[non_exhaustive]` — in-repo exhaustiveness is a feature; revisit before
  the first crates.io publish.

## Wrong-kind drop warnings (T-A)

`project_kind` previously warned about dropped body fields only for *unknown*
types; a modeled body field on the wrong *known* kind was silently discarded
(`lesson` on a pivot was one instance — the regression this change pins).
`project_kind` now warns whenever a modeled body field is present on a kind
that does not project it, for every kind, so nothing is lost silently. New
warnings may appear on the real corpus (already warning-heavy) — they are
fail-closed signal, non-strict.

## Pivot alias migration: ARA005–ARA007 (D1)

`from` / `to` / `trigger` are **not** serde-modeled — they were removed from
`RawNode`. On a pivot node they fall into `extra` (unknown-field warning,
value dropped), exactly the ARA002/ARA003 pattern. New kind-scoped, text-level
lint rules canonicalize them: **ARA005** (`from:` → `prior_direction:`),
**ARA006** (`to:` → `new_direction:`), **ARA007** (`trigger:` → `reason:`).
`fix.rs`'s existing `guard_alias` recovers the value (a `None → Some` manifest
delta); a node carrying both an alias and its canonical key is guard-rejected
and recorded as a `SkippedFix` — no write, no silent ambiguity. Aliases are
consumed by rename and never emitted, so the `--fix` fixpoint is idempotent by
construction. Rejected alternatives: serde-modeled aliases with a parse.rs
rewrite (parse.rs has no text spans, and modeled aliases would warn about
nothing) and the serde `alias` attribute (silently renames with no diagnostic
and no fix hook).

Deliberate name collision: `reason:` is canonical on a `pivot` (ARA007's
rename target) and an alias of `why_failed:` on a `dead_end` (ARA002). The
rules are kind-scoped — a key is only flagged when it sits directly on a node
of the matching kind — so the same spelling is treated correctly per node
type.

## Evidence categories

Discovery enumerates `evidence/figures/`, `evidence/proofs/`,
`evidence/results/`, then `evidence/tables/` — a fixed category order, each
sorted — one `Exhibit` per `*.md`, bodies verbatim. Index enrichment is
unchanged (column-name tolerant across the corpus's README header variants;
`results` / `proofs` rows resolve through the same parse). A basename
duplicated across categories keeps both bodies and warns once (T-C); the
escalation to category-qualified exhibit ids is `T-EXHIBIT-QUALIFIED-IDS` in
TODOS.md, with the warning as the tripwire. `evidence/logs/` is excluded — it
is a pointer-resolution index, not Markdown body content.

## Viewer (`ara-viewer`)

The detail pane renders, when present and omitted when absent: `provenance`
and `timestamp` header pills (following the `support_level` pill idiom); the
pivot narrative as typed fields (`prior direction`, `new direction`, `reason`
[primary], `lesson`); and the experiment narrative (`what it did` [result],
`exploration`, `outcome`, `status`). `exhibit_kind_label` gains `result` /
`proof`. The `source_refs` block is labelled **Sources** (renamed from
"Provenance") so it does not collide with the new provenance metadata. No
layout or filter changes.

## Fail-closed contract

No generic allowlist. Three independent guards:

- a genuinely unknown key (`bogus_field`) → warns non-strict, errors under
  `--strict`;
- a known body field on the wrong kind (`exploration` on a decision, `status`
  on a question) → wrong-kind drop warning (T-A);
- a legacy pivot alias (`from:`) → unknown-field warning plus a fixable
  ARA005–007 diagnostic; the value is recovered only by `--fix`.

The in-repo `published-fields` fixture is a no-op under `ara check --strict
--fix`: byte-identical across runs, zero warnings. The upstream canonical tree
is not: per the T-A disclosure above it currently emits 4 wrong-kind drop
warnings (`status: resolved` on decision nodes N122/N123, `lesson` on
experiment nodes N95/N96), which become errors under `--strict`, pending the
upstream spec decision on widening `status` / `lesson`.

## What is deferred

- `trace/sessions/*.yaml` provenance on `ai_actions` — a separate document
  type.
- Corpus-only fields (`thinking`, `method`, `justification`, `ara-2.0`
  streams) — the `T-REAL-CORPUS` remainder.
- Category-qualified exhibit ids — `T-EXHIBIT-QUALIFIED-IDS`; the duplicate-id
  warning is the tripwire.
- Provenance vocabulary validation — rejected (D2); revisit only if the spec
  freezes the tag set.
- `evidence/logs/` — pointer-resolution index, not body content.
- `#[non_exhaustive]` on `ExhibitKind` — revisit before the first crates.io
  publish.
- The `E##` evidence registry — `T-EVIDENCE`, blocked upstream.
