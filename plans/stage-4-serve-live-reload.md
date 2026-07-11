# Stage 4 — `ara serve` Local Mode + Live Reload

**PR target:** `stage4-serve-live-reload` → `main`. **Depends on:** Stage 3.
**Version bump:** `0.0.5 → 0.0.6`. (Stage 3 took `0.0.5`.)

## Resolved decisions (2026-07-11)

Two forks the original plan left open, decided with the human developer:

1. **Asset delivery — embed by default + `--assets <dir>` override.** `ara-cli`
   embeds a prebuilt viewer `dist/` via `include_dir!` so `cargo install ara-cli`
   → `ara serve ./my-ara` works with zero config (resolves `T-VIEWER-DIST-PACKAGING`).
   `--assets <dir>` overrides with an on-disk `dist/` (dev + the Stage-5 Docker
   `--assets /assets` model). Embedded assets are served by a small custom
   handler (correct `.wasm` MIME, `no-cache` for `index.html`, `immutable` for
   hashed bundles); the `--assets` path uses `tower-http` `ServeDir` with
   `precompressed_brotli()/gzip()`. The committed dist lives at
   `crates/ara-cli/assets/viewer/` (built by `trunk build --release`); this is
   the accepted cost of embedding (binary wasm in git), refreshed at release.
2. **Client mode auto-detection (one wasm bundle).** The client first tries
   `/api/manifest`; on network error / 404 it falls back to the static
   `manifest.json` (so the same bundle works under `ara serve` **and** static
   hosting / `trunk serve`). It always attempts the `/api/live` WebSocket; if the
   socket never opens (static host) live-reload is simply inert.

## Problem background

With parse+layout (Stages 1–2) and a client that renders a static manifest
(Stage 3), the missing piece is a server that serves the client, serves the
live manifest as JSON, streams figures, and — for local editing — re-parses on
file change and pushes an update that refreshes data **without losing pan/zoom/
selection**. `tower-livereload`'s whole-page reload is too jarring for editing.

## Proposed solution

An axum 0.8 server behind `ara serve <dir>`: `ServeDir` for the client (with
precompression), `/api/manifest` (JSON from `ara-core`), `/api/figure/{path}`
(range-capable, sandboxed to the figures dir), and `/api/live` (WebSocket).
`notify` + `notify-debouncer-full` watch the ARA dir; on a debounced change,
re-parse, atomically swap a cache, bump the ETag, and push over the WebSocket so
the client re-fetches `/api/manifest` and re-renders in place.

## Implementation steps

1. **Deps:** `axum` 0.8, `tokio`, `tower-http` (`fs`, `compression`),
   `notify` + `notify-debouncer-full`, `arc-swap`, `bytes`. All native-only
   (behind `ara-core`'s `native` feature / `ara-cli`).
2. **`ara serve <dir> [--port] [--poll] [--full-reload]`** subcommand (clap).
3. **Cache type:**
   ```rust
   struct CachedAra { manifest: Arc<Manifest>, manifest_json: Arc<Bytes>, etag: String, figures_dir: PathBuf }
   // Local: Arc<ArcSwap<CachedAra>>
   ```
   Serialize manifest JSON once; hand out `Arc<Bytes>` clones. `etag` = source
   hash; bump on reparse.
4. **Routes:**
   - `GET /`, `/assets/*` → `ServeDir` with `precompressed_brotli()` /
     `precompressed_gzip()` (serves the Trunk `dist`). Correct `.wasm` MIME.
   - `GET /api/manifest` → `Arc<Bytes>`, `ETag`, `304` on `If-None-Match`,
     `Cache-Control: no-cache`.
   - `GET /api/figure/{*path}` → `ServeFile`/`ServeDir` range requests; reject
     `..`/absolute escapes; constrain to `figures_dir`.
   - `GET /api/live` → WebSocket; emit a message (new etag) on cache swap.
5. **Watcher:** `notify` + debouncer (~200–500 ms) on
   `trace/exploration_tree.yaml` + `evidence/`. On event → `parse_and_layout` →
   `ArcSwap::store` → broadcast etag to WebSocket subscribers. `--poll` selects
   the polling watcher (network mounts / cross-boundary bind mounts).
6. **Client (Stage 3) changes:** open `/api/live`; on message, re-fetch
   `/api/manifest` and update the manifest signal, **preserving** pan/zoom +
   selection where node ids persist. Keep `tower-livereload` full-page reload
   behind `--full-reload` for wasm-bootstrap debugging.

## Tests / verification

- Integration: start server on a temp ARA, `GET /api/manifest` → valid JSON +
  `ETag`; repeat with `If-None-Match` → `304`.
- Edit the temp YAML → assert a WebSocket message fires and the new manifest
  differs (debounce collapses bursts).
- Figure range-request test: `Range: bytes=0-99` → `206` + correct length;
  path-escape (`../secret`) → rejected.
- Precompression: `Accept-Encoding: br` → `.wasm.br` served with `Vary`.

## Milestone / acceptance

`ara serve ./my-ara`, open the browser, edit the YAML → the graph updates in
place without losing selection/zoom.

## Release cut — `0.1.0` (first published release)

Stages 1–4 together are the first usable product, so a dedicated **release PR
follows this stage** (separate from the Stage 4 feature PR):

1. Bump the workspace version `0.0.6 → 0.1.0`; move `CHANGELOG.md`
   `[Unreleased]` entries under a `## [0.1.0]` heading.
2. Wire `version =` on the intra-workspace path deps (`ara-core`, `ara-wasm`).
3. Publish to crates.io **in dependency order**: `ara-core` → `ara-wasm` →
   `ara-cli`, waiting for each to appear in the index before the next.
4. Tag `v0.1.0`.

After this, `cargo install ara-cli` yields a working `ara validate` /
`ara serve`.

## Out of scope (deferred)

Hub multi-ARA mode, Docker, TLS/reverse proxy, immutable-bundle caching (Stage
5). This stage is single-ARA local serving.

## CHANGELOG (Unreleased → Added)

- `ara serve <dir>`: axum server with static assets (precompressed),
  `/api/manifest` (ETag/304), range-capable `/api/figure/*`, and `notify`-driven
  WebSocket live reload (`--poll`, `--full-reload`).
