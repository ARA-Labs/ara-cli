# Plan: Full viewer parity with the ARA Hub

Status: **DRAFT — awaiting human review.** Do not implement until approved.

## Problem background

The official ARA Hub renders far more of an artifact than our local viewer does.
Comparing our viewer against the live hub page for one artifact
(`AmberLJC/ara-paperbench` → `paperbench/self-composing-policies`,
<https://www.agenticresearch.sh/ara/AmberLJC/ara-paperbench/artifacts/paperbench/self-composing-policies>,
node **N07** selected) shows the gap concretely.

### What the hub shows and we don't

**Per-node detail pane.** Corrected against a screenshot of the live hub with N07
selected (`~/.gstack/.../designs/hub-parity-20260716/`, design review 2026-07-16).
The **actual** hub order is: kind badge + support pill (`EXPLICIT`) + title →
**REASONING** (generated narrative prose) → **WHAT IT DID** (= the node `result`) →
evidence chips (`Appendix B`) → **BUILT ON** (RW chips) → RESULT (figures/tables) →
ARTIFACT. The earlier draft order (`BUILT ON · RESULT · WHY · ARTIFACT`) was wrong,
and "WHY" is not a labeled hub section — the labeled sections are REASONING /
WHAT IT DID / BUILT ON. We render title · kind · description · typed fields ·
evidence notes + claims · sources. Missing:

| Hub section | Source of truth | Our status |
|---|---|---|
| **REASONING** | **generated narrative prose** (not in any source file) | ⚠️ **Conflicts with our founding constraint** — see D1 below. Not a straight "build it" item. |
| **WHAT IT DID** | node `result` field | ✅ Have it (Experiment `result` typed field) — just relabel. |
| **RESULT** | `evidence/figures/*.md` + `evidence/tables/*.md`, rendered as full markdown tables | ❌ We never read `evidence/`. Figure refs (`"Figure 3"`) land in `evidence_notes` as bare strings. No markdown-table rendering. |
| **BUILT ON** | `logic/related_work.md` (RW01…), linked node → claim → RW via each RW's `Claims affected` | ❌ No RW model; file never read. |
| **ARTIFACT** | pointer into `src/code/…` | ❌ No code-pointer linkage. |

> **D1 (design review, blocking).** The hub's **REASONING** block is LLM-generated
> narrative prose baked into the hub at publish time. Our README thesis is the
> opposite: *"Renders the YAML directly — never calls an LLM at view time … missing
> upstream prose degrades gracefully to the structured fields — it is never faked at
> view time."* So "full parity" as literally stated is **impossible without breaking
> the product's core promise**. Resolution options: **(a)** drop REASONING, keep
> WHAT IT DID (the structured `result`) as the top block — this is the honest,
> on-brand choice and the recommended default; **(b)** render REASONING *only* if a
> future schema carries it as a stored field (inert until then, like the other
> deferred slots); **(c)** change the product thesis (not recommended — it's the
> headline differentiator). Lock D1 before any per-node rendering work.

> **D2 (RESOLVED 2026-07-16) — keep our skin, port only structure.** There is **no
> single "hub skin" to match**: each artifact's `trace/exploration_tree.html` (and the
> hub view built from it) is **LLM-generated at publish time**, so the colour theme and
> styling **differ per artifact** — it is a non-reproducible target. Confirmed by the
> maintainer. Therefore we keep the **warm-cream vendored tokens + glyph-not-colour**
> skin (deliberate colour-blind-safe choice, README + T-DESIGN-TOKENS) and port only
> the hub's *structure* (which sections exist, their order, the panels). This governs
> every colour/type/spacing choice below: **do not** import the hub's serif, per-kind
> colours, or any artifact-specific styling. Same root cause resolves D1 — the hub's
> *look and its REASONING prose are both LLM-generated and non-deterministic*; we
> render the structured source deterministically instead.
>
> For reference only (not a skin to copy): the baked HTML happens to use cool grays +
> per-kind colours (question=blue, experiment=orange, decision=green, dead_end=red,
> pivot=purple); the live hub uses serif headings + REASONING narrative + modal panels.
> Neither is canonical.

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

6. **Un-inert the reserved slots** in `detail.rs` and add, **in the corrected hub
   order** (D1 governs whether REASONING appears): WHAT IT DID (`result`, relabelled)
   → evidence chips → per-node **BUILT ON** (RW chips) → **RESULT** (figure/table
   blocks with rendered tables) → ARTIFACT. Reuse existing `.block` / `.block.reason`
   styling and the `kind_meta` glyph source; do not invent new chrome.
   - **ARTIFACT** pointer (deferred sub-item if code-linkage data isn't modeled
     — see below).
7. **Four header panels** (Context / Glossary / Dependencies / Recipes). **There is
   no existing "overlay pattern" to reuse** — the resizable divider is a splitter, not
   a modal. This is a **new component** and must be specced, not hand-waved. From the
   hub screenshot, each panel is:
   - a **centered modal overlay** (not a side dock), max-width ~880px, scrim behind,
     opened from the labelled header buttons that carry a **live count**
     (`Glossary 12`, `Dependencies 9`, `Recipes 28` — counts are a required
     affordance, not decoration; a 0 count hides the button).
   - has its **own filter/search box** (Glossary shows `filter…`) scoped to that
     panel's items, plus an **`✕ Esc`** affordance.
   - **Accessibility contract (mandatory, this is a headline project feature):**
     `role="dialog"` + `aria-modal`, focus moves into the panel on open, **focus is
     trapped**, **Esc closes**, focus **returns to the invoking button** on close,
     scrim click closes. Without this the four modals regress the project's stated
     keyboard/ARIA promise.
   - Glossary/Concepts additionally render **term cross-reference chips** (dotted-
     underline concept links), a `mentions N07 N08…` node-chip row, and **LaTeX
     notation** (π^(k), Φ^{k;s}). Math rendering is its own decision — see D3.
8. **Paper header** (title/authors/venue + Abstract `<details>`), warm-cream skin
   per D2 (do not import the hub's serif unless D2 says so).
9. **Interaction states — specify for every new surface (Pass-1 gap):**
   - **Empty:** artifact lacks `related_work.md` → BUILT ON + Dependencies button
     both absent (not an empty box). Node with no `result`/evidence → no RESULT block.
     Matches the hub: N01 (a bare question) shows none of these sections.
   - **Partial:** node bound to a claim but no evidence file resolves → show WHY/claim,
     omit RESULT silently.
   - **Error:** malformed `concepts.md` → warn (never fatal), panel button hidden.
   - **Loading:** hub `/api/manifest` in flight → existing load-state placeholder;
     panels disabled until loaded.
10. **Responsive (Pass-5 gap):** <800px the layout already single-columns and hides
    the gutter — the four modals must go **full-screen** at that width, and **RESULT
    tables must scroll horizontally inside their block** (wide GFM tables are a
    mobile horizontal-scroll trap for the whole page otherwise). Define ≥800px and
    <800px behaviour for each panel + the RESULT tables.
11. **Regen the embedded viewer bundle** (`scripts/embed-viewer.sh`) — the
    `viewer-embed-fresh` CI gate will fail otherwise.

> **D3 (RESOLVED 2026-07-16) — inert monospace for v1.** Concepts/Recipes carry LaTeX
> (`$\pi^{(k)}$`, `$\Phi^{k;s}$`). Render raw `$…$` as monospace inert text for v1:
> honest, cheap, keeps the sub-MB wasm bundle gate green, fakes nothing. A proper
> KaTeX-style renderer is tracked as **future work (GitHub issue, see T-MATH-RENDER
> in TODOS.md)** — deferred because it adds JS/wasm weight that tensions the bundle
> gate and is not needed for structural parity.

### Hub mode

12. **T-HUB-FIGURES**: once figures render, image `src` must resolve under
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

`ara serve` on `paperbench/self-composing-policies` renders, for N07, in the
corrected hub order: paper header + abstract, WHAT IT DID (`result`), evidence
chips, BUILT ON (RW01/RW09), RESULT (fig3 + figB.1 tables), and the four populated
header panels (Context, Glossary 12, Dependencies 9, Recipes 28) — **in our
warm-cream + glyph-only skin (D2), with REASONING handled per D1**. Every new
surface has its empty/partial/error/loading state (Pass 1) and its <800px behaviour
(Pass 5). The four panels satisfy the modal a11y contract (focus-trap, Esc,
return-focus). Corpus sweep emits zero dead-end-field warnings. All snapshots
updated; embedded bundle fresh.

## Design decisions to lock before implementation (from /plan-design-review)

- **D1 — REASONING vs the no-LLM-at-view-time promise** (blocking). Recommend drop
  REASONING, lead with WHAT IT DID.
- **D2 — canonical reference / visual language** (RESOLVED). Keep our warm-cream +
  glyph-only skin, port only the hub's *structure*. Rationale: the hub/baked HTML is
  LLM-generated per artifact, so its look is non-reproducible — there is nothing
  stable to match. Do not import the hub's serif or per-kind colours.
- **D3 — LaTeX rendering** in Glossary/Recipes (RESOLVED). Inert monospace `$…$` for
  v1; KaTeX renderer deferred to future work (T-MATH-RENDER + GitHub issue).
- **Section order corrected** to the live hub: WHAT IT DID → evidence → BUILT ON →
  RESULT → ARTIFACT (REASONING gated on D1).
- **Panels are a new modal component**, not a reuse of the divider; a11y contract is
  mandatory.

## GSTACK REVIEW REPORT

Design review of `plans/hub-parity-full.md` — /plan-design-review, 2026-07-16.
Calibrated against three captured references (live hub screenshots in
`~/.gstack/projects/AmberLJC-ara-paperbench/designs/hub-parity-20260716/` +
`/tmp/hub-parity-refs/`, the baked `trace/exploration_tree.html`, and the current
warm-cream viewer). No `DESIGN.md`; tokens are vendored in
`crates/ara-viewer/public/styles.css` (T-DESIGN-TOKENS open).

| Dimension | Before | After edits | Note |
|---|---|---|---|
| Interaction state coverage | 2/10 | 8/10 | Added empty/partial/error/loading per surface (step 9) |
| AI slop risk | 6/10 | 8/10 | Killed "reuse the overlay pattern" fiction; specced real modal |
| Information architecture | 4/10 | 8/10 | Corrected section order against hub screenshot |
| User journey | 5/10 | 7/10 | Panel counts promoted to required affordance |
| Responsive | 1/10 | 7/10 | <800px full-screen modals + table horizontal-scroll (step 10) |
| Accessibility | 1/10 | 8/10 | Modal focus-trap/Esc/return-focus contract now mandatory |
| Visual system / language conflict | 3/10 | 7/10 | D2 forces a canonical-reference decision |
| **Overall** | **6.5/10** | **~8/10 pending D1–D3** | Data layer was already strong; design layer now specced |

Runs: 1 (inline). Status: issues_found → addressed in-plan.
Findings: 3 blocking design decisions raised (D1 REASONING-vs-no-LLM,
D2 canonical-reference/skin, D3 LaTeX), section order corrected, panels re-specced
as a new modal component with a mandatory a11y contract, interaction states +
responsive behaviour added.

Design mockups: **not generated** — this is a parity task with three existing
official reference renderings, so I captured the live hub as the pixel spec instead
of inventing designs (more faithful than AI mockups here).

VERDICT: Plan is materially stronger and safe to proceed **once D1–D3 are answered**.
D1 and D2 are true blockers — they can invalidate whole slices (per-node rendering,
every colour/type choice). Recommend answering all three before slice 2.

**UNRESOLVED DECISIONS:**
- D1 — REASONING block vs the "never call an LLM at view time" promise (recommend: drop REASONING, lead with WHAT IT DID).

RESOLVED (2026-07-16):
- D2 — keep our warm-cream + glyph-only skin, port only the hub's structure (the hub/baked HTML is LLM-generated per artifact, so its look is non-reproducible).
- D3 — inert monospace `$…$` for v1; KaTeX renderer deferred to future work (T-MATH-RENDER + GitHub issue).
