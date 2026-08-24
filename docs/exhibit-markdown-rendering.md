# Exhibit bodies — client-side markdown rendering in the RESULT block

Design record for the exhibit-body renderer shipped in
[#45](https://github.com/ARA-Labs/ara-cli/pull/45) (issue
[#32](https://github.com/ARA-Labs/ara-cli/issues/32), released in `0.1.11`). It
flips hub-parity item **D4** from deferred to shipped: a node's figures and
tables are now rendered as real HTML in the detail pane instead of being listed
as bare chips.

Companion docs: [`hub-parity.md`](hub-parity.md) (the parity design this
completes) and [`stage-3-viewer.md`](stage-3-viewer.md) (the viewer it lives in).

## Problem / background

`ara-core` already parsed every `evidence/figures/*.md` and
`evidence/tables/*.md` into `Manifest.exhibits`, and each `Exhibit` carried its
**raw GFM markdown body verbatim** in `Exhibit.body`. Node→exhibit linkage was
resolved into `Manifest.node_exhibits`, and the viewer deserialized the whole
manifest — so the body was already present in the browser. The RESULT block just
dropped it: `ExhibitView` deliberately did not carry `body`, and the block
rendered chips only (exhibit id + kind label, file/source in a hover tooltip).

D4 deferred this because a client-side markdown renderer adds wasm weight, in
tension with the sub-MB bundle gate that also deferred KaTeX (D3). Measurement
showed the tension was small in practice, so the gate was cleared rather than
worked around.

## Renderer — `pulldown-cmark`, tables subset

`crates/ara-viewer/src/markdown.rs` renders a body with
`pulldown-cmark = { version = "0.13", default-features = false, features = ["html"] }`.
Only the extensions the corpus needs are enabled: `ENABLE_TABLES` and
`ENABLE_STRIKETHROUGH`.

- Lightest realistic pure-Rust option — pull parser, no AST, three small
  transitive deps (`bitflags`, `pulldown-cmark-escape`, `unicase`).
- comrak buys GitHub-exact output we do not need at meaningfully more weight;
  `markdown-rs` was a viable lighter alternative, but pulldown-cmark is the
  ecosystem default and what #32 named.
- **Math stays inert (D3).** The math extension is off, so `$…$` in an exhibit
  body renders as literal text — the same "never interpreted" posture as
  `latex_view`. Real math rendering remains
  [#31 / `T-MATH-RENDER`](https://github.com/ARA-Labs/ara-cli/issues/31).

## Mounting — `inner_html`, with both injection vectors closed

The renderer emits an HTML string that is mounted via Leptos's `inner_html` on a
wrapper `<div class="exhibit-body">` (`detail.rs`). This is the **first and only
`inner_html` in the viewer**, which otherwise emits exclusively escaped Leptos
nodes; the exception is deliberate, bounded to exhibit content, and paid for by
sanitizing at the event level before any HTML is produced:

1. **Raw HTML in the source is escaped.** `Event::Html` / `Event::InlineHtml`
   are re-emitted as `Event::Text`, so author markup never reaches the DOM.
2. **Link and image destinations are scheme-checked** against an allowlist
   (`http`, `https`, `mailto`; scheme-less relative URLs are also allowed).
   Anything else — `javascript:`, `data:`, `vbscript:`, `file:` — is rewritten
   to `#`. ASCII whitespace and control characters are stripped before the
   scheme is read, because browsers ignore them when parsing a scheme, so
   `java\tscript:` cannot slip through.

The scheme guard matters because the corpus is not always self-authored:
`ara serve` may render a **downloaded** artifact.

The alternative considered was an event-walk building Leptos nodes directly. It
would have preserved the no-`inner_html` invariant, but at the cost of owning
~150 hand-written lines against pulldown-cmark's fuzzed HTML writer, plus a
`createElement` call per node across the wasm↔JS boundary. Decided in favour of
`inner_html` on 2026-07-20.

## Bundle budget — the acceptance gate

The bundle delta was measured **before** any UI work, since a failure meant
falling back to a lazily-loaded JS renderer or a core-side table AST. Measured
at merge (pulldown-cmark 0.13.4, wired into the RESULT block so it survives
dead-code elimination):

| | baseline | with renderer | delta | budget | % of budget |
|---|---|---|---|---|---|
| uncompressed | 522,465 | 682,821 | +160,356 | 1,048,576 | 65.1% |
| brotli q11 | 172,800 | 229,058 | +56,258 | 358,400 | 63.9% |

Both cleared with room to spare, so neither fallback was needed. The budgets are
enforced on every PR by the `viewer-size` CI job
(`.github/workflows/ci.yml`) — 1 MB uncompressed, 350 KB brotli — which is what
keeps a future renderer or font from silently eating the headroom.

## Rendering and layout

- `ExhibitView` carries `body`, populated from `ex.body` in `detail_model`.
- Each exhibit chip is followed by its rendered body in a
  `<div class="exhibit-body">`; blank bodies are skipped.
- The `.exhibit-body { overflow-x: auto; max-width: 100% }` overflow contract
  is unchanged: a wide table scrolls **inside its own block** rather than
  pushing the page into a horizontal scroll at narrow viewports.
- Existing table visual declarations are unchanged and remain shared with the
  reserved `table.md` slot: `.exhibit-body table` / `th` / `td` use collapsed
  borders, cell padding, and a shaded header.
- Non-table CommonMark styling covers headings, paragraphs, lists,
  blockquotes, inline and fenced code, links, and horizontal rules. These
  styles reuse the detail pane's `--ink`, `--muted`, `--panel2`, `--line`,
  `--font-mono`, `--accent-text`, and `--accent` tokens rather than adding
  exhibit-only tokens.
- The mixed-flow selector `.exhibit-body > :not(table) + table` adds `0.75rem`
  before a table only when top-level non-table content immediately precedes
  it. A first-child table and consecutive tables retain their existing
  boundary.

## Testing

- Native unit tests in `markdown.rs`: a GFM table produces `<table>`/`<th>`/
  `<td>`; raw `<script>` is escaped; `$x$` stays literal; `javascript:`,
  entity-encoded, control-character-evading, and `data:` image destinations are
  all neutralised; safe and relative links survive; `is_safe_url` is classified
  directly.
- Headless wasm test `exhibit_body_renders_table_in_scroll_container`
  (`crates/ara-viewer/tests/web.rs`) asserts the detail pane renders a real
  `<table>` with the expected cell text inside the `.exhibit-body` container.
- Headless wasm test `non_table_exhibit_markdown_uses_viewer_styles` mounts the
  mixed CommonMark fixture with the real viewer stylesheet and checks computed
  heading hierarchy, nested-flow spacing, list, blockquote, code, link, and
  rule styles. It also checks the unchanged table skin and `.exhibit-body`
  overflow contract, including the `0.75rem` mixed-flow table boundary and
  zero margin for a first-child table.
- Source-viewer browser verification checked the actual detail-pane surface at
  1280×800, 799×700, and 375×667. Computed-style and geometry audits passed,
  and screenshot inspection found no clipped markers, pane-level horizontal
  overflow, nested code boxes, changed table appearance, overlap, or hierarchy
  inversion.
- `scripts/embed-viewer.sh` was re-run so the committed bundle in
  `crates/ara-cli/assets/viewer/` matches the viewer source; the
  `viewer-embed-fresh` CI job enforces this.

## Known gaps (tracked)

- **Figure images are not rendered** as inline images
  ([#60](https://github.com/ARA-Labs/ara-cli/issues/60), the `T-HUB-FIGURES`
  follow-on). The sampled corpus is overwhelmingly markdown tables.
- **Math is inert** ([#31](https://github.com/ARA-Labs/ara-cli/issues/31)).
