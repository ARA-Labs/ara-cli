# Plan: Align ara-cli with published ARA node metadata and evidence fields

**Issue:** #75 · **Branch:** `feat/published-fields-alignment` · **Date:** 2026-08-19
**Status:** REVIEWED — eng review 2026-08-19 approved with amendments (see Resolved decisions + review report at bottom). Ready to implement.

## Problem background

`ara check` was scoped canonical-only against `minimal-artifact` +
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
| `lesson` | 25 | dead_end **and pivot** | dead_end only → **silently dropped on pivot** (no warning) |
| `hypothesis`, `failure_mode` | 21 | dead_end | yes |
| `children` | 20 | nesting | yes |
| `status` | 2 | **experiment only** (`resolved`) | **no** |
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
  `justification`, `ara-2.0` streams) remain deferred. The T-REAL-CORPUS entry
  in TODOS.md gets a status line in this PR.
- #12 (per-node narrative field): superseded by this broader contract — close on
  merge.
- #64 (viewer rendering of narrative): the detail-pane slice lands here.

## Resolved decisions (eng review 2026-08-19, all human-approved)

- **D1 — pivot alias mechanism: ARA002/ARA003 precedent.** `from`/`to`/`trigger`
  are **removed from `RawNode`** (not kept as serde-modeled aliases). On a pivot
  node they fall into `extra` → unknown-field warning, value dropped — exactly
  like `reason:` on dead_end today. New kind-scoped lint rules
  **ARA005** (`from:`→`prior_direction:`), **ARA006** (`to:`→`new_direction:`),
  **ARA007** (`trigger:`→`reason:`) detect them on pivot nodes from the unparsed
  text; `fix.rs` `guard_alias` recovers the value (None→Some delta). A node
  carrying both an alias and its canonical key is auto-rejected by the guard and
  recorded as a `SkippedFix` — no silent ambiguity, no corruption. (Rejected:
  serde-modeled aliases with a parse.rs rewrite — parse.rs has no text spans and
  serde-modeled aliases would warn about nothing. Rejected: serde `alias`
  attribute — verified Layer-1 built-in, but it silently renames with no
  diagnostic and no fix hook.)
- **D2 — provenance is a free string.** No vocabulary validation; the tag set is
  advisory and evolving. Fail-closed applies to unknown *keys*, not values.
- **D3 — `ExhibitKind` gains `Result` and `Proof` variants.** No
  `#[non_exhaustive]` — the only exhaustive match is `detail.rs`'s label fn
  (updated here), and in-repo exhaustiveness is a feature. Revisit before the
  first crates.io publish.
- **T-A — wrong-kind drop warnings (generalized fail-closed).** `project_kind`
  currently warns about dropped body fields only for *unknown* types; any
  modeled body field on the wrong *known* kind is silently discarded (the
  lesson-on-pivot bug is one instance). `project_kind` now warns whenever a
  modeled body field is present on a kind that does not project it, for every
  kind. New warnings may appear on the real corpus (already warning-heavy) —
  acceptable, non-strict.
- **T-B — `status` is experiment-scoped**, not universal: evidence is 2/116
  nodes, both experiments. It lives in `NodeFields::Experiment`; with T-A,
  `status` on any other kind warns instead of being silently accepted.
  `provenance`/`timestamp` stay universal (116/116 evidence).
- **T-C — duplicate exhibit id across categories warns.** Exhibit identity stays
  basename-based; `figures/foo.md` + `results/foo.md` now warns instead of
  silently colliding in the index map / consumed set / `NodeExhibit` dedup.
  Escalation path captured as T-EXHIBIT-QUALIFIED-IDS in TODOS.md.
- **C1 — doc/comment maintenance is an explicit step** (see step 11).
- **T1 — full test surface** (16 additions) is in scope, including the critical
  regression pin for lesson-on-pivot.
- **Wire break documented:** `NodeFields::Pivot`'s serialized keys change
  (`from`/`to`/`trigger` → `prior_direction`/`new_direction`/`reason`, +
  `lesson`). Pre-crates.io blast radius is this repo's viewer + snapshots;
  CHANGELOG calls it out as breaking for manifest consumers.
- **Name collision documented:** `reason` is canonical on pivot (ARA007 target)
  and an alias of `why_failed` on dead_end (ARA002). Kind-scoped lint keeps them
  unambiguous; documented in `lint.rs` header + `docs/manifest-schema.md`.

## Proposed solution

Model the published fields explicitly, fail-closed on everything else.

### 1. Raw + manifest model (`ara-core`)

- `RawNode` (`schema.rs`): add `provenance`, `timestamp`, `status`,
  `prior_direction`, `new_direction`, `reason`, `exploration`, `outcome`
  (optional strings). **Remove** `from`/`to`/`trigger` (D1). `lesson` stays
  modeled (now projected for pivot too).
- `Node` (`manifest.rs`): add `provenance: Option<String>`,
  `timestamp: Option<String>` — `skip_serializing_if = "Option::is_none"`.
  (`status` is NOT here; see T-B.)
- `NodeFields::Pivot`: replace `{from, to, trigger}` with
  `{prior_direction, new_direction, reason, lesson}`.
- `NodeFields::Experiment`: add `exploration`, `outcome`, `status`.
- `NodeFields::DeadEnd`: unchanged.
- `ExhibitKind`: add `Result` and `Proof` variants (additive to the wire type).

### 2. Evidence discovery (`evidence.rs`)

Enumerate `figures/`, `proofs/`, `results/`, `tables/` in that fixed order
(sorted within each), one `Exhibit` per `*.md`, bodies verbatim. Same
index-enrichment rules as today; `results`/`proofs` rows resolve through the
same column-tolerant index parse (already tolerant across the corpus's 8 README
header variants). Two additions:
- **Duplicate-id warning** (T-C): same basename in two categories → warn.
- **Index-coverage contingency:** during implementation, verify the canonical
  clone's `evidence/README.md` actually indexes its `results/`/`proofs/` bodies.
  If bodies lack index rows, the existing "body file has no index row" warning
  fires per current rules — metric 1's target adjusts accordingly and the
  delta is reported in the PR, not silently absorbed.

### 3. Viewer (ara-viewer)

Detail pane renders, when present and omitted when absent: provenance,
timestamp chips (following the existing `support_level` pill / `source_refs`
chip idioms); pivot narrative (prior/new direction, reason, lesson); experiment
narrative (exploration, outcome, status). `exhibit_kind_label` gains
`result`/`proof`. The stale `── 7. Provenance ──` comment (renders
`source_refs`) is renamed to "Sources" so it doesn't collide with the new
provenance metadata. No layout/filter changes.

### 4. `--fix` and canonicalization (D1)

Published fields are canonical: `--strict --fix` on published input is a no-op.
Alias canonicalization happens through **ARA005/ARA006/ARA007** in `lint.rs`
(kind-scoped, pivot-only) + `fix.rs` (`AliasField` extension, existing
`guard_alias`): aliases are consumed by rename, never emitted, so the fixpoint
is idempotent by construction. Guard-extension note: honor the
multiset-containment invariant in `errors_subset` (duplicate diagnostics must
not satisfy each other) — prior session pitfall, confidence 10.

### 5. Fail-closed guarantee

No generic allowlist. Three independent guards:
- genuinely unknown key (`bogus_field`) → warns non-strict, errors `--strict`;
- known body field on the wrong kind (`exploration` on a decision, `status` on
  a question) → wrong-kind drop warning (T-A);
- legacy pivot alias (`from:`) → unknown-field warning + fixable ARA005–007
  diagnostic; value recovered only by `--fix`.

## What already exists (reused, not rebuilt)

- `lint.rs` ARA002/ARA003: kind-scoped, text-level alias detection with
  `ReplaceInLine` fixes — ARA005–007 clone this shape.
- `fix.rs` `guard_alias` + `AliasField` + `errors_subset`: value-recovery guard
  extended, not reinvented.
- `parse.rs` `body_field_names`: wrong-kind warning list extended to all kinds.
- `evidence.rs` ordered `(subdir, kind)` loop + column-tolerant index parse
  (8 corpus README variants already handled).
- `detail.rs` pill/chip idioms (`support_level`, `source_refs`) and the
  `TypedField` ordering convention.
- `corpus_no_panic` regression net + insta snapshot suite (regen, not new
  harness).
- CI `viewer-embed-fresh` gate — forces the `embed-viewer.sh` regen step.

## Implementation steps

1. `schema.rs`: add the eight `RawNode` fields; **remove** `from`/`to`/`trigger`;
   unit tests (published fields leave `extra` empty; legacy keys land in
   `extra`).
2. `manifest.rs`: `Node` + `NodeFields` + `ExhibitKind` changes; serde
   round-trip tests.
3. `parse.rs`: project new fields per kind (pivot incl. `lesson` — regression
   pin; experiment incl. `status`); wrong-kind drop warnings for **all** scoped
   body fields on **all** kinds (T-A); `body_field_names` extended accordingly.
4. `lint.rs`: ARA005/006/007 kind-scoped rules + tests (detection; negative:
   same keys on non-pivot kinds; `type:` after the aliased key; nested
   `children:` pivots).
5. `fix.rs`: `AliasField` + guard extension + tests (single-rename accept;
   both-present reject → `SkippedFix`; multi-alias fixpoint; second-run no-op).
6. `evidence.rs`: four-category loop + duplicate-id warning + tests (order
   pinned; results/proofs index enrichment; duplicate id warns).
7. Viewer: `detail.rs` chips/narrative/labels + DetailModel unit tests (per
   repo convention) + browser test (`tests/web.rs`) + regenerate the embedded
   bundle (`scripts/embed-viewer.sh`) — CI `viewer-embed-fresh` fails otherwise.
8. Fixtures (each with `SOURCE.md`: upstream commit pin + source map, per
   `fixtures/official/SOURCE.md` convention):
   - `published-fields/` — reduced the-ara-of-ara slice covering every new
     field, warning-free;
   - `published-fields-unknown/` — sibling with one `bogus_field`;
   - wrong-kind matrix — each scoped body field on a wrong kind → drop warning;
   - `published-fields-aliased/` — one pivot node on `from`/`to`/`trigger`.
9. Idempotency: canonical fixture → two `check --strict --fix` runs, exit 0 and
   byte-identical YAML across both. Aliased fixture → first run rewrites;
   first-output → second-output byte-identical (byte-identity is claimed
   post-fix, not against the pre-fix original).
10. Snapshots: regen insta snapshots whose JSON gains fields
    (`parse_fixtures.rs`, `layout_integration.rs`). No "native≡wasm golden"
    harness exists — the CI wasm job is compile-only; keep it green (no new
    wasm-incompatible deps).
11. Docs + comments (C1): `docs/hub-parity.md` (Pivot fields line, exhibits
    table, evidence section); `docs/manifest-schema.md` extensibility section
    (pivot rename = logical breaking change, not geometry; `reason`
    pivot-vs-ARA002 note); `docs/ara-format-feedback.md` §13 close-out;
    `TODOS.md` T-REAL-CORPUS status line; stale comments (`evidence.rs` header,
    `Exhibit`/`ExhibitKind` docs, `detail.rs` "Provenance"→"Sources",
    `lint.rs` header).
12. `Cargo.toml` patch bump (**0.1.14 → 0.1.15** — workspace is already 0.1.14)
    + `Cargo.lock` + CHANGELOG under `[Unreleased]`: `Added` for the new
    contract, `Changed` flagging the pivot wire rename as breaking for manifest
    consumers.
13. PR body: close #12 as superseded; reference #64's rendering slice; report
    LARA before/after warning counts.

## NOT in scope (considered, explicitly deferred)

- `trace/sessions/*.yaml` provenance on `ai_actions` — separate document type.
- Corpus-only fields (`thinking`, `method`, `justification`, `ara-2.0` streams)
  — T-REAL-CORPUS remainder; the corpus delta (metric 3) proves they still warn.
- Category-qualified exhibit ids — T-EXHIBIT-QUALIFIED-IDS; the duplicate-id
  warning (T-C) is the tripwire.
- Provenance vocabulary validation — rejected (D2); revisit only if the spec
  freezes the tag set.
- `evidence/logs/` — pointer-resolution index, not Markdown body content.
- `#[non_exhaustive]` on `ExhibitKind` — revisit before first crates.io publish.
- LARA-side gate changes — downstream repo's call (issue #75 Downstream Policy).
- `E##` evidence registry — T-EVIDENCE, blocked upstream.

## Failure modes (per new codepath)

| codepath | realistic failure | test? | handling | user sees |
|---|---|---|---|---|
| ARA005–007 rename | alias + canonical key both present | guard-reject test (step 5) | guard rejects, `SkippedFix` recorded, no write | skipped-fix report — loud |
| wrong-kind field | `exploration` on decision silently dropped | matrix fixture (step 8) | drop warning (T-A); strict fails | clear warning |
| duplicate exhibit id | `figures/foo.md` + `results/foo.md` collide | step 6 test | warn; bodies still preserved | clear warning |
| results/proofs unindexed | bodies warn "no index row" | step 2 contingency + metric 1 | existing warning path | clear warning; metric adjusted openly |
| old viewer, new manifest | new fields silently dropped | — | serde ignores unknown JSON keys; viewer ships in lockstep, versioned release | silent but versioned — accepted |
| stale embedded viewer | CLI serves old wasm | CI `viewer-embed-fresh` | build fails loudly | CI red — loud |
| snapshot drift | new fields change JSON | `cargo test` | insta failure | test red — loud |

No silent + untested + unhandled paths: **0 critical gaps.**

## Worktree parallelization

Effectively sequential — a single-crate chain (ara-core model → lint/fix →
evidence → viewer → fixtures/snapshots/docs). A 2-lane split (Lane A: steps
1–5; Lane B: step 6) is possible since `evidence.rs` shares no symbols with the
alias work, but both lanes touch the same insta snapshots and CHANGELOG — merge
friction exceeds the savings. **Sequential implementation recommended.**

## Implementation Tasks

Synthesized from the eng review findings. Run with Claude Code or Codex;
checkbox as you ship.

- [ ] **T1 (P1, human: ~3h / CC: ~20min)** — ara-core model — add published fields; remove `from`/`to`/`trigger`; rename `NodeFields::Pivot`; scope `status` to Experiment; `ExhibitKind` + Result/Proof
  - Surfaced by: Architecture review — D1/D3/T-B
  - Files: `crates/ara-core/src/schema.rs`, `crates/ara-core/src/manifest.rs`
  - Verify: `cargo test -p ara-core schema manifest`
- [ ] **T2 (P1, human: ~3h / CC: ~20min)** — parse.rs — project new fields; wrong-kind drop warnings on all kinds; extend `body_field_names`
  - Surfaced by: Architecture review + Codex #4 (T-A); REGRESSION: lesson-on-pivot silent drop
  - Files: `crates/ara-core/src/parse.rs`
  - Verify: `cargo test -p ara-core parse` + wrong-kind matrix fixture
- [ ] **T3 (P1, human: ~1d / CC: ~45min)** — lint+fix — ARA005/006/007 kind-scoped rules; `AliasField`/`guard_alias` extension; guard accept/reject/fixpoint tests
  - Surfaced by: Architecture review D1 (ARA002/003 precedent); prior learning: multiset containment in `errors_subset`
  - Files: `crates/ara-core/src/lint.rs`, `crates/ara-core/src/fix.rs`
  - Verify: `cargo test -p ara-core lint fix`
- [ ] **T4 (P1, human: ~3h / CC: ~20min)** — evidence.rs — four-category discovery; duplicate-id warning; order + index-enrichment tests
  - Surfaced by: plan §2 + Codex #6 (T-C)
  - Files: `crates/ara-core/src/evidence.rs`
  - Verify: `cargo test -p ara-core evidence`
- [ ] **T5 (P1, human: ~4h / CC: ~30min)** — viewer — detail-pane chips/narrative/labels; DetailModel unit tests; browser test; `scripts/embed-viewer.sh` regen
  - Surfaced by: plan §3 + Codex #9 (embed regen)
  - Files: `crates/ara-viewer/src/detail.rs`, `crates/ara-viewer/tests/web.rs`, `crates/ara-cli/assets/viewer/`
  - Verify: `cargo test -p ara-viewer` + `wasm-pack test --headless --chrome crates/ara-viewer` + `scripts/embed-viewer.sh --check`
- [ ] **T6 (P1, human: ~3h / CC: ~20min)** — fixtures — published-fields (+SOURCE.md pin), published-fields-unknown, wrong-kind matrix, published-fields-aliased; idempotency pair
  - Surfaced by: test review + Codex #8/#11 (fixture split, provenance convention)
  - Files: `crates/ara-core/tests/fixtures/published-fields*/`
  - Verify: `cargo test -p ara-core --test parse_fixtures` + two-run byte-compare
- [ ] **T7 (P2, human: ~1h / CC: ~10min)** — release mechanics — insta snapshot regen; version 0.1.14→0.1.15 + Cargo.lock; CHANGELOG with breaking-pivot note
  - Surfaced by: Codex #2 (version drift) + #3 (no wasm-parity harness — insta instead)
  - Files: `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, `crates/ara-core/tests/snapshots/`
  - Verify: `cargo test --workspace` + `cargo insta review`
- [ ] **T8 (P2, human: ~1h / CC: ~10min)** — docs — hub-parity.md, manifest-schema.md, ara-format-feedback §13, TODOS.md status line, five stale comments
  - Surfaced by: code-quality review C1 + Codex #10 (doc retargets)
  - Files: `docs/hub-parity.md`, `docs/manifest-schema.md`, `docs/ara-format-feedback.md`, `TODOS.md`, `crates/ara-core/src/evidence.rs`, `crates/ara-viewer/src/detail.rs`
  - Verify: doc reads true against the merged code

## Evaluation

How we will judge whether this change achieves the contract. Baseline measured
2026-08-19 against a fresh clone of `ARA-Labs/Agent-Native-Research-Artifact@main`
(main was then 0.1.11; the workspace is now 0.1.14 — re-verify counts at
implementation time).

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
   pre-existing unrelated warnings remain). Contingency (step 2): if the
   clone's `results/`/`proofs/` bodies lack index rows, new "body has no index
   row" warnings fire per existing rules — report the honest delta in the PR
   instead of absorbing it.
2. **Preservation, not just acceptance.** Inspect the emitted
   `manifest.json` for the canonical artifact: all 116 nodes carry
   `provenance` + `timestamp`; pivot nodes carry `prior_direction` /
   `new_direction` / `reason` / `lesson`; N95/N96 carry `exploration` /
   `outcome` / `status`. This catches silent drops — today `lesson` on a pivot
   node is *not* warned but *is* discarded by `project_kind`, so warnings-only
   evaluation would miss it. The unit regression pin lands in step 3.
3. **Real-corpus delta (scope control).** Run the vendored
   `ara-paperbench` subset (`crates/ara-core/tests/corpus`) before/after and
   diff warning counts by field name. Expected: `provenance` / `status` /
   `timestamp` warnings disappear; `thinking` / `method` / `justification` /
   `failure_mode`-family warnings **remain** (T-REAL-CORPUS, out of scope).
   Wrong-kind warnings (T-A) may *add* corpus warnings — diff by message kind
   and report; they are fail-closed signal, not regressions.
4. **Fail-closed checks.** The `published-fields-unknown/` sibling fixture
   (one `bogus_field` key) warns non-strict and errors under `--strict`; the
   wrong-kind matrix fixture warns per kind.
5. **Idempotency.** Canonical fixture: two `check --strict --fix` runs →
   exit 0, zero warnings, byte-identical YAML across both runs. Aliased
   fixture: first run rewrites the alias; first-output → second-output
   byte-identical. (Byte-identity is claimed post-fix and between runs — a
   fixture containing an alias is by definition not byte-identical to its own
   pre-fix state.)
6. **Cross-target.** Regenerated insta snapshots match on native; the wasm CI
   compile check stays green; the wasm viewer renders the new fields (browser
   test asserts on the detail pane for a node with metadata + narrative).

### Acceptance-criteria mapping (issue #75)

| #75 criterion | evaluated by |
|---|---|
| Representative published-field fixture parses without unknown-field diagnostics | metric 1 + fixture test (step 8) |
| Native parse, normalized JSON, JSON/WASM, strict/fix, evidence, and browser tests cover the accepted fields | steps 1–7, 10; metrics 2, 6 |
| Canonical published input byte-identical after two `ara check --strict --fix` runs | metric 5 |
| Sibling unknown-field fixture fails in strict mode | metric 4 |
| `cargo test --workspace`, formatting, clippy, WASM tests, and viewer browser tests pass | step 12 + CI |
| Contract documented, released under an immutable version | steps 11–12; tag `v0.1.15` after merge |

### Downstream smoke check

LARA's gate is non-strict `ara check` with zero errors (issue #75, Downstream
Policy). After merge, run the released binary against `EYH0602/lara`'s ARA
directory and confirm: exit 0, zero errors, and no unknown-field warnings for
any published field. Report the before/after warning count in the PR. Note:
ARA005–007 are *fixable* lint issues — like `ruff`, non-strict `ara check`
exits non-zero while an unfixed fixable issue remains, so any downstream
artifact still on legacy pivot keys needs one `ara check --fix` pass (same
contract ARA002/003 established).

## Verification

- `cargo test --workspace`, `cargo clippy --all-targets`, `cargo fmt --check`
- WASM compile check (CI `wasm` job) + viewer browser tests (`wasm-pack test`)
- New fixture tests: published-fields fixture → zero unknown-field diagnostics;
  unknown sibling → strict-mode error; wrong-kind matrix → drop warnings;
  aliased fixture → ARA005–007 fire, `--fix` recovers values
- Idempotency: per metric 5 (canonical pair + aliased pair)

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | outside voice (plan) | Independent 2nd opinion | 1 | ISSUES_FOUND | 12 findings: 9 folded as amendments, 3 tensions resolved |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | 9 issues, 0 critical gaps — all folded into plan |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | — |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

- **CODEX:** 12 findings — version drift (0.1.15 not 0.1.12), nonexistent "wasm golden" harness (insta snapshots instead), missing `embed-viewer.sh` step, wrong doc targets (+hub-parity.md), idempotency-fixture contradiction split, fixture SOURCE.md provenance, strict-exit vs byte-identity wording, index-coverage contingency — all folded; 3 tensions (wrong-kind warnings, status scoping, duplicate exhibit ids) resolved by the human toward fail-closed.
- **CROSS-MODEL:** agreement on the silent-drop class (review found the lesson-on-pivot instance; Codex generalized it to all wrong-kind fields) — resolved as T-A. No unresolved disagreements.
- **VERDICT:** ENG CLEARED — ready to implement. 10 decisions made (D1–D3, T-A/B/C, C1, T1, wire-break doc, reason-collision doc), all reflected in the amended plan above.

NO UNRESOLVED DECISIONS
