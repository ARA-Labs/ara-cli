# Exhibit Body Markdown Styles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` or `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Style non-table CommonMark elements in exhibit bodies so they match the viewer’s detail pane without changing markdown parsing, sanitization, table styling, or table overflow.

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

## Scope Boundaries

**In scope:** `h1`–`h6`, `p`, `ul`, `ol`, `li`, `blockquote`, inline `code`, `pre`, `a`, `hr`, and their spacing inside `.exhibit-body`.

**Out of scope:** figure-image resolution (#60), math rendering (#31), raw HTML support, syntax highlighting, copy buttons, new design tokens, renderer changes, and shared prose components outside exhibit bodies.

## File Map

- Modify `crates/ara-viewer/tests/web.rs`: add a mixed-markdown fixture and a browser test that loads the shipped stylesheet and asserts computed-style invariants.
- Modify `crates/ara-viewer/public/styles.css`: add scoped non-table markdown rules next to the existing exhibit table rules.
- Modify `docs/exhibit-markdown-rendering.md`: record the shipped style coverage, browser contract, and remaining gaps.
- Modify `Cargo.toml`: bump the workspace patch version from `0.1.15` to `0.1.16` because the shipped viewer behavior changes.
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

---

### Task 1: Add a Failing Browser Style Contract

**Files:**
- Modify: `crates/ara-viewer/tests/web.rs`, near `EXHIBIT_BODY_FIXTURE_JSON` and `exhibit_body_renders_table_in_scroll_container`

**Interfaces:**
- Consumes: `DetailPane`, `parse_manifest`, the `.exhibit-body` wrapper, and `crates/ara-viewer/public/styles.css`.
- Produces: a browser regression test named `non_table_exhibit_markdown_uses_viewer_styles`.

- [ ] **Step 1: Expand the exhibit fixture without removing its table**

Prepend representative non-table markdown to E01’s existing table body:

````markdown
## Findings

1. Train the baseline.
2. Compare the residual model.

> Residual learning converged reliably.

Use `learning_rate = 0.1` for this run.

```text
baseline = 27.1
residual = 28.4
```

[Read the experiment notes](https://example.com/notes)

---
````

Keep the current GFM table after this content so `exhibit_body_renders_table_in_scroll_container` continues to defend the #32 table contract.

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

Mount `DetailPane` with `EXHIBIT_BODY_FIXTURE_JSON`, install the stylesheet, query the semantic elements, and assert behavior-level invariants:

```rust
#[wasm_bindgen_test]
fn non_table_exhibit_markdown_uses_viewer_styles() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let stylesheet = install_viewer_styles(&doc);
    let container = body_div(&doc);
    let manifest =
        parse_manifest(EXHIBIT_BODY_FIXTURE_JSON).expect("exhibit-body fixture must parse");
    let selected = RwSignal::new(Some(ara_core::NodeId::new("N01")));
    let (load_state, _) = signal(LoadState::Loaded(manifest));

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DetailPane load_state=load_state selected=selected /> }
    });

    let body = container
        .query_selector(".exhibit-body")
        .unwrap()
        .expect("mixed exhibit body must render");
    let heading = body.query_selector("h2").unwrap().expect("heading must render");
    let list = body.query_selector("ol").unwrap().expect("list must render");
    let quote = body
        .query_selector("blockquote")
        .unwrap()
        .expect("blockquote must render");
    let inline_code = body
        .query_selector("p code")
        .unwrap()
        .expect("inline code must render");
    let pre = body.query_selector("pre").unwrap().expect("code block must render");

    assert_eq!(computed_style(&heading).get_property_value("font-weight").unwrap(), "600");
    assert_ne!(computed_style(&list).get_property_value("padding-left").unwrap(), "0px");
    assert_ne!(computed_style(&quote).get_property_value("border-left-width").unwrap(), "0px");
    assert_ne!(computed_style(&inline_code).get_property_value("background-color").unwrap(), "rgba(0, 0, 0, 0)");
    assert_eq!(computed_style(&pre).get_property_value("overflow-x").unwrap(), "auto");

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

Add explicit top spacing only to non-table top-level blocks so the global reset no longer collapses the document. Keep tables out of the selector. Use the following scale as the review baseline:

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

.exhibit-body h1 { font-size: 1rem; }
.exhibit-body h2 { font-size: 0.95rem; }
.exhibit-body h3 { font-size: 0.9rem; }
.exhibit-body h4,
.exhibit-body h5,
.exhibit-body h6 { font-size: 0.85rem; }
```

- [ ] **Step 2: Add list and blockquote styles**

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

.exhibit-body a:hover {
  color: var(--accent);
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
- Consumes: the real Trunk or embedded viewer stylesheet and the mixed semantic HTML covered by Task 1.
- Produces: visual evidence that the style rules work in the detail pane and do not regress tables.

- [ ] **Step 1: Launch the real viewer**

Use `trunk serve` while iterating on CSS:

```bash
cd crates/ara-viewer && trunk serve
```

For the final embedded-asset check, run `cargo run -p ara-cli -- serve` against
an existing valid ARA artifact available in the workspace or a temporary valid
artifact created for this check. Pick and record the concrete artifact during
implementation; its content is not the style-test input because Step 2 injects
the representative semantic body. Run long-lived servers through the harness
process manager rather than a blocking shell.

- [ ] **Step 2: Exercise the style-only surface in Chromium**

Open the viewer, select a node so `.detail-root` exists, and use browser evaluation to append one temporary `<div class="exhibit-body">` containing the same `h2`, `ol`, `blockquote`, inline `code`, `pre > code`, `a`, `hr`, and table elements from Task 1. This temporary DOM is verification data only; do not commit it to `public/manifest.json`.

- [ ] **Step 3: Check normal and narrow layouts**

At a normal split-pane width and again near the 800px responsive boundary, verify:

- headings remain subordinate to `.detail-title` and preserve a visible hierarchy;
- lists keep markers inside the reading column;
- blockquotes use muted text and a single left rule;
- inline code does not gain block-level padding;
- fenced code scrolls horizontally without widening the pane;
- links show hover and keyboard-focus states;
- the horizontal rule uses the existing divider color;
- the table retains its borders, header fill, cell padding, and `.exhibit-body` scroll containment.

Capture screenshots for review. Treat overlap, clipped markers, pane-level horizontal overflow, nested code boxes, or changed table appearance as failures.

---

### Task 4: Update the Shipped Bundle and Project Records

**Files:**
- Modify: `docs/exhibit-markdown-rendering.md`
- Modify: `Cargo.toml`
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
- state that table selectors and overflow behavior remain unchanged.

- [ ] **Step 2: Add release metadata**

Change `[workspace.package] version` in `Cargo.toml` from `0.1.15` to `0.1.16`.

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

Serve an existing or temporary valid ARA artifact with the embedded viewer, inject the temporary mixed-markdown body described in Task 3, and repeat the normal-width and narrow-width browser checks. Record the concrete artifact path in the verification evidence. This final pass verifies the CSS that ships in `ara-cli`, not only Trunk’s development output.

## Review Gate

Implementation must not begin until the human developer reviews and approves this plan. Review should resolve the proposed heading scale, block spacing, inline-code selector, computed-style assertions, and visual acceptance criteria. After approval, execute the tasks in order and keep renderer, table, image, and math behavior out of scope.
