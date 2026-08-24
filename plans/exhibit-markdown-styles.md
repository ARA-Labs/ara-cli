# Exhibit Body Markdown Styles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` or `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Style layout-bearing non-table CommonMark elements in exhibit bodies so they match the viewer’s detail pane, while retaining native bold and italic emphasis and without changing markdown parsing, sanitization, table styling, or table overflow.

**Architecture:** Keep `render_exhibit_body` and the `.exhibit-body` mounting path unchanged. Add only `.exhibit-body`-scoped CSS, cover the shipped stylesheet through a headless-browser computed-style test, and verify the result in the embedded viewer at normal and narrow pane widths.

**Tech stack:** CSS, Leptos 0.8, `pulldown-cmark` 0.13, `wasm-bindgen-test`, headless Chrome, Trunk.

**Spec:** [GitHub issue #46](https://github.com/ARA-Labs/ara-cli/issues/46), with renderer constraints recorded in [`docs/exhibit-markdown-rendering.md`](../docs/exhibit-markdown-rendering.md).

## Problem Background

Issue #32 made exhibit bodies real HTML. `render_exhibit_body` already emits CommonMark headings, paragraphs, lists, blockquotes, inline code, fenced code, links, horizontal rules, emphasis, and tables. It also escapes raw HTML and neutralizes unsafe link and image schemes before Leptos mounts the result through `inner_html`.

The viewer’s global reset sets every element’s margin and padding to zero. Only tables have `.exhibit-body` rules today, so headings have no intentional scale or spacing, lists lose indentation, blockquotes have no visual treatment, and code blocks do not match the viewer’s existing monospace panels. This is a stylesheet gap, not a parser or data-model gap.

## Proposed Solution

Add baseline typography and spacing under `.exhibit-body` in `crates/ara-viewer/public/styles.css`:

- explicit vertical rhythm for non-table top-level blocks;
- a restrained `h1`–`h6` scale bounded by the existing `.detail-title` size;
- list indentation, item spacing, and nested-list spacing;
- a muted blockquote with a `--line` left border;
- separate inline-code and fenced-code treatments using `--font-mono`, `--panel2`, and `--line`;
- link color plus hover and keyboard-focus states;
- a horizontal rule using `--line`.

Keep the existing `.exhibit-body`, table, `th`, and `td` declarations intact. Do not change `render_exhibit_body`, `DetailPane`, markdown options, URL sanitization, image handling, or math handling.

## Information Hierarchy

The detail pane keeps one primary anchor. Markdown structure supports that anchor
rather than creating a second page title:

```text
1. .detail-title                     selected node; sole primary title
2. exhibit h1 / h2                   structural scanning layer
3. prose, lists, quotes, code, table evidence, and h3–h6
                                      uninterrupted reading layer
```

The RESULT block label, exhibit chip, and optional caption remain contextual
chrome around this hierarchy. If only three things can attract attention, they
are the selected node title, the exhibit’s structural headings, and its evidence
content, in that order.

## What Already Exists

- No repository-level `DESIGN.md` exists. This plan therefore treats the shipped
  stylesheet as the local source of truth instead of inventing a second system.
- `.detail-title` owns the primary `1rem`/`600` node-title treatment.
- The global `code, kbd, samp, pre` rule already applies `--font-mono`; inline
  code inherits it without a duplicate scoped declaration.
- `pre.diff` supplies the approved code-container vocabulary: `--panel2`,
  `--line`, a 4px radius, compact mono type, and local horizontal scrolling.
- `.quote` remains the stronger pull-quote pattern. CommonMark blockquotes use
  the intentionally quieter source-annotation treatment defined below.
- `.exhibit-body` already owns table overflow containment, and its existing
  table, `th`, and `td` declarations remain the table visual contract.

## Scope Boundaries

**In scope:** `h1`–`h6`, `p`, `ul`, `ol`, `li`, `blockquote`, inline `code`, `pre`, `a`, `hr`, and their spacing inside `.exhibit-body`. `strong` and `em` intentionally retain their native font weight and style; they receive no new color or decoration.

## NOT in Scope

- Figure-image resolution remains tracked by #60.
- Math rendering remains tracked by #31.
- Raw HTML support remains excluded by the existing security boundary.
- Syntax highlighting and copy buttons would add interaction and dependency
  scope beyond baseline markdown readability.
- New design tokens or fonts would change the viewer-wide system.
- Renderer, sanitization, image, and math behavior remain unchanged.
- Shared prose components outside exhibit bodies require a separate viewer-wide
  design decision.
- New loading, empty, error, success, onboarding, or motion UI is unnecessary
  for this read-only CSS change.

## File Map

- Modify `crates/ara-viewer/tests/web.rs`: add a mixed-markdown fixture and a browser test that loads the shipped stylesheet and asserts computed-style invariants.
- Modify `crates/ara-viewer/public/styles.css`: add scoped non-table markdown rules next to the existing exhibit table rules.
- Modify `docs/exhibit-markdown-rendering.md`: record the shipped style coverage, browser contract, and remaining gaps.
- Modify `Cargo.toml`: bump the workspace patch version from `0.1.15` to `0.1.16` because the shipped viewer behavior changes.
- Modify `Cargo.lock`: refresh the four workspace-package versions after the
  workspace version bump.
- Modify `CHANGELOG.md`: add an `Unreleased` viewer entry for #46.
- Regenerate `crates/ara-cli/assets/viewer/` and `crates/ara-cli/assets/viewer.source-hash` with `scripts/embed-viewer.sh`.
- Remove `plans/exhibit-markdown-styles.md` after its content has been incorporated into the design record and all implementation checks pass. Git history retains the reviewed plan.

## Global Constraints

- Preserve the existing renderer and security boundary in `crates/ara-viewer/src/markdown.rs`.
- Preserve `.exhibit-body { overflow-x: auto; max-width: 100%; }` and all existing table declarations.
- Scope every new selector to `.exhibit-body`; no global element rules.
- Reuse `--ink`, `--muted`, `--accent-text`, `--accent`, `--line`, `--panel2`, and `--font-mono`; add no tokens or dependencies.
- Keep inline-code box styling off descendants of `pre` so fenced code receives one container treatment.
- Include keyboard focus styling for links; hover alone is insufficient.
- Do not commit implementation changes unless the human developer requests a commit.

## State Applicability

| State | User sees | Contract for this change |
|---|---|---|
| Loading | Existing skeleton | Unchanged; no `.exhibit-body` is present. |
| Empty or no selected node | Existing placeholder or `.empty-note` | Unchanged; add no new empty-state UI. |
| Load/render error | Existing `.error-card` | Unchanged; scoped selectors must not leak into it. |
| Loaded mixed markdown | Any supported combination of prose, headings, lists, quotations, code, links, rules, and tables | Styled as one document flow; elements may be absent without leaving artificial gaps. |
| Blank exhibit body | No exhibit body, as today | Unchanged; `DetailPane` continues to skip it. |
| Success | Not applicable | The detail pane is read-only and has no success-producing action. |

## Reading Journey

| Step | User does | Intended feeling | Plan support |
|---|---|---|---|
| 1 | Selects a graph or tree node | Oriented | `.detail-title` remains the sole primary anchor. |
| 2 | Opens or scans the RESULT block | Confident about context | Existing block label, exhibit chip, and caption remain unchanged. |
| 3 | Scans exhibit headings | In control of depth | `h1`/`h2` form a restrained structural layer; lower headings remain reading cues. |
| 4 | Reads prose, lists, quotations, code, and tables | Uninterrupted | Consistent rhythm and local overflow prevent layout jumps and pane-level panning. |
| 5 | Follows a reference | Certain it is interactive | Underline, hover, and keyboard-focus states remain visible. |

The first five seconds preserve orientation, five minutes of reading remain calm
and scannable, and repeated use stays compact enough for a research workspace.
No decorative motion, onboarding, or extra chrome is introduced.

---

### Task 1: Add a Failing Browser Style Contract

**Files:**
- Modify: `crates/ara-viewer/tests/web.rs`, near `EXHIBIT_BODY_FIXTURE_JSON` and `exhibit_body_renders_table_in_scroll_container`

**Interfaces:**
- Consumes: `DetailPane`, `parse_manifest`, the `.exhibit-body` wrapper, and `crates/ara-viewer/public/styles.css`.
- Produces: a browser regression test named `non_table_exhibit_markdown_uses_viewer_styles`.

- [ ] **Step 1: Add a mixed-markdown fixture and preserve the table-only fixture**

Leave `EXHIBIT_BODY_FIXTURE_JSON` unchanged so
`exhibit_body_renders_table_in_scroll_container` continues to represent a table
as the first and only body block. Add `MIXED_EXHIBIT_BODY_FIXTURE_JSON` with the
same manifest shell and prepend this representative markdown to E01’s table body:

````markdown
# Experiment result

## Findings

1. Train the baseline.

   Record the seed and environment before continuing.

   - Capture the logs.
     - Archive the raw output.
2. Compare the residual model.

> Residual learning converged reliably.
>
> Confirm the dataset and seed.
>
> #### Promotion gate
>
> Recheck `learning_rate` before promotion.
>
> ---

Use `learning_rate = 0.1` for this run.

The result is **reliable** and *repeatable*.

### Metrics

```text
baseline = 27.1
residual = 28.4
```

##### Detailed notes

Retain every checkpoint.

###### References

[https://example.com/experiments/residual-learning/notes](https://example.com/experiments/residual-learning/notes)

---
````

Keep the GFM table after this content so the new style contract covers the mixed
non-table-to-table boundary. The unchanged table-only fixture and
`exhibit_body_renders_table_in_scroll_container` continue to defend #32.

- [ ] **Step 2: Add stylesheet and computed-style helpers**

Add helpers that exercise the actual CSS rather than inspecting stylesheet source text:

```rust
fn install_viewer_styles(doc: &Document) -> web_sys::Element {
    let style = doc.create_element("style").unwrap();
    style.set_text_content(Some(include_str!("../public/styles.css")));
    doc.head().unwrap().append_child(&style).unwrap();
    style
}

fn computed_style(element: &web_sys::Element) -> web_sys::CssStyleDeclaration {
    web_sys::window()
        .unwrap()
        .get_computed_style(element)
        .unwrap()
        .expect("element must have computed style")
}
```

The existing wasm dev-dependencies already enable `Window`, `Element`, `HtmlHeadElement`, `Node`, and `CssStyleDeclaration`; do not change `Cargo.toml` for this helper.

- [ ] **Step 3: Add the computed-style regression test**

Install the stylesheet, mount `MIXED_EXHIBIT_BODY_FIXTURE_JSON` through the existing `mount_detail` helper, query the semantic elements, and assert behavior-level invariants:

```rust
fn computed_px(element: &web_sys::Element, property: &str) -> f64 {
    computed_style(element)
        .get_property_value(property)
        .unwrap()
        .trim_end_matches("px")
        .parse()
        .expect("computed property must use px")
}

fn assert_px_close(element: &web_sys::Element, property: &str, expected: f64) {
    let actual = computed_px(element, property);
    assert!(
        (actual - expected).abs() <= 0.05,
        "{property}: expected {expected}px, got {actual}px"
    );
}

#[wasm_bindgen_test]
fn non_table_exhibit_markdown_uses_viewer_styles() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let root = doc.document_element().expect("document root must exist");
    let rem = computed_px(&root, "font-size");
    let stylesheet = install_viewer_styles(&doc);
    let container = mount_detail(MIXED_EXHIBIT_BODY_FIXTURE_JSON, "N01");

    let body = container
        .query_selector(".exhibit-body")
        .unwrap()
        .expect("mixed exhibit body must render");
    let detail_title = container
        .query_selector(".detail-title")
        .unwrap()
        .expect("detail title must render");
    let headings = ["h1", "h2", "h3", "h4", "h5", "h6"].map(|selector| {
        body.query_selector(selector)
            .unwrap()
            .unwrap_or_else(|| panic!("{selector} must render"))
    });
    let h1 = headings[0].clone();
    let h3 = headings[2].clone();
    let list = body.query_selector("ol").unwrap().expect("list must render");
    let second_item = body
        .query_selector("ol > li + li")
        .unwrap()
        .expect("second list item must render");
    let nested_paragraph = body
        .query_selector("ol > li > p + p")
        .unwrap()
        .expect("multi-paragraph list item must render");
    let nested_list = body
        .query_selector("ol > li > ul")
        .unwrap()
        .expect("nested list must render");
    let prose = body.query_selector("p").unwrap().expect("paragraph must render");
    let quote = body
        .query_selector("blockquote")
        .unwrap()
        .expect("blockquote must render");
    let quote_second_paragraph = quote
        .query_selector("p + p")
        .unwrap()
        .expect("multi-paragraph quote must render");
    let nested_heading = quote
        .query_selector("p + h4")
        .unwrap()
        .expect("nested heading must render");
    let nested_rule = quote
        .query_selector("p + hr")
        .unwrap()
        .expect("nested rule must render");
    let inline_code = body
        .query_selector("p code")
        .unwrap()
        .expect("inline code must render");
    let pre = body.query_selector("pre").unwrap().expect("code block must render");
    let pre_code = pre.query_selector("code").unwrap().expect("pre code must render");
    let link = body.query_selector("a").unwrap().expect("link must render");
    let rule = body
        .query_selector(":scope > hr")
        .unwrap()
        .expect("top-level rule must render");
    let table = body.query_selector("table").unwrap().expect("table must render");
    let th = table.query_selector("th").unwrap().expect("header cell must render");
    let td = table.query_selector("td").unwrap().expect("data cell must render");

    for heading in &headings {
        let style = computed_style(heading);
        assert_eq!(style.get_property_value("font-weight").unwrap(), "600");
        assert_eq!(style.get_property_value("color").unwrap(), "rgb(47, 42, 35)");
        assert_eq!(style.get_property_value("word-break").unwrap(), "break-word");
        let ratio =
            computed_px(heading, "line-height") / computed_px(heading, "font-size");
        assert!((ratio - 1.3).abs() <= 0.02, "heading line-height ratio: {ratio}");
    }
    let upper_size = computed_px(&headings[0], "font-size");
    for heading in &headings[..2] {
        assert_px_close(heading, "font-size", upper_size);
    }
    let lower_size = computed_px(&headings[2], "font-size");
    for heading in &headings[2..] {
        assert_px_close(heading, "font-size", lower_size);
    }
    assert!(upper_size < computed_px(&detail_title, "font-size"));
    assert!(upper_size > lower_size);
    assert_eq!(computed_px(&h1, "margin-top"), 0.0);
    assert_px_close(&h3, "margin-top", rem * 0.75);
    assert!(computed_px(&list, "padding-left") > 0.0);
    assert_px_close(&second_item, "margin-top", rem * 0.2);
    assert_px_close(&nested_paragraph, "margin-top", rem * 0.5);
    assert_px_close(&nested_list, "margin-top", rem * 0.25);
    assert!(computed_px(&quote, "border-left-width") > 0.0);
    assert_px_close(&quote_second_paragraph, "margin-top", rem * 0.5);
    assert_px_close(&nested_heading, "margin-top", rem * 0.5);
    assert_px_close(&nested_rule, "margin-top", rem * 0.5);
    assert_eq!(
        computed_style(&quote).get_property_value("color").unwrap(),
        "rgb(114, 103, 81)"
    );
    assert_ne!(
        computed_style(&inline_code).get_property_value("background-color").unwrap(),
        computed_style(&body).get_property_value("background-color").unwrap()
    );
    assert_eq!(
        computed_style(&inline_code).get_property_value("font-family").unwrap(),
        computed_style(&pre).get_property_value("font-family").unwrap()
    );
    assert_eq!(
        computed_style(&prose).get_property_value("overflow-wrap").unwrap(),
        "anywhere"
    );
    assert_eq!(
        computed_style(&inline_code).get_property_value("overflow-wrap").unwrap(),
        "anywhere"
    );
    assert_eq!(computed_style(&pre).get_property_value("overflow-x").unwrap(), "auto");
    assert_eq!(computed_px(&pre_code, "padding-left"), 0.0);
    assert_eq!(computed_px(&pre_code, "border-top-width"), 0.0);
    assert_ne!(
        computed_style(&pre_code).get_property_value("background-color").unwrap(),
        computed_style(&inline_code).get_property_value("background-color").unwrap()
    );
    assert_eq!(
        computed_style(&link).get_property_value("color").unwrap(),
        "rgb(140, 68, 20)"
    );
    assert_eq!(
        computed_style(&link).get_property_value("overflow-wrap").unwrap(),
        "anywhere"
    );
    assert_eq!(computed_px(&rule, "border-top-width"), 1.0);
    assert_eq!(
        computed_style(&rule).get_property_value("border-top-color").unwrap(),
        "rgb(230, 221, 204)"
    );
    assert!(computed_px(&table, "margin-top") > 0.0);
    let table_only_container = mount_detail(EXHIBIT_BODY_FIXTURE_JSON, "N01");
    let first_table = table_only_container
        .query_selector(".exhibit-body > table")
        .unwrap()
        .expect("table-only exhibit must render its table first");
    assert_eq!(computed_px(&first_table, "margin-top"), 0.0);
    assert_eq!(
        computed_style(&th).get_property_value("background-color").unwrap(),
        computed_style(&pre).get_property_value("background-color").unwrap()
    );
    assert_eq!(computed_px(&td, "border-top-width"), 1.0);
    assert_px_close(&td, "padding-left", rem * 0.5);
    assert_eq!(computed_style(&body).get_property_value("overflow-x").unwrap(), "auto");

    stylesheet.parent_node().unwrap().remove_child(&stylesheet).unwrap();
}
```

If Chrome serializes transparent colors differently, compare the inline-code background with the surrounding paragraph background instead of accepting multiple string spellings.

- [ ] **Step 4: Run the browser suite and confirm the new test fails for the missing CSS**

Run:

```bash
wasm-pack test --headless --chrome crates/ara-viewer --locked
```

Expected: `non_table_exhibit_markdown_uses_viewer_styles` fails on at least the zero list indentation, zero blockquote border, transparent inline-code background, or visible `pre` overflow assertion. Existing tests, including the table test, remain green.

---

### Task 2: Implement Scoped Exhibit Markdown Styles

**Files:**
- Modify: `crates/ara-viewer/public/styles.css`, immediately after the base `.exhibit-body` rule and before the existing table rules

**Interfaces:**
- Consumes: HTML emitted by `render_exhibit_body` and existing viewer design tokens.
- Produces: `.exhibit-body`-scoped presentation with no Rust or manifest changes.

- [ ] **Step 1: Add non-table block rhythm and headings**

Add explicit top spacing only to non-table top-level blocks so the global reset no longer collapses the document. Keep tables out of the selector. Use the approved hierarchy and scale:

```css
.exhibit-body > h1,
.exhibit-body > h2,
.exhibit-body > h3,
.exhibit-body > h4,
.exhibit-body > h5,
.exhibit-body > h6,
.exhibit-body > p,
.exhibit-body > blockquote,
.exhibit-body > ul,
.exhibit-body > ol,
.exhibit-body > pre,
.exhibit-body > hr {
  margin-top: 0.75rem;
}

.exhibit-body > :first-child {
  margin-top: 0;
}

.exhibit-body :where(blockquote, li)
  > :where(p, ul, ol, blockquote, pre, h1, h2, h3, h4, h5, h6, hr)
  + :where(p, ul, ol, blockquote, pre, h1, h2, h3, h4, h5, h6, hr) {
  margin-top: 0.5rem;
}

.exhibit-body > :not(table) + table {
  margin-top: 0.75rem;
}

.exhibit-body h1,
.exhibit-body h2,
.exhibit-body h3,
.exhibit-body h4,
.exhibit-body h5,
.exhibit-body h6 {
  color: var(--ink);
  font-weight: 600;
  line-height: 1.3;
  word-break: break-word;
}

.exhibit-body p,
.exhibit-body li,
.exhibit-body blockquote,
.exhibit-body a,
.exhibit-body :not(pre) > code {
  overflow-wrap: anywhere;
}

.exhibit-body h1,
.exhibit-body h2 { font-size: 0.95rem; }
.exhibit-body h3,
.exhibit-body h4,
.exhibit-body h5,
.exhibit-body h6 { font-size: 0.9rem; }
```

- [ ] **Step 2: Add list and blockquote styles**

Markdown blockquotes are tertiary source annotations. They intentionally use a
quieter `--line` spine and muted text instead of the existing `.quote` pull-quote
pattern’s accent spine, filled panel, italics, and radius. This keeps routine
quoted evidence below `.detail-title`, structural headings, and reason blocks;
future work must not merge the two treatments without revisiting that semantic
distinction.

```css
.exhibit-body ul,
.exhibit-body ol {
  padding-left: 1.25rem;
}

.exhibit-body li + li {
  margin-top: 0.2rem;
}

.exhibit-body li > ul,
.exhibit-body li > ol {
  margin-top: 0.25rem;
}

.exhibit-body blockquote {
  border-left: 3px solid var(--line);
  padding-left: 0.75rem;
  color: var(--muted);
}
```

- [ ] **Step 3: Add separate inline-code and fenced-code styles**

Use a child selector for inline code so `pre > code` does not acquire a second background, border, or padding layer:

```css
.exhibit-body :not(pre) > code {
  padding: 0.05rem 0.25rem;
  background-color: var(--panel2);
  border: 1px solid var(--line);
  border-radius: 3px;
  font-size: 0.9em;
}

.exhibit-body pre {
  padding: 0.5rem 0.75rem;
  overflow-x: auto;
  background-color: var(--panel2);
  border: 1px solid var(--line);
  border-radius: 4px;
  color: var(--ink);
  font-family: var(--font-mono);
  font-size: 0.78rem;
  line-height: 1.5;
  white-space: pre;
}

.exhibit-body pre code {
  padding: 0;
  background: transparent;
  border: 0;
  color: inherit;
  font: inherit;
}
```

- [ ] **Step 4: Add link and horizontal-rule states**

```css
.exhibit-body a {
  color: var(--accent-text);
  text-decoration-thickness: 1px;
  text-underline-offset: 2px;
}

.exhibit-body a:visited {
  color: var(--muted);
}

.exhibit-body a:hover {
  text-decoration-color: var(--accent);
  text-decoration-thickness: 2px;
}

.exhibit-body a:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.exhibit-body hr {
  border: 0;
  border-top: 1px solid var(--line);
}
```

- [ ] **Step 5: Run the browser suite and confirm the contract passes**

Run:

```bash
wasm-pack test --headless --chrome crates/ara-viewer --locked
```

Expected: all browser tests pass, including `non_table_exhibit_markdown_uses_viewer_styles` and `exhibit_body_renders_table_in_scroll_container`.

---

### Task 3: Verify the Actual Viewer Surface

**Files:**
- No permanent file changes

**Interfaces:**
- Consumes: the Trunk development viewer and the mixed semantic HTML covered by Task 1.
- Produces: visual evidence that the style rules work in the detail pane and do not regress tables.

- [ ] **Step 1: Launch the real viewer**

Use `trunk serve` while iterating on CSS:

```bash
cd crates/ara-viewer && trunk serve
```

Run `trunk serve` through the harness process manager rather than a blocking
shell. The embedded `ara serve` check belongs only to Task 5, after Task 4
regenerates and verifies the committed bundle.

- [ ] **Step 2: Exercise the style-only surface in Chromium**

Open the viewer, select a node so `.detail-root` exists, and use browser evaluation to append one temporary `<div class="exhibit-body">` containing the same headings, nested lists, blockquote, inline `code`, `pre > code`, `strong`, `em`, `a`, `hr`, and table elements from Task 1. Include a visible long URL, one unbroken inline token, a three-level list, and an over-wide code line. This temporary DOM is verification data only; do not commit it to `public/manifest.json`.

- [ ] **Step 3: Check normal and narrow layouts**

Verify the same content at all three fixed viewports:

| Viewport | Layout exercised |
|---|---|
| `1280×800` | Normal split pane |
| `799×700` | Immediately below the responsive switch |
| `375×667` | Phone-sized forced stack |

- `.detail-title` remains the sole primary anchor; exhibit `h1`/`h2` are a
  subordinate scanning layer, and `h3`–`h6` remain distinguishable reading cues
  rather than competing titles;
- list markers remain visible through three nesting levels;
- blockquotes use muted text and a single left rule;
- inline code does not gain block-level padding;
- bold and italic text remain distinguishable without becoming a new hierarchy
  level or receiving decorative color;
- ordinary prose, visible URLs, and inline tokens wrap without pane-level
  horizontal scrolling; only fenced code and the existing table containment
  may scroll horizontally;
- unvisited link text retains the 7.0:1 `--accent-text` contrast on `--panel`,
  visited links use the 5.47:1 `--muted` token, hover strengthens the underline
  without reducing text contrast, and keyboard focus uses the `--accent` outline;
- the horizontal rule uses the existing divider color;
- tables that follow non-table content receive `0.75rem` separation, while a
  first-child table keeps its existing placement, borders, header fill, cell
  padding, and `.exhibit-body` scroll containment;
- loading, empty/no-selection, blank-body, and error surfaces retain their
  existing appearance because no new selector applies outside `.exhibit-body`.
- the complete detail-pane sequence remains calm and document-like: the selected
  node orients first, exhibit headings guide scanning second, and evidence
  containers become prominent only when the reader reaches them.

Capture screenshots for review. Treat overlap, clipped markers, pane-level horizontal overflow, nested code boxes, or changed table appearance as failures.

---

### Task 4: Update the Shipped Bundle and Project Records

**Files:**
- Modify: `docs/exhibit-markdown-rendering.md`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `CHANGELOG.md`
- Regenerate: `crates/ara-cli/assets/viewer/`
- Regenerate: `crates/ara-cli/assets/viewer.source-hash`
- Remove after completion: `plans/exhibit-markdown-styles.md`

**Interfaces:**
- Consumes: the approved CSS and passing tests.
- Produces: version `0.1.16`, a fresh embedded viewer, and a durable design record.

- [ ] **Step 1: Update the design record**

In `docs/exhibit-markdown-rendering.md`:

- extend “Rendering and layout” with the styled non-table elements and token reuse;
- extend “Testing” with the computed-style browser contract and actual-surface visual check;
- remove #46 from “Known gaps”; retain #60 and #31 unchanged;
- state that the existing table visual declarations and `.exhibit-body`
  overflow contract remain unchanged, and document the new mixed-flow selector
  that adds `0.75rem` before a table only when non-table content precedes it.

- [ ] **Step 2: Add release metadata**

Change `[workspace.package] version` in `Cargo.toml` from `0.1.15` to `0.1.16`.

Refresh the lockfile once without `--locked`:

```bash
cargo check --workspace
```

Review the resulting `Cargo.lock` diff before continuing. It must contain only
the expected `ara-cli`, `ara-core`, `ara-viewer`, and `ara-wasm` workspace
version updates; unrelated dependency updates are a failure.

Add this entry under `## [Unreleased]` in `CHANGELOG.md`:

```markdown
### Added
- Viewer: exhibit bodies now style CommonMark headings, lists, blockquotes,
  inline and fenced code, links, and horizontal rules with the existing detail
  pane design tokens (#46).
```

- [ ] **Step 3: Regenerate the embedded viewer**

Run:

```bash
scripts/embed-viewer.sh
scripts/embed-viewer.sh --check
```

Expected: regeneration completes, the source-hash check reports `OK`, and `crates/ara-cli/assets/viewer/` contains the newly built CSS bundle.

- [ ] **Step 4: Retire the reviewed plan after implementation is complete**

Delete `plans/exhibit-markdown-styles.md` only after the design record contains the implemented decisions and every verification step below passes. Do not delete `plans/README.md`.

---

### Task 5: Run Final Verification

**Files:**
- No additional source changes

**Interfaces:**
- Consumes: the completed implementation, tests, records, and embedded bundle.
- Produces: evidence that the workspace and shipped viewer remain healthy.

- [ ] **Step 1: Check formatting and native behavior**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Expected: all commands pass without warnings or test failures.

- [ ] **Step 2: Check wasm and browser behavior**

```bash
cargo clippy -p ara-viewer --target wasm32-unknown-unknown --tests --locked -- -D warnings
cargo build -p ara-core -p ara-wasm --target wasm32-unknown-unknown --locked
wasm-pack test --headless --chrome crates/ara-viewer --locked
```

Expected: the wasm build and every headless Chrome test pass.

- [ ] **Step 3: Check the committed viewer bundle**

```bash
scripts/embed-viewer.sh --check
```

Expected: `OK: embedded viewer is up to date`.

- [ ] **Step 4: Repeat the visual smoke check against `ara serve`**

Serve an existing or temporary valid ARA artifact with the embedded viewer, inject the temporary mixed-markdown body described in Task 3, and repeat the `1280×800`, `799×700`, and `375×667` browser checks. Record the concrete artifact path in the verification evidence. This final pass verifies the CSS that ships in `ara-cli`, not only Trunk’s development output.

## Engineering Flow and Failure Modes

```text
MIXED_EXHIBIT_BODY_FIXTURE_JSON
        │ parse_manifest → DetailPane → render_exhibit_body
        ▼
semantic .exhibit-body DOM + source styles.css
        │
        └── headless Chrome computed-style contract

approved styles.css
        │ Trunk wasm-release build
        ▼
crates/ara-viewer/dist/
        │ scripts/embed-viewer.sh
        ▼
committed viewer assets + source hash
        │ scripts/embed-viewer.sh --check
        ▼
ara serve embedded-viewer smoke matrix
```

| Failure mode | Detection and user-visible result |
|---|---|
| Renderer stops emitting an expected semantic element | The browser test’s selector fails with the missing element name before style assertions run. |
| A scoped selector or stable property regresses | Tolerant computed-style assertions fail in the headless Chrome gate. |
| Hover, visited, focus, wrapping, or narrow-pane composition regresses | The three fixed-viewports source and embedded smoke checks fail; screenshots show the visible defect. |
| Workspace version changes without refreshing `Cargo.lock` | Task 4’s non-locked check refreshes exactly four workspace versions; later locked gates reject drift. |
| Viewer source changes without a fresh embedded bundle | `scripts/embed-viewer.sh --check` fails before the final `ara serve` check. |

No failure mode is silent and untested. No inline code diagram is warranted for
the flat CSS rules; the browser test comments should name the source, mixed, and
table-only scenarios.

**Parallelization:** Sequential implementation, no parallelization opportunity.
The red browser contract must precede CSS; source visual approval precedes bundle
regeneration; locked final gates require the version, lockfile, records, and
embedded assets to be complete.

## Implementation Tasks

Synthesized from the design and engineering reviews. Each task derives from a
verified finding above. Run with Claude Code or Codex; check each item as it ships.

_No new tasks from Architecture Review._

- [ ] **T1 (P1, human: ~2h / CC: ~20min)** — Browser contract — Preserve the
  table-only fixture, add the mixed fixture, reuse `mount_detail`, and implement
  tolerant computed-style coverage for every approved stable selector.
  - Surfaced by: Code Quality and Test Review — duplicated mount setup,
    incomplete heading/nested coverage, and exact subpixel comparisons could
    miss regressions or reject correct CSS.
  - Files: `crates/ara-viewer/tests/web.rs`
  - Verify: red test before CSS, then
    `wasm-pack test --headless --chrome crates/ara-viewer --locked`
- [ ] **T2 (P1, human: ~3h / CC: ~25min)** — Exhibit body CSS — Implement the
  approved hierarchy, top-level and nested rhythm, wrapping, quote, code, link,
  rule, and mixed-table boundary styles without changing existing table visuals.
  - Surfaced by: Design Review and outside voice — nested headings/rules,
    long-token wrapping, title competition, and low-contrast hover text were
    shipping risks.
  - Files: `crates/ara-viewer/public/styles.css`
  - Verify: browser contract plus the Task 3 source-viewer matrix
- [ ] **T3 (P1, human: ~1.5h / CC: ~15min)** — Source visual verification —
  Exercise loaded mixed markdown and unchanged states through `trunk serve` at
  all three approved viewports.
  - Surfaced by: User Journey and Responsive & Accessibility — “near 800px” did
    not cover the split, transition, and phone-sized reading surfaces.
  - Files: no permanent files
  - Verify: screenshots at `1280×800`, `799×700`, and `375×667`
- [ ] **T4 (P1, human: ~1h / CC: ~10min)** — Project records and bundle —
  Update the design record, version, lockfile, changelog, and embedded assets.
  - Surfaced by: outside voice — stale lock metadata blocks locked gates, and
    the durable record must describe the mixed-table selector accurately.
  - Files: `docs/exhibit-markdown-rendering.md`, `Cargo.toml`, `Cargo.lock`,
    `CHANGELOG.md`, `crates/ara-cli/assets/viewer/`,
    `crates/ara-cli/assets/viewer.source-hash`
  - Verify: lockfile diff contains only four workspace version updates and
    `scripts/embed-viewer.sh --check` passes
- [ ] **T5 (P1, human: ~1h / CC: ~15min)** — Final gates — Run native and wasm
  Clippy, native and wasm builds/tests, browser tests, bundle freshness, and the
  final embedded `ara serve` matrix.
  - Surfaced by: Test Review and outside voice — wasm-only Rust needs target
    linting, and embedded verification is valid only after regeneration.
  - Files: no additional source files
  - Verify: every command and browser scenario in Task 5

_No new tasks from Performance Review._

## Review Gate

Human approval was recorded on 2026-08-23 after design and engineering review resolved the heading hierarchy, complete CommonMark selector coverage, tolerant browser assertions, source-versus-embedded verification order, lockfile refresh, state applicability, and three-viewport visual criteria. Implementation may begin when requested; execute the tasks sequentially and keep renderer, table visuals, image, and math behavior out of scope.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|---|---|---|---:|---|---|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | NOT RUN | The local CSS enhancement did not surface a product-direction gap. |
| Codex Review | `/codex review` | Independent 2nd opinion | 1 | ABSORBED (Claude fallback) | Codex timed out; the fallback found 5 issues, all folded with no tension. |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR (FULL_REVIEW) | 10 issues resolved; 0 critical gaps; 0 unresolved. |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | CLEAR (FULL) | Score: 7/10 → 10/10; 11 design decisions resolved. |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | NOT RUN | No developer-facing workflow change was identified. |

- **VERDICT:** DESIGN + ENG CLEARED; HUMAN APPROVED — ready to implement.

NO UNRESOLVED DECISIONS

