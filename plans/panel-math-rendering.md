# Typeset math in Glossary and Solution files (#31)
**Date:** 2026-10-02

## TL;DR
Use a pinned, locally packaged KaTeX renderer that loads only when a mounted Glossary or Solution-files entry contains math. Keep the renderer outside the main wasm bundle and work without a CDN. Preserve all non-math source text and show the original LaTeX with a visible error when rendering fails. This change typesets panel math; it does not add exhibit math or full Markdown rendering to Solution files.

## Problem

Issue [#31](https://github.com/ARA-Labs/ara-cli/issues/31) requests readable inline and display equations in the Glossary and Recipes panels. The Recipes launcher is now named Solution files. `latex_segments` and `latex_view` in `crates/ara-viewer/src/panels.rs` split on individual dollar signs and render `<code class="latex-inert">`. They cannot distinguish display delimiters or protected code. The checked-in self-composing-policies corpus uses inline pi/Phi notation, display equations, cases, matrices, and fenced source code. Solution bodies already preserve complete Markdown source; no manifest change is needed.

## Constraints

Keep the existing wasm limits of 1,048,576 bytes raw and 358,400 bytes with brotli q11. Package JS, CSS, fonts, and their licenses with both Trunk and the embedded viewer. All rendering must work when external requests are blocked. Preserve plain prose, whitespace, Markdown markers, and fenced source exactly outside math spans. Modal close, filtering, reopening, and manifest replacement must not leave stale asynchronous DOM changes. Keep `render_exhibit_body` and unrelated panels unchanged.

## Proposed approach

Vendor the unmodified prebuilt renderer and referenced font assets from [KaTeX 0.19.0](https://github.com/KaTeX/KaTeX/releases/tag/v0.19.0), a published non-prerelease. Its release archive has the published SHA-256 `966d9c85655081cca9a57e8a1d667f13f96b1c85c3f93655a8acef74c4ce3875`. Verify that digest before copying assets. Record the upstream version, source URL, and shipped-file hashes. Preserve KaTeX's MIT notice and inspect the exact font binaries for their own license/copyright notices; do not assume the renderer's MIT label covers every font. Ship the applicable font license text and preserve binary metadata without subsetting or renaming fonts.

Add a small local loader script and declare the versioned vendor directory with Trunk's copy-dir mechanism. The loader captures its own script URL and resolves renderer assets beside it. Do not resolve shared vendor assets from `document.baseURI`, which points to `/a/{id}/` in hub mode. Use one shared load promise for renderer JS and CSS, triggered by the first mounted math fragment. Verify the expected KaTeX version/API, stylesheet loading, and the required font faces before declaring rendering successful. Keep the failure cached for that page and require a reload to retry; add no CDN or automatic retry fallback.

Move math-specific splitting and rendering into `src/math.rs`, replacing the obsolete public inert helper and updating its consumers through language-server references. The approved syntax is `$...$` for inline math and `$$...$$` for display math, including delimiter-wrapped `aligned`, `cases`, and matrix environments. Prioritize double-dollar delimiters, preserve unmatched or escaped delimiters, keep inline math on one source line, and allow multiline display math. Single-dollar delimiters must have non-whitespace content edges; a closing delimiter followed immediately by a digit is not a math close, preserving ordinary text such as `$5 and $10`. Protect inline code and fenced/indented code ranges using the existing pulldown-cmark offset parser. Preserve source ranges rather than reserializing Markdown. Tests must prove exact text preservation around and inside protected ranges.

Each fragment retains original spelling, TeX content, and inline/display mode. Leptos owns a span or div host and escaped fallback/status nodes; KaTeX owns only the host's descendants. Reuse the existing `NodeRef`, `Effect`, and `on_cleanup` pattern in `modal.rs`. After each asynchronous load, check that the owner is live before committing output or updating signals. Filtering out an entry, closing a modal, or replacing the manifest invalidates pending work for that fragment. New mounts reuse the loaded renderer but render their own current text.

Render with `output: "htmlAndMathml"`, `trust: false`, `globalGroup: false`, a fresh macro object for each expression, `maxExpand: 1000`, finite `maxSize: 20`, and `throwOnError: true`, following [KaTeX's options](https://katex.org/docs/options). Keep the default strict warning policy. Trust-blocked commands can produce error-colored output without throwing, so treat a blocked command as a fragment failure rather than calling it successful. Original text and diagnostics remain escaped; never concatenate them into HTML. An invalid expression must not prevent its neighbors from rendering.

Replace the recipe `<pre>` wrapper with a raw-text container that can legally contain display blocks. Preserve its monospace/preformatted treatment for source text and reset whitespace rules inside math hosts. Use shrinkable glossary field containers and local horizontal scrolling for long display equations. While loading, keep source visible with a loading status. A renderer/CSS/font failure shows source and a visible availability message. A malformed equation shows its original source and a local failure label with escaped details. No failure is hidden only in the console.

## Alternatives considered

| Option | Benefit | Cost |
| --- | --- | --- |
| Locally lazy-loaded KaTeX | Known equation coverage; keeps renderer code outside wasm; works offline | Requires loader lifecycle and local font packaging |
| KaTeX auto-render over panel text | Less direct rendering glue | Ignores the current recipe pre element and mutates nodes owned by Leptos; still needs code protection |
| Pure-Rust typesetter | Avoids JS load coordination | Equation coverage and wasm size would require a separate evaluation |

## Tradeoffs

The wasm size gate does not measure added vendor JS, CSS, or fonts. Report their raw/compressed sizes and the total distribution size separately; do not claim that clearing the wasm gate makes the total download unchanged. Lazy loading avoids those requests on visits that never mount math. Bare AMS environments, additional delimiter forms, and multiline continuation parsing in `logic/concepts.md` are outside this proposal. The existing concept parser is line-oriented, so a single-line display value is supported, while multiline aligned source is exercised through complete Solution-file bodies.

## Migration

1. Add behavior tests before replacing the splitter: mixed inline/display math, adjacent display expressions from the corpus, multiline aligned equations, escaped/unmatched dollars, ordinary currency, UTF-8, protected inline/fenced/indented code, and exact source preservation.
2. Vendor the checked release, inspect and preserve code/font licenses, add Trunk asset declarations, and implement the small loader. Keep production asset dependencies local.
3. Implement owner-scoped math hosts, visible source/error states, and raw-text/display layout. Remove obsolete inert rendering, CSS, tests, comments, and helper exports; migrate every consumer.
4. Extend the existing browser suite using the real renderer, not mocked output. Assert superscript and aligned-row MathML structure, accessible math, preserved surrounding text/code, isolated malformed-expression failures, and safe rejection of artifact-controlled links, HTML commands, or recursive macros. Cover unmount during loading, filter/reopen, and manifest replacement.
5. Exercise the built viewer in embedded `ara serve`, source `--assets`, static hosting under a prefix, and hub `/a/{id}/` mode. Block external network access, observe no math-asset requests with panels closed, then open panels and inspect actual equations and font loads. Deliberately fail script, stylesheet, and font delivery and verify visible fallback. Check screenshots and overflow at desktop and narrow viewports.
6. Run workspace/browser tests, formatting, Clippy, wasm build and size checks. Measure vendor and total asset sizes, regenerate the embedded viewer, and check freshness. Bump the next patch version and update the changelog, `docs/hub-parity.md`, panel/viewer documentation, the exhibit document's panel-versus-exhibit math distinction, and `T-MATH-RENDER`.

## Next Steps

Approve local lazy KaTeX, the dollar-delimiter contract, and the visible source fallback. Implement as `feat/31-panel-math-rendering`, stacked after #60 to avoid conflicts in shared styles, packaging, and release metadata. After verification, rewrite this plan as a permanent design document and remove it from `plans/`. Use `Closes #31` in the implementation PR.
