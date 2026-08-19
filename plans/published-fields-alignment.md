# Plan: Align ara-cli with published ARA node metadata and evidence fields

**Issue:** #75 · **Branch:** `feat/published-fields-alignment` · **Date:** 2026-08-19
**Status:** DRAFT — awaiting human review before implementation.

## Problem background

`ara check` 0.1.11 was scoped canonical-only against `minimal-artifact` +
`resnet-ara-example`. The published format — defined by
`ARA-Labs/Agent-Native-Research-Artifact` (`skills/compiler/references/ara-schema.md`,
`exploration-tree-spec.md`) and exercised by the canonical
`examples/the-ara-of-ara` artifact — uses node metadata and narrative fields the
CLI does not model. Every one of the 116 nodes in
`the-ara-of-ara/trace/exploration_tree.yaml` currently emits two unknown-field
warnings (`provenance`, `timestamp`); pivot and deep-dive experiment nodes emit
more. Downstream artifacts must delete research provenance to satisfy the
checker. This is a runtime/schema mismatch, not bad data.

## Research findings (source: cloned spec repo, 2026-08-19)

### Field inventory of the canonical tree (116 nodes, frequency-verified)

| field | count | where | in ara-cli today? |
|---|---|---|---|
| `id`, `type`, `title` | 116 | all nodes | yes |
| `provenance` | 116 | all nodes | **no** → unknown-field warning |
| `timestamp` | 116 | all nodes (ISO date string) | **no** → unknown-field warning |
| `evidence` | 96 | prose refs (rarely `C##`) | yes (evidence_notes) |
| `choice`, `alternatives` | 45 | decision | yes |
| `result` | 44 | experiment | yes |
| `also_depends_on` | 33 | DAG cross-edges | yes |
| `lesson` | 25 | dead_end **and pivot** | dead_end only → warns on pivot |
| `hypothesis`, `failure_mode` | 21 | dead_end | yes |
| `children` | 20 | nesting | yes |
| `status` | 2 | experiment (`resolved`) | **no** |
| `prior_direction`, `new_direction`, `reason` | 2 | **pivot** | **no** |
| `exploration`, `outcome` | 2 | **experiment** (deep-dive nodes N95/N96) | **no** |
| `description` | 2 | question | yes |

Notably absent from the canonical tree: `support_level`, `source_refs` (0 uses;
already optional in the model), and the spec's pivot fields `from`/`to`/`trigger`
(the spec's `exploration-tree-spec.md` contradicts its own canonical example —
the published example wins per issue #75).

### Provenance vocabulary (from `skills/research-manager/SKILL.md`)

Closed tag set: `user | ai-suggested | ai-executed | user-revised`. The canonical
tree uses only `user`. Sessions files (`trace/sessions/*.yaml`) use the same
tags on `ai_actions` — sessions are out of scope for this issue.

### Evidence categories

`ara-schema.md` defines four evidence subdirs: `tables/`, `figures/` (both
mandatory), `results/` (per-node run records), `proofs/` (derivations), plus
`logs/` (pointer index only). `ara-core`'s `read_evidence`
(`crates/ara-core/src/evidence.rs:344`) discovers only `figures/` and `tables/`
today. Issue #75 asks for the four Markdown-body categories (`logs/` excluded —
it is pointer-resolution, not body content).

### Related deferred work this partially resolves

- T-REAL-CORPUS: this issue covers the *published* subset (provenance/timestamp/
  status/pivot narrative). Corpus-only fields (`thinking`, `method`,
  `justification`, `ara-2.0` streams) remain deferred.
- #12 (per-node narrative field): superseded by this broader contract — close on
  merge.
- #64 (viewer rendering of narrative): the detail-pane slice lands here.

## Proposed solution

Model the published fields explicitly, fail-closed on everything else.

### 1. Raw + manifest model (`ara-core`)

- `RawNode` (`schema.rs`): add `provenance`, `timestamp`, `status` (universal
  optional strings) and `prior_direction`, `new_direction`, `reason`,
  `exploration`, `outcome`, plus `lesson` accepted on pivot.
- `Node` (`manifest.rs`): add `provenance: Option<String>`,
  `timestamp: Option<String>`, `status: Option<String>` — all
  `skip_serializing_if = "Option::is_none"`.
- `NodeFields::Pivot`: replace `{from, to, trigger}` with
  `{prior_direction, new_direction, reason, lesson}`. Keep `from`/`to`/`trigger`
  as **legacy aliases** in `RawNode`; `--fix` canonicalizes
  `from→prior_direction`, `to→new_direction`, `trigger→reason` (see D1).
- `NodeFields::Experiment`: add `exploration`, `outcome`.
- `NodeFields::DeadEnd`: unchanged.
- `ExhibitKind`: add `Result` and `Proof` variants (additive to the wire type).

### 2. Evidence discovery (`evidence.rs`)

Enumerate `figures/`, `proofs/`, `results/`, `tables/` in that fixed order
(sorted within each), one `Exhibit` per `*.md`, bodies verbatim. Same
index-enrichment rules as today; `results`/`proofs` rows resolve through the
same column-tolerant index parse.

### 3. Viewer (ara-viewer)

Detail pane renders, when present and omitted when absent: provenance,
timestamp, status chips; pivot narrative (prior/new direction, reason, lesson);
experiment narrative (exploration, outcome). No layout/filter changes.

### 4. `--fix` and canonicalization

Published fields are canonical: `--strict --fix` on published input is a no-op,
byte-identical across two runs. Alias canonicalization (D1) is the only rewrite
added, and is idempotent by construction (aliases are consumed, never emitted).

### 5. Fail-closed guarantee

No generic allowlist. A sibling fixture with a genuinely unknown key
(`bogus_field`) still warns non-strict and errors under `--strict`.

## Implementation steps

1. `schema.rs`: new `RawNode` fields + unit tests (published fields leave
   `extra` empty; alias fields land in their slots).
2. `manifest.rs`: `Node` + `NodeFields` + `ExhibitKind` changes; serde
   round-trip tests.
3. `parse.rs`: project new fields per kind; alias→canonical mapping with a
   one-time `fix`-recorded rewrite; unknown-kind body-field warning list
   updated (`body_field_names`).
4. `evidence.rs`: four-category discovery + tests.
5. Viewer: detail-pane rendering + browser test (`tests/web.rs`).
6. Fixtures: `tests/fixtures/published-fields/` (reduced the-ara-of-ara tree
   slice covering every new field) and `published-fields-unknown/` (sibling
   with one bogus key).
7. Idempotency test: canonical fixture through `check --strict --fix` twice,
   byte-compare.
8. WASM golden test updated (native≡wasm byte-parity) — layout unaffected
   (no geometry change), but the manifest JSON gains fields, so regen goldens.
9. Docs: update `docs/manifest-schema.md`, close the loop in
   `docs/ara-format-feedback.md` §13 (published subset now modeled).
10. `Cargo.toml` patch bump (0.1.11 → 0.1.12) + CHANGELOG entry under
    `[Unreleased]`.
11. Close #12 as superseded in the PR body; reference #64's rendering slice.

## Open decisions (need human call)

- **D1 — pivot alias direction.** Plan assumes published
  (`prior_direction`/`new_direction`/`reason`) is canonical and
  `from`/`to`/`trigger` become legacy aliases rewritten by `--fix`. Alternative:
  accept both as peers with no rewriting (simpler, but two names for one slot —
  the thing ask 4 in ara-format-feedback.md argues against). Recommend the
  alias direction as written.
- **D2 — provenance value validation.** Plan stores `provenance` as a free
  string (no enum) and does not warn on out-of-vocabulary values. Alternative:
  warn when not in `user|ai-suggested|ai-executed|user-revised`. Recommend
  free string — the vocabulary is advisory and closed-set validation risks
  rejecting future tags.
- **D3 — `ExhibitKind` growth.** Add `Result`/`Proof` variants (typed, additive)
  vs mapping both to `Other` (no wire change, less info). Recommend variants.

## Evaluation

How we will judge whether this change achieves the contract, with a baseline
measured on current `main` (0.1.11, 2026-08-19) against a fresh clone of
`ARA-Labs/Agent-Native-Research-Artifact@main`.

### Baseline (measured)

`cargo run -- check examples/the-ara-of-ara` today:

```
PASS — 0 error(s), 246 warning(s), 0 fixable issue(s)
```

Warning composition:

| warning | count | covered by this issue? |
|---|---|---|
| unknown field `timestamp` | 116 | yes |
| unknown field `provenance` | 116 | yes |
| unknown field `status` / `reason` / `prior_direction` / `new_direction` / `exploration` / `outcome` | 2 each (12) | yes |
| redundant `also_depends_on` on ancestor | 1 | no (pre-existing, unrelated) |
| evidence index row with no body file | 1 | no (pre-existing, unrelated) |

### Success metrics

1. **Primary: unknown-field elimination on the canonical artifact.** Re-run
   `ara check` on the same clone after the change. Target: **244 → 0**
   unknown-field warnings; total warnings **246 → 2** (only the two
   pre-existing unrelated warnings remain). This is the headline number for the
   PR description.
2. **Preservation, not just acceptance.** Inspect the emitted
   `manifest.json` for the canonical artifact: all 116 nodes carry
   `provenance` + `timestamp`; pivot nodes carry `prior_direction` /
   `new_direction` / `reason` / `lesson`; N95/N96 carry `exploration` /
   `outcome` / `status`. This catches silent drops — today `lesson` on a pivot
   node is *not* warned but *is* discarded by `project_kind`, so warnings-only
   evaluation would miss it.
3. **Real-corpus delta (scope control).** Run the vendored
   `ara-paperbench` subset (`crates/ara-core/tests/corpus`) before/after and
   diff warning counts by field name. Expected: `provenance` / `status` /
   `timestamp` warnings disappear; `thinking` / `method` / `justification` /
   `failure_mode`-family warnings **remain** (T-REAL-CORPUS, out of scope). A
   change that accidentally silences those means we added an allowlist — a
   fail-closed violation.
4. **Fail-closed check.** The `published-fields-unknown/` sibling fixture (one
   `bogus_field` key) must still warn non-strict and error under `--strict`.
5. **Idempotency.** `ara check --strict --fix` twice on the
   `published-fields/` canonical fixture → byte-identical tree YAML and an
   unchanged second-run diff. Include one aliased node
   (`from`/`to`/`trigger`) to prove alias canonicalization fires once and
   stabilizes.
6. **Cross-target parity.** Regenerated native≡wasm goldens match byte-for-
   byte; the wasm viewer renders the new fields (browser test asserts on the
   detail pane for a node with metadata + narrative).

### Acceptance-criteria mapping (issue #75)

| #75 criterion | evaluated by |
|---|---|
| Representative published-field fixture parses without unknown-field diagnostics | metric 1 + fixture test (step 6) |
| Native parse, normalized JSON, JSON/WASM, strict/fix, evidence, browser tests cover accepted fields | steps 1–5, 8; metrics 2, 6 |
| Canonical published input byte-identical after two `--strict --fix` runs | metric 5 |
| Sibling unknown-field fixture fails in strict mode | metric 4 |
| `cargo test --workspace`, fmt, clippy, WASM, browser tests pass | step 11 in CI |
| Contract documented, released under immutable version | step 9–10; tag `v0.1.12` after merge |

### Downstream smoke check

LARA's gate is non-strict `ara check` with zero errors (issue #75, Downstream
Policy). After merge, run the released binary against `EYH0602/lara`'s ARA
directory and confirm: exit 0, zero errors, and no unknown-field warnings for
any published field. Report the before/after warning count in the PR.

## Verification

- `cargo test --workspace`, `cargo clippy --all-targets`, `cargo fmt --check`
- WASM tests (`wasm-pack test` per CI) + viewer browser tests
- New fixture tests: published-fields fixture → zero unknown-field diagnostics;
  unknown sibling → strict-mode error
- Idempotency: two `check --strict --fix` runs on the canonical fixture,
  byte-identical
