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
| **REASONING** | **generated narrative prose** (not in any source file) | 🚫 **Dropped for v1** (D1 RESOLVED) — LLM-generated at publish time; reproducing it would break the no-LLM-at-view-time promise. Reserved as an inert slot for a future stored `reasoning:` field. See D1. |
| **WHAT IT DID** | node `result` field | ✅ Have it (Experiment `result` typed field) — just relabel. |
| **RESULT** | `evidence/figures/*.md` + `evidence/tables/*.md` | ❌ We never read `evidence/`. Figure refs (`"Figure 3"`) land in `evidence_notes` as bare strings. **v1 ships the data layer only** (parse into `exhibits`, resolve node→exhibit linkage, show evidence chips); **table/markdown rendering is deferred to D4** (client-side markdown, GitHub issue) to keep the sub-MB wasm bundle gate green. |
| **BUILT ON** | `logic/related_work.md` (RW01…), linked node → claim → RW via each RW's `Claims affected` | ❌ No RW model; file never read. |
| **ARTIFACT** | pointer into `src/code/…` | ❌ No code-pointer linkage. |

> **D1 (RESOLVED 2026-07-16) — drop REASONING for v1; lead with WHAT IT DID; reserve
> an inert slot for a future stored field.** The hub's **REASONING** block is
> LLM-generated narrative prose baked in at publish time — it is **not present in any
> source file** (N07's `exploration_tree.yaml` carries only a terse `result:`). Our
> README thesis is the opposite: *"Renders the YAML directly — never calls an LLM at
> view time … missing upstream prose degrades gracefully to the structured fields — it
> is never faked at view time."* So literal "full parity" here is **impossible without
> breaking the product's core promise**.
>
> **Decision (a + b):**
> - **(a) v1:** do **not** render a REASONING block. Make the structured `result`
>   field the top block, labelled **WHAT IT DID** (the hub's own label for it). Honest,
>   on-brand, needs no new data.
> - **(b) upgrade path:** reserve REASONING as an **inert slot** (like the other
>   deferred slots at `crates/ara-viewer/src/detail.rs:386`). If the ARA schema ever
>   adds a stored per-node `reasoning:`/`narrative:` field, render it above WHAT IT DID
>   — because then it is *source data*, not view-time fabrication. Until then it shows
>   nothing.
>
> **Rejected (c):** generating/serving prose like the hub. This throws away the
> single clearest differentiator (deterministic, byte-reproducible, no LLM at view
> time) and drags LLM infra + non-determinism into the viewer. Only revisit if the
> product goal changes from "faithful-to-source local viewer" to "be the hub."
>
> **Accepted cost:** on nodes with a rich authored `result`, our pane opens with the
> terse structured field instead of the hub's polished paragraph, so it looks sparser.
> That sparseness is the visible price of "deterministic and honest" — and since the
> README sells exactly that, sparser-but-faithful is on-brand, not a regression.
> Same root cause as D2: the hub's *look and its REASONING prose are both
> LLM-generated per artifact and non-deterministic*; we render the structured source
> deterministically instead.

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
   - `exhibits: Vec<Exhibit>` (id/file, source, `claims: Vec<ClaimId>`,
     description, **raw markdown body string**). **Named `exhibits`, NOT
     `evidence` (E4)** — the node-level `evidence:` concept (`C##` claim-refs +
     prose, already modeled as `Binding` + `evidence_notes`) is unrelated, and a
     second `evidence` in the same `Manifest` would mislead every reader. v1
     carries the raw markdown string through; the client renders it later (D4).
3. **New readers** in `parse_dir` (each optional, missing → skipped, never
   fatal; malformed → warning):
   - `PAPER.md` frontmatter parser (needs a YAML-frontmatter split; reuse
     existing `serde-saphyr`).
   - `logic/related_work.md`, `logic/concepts.md`, `logic/problem.md`,
     `logic/solution/*.md` markdown-section parsers.
   - `evidence/README.md` index + `evidence/**/*.md` bodies.
   - **Readers live in `parse_dir` only (E1).** The wasm client never runs the
     parser — it fetches the already-built `Manifest` JSON from `/api/manifest`
     (`source.rs:64`) and deserializes it; `ara-wasm` is a stub. So the enriched
     manifest is assembled entirely native-side in `parse_dir`, serialized to
     JSON, and the client just deserializes it. **Keep `parse_sources` as the
     pure 2-arg (tree + claims) core; do NOT thread the new files through it**
     (the earlier "wasm callers can pass them too" wording was wrong — see the
     Risks note that already said wasm needn't read them). Section-parsing logic
     for `related_work.md` / `concepts.md` / `problem.md` / `solution/*.md` and
     the `evidence/README.md` index runs inside `parse_dir`, integration-tested
     over on-disk fixtures (E2).
4. **Resolution passes** (deterministic, source-order preserving):
   - node → exhibit (claim-based, rule 1 above).
   - node → BUILT ON (node → claims → RW via `claims_affected`).
   - This finally gives **T-EVIDENCE**-adjacent linkage; keep `E##` proof refs
     out of scope (still no registry).
   - **Evidence-index parser must be column-NAME-tolerant (E5).** Verified
     across all 32 artifacts: the `evidence/README.md` header is **not** stable
     — 26 use `File|Source|Claims|Description`, 3 use a different column *order*
     (`File|Description|Source|Claims`), 1 uses `Key refs`, and **2 have no
     `Claims` column at all**. Match columns by header name, not position; a
     missing `Claims` column → "no linkage, empty RESULT, warn-not-fatal". **Re-
     derive/validate the resolution rule against ≥10 artifacts before locking**
     (the plan's earlier "sample 2–3" was calibrated to the wrong risk). Add the
     header variants above as fixtures.
5. **No table AST in core (E6 / D4).** RESULT rendering is deferred (D4): the
   core carries each exhibit's **raw markdown body string** through the manifest
   and does no table parsing. The client renders markdown later. This drops the
   plan's earlier "minimal table AST in `Vec<Row>` in core" recommendation —
   with client-side rendering locked, a core AST would re-implement what the
   client renderer already does, and the files carry caption/axes prose that
   doesn't fit a row model anyway.

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
   a modal. This is a **new component** and must be specced, not hand-waved.
   - **Build ONE shared `Modal` component first, before any panel (E7).** The
     a11y contract below (focus-trap, return-focus across a wasm re-render,
     scrim-vs-content click) is fiddly in Leptos with no existing modal to copy.
     Build + test the reusable `Modal` once (with its wasm a11y tests, GAP-12);
     all four panels consume it. This proves the hard part before three panels
     depend on it, and keeps the four panels DRY.
   From the hub screenshot, each panel is:
   - a **centered modal overlay** (not a side dock), max-width ~880px, scrim behind,
     opened from the labelled header buttons that carry a **live count**
     (`Glossary 12` = `## Term` blocks, `Dependencies 9` = `## RW` blocks; a 0
     count hides the button). **`Recipes` count is an OPEN QUESTION (E8) —
     blocks slice with panels.** The source (`AmberLJC/ara-paperbench` README)
     defines only `solution/` = 4 files (`algorithm`/`architecture`/
     `constraints`/`heuristics`); "recipe" is a viewer-side label with no schema
     unit, and the plan's "28" is not reproducible from any counting scheme (4
     files, 16 `##` sections, 31 `##`+`###`). **Ask the maintainer what a recipe
     is before locking; fall back to `recipe = one solution file` (count = 4) if
     no timely answer.**
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

> **D4 (RESOLVED 2026-07-16, eng review) — RESULT table rendering deferred to a
> GitHub issue; slice 2 ships only the data layer.** RESULT bodies are full GFM
> markdown. Rendering them client-side adds a markdown renderer to the wasm bundle,
> which tensions the same sub-MB gate D3 cited when deferring KaTeX. **Decision:**
> the core parses each exhibit into the manifest as a **raw markdown body string**
> and resolves node→exhibit linkage (slice 2); the viewer's RESULT block shows the
> evidence chips + linkage in v1; **client-side markdown rendering of the tables is
> raised to a GitHub issue** ([ARA-Labs/ara-cli#32](https://github.com/ARA-Labs/ara-cli/issues/32)) and gated on a bundle-size check (pick a light pure-Rust
> markdown crate, verify the bundle stays under the gate, else fall back). Same
> treatment as D3/T-MATH-RENDER. **Consequence:** N07's RESULT shows which exhibits
> apply (chips + linkage), not the rendered fig3/figB.1 tables, in v1.

### Hub mode

12. **T-HUB-FIGURES**: once figures render, image `src` must resolve under
    `<base href="/a/{id}/">` and the hub needs `/a/{id}/api/figure/*`. The
    sampled artifacts use **markdown tables, not image files**, so image serving
    may not even be on the critical path — verify. Build figure-image serving in
    the same change that renders images (with `../` traversal tests), per the
    existing T-HUB-FIGURES note.

## Implementation steps (suggested slices, each shippable + patch-bump)

Restructured per eng review (E-seq): **land the full `ara-core` schema in ONE
core slice** rather than widening the `Manifest` across four UI slices. Every
schema change re-baselines `insta` snapshots and (if it touches the UI)
regenerates the embedded wasm bundle (the expensive `viewer-embed-fresh` gate);
splitting the additive, serde-defaulted schema across slices only buys snapshot
churn + rebase friction. Core-only slices do NOT regen the bundle; UI slices
regen once each when the UI actually changes.

1. **Node-body widening** (T-REAL-CORPUS core, pure core): dead-end 3 fields +
   `Pivot { from, to, trigger }` + `NodeKind::Pivot`. Snapshot tests over
   vendored fixtures; assert the ×67 dead-end-field warnings drop to zero on the
   corpus. Verified: all 6 real `pivot` nodes carry an `id`, so the id-drop
   error path is not tripped. No UI, no bundle regen.
2. **Full manifest schema + all readers + resolution** (pure core, ONE
   rebaseline, no bundle regen): `paper` / `related_work` / `concepts` /
   `problem` / `recipes` (solution) / `exhibits`, all readers in `parse_dir`
   (E1), both resolution passes (node→exhibit column-name-tolerant E5,
   node→claims→RW). Decide the RESULT resolution rule here, validated against
   ≥10 artifacts (E5). No table AST (E6). Enumerated malformed/partial fixtures
   per reader (E-tests). This is the whole data layer in one reviewable slice.
3. **PAPER.md paper header + Abstract** (UI): `PaperMeta` → viewer header +
   Abstract `<details>`. Smallest visible win; consumes slice-2 schema.
4. **Per-node blocks** (UI): un-inert detail-pane slots → WHAT IT DID (relabel)
   → evidence chips → BUILT ON (RW chips). RESULT block shows evidence chips +
   linkage only; **table rendering is D4 (deferred to a GitHub issue)**.
5. **Shared `Modal` + Dependencies panel** (UI): build the reusable a11y `Modal`
   component first with its wasm a11y tests (E7 / GAP-12), then the Dependencies
   overlay as its first consumer.
6. **Glossary + Context + Recipes panels** (UI): three more `Modal` consumers.
   **Blocked on E8** (maintainer's definition of a "recipe" / the count).
7. **ARTIFACT pointer** + **hub figure serving** (only if images are actually
   used by any artifact; the sampled artifacts use markdown tables, not images —
   verify first).
8. **RESULT markdown rendering** (D4, tracked as a GitHub issue): client-side
   markdown renderer for exhibit bodies, gated on a wasm bundle-size check.

Per-step: bump patch version + `CHANGELOG.md` entry (functional). Each core step
extends the `insta` snapshots and the `corpus_no_panic` net. Run
`cargo test --workspace` + wasm build + `scripts/embed-viewer.sh --check`.

## Risks / decisions to lock before coding

- **Resolution rule for RESULT** (claim-based vs direct-ref) — RESOLVED
  claim-based (rule 1), but the evidence-index parser must be **column-name-
  tolerant** and the rule **validated against ≥10 artifacts** before locking (E5;
  verified: 5 header variants across 32 artifacts, 2 with no `Claims` column).
  Blocks slice 2's resolution pass.
- **Table rendering location** — RESOLVED: **client-side, deferred to a GitHub
  issue (D4)**. No core table AST (E6). Slice 2 carries raw markdown strings.
- **Reader placement** — RESOLVED: readers live in **`parse_dir` only** (E1);
  `parse_sources` stays the pure 2-arg core; the wasm client only deserializes
  the enriched `/api/manifest` JSON. Confirm the enriched manifest serializes and
  the static `manifest.json` fallback still round-trips (serde-default old
  manifests — add an explicit round-trip test, GAP-1).
- **Recipes unit / count** — OPEN (E8): ask the maintainer what a "recipe" is;
  fall back to `recipe = one solution file` (count = 4). **Blocks slice 6.**
- **Schema drift**: model the *observed* convention now; T-ARA-SCHEMA swaps to a
  published schema later. Keep readers tolerant (warn, never fatal) so
  non-conforming artifacts still open.
- **Scope of ARADemo corpus**: verify conventions hold on `ARA-Labs/ARA-Demo`
  too (it uses a DOM tree-list viewer), not just paperbench. **Load-bearing** —
  5/32 paperbench artifacts already lack `related_work.md`, so BUILT ON /
  Dependencies being absent is a normal state, not an error path.

## Definition of done

`ara serve` on `paperbench/self-composing-policies` renders, for N07, in the
corrected hub order: paper header + abstract, WHAT IT DID (`result`), evidence
chips, BUILT ON (RW01/RW09), RESULT (**exhibit chips + node→exhibit linkage for
fig3 + figB.1; table markdown rendering deferred to D4**), and the four populated
header panels (Context, Glossary 12, Dependencies 9, Recipes — count per E8) —
**in our warm-cream + glyph-only skin (D2), with REASONING handled per D1**. Every
new surface has its empty/partial/error/loading state (Pass 1) and its <800px
behaviour (Pass 5). The shared `Modal` component (E7) satisfies the a11y contract
(focus-trap, Esc, return-focus, scrim-close) and has **mandatory wasm a11y tests in
`tests/web.rs`** (GAP-12); all four panels consume it. The five new `parse_dir`
readers have **enumerated malformed/partial/empty fixtures** (missing `---` fence,
RW block with no DOI / no `Claims affected`, concept term with no Definition,
evidence index row → missing file, node with a claim but no matching exhibit, RW
referenced but file absent), each asserting warn-not-fatal + correct partial output.
Old manifests still deserialize (serde-default round-trip, GAP-1). Corpus sweep
emits zero dead-end-field warnings. All snapshots updated; embedded bundle fresh.

## Design decisions to lock before implementation (from /plan-design-review)

- **D1 — REASONING vs the no-LLM-at-view-time promise** (RESOLVED). v1 drops
  REASONING and leads with WHAT IT DID (the structured `result`); a REASONING slot
  stays inert until the schema carries a stored `reasoning:` field. Rationale: the
  hub's REASONING is LLM-generated at publish time and absent from source, so
  reproducing it would break the "never fake prose at view time" promise.
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
- **D4 — RESULT table rendering** (RESOLVED, eng review). Client-side markdown,
  deferred to a GitHub issue; slice 2 ships raw markdown strings in `exhibits`.

## Eng review decisions (2026-07-16, /plan-eng-review)

- **E1 — Readers in `parse_dir` only.** `parse_sources` stays the pure 2-arg
  (tree + claims) core; the wasm client only deserializes the enriched
  `/api/manifest` JSON (`source.rs:64`). Drops the wrong "wasm passes them too"
  wording.
- **E2 — Section parsers integration-tested in `parse_dir`** (over on-disk
  fixtures), per user choice. Tradeoff accepted: malformed-input edge cases are
  disk-fixture-only, not pure unit tests.
- **E4 — Manifest section named `exhibits: Vec<Exhibit>`**, not `evidence` —
  avoids collision with the node-level `evidence:` (claim-refs) concept.
- **E5 — Evidence-index parser is column-name-tolerant**; RESULT resolution rule
  validated against ≥10 artifacts before locking (5 header variants across 32
  artifacts, 2 with no `Claims` column).
- **E6 — No table AST in core** (follows D4).
- **E7 — Build one shared a11y `Modal` component first**, before any panel; all
  four panels consume it. Mandatory wasm a11y tests (GAP-12).
- **E8 — Recipes count is an OPEN QUESTION.** Ask the maintainer what a "recipe"
  is; fall back to `recipe = one solution file` (count = 4). Blocks slice 6.
- **E-seq — Land the full `ara-core` schema in ONE core slice** (slice 2), not
  spread across UI slices — avoids repeated snapshot rebaselines + bundle regens.
- **E-tests — Enumerate malformed/partial/empty fixtures** per reader; add a
  serde-default old-manifest round-trip test (GAP-1).

## NOT in scope

- **RESULT table markdown rendering** — deferred to a GitHub issue (D4); v1 ships
  the data layer + linkage only.
- **KaTeX / real math rendering** — inert monospace for v1 (D3, T-MATH-RENDER).
- **REASONING narrative block** — dropped for v1 (D1); inert slot reserved.
- **`E##` proof-ref registry** (T-EVIDENCE) — no registry upstream; refs stay raw.
- **Hub figure-IMAGE serving** (slice 7) — only if any artifact uses image files;
  the sampled artifacts use markdown tables, so verify before building.
- **`T-PARSE-DEPTH`, `T-EDGE-ROUTING`, `T-VIEWER-TREE-LIST`** — unrelated deferred
  backlog, not touched here.
- **Adopting a published ARA schema** (T-ARA-SCHEMA) — model the observed
  convention now; swap later without changing the viewer.

## What already exists (reused, not rebuilt)

- **`parse_sources` / `parse_dir` / `Normalizer`** (`parse.rs`) — the tolerant,
  warn-never-fatal pipeline the new readers extend. Reuse the `parse_claims`
  pattern (pure str→struct) as the model for new section parsers.
- **`serde` default + `skip_serializing_if`** round-tripping (`manifest.rs`, as
  `isolated`/`pos` already do) — new sections ride the same additive path.
- **`DetailModel` / `DetailPane` + reserved inert slots** (`detail.rs:386`) — the
  new blocks plug into slots already reserved and CSS-styled; do not invent chrome.
- **`kind_meta` glyph source + `.block` / `.block.reason` styling** — reused for
  the new blocks and the Pivot kind.
- **Parse-once + `ArcSwap` cache + ETag/304** (`serve/mod.rs`) — the enriched
  manifest rides the existing cache; no per-request cost, no new serving path
  (except optional slice-7 figure images).
- **`corpus_no_panic` net + `insta` snapshot harness** (`tests/`) — extended, not
  replaced, for the new schema.
- **`<base href>` + relative-URL manifest fetch** (`source.rs`) — the enriched
  manifest flows through the existing local + hub `/api/manifest` path unchanged.

## Implementation Tasks
Synthesized from this review's findings. Each task derives from a specific
finding above. Run with Claude Code or Codex; checkbox as you ship.

- [ ] **T1 (P1, human: ~1d / CC: ~40min)** — ara-core — Land full manifest schema
  + `parse_dir` readers + resolution passes in one core slice
  - Surfaced by: E1, E-seq — readers in `parse_dir`; single schema rebaseline
  - Files: `crates/ara-core/src/manifest.rs`, `crates/ara-core/src/parse.rs`
  - Verify: `cargo test --workspace`; snapshots rebaselined; no bundle regen
- [ ] **T2 (P1, human: ~1h / CC: ~15min)** — ara-core — Widen `DeadEnd` (3 fields)
  + add `Pivot { from, to, trigger }` + `NodeKind::Pivot`
  - Surfaced by: T-REAL-CORPUS core; ×67 warnings; 6 real pivot nodes (all have ids)
  - Files: `crates/ara-core/src/manifest.rs`, `crates/ara-core/src/schema.rs`, `parse.rs`
  - Verify: corpus sweep emits zero dead-end-field warnings
- [ ] **T3 (P1, human: ~half day / CC: ~30min)** — ara-core — Column-name-tolerant
  evidence-index parser; validate RESULT rule against ≥10 artifacts
  - Surfaced by: E5 — 5 header variants across 32 artifacts, 2 with no Claims column
  - Files: `crates/ara-core/src/parse.rs`, `crates/ara-core/tests/fixtures/`
  - Verify: fixtures for each header variant; missing-column → warn-not-fatal
- [ ] **T4 (P1, human: ~1d / CC: ~40min)** — ara-core/tests — Enumerate
  malformed/partial fixtures per reader + serde-default round-trip test
  - Surfaced by: E-tests, GAP-1..GAP-9 — "tolerant" must be verified
  - Files: `crates/ara-core/tests/`
  - Verify: each malformed case asserts warn-not-fatal + correct partial output
- [ ] **T5 (P1, human: ~1.5d / CC: ~40min)** — ara-viewer — Build shared a11y
  `Modal` component with wasm a11y tests, before any panel
  - Surfaced by: E7, GAP-12 — focus-trap/return-focus is the headline a11y feature
  - Files: `crates/ara-viewer/src/`, `crates/ara-viewer/tests/web.rs`
  - Verify: focus in-on-open, trap, Esc, return-focus, scrim-close all tested
- [ ] **T6 (P2, human: ~30min / CC: ~5min)** — plans/docs — Ask maintainer for the
  "recipe" definition + count before slice 6
  - Surfaced by: E8 — "28" is ungrounded; source defines only `solution/` = 4 files
  - Files: `plans/hub-parity-full.md`
  - Verify: count unit locked; fall back to file=recipe (4) if no answer

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 1 | issues_found | outside voice (Claude subagent; codex not authed): 8 findings, 3 verified-new folded |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | issues_open | 11 issues folded (E1–E8, E-seq, E-tests, D4); 1 open (E8 recipe count) |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | clean | score 6.5→8/10, D1–D3 resolved |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

**Eng review summary (2026-07-16):** Scope confirmed — full 6-slice parity
(user chose complete over reduced). Architecture: 2 findings (E1 reader
placement → `parse_dir` only, verified against `source.rs:64`; E2 parser seam →
integration-tested per user), 1 verified-clear (all 6 `pivot` nodes carry ids →
no id-drop). Code quality: 3 (E4 `exhibits` rename; D4 RESULT rendering
deferred; DoD corrected). Tests: coverage diagram produced, ~15 gaps — 2
resolved as decisions (GAP-12 mandatory wasm a11y tests; GAP-2..9 enumerated
malformed fixtures), rest folded. Performance: 0 issues — parse-once + ArcSwap
cache + ETag/304 already handles the tiny corpus scale (max 236 nodes, 9 RW, 9
exhibits per artifact).

**Outside voice (Claude subagent — Codex installed but not authed):** sampled
all 32 artifacts (my review had calibrated on N07's artifact only) and surfaced
4 verified-new findings, all confirmed against the corpus and folded: E5
(evidence-index format not stable — 5 header variants, 2 with no `Claims`
column → column-name-tolerant parser + validate rule vs ≥10 artifacts), E8
(`Recipes 28` is ungrounded — source defines only `solution/` = 4 files → open
question for the maintainer), E-seq (schema spread across 4 slices → land in one
core slice), E7 (modal component unspecced → shared `Modal` spike first). Its
other findings (RESULT deferral, drop core table AST, sequencing) overlapped
decisions already made in-session.

**CROSS-MODEL:** no tension — the outside voice agreed with the eng-review
direction on RESULT deferral and dropping the core table AST, and its new
findings were verified and folded rather than contested.

**VERDICT:** DESIGN + ENG reviewed; plan materially strengthened and internally
consistent. Ready to implement starting with slice 1 (node-body widening, pure
core) then slice 2 (full core schema) — with **one open item (E8)** to resolve
before slice 6 (Glossary/Context/Recipes panels).

**UNRESOLVED DECISIONS:**
- **E8 — the "recipe" unit / count is undefined.** Source (`AmberLJC/ara-paperbench` README) defines only `solution/` = 4 files; the plan's "28" is not reproducible. Ask the maintainer before locking slice 6; fall back to `recipe = one solution file` (count = 4) if no timely answer. Does not block slices 1–5.
