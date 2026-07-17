# Plan: Full viewer parity with the ARA Hub

Status: **DRAFT — awaiting human review.** Do not implement until approved.

## Problem background

The official ARA Hub renders far more of an artifact than our local viewer does.
Comparing our viewer against the live hub page for one artifact
(`AmberLJC/ara-paperbench` → `paperbench/self-composing-policies`,
<https://www.agenticresearch.sh/ara/AmberLJC/ara-paperbench/artifacts/paperbench/self-composing-policies>,
node **N07** selected) shows the gap concretely.

### What the hub shows and we don't

**Per-node detail pane.** Hub order for N07: title · kind · description ·
**BUILT ON** · **RESULT** · **WHY** · **ARTIFACT**. We render title · kind ·
description · typed fields · evidence notes + claims · sources. Missing:

| Hub section | Source of truth | Our status |
|---|---|---|
| **RESULT** | `evidence/figures/*.md` + `evidence/tables/*.md`, rendered as full markdown tables | ❌ We never read `evidence/`. Figure refs (`"Figure 3"`) land in `evidence_notes` as bare strings. No markdown-table rendering. |
| **BUILT ON** | `logic/related_work.md` (RW01…), linked node → claim → RW via each RW's `Claims affected` | ❌ No RW model; file never read. |
| **WHY** | claim (e.g. C01) | ✅ Have it (claims block). |
| **ARTIFACT** | pointer into `src/code/…` | ❌ No code-pointer linkage. |

**Global header panels** — all four missing. These are exactly the "inert" slots
already reserved (commented) in `crates/ara-viewer/src/detail.rs:386`:

| Panel | Source file | Count on the sampled artifact |
|---|---|---|
| ◧ Context | `logic/problem.md` | — |
| ▤ Glossary | `logic/concepts.md` | 12 terms |
| ⇄ Dependencies | `logic/related_work.md` | 9 refs |
| ▦ Recipes | `logic/solution/*.md` | 28 items |

**Paper header.** Hub shows title + authors + venue/year + abstract, from
`PAPER.md` frontmatter. We show none of it.

**Node body under-modeling.** Real dead-ends (N03, N05) carry
`hypothesis` / `failure_mode` / `lesson`; our `NodeFields::DeadEnd`
(`crates/ara-core/src/manifest.rs:156`) only has `why_failed`, so those three
become unknown-field warnings and render as nothing. `pivot` is a real node
`type:` in the corpus with no kind of ours. This is the core of **T-REAL-CORPUS**.

### Why this was deferred (and why we can act now)

Our parser reads only `trace/exploration_tree.yaml` (required) and
`logic/claims.md` (`crates/ara-core/src/parse.rs:154,167`). Everything above
lives in files we never open. The backlog deferred this pending an upstream
schema (see `TODOS.md`: **T-REAL-CORPUS**, **T-VIEWER-TREE-LIST** #7,
**T-HUB-FIGURES**, **T-EVIDENCE**). But the corpus uses a **stable, observable
convention** (`PAPER.md` frontmatter, `logic/{problem,concepts,related_work}.md`,
`logic/solution/*.md`, `evidence/{figures,tables}/*.md` + `evidence/README.md`
index). We can model what the corpus actually does now, and swap to a published
schema later (T-ARA-SCHEMA) without changing the viewer.

## Corpus conventions (verified against the submodule)

Confirmed from
`corpus-external/ara-paperbench/artifacts/paperbench/self-composing-policies/`:

- **`PAPER.md`** — YAML frontmatter: `title`, `authors[]`, `year`, `venue`,
  `doi`, `abstract`, `keywords[]`, `claims_summary[]`, `ara_version`.
- **`logic/problem.md`** — `## Observations` (O1…), `## Key Insight` (I1…),
  plus a problem statement / setting / gaps. → Context panel.
- **`logic/concepts.md`** — `## <Term>` blocks with `Notation` / `Definition` /
  `Boundary conditions` / `Related concepts`. → Glossary.
- **`logic/related_work.md`** — `## RW0N: <cite>` with `DOI`, `Type`
  (baseline/imports/bounds/refutes/extends), `Delta` (What changed / Why),
  `Claims affected` (→ links RW to claims, hence to nodes), `Adopted elements`.
  → Dependencies panel **and** per-node BUILT ON.
- **`logic/solution/*.md`** — `algorithm.md`, `architecture.md`,
  `constraints.md`, `heuristics.md`, each with math + steps. → Recipes.
- **`evidence/README.md`** — index tables mapping each figure/table file to its
  paper `Source` (e.g. "Figure 3, §4.3") and `Claims` (e.g. `C01, C04`).
- **`evidence/figures/*.md`, `evidence/tables/*.md`** — the actual content
  (caption, axes, markdown data tables). → RESULT.

### Open question — how RESULT resolves for a node (decide before coding)

N07's `evidence: ["C01", "Appendix B", "Figure B.1", "Figure 3 (right)"]`, and
the hub RESULT for N07 showed **fig3_scalability.md + figb1_memory_growth.md**.
Both those files list `C01` in the evidence index's `Claims` column, and N07 is
bound to C01. Two plausible resolution rules produce the same output here:

1. **Claim-based**: node → its claims (C01) → evidence files whose `Claims`
   column contains C01 (via `evidence/README.md`).
2. **Direct-ref**: fuzzy-match the node's `"Figure …"` evidence strings to
   figure files.

Recommendation: **claim-based (rule 1)** — it uses the explicit index and needs
no fuzzy string matching. Confirm by sampling 2–3 more artifacts before locking.

## Proposed solution

Keep the split: `ara-core` models the data (native + wasm, deterministic,
snapshot-tested); `ara-viewer` renders it. Add new logical sections to the
`Manifest` and new files to the reader. Everything stays additive and
serde-defaulted so old manifests round-trip (as `isolated`/`pos` already do).

### Core (`ara-core`)

1. **Widen `NodeFields`** (`manifest.rs`):
   - `DeadEnd { hypothesis, failure_mode, lesson, why_failed }` (all `Option`).
   - Add `Pivot { from, to, trigger }` and `NodeKind::Pivot`.
   - Keep unknown fields tolerant (still warn, never error).
2. **New manifest sections** (all `Option`/`Vec`, `skip_serializing_if` empty):
   - `paper: Option<PaperMeta>` (title/authors/year/venue/doi/abstract/keywords).
   - `related_work: Vec<RelatedWork>` (id, cite, doi, kind, what/why, adopted,
     `claims_affected: Vec<ClaimId>`).
   - `concepts: Vec<Concept>` (term, notation, definition, boundary, related).
   - `problem: Option<Problem>` (observations, insights, statement).
   - `recipes: Vec<Recipe>` (name, kind, body markdown).
   - `evidence: Vec<Evidence>` (id/file, source, `claims: Vec<ClaimId>`,
     description, rendered body / parsed tables).
3. **New readers** in `parse_dir` (each optional, missing → skipped, never
   fatal; malformed → warning):
   - `PAPER.md` frontmatter parser (needs a YAML-frontmatter split; reuse
     existing `serde-saphyr`).
   - `logic/related_work.md`, `logic/concepts.md`, `logic/problem.md`,
     `logic/solution/*.md` markdown-section parsers.
   - `evidence/README.md` index + `evidence/**/*.md` bodies.
   - Keep `parse_sources` (wasm, in-memory) working: thread the new files
     through as additional `(path, contents)` inputs so wasm callers can pass
     them too. `parse_dir` becomes the native "read all these files" wrapper.
4. **Resolution passes** (deterministic, source-order preserving):
   - node → RESULT evidence (rule 1 above).
   - node → BUILT ON (node → claims → RW via `claims_affected`).
   - This finally gives **T-EVIDENCE**-adjacent linkage; keep `E##` proof refs
     out of scope (still no registry).
5. **Markdown table rendering.** RESULT/tables need GFM tables → a structured
   form (`Vec<Row>`) so the viewer renders real `<table>`, not raw text. Decide:
   parse to rows in core (testable, deterministic) vs. render markdown in the
   client. Recommendation: **parse to a minimal table AST in core**; leave prose
   as markdown strings the client renders with a tiny inline formatter.

### Viewer (`ara-viewer`)

6. **Un-inert the reserved slots** in `detail.rs` and add:
   - Per-node **BUILT ON** (RW chips) and **RESULT** (figure/table blocks with
     rendered tables), in the hub's section order.
   - **ARTIFACT** pointer (deferred sub-item if code-linkage data isn't modeled
     — see below).
7. **Four header panels** as overlays (Context / Glossary / Dependencies /
   Recipes) with counts, matching #7's inert design. Reuse the overlay pattern
   the resizable-divider work already established.
8. **Paper header** (title/authors/venue + Abstract `<details>`).
9. **Regen the embedded viewer bundle** (`scripts/embed-viewer.sh`) — the
   `viewer-embed-fresh` CI gate will fail otherwise.

### Hub mode

10. **T-HUB-FIGURES**: once figures render, image `src` must resolve under
    `<base href="/a/{id}/">` and the hub needs `/a/{id}/api/figure/*`. The
    sampled artifacts use **markdown tables, not image files**, so image serving
    may not even be on the critical path — verify. Build figure-image serving in
    the same change that renders images (with `../` traversal tests), per the
    existing T-HUB-FIGURES note.

## Implementation steps (suggested slices, each shippable + patch-bump)

Sequenced so each step is independently reviewable and testable. Steps 1–2 are
pure core; 3+ light up the UI.

1. **Node-body widening** (T-REAL-CORPUS core): dead-end 3 fields + `pivot`.
   Snapshot tests over vendored fixtures; assert the ×67 dead-end-field warnings
   drop to zero on the corpus. No UI yet beyond typed-field rendering.
2. **PAPER.md + paper header**: frontmatter reader → `PaperMeta` → viewer header
   + Abstract. Smallest visible win.
3. **RESULT**: evidence readers + index + claim-based resolution + table AST +
   per-node RESULT block. Decide the resolution rule first (sample ≥3 artifacts).
4. **BUILT ON + Dependencies panel**: `related_work.md` reader + node→claim→RW
   linkage + RW chips (per-node) and the Dependencies overlay (global).
5. **Glossary + Context + Recipes panels**: `concepts.md` / `problem.md` /
   `solution/*.md` readers + three overlays.
6. **ARTIFACT pointer** + **hub figure serving** (only if images are actually
   used by any artifact; tables need neither).

Per-step: bump patch version + `CHANGELOG.md` entry (functional). Each core step
extends the `insta` snapshots and the `corpus_no_panic` net. Run
`cargo test --workspace` + wasm build + `scripts/embed-viewer.sh --check`.

## Risks / decisions to lock before coding

- **Resolution rule for RESULT** (claim-based vs direct-ref) — sample more
  artifacts. Blocks step 3.
- **Table rendering location** (core AST vs client markdown) — recommend core.
- **wasm file-passing**: hub/live already fetch `manifest.json`; the new files
  are read server-side into the manifest, so wasm needn't read them directly —
  confirm the live/hub `/api/manifest` path carries the enriched manifest and the
  static `manifest.json` fallback still works.
- **Schema drift**: model the *observed* convention now; T-ARA-SCHEMA swaps to a
  published schema later. Keep readers tolerant (warn, never fatal) so
  non-conforming artifacts still open.
- **Scope of ARADemo corpus**: verify conventions hold on `ARA-Labs/ARA-Demo`
  too (it uses a DOM tree-list viewer), not just paperbench.

## Definition of done

`ara serve` on `paperbench/self-composing-policies` renders, for N07: paper
header + abstract, BUILT ON (RW01/RW09), RESULT (fig3 + figB.1 tables), WHY
(C01), and the four populated header panels (Context, Glossary 12,
Dependencies 9, Recipes 28) — matching the hub. Corpus sweep emits zero
dead-end-field warnings. All snapshots updated; embedded bundle fresh.
