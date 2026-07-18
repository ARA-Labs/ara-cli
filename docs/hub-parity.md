# Viewer parity with the ARA Hub — logic layer, per-node blocks & panels

Design doc for the hub-parity work (workspace `0.1.7`). The official ARA Hub
renders far more of an artifact than the local viewer did: a paper header, a
per-node detail pane (what a node did, its evidence, what it built on, its
results), and global panels (Context, Glossary, Dependencies, Recipes). This
change models what the corpus actually carries and renders those surfaces —
**without** importing the hub's look. It implements the backlog item
`T-REAL-CORPUS` and the adjacent `T-HUB-FIGURES` / `T-EVIDENCE` groundwork.

The split is unchanged: `ara-core` models the data (native + wasm, deterministic,
snapshot-tested); `ara-viewer` renders it. Every new section is `Option`/`Vec`,
`serde`-defaulted, and `skip_serializing_if`-empty, so old manifests round-trip
and artifacts lacking a file serialize identically to before.

## What "parity" means here (D2)

There is **no single hub skin to match**: each artifact's baked
`trace/exploration_tree.html` is LLM-generated at publish time, so its colour
theme and typography differ per artifact — a non-reproducible target. We keep
the vendored **warm-cream + glyph-not-colour** skin (a deliberate,
colour-blind-safe choice) and port only the hub's *structure*: which sections
exist, their order, and the panels. We do **not** import the hub's serif,
per-kind colours, or any artifact-specific styling.

The same root cause governs REASONING (D1): the hub's REASONING block is
LLM-generated narrative prose baked in at publish time and **absent from every
source file**. Reproducing it would break the product's core promise — *renders
the YAML directly, never calls an LLM at view time*. So the structured `result`
field is the top per-node block, labelled **WHAT IT DID** (the hub's own label);
a REASONING slot stays inert until the schema ever carries a stored `reasoning:`
field.

## Corpus conventions (observed, not yet a published schema)

The corpus uses a stable, observable convention, modelled now and swappable for
a published schema later (`T-ARA-SCHEMA`) without changing the viewer:

- **`PAPER.md`** — YAML frontmatter: `title`, `authors[]`, `year`, `venue`,
  `doi`, `abstract`, `keywords[]` (+ ignored extras like `domain`). One artifact
  opens with no `---` fence; the title then falls back to the first `# H1`.
- **`logic/problem.md`** — `## Observations` (O#), `## Gaps` (G#), `## Key
  Insight` (I#), a statement/setting; three heading dialects are tolerated.
- **`logic/concepts.md`** — `## <Term>` blocks with `Notation` / `Definition` /
  `Boundary conditions` / `Related concepts`; heavy inline LaTeX.
- **`logic/related_work.md`** — `## RW0N: <cite>` with `DOI`, `Type`
  (baseline/imports/bounds/refutes/extends), `Delta` (What changed / Why),
  `Claims affected` (`C##` list, linking RW → claim → node), `Adopted elements`.
- **`logic/solution/*.md`** — the recipes (filenames are **not** stable; the
  reader enumerates the directory rather than assuming names).
- **`evidence/README.md`** — one or more index tables mapping each figure/table
  file to its paper `Source` and `Claims`; **8 header variants** exist across the
  corpus, **4 with no `Claims` column**.
- **`evidence/{figures,tables}/*.md`** — caption / axes / markdown data tables.

## Data model (`ara-core`)

`NodeKind` gains `Pivot`; `NodeFields::DeadEnd` gains
`hypothesis`/`failure_mode`/`lesson` (keeping `why_failed`); `NodeFields::Pivot`
carries `from`/`to`/`trigger`. `Manifest` gains eight sections:

| Field | Source | Notes |
|---|---|---|
| `paper: Option<PaperMeta>` | `PAPER.md` frontmatter | `year` normalized to `String` (int-or-string); `doi: null` → `None` |
| `problem: Option<Problem>` | `logic/problem.md` | statement + observations/gaps/insights |
| `concepts: Vec<Concept>` | `logic/concepts.md` | term + notation/definition/boundary + related |
| `related_work: Vec<RelatedWork>` | `logic/related_work.md` | id, cite, kind, delta, adopted, `claims_affected` |
| `recipes: Vec<Recipe>` | `logic/solution/*.md` | one per file (name, title, raw body) |
| `exhibits: Vec<Exhibit>` | `evidence/**` | id, file, kind, source, description, claims, **raw markdown body** |
| `built_on: Vec<BuiltOn>` | resolution | node → related-work id |
| `node_exhibits: Vec<NodeExhibit>` | resolution | node → exhibit id |

The section is named `exhibits`, **not** `evidence` (E4): the node-level
`evidence:` concept (`C##` claim-refs + prose) is unrelated and already modelled
as `Binding` + `evidence_notes`; a second `evidence` would mislead every reader.

### Readers (`parse_dir` only, E1)

`parse_sources` stays the pure 2-arg (tree + claims) wasm-safe core; the wasm
client only deserializes the already-built `/api/manifest` JSON. All new readers
live in `parse_dir` (native), gated behind the `native` feature so the wasm
client build stays warning-free. Each reader is tolerant: an **absent file is
skipped silently**; a **malformed present file warns**, never fatal, never
panics. The `evidence/README.md` index parser matches columns by **header name,
not position** (`file` / `claim`|`key ref`|`used by`), normalizes file cells
across markdown-link / backtick / `(png/md)` forms, and falls back to an inline
`Supports: C##` body line when a table has no claims column.

### Resolution passes (deterministic, source-order)

Two passes run in `parse_dir` after the base manifest and its `bindings` exist:

- **node → exhibit** (RESULT): a node's claims (from `bindings`) ∩ an exhibit's
  claims → `node_exhibits`.
- **node → related-work** (BUILT ON): a node's claims ∩ an RW's `claims_affected`
  → `built_on`.

Validated across 14 artifacts: `self-composing-policies` **N07** resolves to
exactly `{fig3_scalability, figb1_memory_growth}` with `built_on` `{RW01, RW09}`;
artifacts with no `C##` refs resolve empty — never wrong.

### Cycle tolerance for ancestor back-edges

Real traces have children that restate `also_depends_on` on their own parent.
Combined with the parent→child nesting edge this closes a cycle, which the
Stage-1 acyclicity check flagged as fatal — making such artifacts (including the
sampled one) fail to open at all. A dependency whose target is an **ancestor**
of the source is redundant (the nesting already encodes it) and is now dropped
with a *warning*. Genuine cross-cycles (a dependency on a sibling or descendant
that closes a loop) remain fatal.

## Viewer surfaces (`ara-viewer`)

- **Paper header** — title, an authors · venue · year byline (absent parts
  dropped cleanly), and a collapsed `<details>` Abstract. Falls back to the "ARA
  Viewer" brand when the manifest carries no titled paper.
- **Per-node detail blocks**, in the corrected hub order: *(inert REASONING
  slot)* → **WHAT IT DID** (`result`, relabelled) → **evidence** (notes +
  claims) → **BUILT ON** (related-work chips) → **RESULT** (exhibit chips +
  linkage) → provenance. BUILT ON and RESULT omit entirely when empty. RESULT
  shows *which* exhibits apply (chips + `node→exhibit` linkage), **not** the
  rendered figure/table bodies (D4).
- **Shared `Modal`** (E7) — one reusable, accessible dialog: `role="dialog"` +
  `aria-modal` + `aria-labelledby`, focus moves in on open, a Tab/Shift+Tab
  focus trap that wraps at both ends, Esc and scrim-click close, focus returns
  to the invoking element on close, and it goes full-screen below 800px. The
  browser-only focus/key logic is `wasm32`-gated; a mandatory `wasm-bindgen`
  a11y suite covers the whole contract.
- **Four header panels**, each a `Modal` consumer with a live count (hidden at 0)
  and its own case-insensitive filter: **Context** (problem framing),
  **Glossary** (concept terms with dotted cross-reference chips), **Dependencies**
  (related work), **Recipes** (solution files). Concept/recipe LaTeX renders as
  inert monospace (`$…$` kept verbatim, never interpreted — D3).

## Deferred (tracked, not in this design)

- **RESULT table/markdown rendering** (D4) — exhibits carry raw markdown bodies
  in the manifest; client-side rendering is gated on a wasm bundle-size check and
  tracked in [ARA-Labs/ara-cli#32](https://github.com/ARA-Labs/ara-cli/issues/32).
- **KaTeX / real math** (D3, `T-MATH-RENDER`) — inert monospace for now.
- **REASONING** (D1) — inert slot, pending a stored `reasoning:` field.
- **Figure-image serving** (`T-HUB-FIGURES`) — the corpus is overwhelmingly
  markdown tables (the sampled artifact has zero image files) and no v1 surface
  renders exhibit bodies, so there is no consumer yet.
- **ARTIFACT code-pointer** — code linkage is not modelled.
- **The "recipe" unit** (E8) — undefined upstream; the Recipes count uses the
  fallback of one recipe per `logic/solution/*.md` file, pending a maintainer
  answer.

## Testing

`insta` snapshots over a full vendored artifact lock the parsed sections and the
`node→exhibit` / `node→RW` linkage (exhibit/recipe bodies are redacted to keep
the snapshot readable). Enumerated malformed/partial/empty fixtures per reader
assert warn-not-fatal; a serde-default round-trip test proves old manifests
still deserialize. The always-on `corpus_no_panic` net and the opt-in full
corpus sweep guard robustness. The viewer's `wasm-bindgen` suite covers the
`Modal` a11y contract and each panel's count/hide/open/filter behaviour, plus
the paper header and the per-node blocks. End to end, `ara serve` on the sampled
artifact returns an enriched `/api/manifest` — paper title set, concepts 12,
related_work 9, recipes 4, exhibits 9, with N07's exhibit and built-on linkage
as above.
