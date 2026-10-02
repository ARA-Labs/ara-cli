# Render figure exhibits with local images (#60)
**Date:** 2026-10-02

## TL;DR
Add an optional image reference to figure exhibits and render it with an escaped description caption. Keep existing Markdown bodies, including supporting data tables, beneath the image. Resolve local references from the manifest source that actually loaded, so local serving, hub serving, and static hosting all work. Static deployment copies the referenced evidence files alongside the exported manifest; this plan does not add a new export command or inline every image into JSON.

## Problem

Issue [#60](https://github.com/ARA-Labs/ara-cli/issues/60) requests inline figure images, a no-image Markdown fallback, and working serve/static delivery. `Exhibit` in `crates/ara-core/src/manifest.rs` has no image field. `read_evidence` in `evidence.rs` discovers only Markdown bodies. `DetailPane` filters out blank bodies and never activates the existing `figure.detail-figure` styles. `source.rs` fetches an API manifest with a static fallback but discards which source succeeded. Local serving already exposes `/api/figure/*` over the evidence directory; hub serving has no corresponding per-artifact route.

## Constraints

Keep old manifests readable and old no-image serialization unchanged. Preserve exhibit identity, description/claim precedence, body content, category ordering, and the existing Markdown sanitizer. Reuse the styles delivered for #46; that issue was already implemented in PR #82 and has now been verified and closed. Keep filesystem work out of wasm, and retain the existing wasm size limits of 1,048,576 bytes raw and 358,400 bytes with brotli q11. Artifact-root symlinks used by hub deployment must continue to work.

## Proposed approach

Add `Exhibit.image: Option<String>` with serde default and omission when absent. The wire value is an artifact-root-relative path such as `evidence/figures/loss.png`. Begin with PNG and JPEG (`.png`, `.jpg`, `.jpeg`), case-insensitive. Author declarations use the existing evidence conventions: an indexed raster File entry, an explicit `- **Image**: figures/loss.png` body bullet relative to `evidence/`, or an unambiguous same-stem raster sibling. Explicit body metadata wins over the indexed raster path, which wins over sibling inference. An explicit missing or rejected reference leaves the image absent and preserves the body; do not silently replace it with another file. Multiple candidate siblings need an explicit declaration.

Gather figure Markdown and raster files together, emitting one exhibit for a companion pair. Preserve the Markdown path in `file` when a body exists. An indexed raster-only figure uses its actual file path and an empty body. A differently named asset referenced by a Markdown figure does not become a duplicate standalone exhibit. Keep direct-child discovery and existing category ordering. Report missing, unsafe, or ambiguous image declarations through a new stable evidence warning code, allocated against the final rule registry and covered by its diagnostic tests.

Before native access or viewer URL construction, reject schemes, authority forms, absolute paths, parent-directory components, backslashes, drive paths, and control characters. Native paths are filesystem names, not URL-encoded strings. Decode HTTP route paths once before containment checks; encode validated filename segments once when constructing viewer URLs.

Associate image URL resolution with the successful fetch in `source.rs`. Reuse its existing browser URL support and `document.baseURI`. For an API manifest, strip the artifact path's `evidence/` prefix and resolve `figure/<encoded path>` beside `api/manifest`. For static JSON, resolve the complete artifact-relative path beside that manifest's effective URL. API failure followed by static fallback must use the static mapping. Resolve each filename segment without treating spaces, Unicode, `#`, `?`, or `%` as URL syntax. Keep this context tied to manifest refetches; avoid changing every unrelated graph and panel consumer of `LoadState`.

Use a shared local/hub image-serving helper and add `/a/{id}/api/figure/*` to the existing hub router. Keep file-service range and conditional request handling. Check that requested images are regular supported files contained by the canonical evidence root, including after parsing; reject escaping symlinks and traversal. Allow a symlinked selected artifact root and contained asset symlinks. Return real missing-file responses, explicit image MIME types, and `X-Content-Type-Options: nosniff`. Canonicalize-then-open checks do not promise race-proof access against concurrent hostile filesystem mutation; changing the entire server's filesystem access model is outside this issue.

Render image-bearing `Figure` exhibits as normal Leptos `<figure class="detail-figure"><img><figcaption>` nodes. Use `description` for the caption and image alternative text, falling back to the exhibit id for alternative text when no description exists. Omit absent captions. Show the nonblank Markdown body below the image in the existing `.exhibit-body` container, without repeating the description paragraph. Image-only exhibits must render even with blank bodies. No-image and rejected-image figures keep the current caption-above-Markdown behavior. Non-figure exhibits keep their current rendering. Apply a viewer-side image path guard because downloaded manifest JSON can bypass native parsing; do not loosen Markdown's rejection of arbitrary data or script URLs.

## Alternatives considered

| Option | Benefit | Cost |
| --- | --- | --- |
| Local references plus source-aware resolution | Reuses image streaming; avoids base64 expansion and repeated manifest image copies | Static deployment must include the referenced files |
| Inline raster bytes in every manifest | Self-contained JSON with no image routes | Expands every manifest and live reload with image data |
| Stream in serve mode, inline during export | Efficient serve mode and self-contained static JSON | Adds an export option and two image representations |

## Tradeoffs

Reference-based static export matches the existing `ara layout --json` command, which writes JSON to stdout and has no asset-copy step. Document a complete static deployment layout with `manifest.json` beside `evidence/figures/`, including a manifest in a directory different from the viewer document. Embedded mode embeds reusable viewer assets, not the user's artifact images; those continue to come from the server. SVG, remote image fetching, recursive discovery, and single-file inline export are outside this proposal.

## Migration

1. Use language-server references before changing the exported `Exhibit` model. Add unit and fixture tests for image-bearing and no-image figures, companion grouping, raster-only claim linkage, old JSON compatibility, and missing/unsafe references.
2. Implement the optional field, native discovery, metadata selection, and stable diagnostics. Update all struct literals and snapshot expectations affected by the additive field.
3. Implement source-aware resolution and local/hub delivery. Add tests for actual image bytes and MIME, range requests, static fallback, nested manifest URLs, traversal, and escaping symlinks.
4. Implement figure rendering and scoped styles. Extend existing browser tests for decoded images, captions, blank-body figures, retained body tables, and the unchanged no-image branch.
5. Launch `ara serve` with embedded assets and with `--assets`; inspect both rendering branches. Deploy exported JSON and copied image files under a nested static path without an API and observe decoded images. Exercise the hub image route. Check wide and narrow pane geometry with screenshots.
6. Run workspace and browser tests, formatting, Clippy, wasm build and size checks. Regenerate the embedded viewer even for upstream core changes, then check freshness. Bump the next patch version, add a changelog entry, and update `docs/exhibit-markdown-rendering.md`, `docs/stage-4-serve.md`, `docs/manifest-schema.md`, hub deployment guidance, and the relevant backlog entries.

## Next Steps

Approve the reference-based static packaging contract and the author/image selection rules. Implement as `feat/60-figure-exhibit-images`, preferably stacked on the recovered #40 PR. After verification, rewrite this plan as the permanent figure design document and remove it from `plans/`. Use `Closes #60` in the implementation PR.
