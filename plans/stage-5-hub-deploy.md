# Stage 5 — Hub Deployment + Caching (Docker)

**PR target:** `stage5-hub-deploy` → `main`. **Depends on:** Stage 4 (`ara serve`,
merged; `0.1.0` cut in #10). **Version bump:** `0.1.2 → 0.1.3` — this PR adds a
new `--hub` runtime mode + new CLI flags + a viewer-source change (relative API
URLs), so it is **functional**: patch bump + `CHANGELOG.md` `[Unreleased]` entry
per `CLAUDE.md`. The release cut (roll changelog, pin, tag) is a **separate**
Release PR (like #15), not this PR.

> **Refresh note (2026-07-11).** This plan was re-audited against the shipped
> Stage 4 code (`crates/ara-cli/src/serve/{mod,cache,assets,watch}.rs` and the
> viewer's `crates/ara-viewer/src/source.rs`). Two gating forks the original
> plan hand-waved are now resolved by the human dev:
> - **D1 — Hub routing = path-based `/a/{id}/`** (not host/subdomain, not
>   deferred). Requires the viewer to resolve its API/WS URLs **relative** to the
>   page, and the hub to inject `<base href="/a/{id}/">` into the per-ARA
>   `index.html`. This is a viewer-source change → **triggers a viewer-embed
>   regen** (`scripts/embed-viewer.sh`; the `viewer-embed-fresh` CI gate).
> - **D2 — Docker assets = embedded-only.** Ship the self-contained musl binary
>   (viewer baked in via `include_dir!`); no `dist/` copy, no `--assets` in the
>   image. Consequence: `embedded_handler` does **no** brotli/gzip content
>   negotiation, so wasm is served uncompressed on the wire — compression is the
>   reverse proxy's job (Caddy/nginx). The original plan's "precompressed
>   brotli/gzip assets" step is therefore **out of scope** here.

## Problem background

Local serving works (Stage 4): `ara serve <dir>` parses one ARA once, caches the
positioned manifest JSON + a content-hash `ETag`, serves the embedded viewer, and
live-reloads on file changes. The hub instead serves **many** ARAs read-only,
where every request is a pure cache read (parse-once-at-**ingest**, no watcher),
and the whole thing ships as one small static Docker image.

The build matrix stays intentionally tiny: one `wasm32` client artifact (already
baked into the binary) + one `x86_64-unknown-linux-musl` server binary — the
same binary for local and hub. Windows → WSL, macOS → Docker; no cross-compile.

### What Stage 4 already gives us (reused verbatim)

- `cache::CachedAra` — parse + layout + serialize + **per-ARA content-hash
  `ETag`** + sandboxed `figures_dir` (`<dir>/evidence`). Already per-ARA; the hub
  just holds many of them. **No change needed.**
- `serve::manifest` handler — cached JSON body, `ETag`, `304` on
  `If-None-Match`. Reused; only the extractor for *which* ARA differs.
- `assets::embedded_handler` + `include_dir!` viewer — the self-contained bundle
  the hub serves as shared immutable assets.
- The `AppState` / `build_router` split and the `oneshot` test harness — extended,
  not rewritten.

### The routing problem (D1) and why the viewer must change

The Stage-4 viewer hardcodes **absolute** API URLs
(`crates/ara-viewer/src/source.rs:42–46`):

```rust
manifest_url: "/api/manifest",  fallback_url: "manifest.json",  live_url: "/api/live",
```

Absolute `/api/manifest` cannot address per-ARA state under `/a/{id}/`. The fix
(path-based, D1) is to make the viewer fetch **relative to its document base**
and have the hub set that base per ARA:

- Viewer: `manifest_url: "api/manifest"`, `live_url: "api/live"`, fallback stays
  `"manifest.json"` — all **relative**. Under local serve the page is at `/`, so
  `new URL("api/manifest", document.baseURI)` → `/api/manifest` (unchanged
  behaviour). Under the hub the served `index.html` carries
  `<base href="/a/{id}/">`, so the same relative URL → `/a/{id}/api/manifest`.
- Trunk's fingerprinted bundle URLs are **root-absolute** (`/ara-viewer-{hash}.js`,
  `/styles-{hash}.css`), which **ignore `<base>`**, so they keep loading from the
  shared root path in both modes. No `Trunk.toml` / `public_url` change needed.

This is the accepted cost of D1: a viewer-source edit → the embedded bundle must
be regenerated (`scripts/embed-viewer.sh`), which the `viewer-embed-fresh` gate
enforces.

## Proposed solution

Add a `--hub --ara-root <dir>` mode that scans `<dir>` once at startup, parses
each child directory into a `CachedAra`, and serves them read-only under
`/a/{id}/…` with a shared immutable viewer at root; make the viewer resolve
API/WS URLs relative to `document.baseURI`; ship it as a multi-stage
musl→distroless Docker image with correct HTTP caching headers.

## Implementation steps

### 1. Viewer: relative API/WS URLs (D1) — `crates/ara-viewer/src/source.rs`

- `ManifestSource::default()` → `manifest_url: "api/manifest".into()`,
  `live_url: "api/live".into()` (fallback `"manifest.json"` already relative).
- `absolute_ws_url(path)` → resolve against the document base rather than
  `location.host + path`: `web_sys::Url::new_with_base(path, &document_base)`
  (or `new URL(path, document.baseURI)`), then swap scheme `https→wss` / else
  `ws`. This makes `api/live` resolve under `<base href="/a/{id}/">` on the hub
  and under `/` locally. (`document.baseURI` reflects any `<base>` tag.)
- Native stubs unchanged.
- **Unit test (native):** assert the new relative defaults (`"api/manifest"`,
  `"api/live"`, `"manifest.json"`) — cheap regression guard on the wire contract.
- **Regen the embed** (step 8) — this edit changes the frontend source, so
  `viewer-embed-fresh` will (correctly) fail until `scripts/embed-viewer.sh` runs.

### 2. Hub cache — `crates/ara-cli/src/serve/hub.rs` (new)

- `type Aras = Arc<HashMap<String, Arc<CachedAra>>>` — built **once at startup**,
  immutable thereafter (read-only hub, parse-once-at-ingest). Lock-free reads; no
  `RwLock` (the original `Arc<RwLock<HashMap>>` is only needed once a hot
  upload/ingest API exists — explicitly out of scope, see "Out of scope").
- `fn ingest(root: &Path) -> (Aras, Vec<(String, ParseReport)>)`: for each
  immediate subdirectory, `id = dir_name`, `CachedAra::from_dir(&dir)`; collect
  successes into the map and failures into a skipped-list. A broken ARA is
  **logged and skipped**, not fatal — one bad artifact must not sink the hub.
- Reject/skip ids that aren't clean single path segments (no `/`, `.`, `..`);
  ids come from directory names but guard anyway.
- Log an ingest summary at startup: `N ARAs ingested, M skipped`.
- **Unit tests (native):** ingest a temp root with two fixture ARAs → map has
  both ids; a root containing one good + one intentionally-broken dir → good id
  present, broken id skipped, summary counts correct; empty root → empty map.

### 3. Hub routing — `crates/ara-cli/src/serve/mod.rs`

New `ServeArgs` fields (clap):

- `--hub` (`bool`) — enable hub mode.
- `--ara-root <dir>` (`Option<PathBuf>`) — required with `--hub`; the dir scanned
  by `ingest`. Validate the `--hub`/`--ara-root` pairing (clap `requires`, or a
  manual check that errors clearly). In hub mode the positional `dir` arg is not
  used; make the positional optional and error if both/neither are supplied for
  the chosen mode.

Hub route table (a `build_hub_router(aras: Aras, assets: Assets) -> Router`
sibling to `build_router`):

```
GET  /a/{id}                 -> 301 redirect to /a/{id}/           (trailing slash)
GET  /a/{id}/                -> index.html with <base href="/a/{id}/"> injected, no-cache
GET  /a/{id}/api/manifest    -> cache[id] manifest (ETag/304); 404 if id unknown
GET  /a/{id}/api/figure/*    -> ServeDir(cache[id].figures_dir)   ; 404 if id unknown
GET  /                       -> minimal HTML/JSON index of available ARA ids
GET  /{asset}                -> embedded_handler (shared immutable js/wasm/css)
```

- **No `/api/live`, no watcher in hub mode** — live reload is local-only. The
  `watch::spawn` + `reparse_and_swap` + broadcast path is not wired in hub mode;
  the viewer's live WebSocket simply never opens (it already degrades to inert,
  `source.rs:133-136`).
- `/a/{id}/` handler: take the embedded `index.html` bytes and insert
  `<base href="/a/{id}/">` immediately after `<head>` (single string splice),
  serve `text/html; charset=utf-8`, `Cache-Control: no-cache`. Under `--assets`
  (dev only; not used in the shipped image) read the on-disk index instead.
  404 if `id` is unknown (do **not** fall back to the SPA index for an unknown
  ARA — that would mask a bad link).
- Per-ARA `api/manifest`: reuse the Stage-4 `manifest` handler logic keyed by the
  path `id` (look up `cache[id]`, else 404). `304`/`ETag` semantics identical.
- Per-ARA `api/figure`: `ServeDir::new(cache[id].figures_dir)` — same traversal
  safety as Stage 4 (`..` rejected).
- The Stage-4 single-ARA path (`build_router` + `run`) is unchanged; `run`
  branches to the hub path when `--hub` is set.

**Router tests (native, `oneshot` — mirror the Stage-4 suite):**

- `/a/{id}/api/manifest` → 200, `application/json`, correct per-ARA `ETag`;
  `If-None-Match` → `304`.
- Two distinct ARAs return **different** manifests + etags at their own paths.
- `/a/{unknown}/api/manifest` → `404` (not the other ARA, not index.html).
- `/a/{id}/` → 200 `text/html`, body contains `<base href="/a/{id}/">`,
  `no-cache`.
- `/a/{id}` (no slash) → `301` → `/a/{id}/`.
- `/a/{id}/api/figure/../../Cargo.toml` → not `200` (per-ARA traversal guard).
- shared asset (`/` index / an embedded root asset) → served.

### 4. HTTP caching headers (audit against the shipped behaviour)

Already correct in Stage 4 and reused as-is:

- Fingerprinted wasm/js/css via `embedded_handler` → `public, max-age=31536000,
  immutable` (`assets.rs:65-71`). ✓
- `index.html` / `manifest.json` → `no-cache`. ✓ (the per-ARA `/a/{id}/` index is
  also `no-cache`.)
- `/api/manifest` → `ETag` + `no-cache` + `304` on `If-None-Match`
  (`mod.rs:161-191`). ✓ Reused per-ARA unchanged.
- `/api/figure/*` → `ServeDir` defaults (Last-Modified/ETag range support). ✓

**No header changes required** — this step is now an audit line, not new work.
(Original plan's per-manifest `max-age` tuning is unnecessary: `no-cache` +
strong `ETag` already gives a cheap conditional GET, which is what the hub wants.)

### 5. Dockerfile (multi-stage, musl → distroless) — D2 embedded-only

- **Builder stage** (`rust:1-alpine` or `rust:1` + musl target): install
  `wasm32-unknown-unknown` + `trunk` + `wasm-bindgen-cli`; run
  `scripts/embed-viewer.sh` (or `trunk build --release` + copy) so the bundle is
  baked in; then
  `cargo build --release --target x86_64-unknown-linux-musl -p ara-cli`.
  Use `cargo-chef` to cache the dependency compile as its own layer.
- **Runtime stage** (`gcr.io/distroless/static:nonroot`): copy only the static
  `ara` binary; run as nonroot; `ENTRYPOINT ["/ara"]`. No shell, no libc, no
  `dist/` copy (D2: viewer is baked into the binary).
- `.dockerignore`: exclude `target/`, `corpus-external/`, git, plans/docs.
- Target compressed image **< 20 MB** (static binary + distroless; the wasm is a
  few hundred KB inside the binary).
- Document the run: `docker run -p 8080:8080 -v /aras:/aras ara --hub --ara-root
  /aras` (binds `0.0.0.0` inside the container — see step 7 note on bind address).

**Bind-address note:** Stage 4 binds `127.0.0.1` (`mod.rs:125`). In a container
that's unreachable from the host. Add a `--host <ip>` flag (default `127.0.0.1`
to preserve the safe local default) and set it to `0.0.0.0` in the container/
compose. Do **not** change the default — the local dev tool stays loopback-only.

### 6. Ops docs — `docs/deploy.md` (new)

- `docker run` / a minimal `compose.yaml` (bind-mount `--ara-root`, publish port,
  `--host 0.0.0.0`).
- systemd unit for a bare-metal binary: `Restart=on-failure`, a non-root user,
  `--host`/`--ara-root` via `Environment=`/`ExecStart`.
- Reverse-proxy front for TLS **and gzip/brotli of the wasm** (Caddy auto-TLS
  recommended; nginx sample). Call out explicitly that **the proxy owns
  compression** under D2 (embedded assets are served uncompressed).
- `--poll` guidance is **local-only** (hub has no watcher); note it under the
  Stage-4 local-serve docs, not here.

### 7. CI — `.github/workflows/ci.yml`

- Add a `docker` job: `docker build` the image, `docker run` it against a tiny
  bundled fixture ARA root, then smoke-test with `curl`:
  - `/a/{id}/api/manifest` → `200`, `content-type: application/json`, has an
    `ETag`; a second request with `If-None-Match` → `304`.
  - `/a/{id}/` → `200 text/html`, body contains `<base href=`.
  - a root wasm asset → `200 application/wasm`.
  - image compressed size under budget (fail the job if exceeded).
- Keep it a separate job (Docker/buildx setup) so it doesn't slow the Rust jobs.
- Registry push on tags is **optional / deferred** (note it; don't wire secrets
  in this PR).

### 8. Regenerate the embedded viewer + version/docs

- `scripts/embed-viewer.sh` — **required** (step 1 changed frontend source). CI's
  `viewer-embed-fresh` gate fails otherwise.
- Bump workspace version `0.1.2 → 0.1.3` in `Cargo.toml`.
- `CHANGELOG.md` `[Unreleased]` → `Added`: `ara serve --hub --ara-root` read-only
  multi-ARA mode (path-based `/a/{id}/` routing, per-ARA parse-once cache); musl
  → distroless Docker image; `--host` flag; deploy docs. `Changed`: viewer now
  resolves its API/live URLs relative to the page (enables hub sub-path serving;
  local serve unchanged).
- Fold this plan into `docs/stage-5-hub-deploy.md` (or extend `docs/stage-4-serve.md`
  with a "Hub mode" section) and remove it from `plans/`, per `CLAUDE.md`.

## Tests / verification (summary)

| Layer | Test |
|-------|------|
| viewer (native) | `ManifestSource::default()` returns the relative URLs |
| hub cache (native) | ingest 2 good → both; 1 good + 1 broken → broken skipped; empty root → empty |
| hub router (native, `oneshot`) | per-ARA manifest 200/etag/304; two ARAs differ; unknown id → 404; `/a/{id}/` has `<base>` + no-cache; `/a/{id}` → 301; per-ARA figure traversal rejected |
| container (CI) | `docker run` smoke: manifest 200/etag/304, `/a/{id}/` html+base, wasm `application/wasm`, image size budget |

`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, and `wasm-pack test --headless --chrome
crates/ara-viewer` all green; `scripts/embed-viewer.sh --check` passes after regen.

## Milestone / acceptance

`docker run` behind Caddy/nginx serves multiple ARAs over TLS at `/a/{id}/`; hub
reads are pure cache hits (no reparse after startup); the image is small and
static; local `ara serve <dir>` is unchanged. Ships as `0.1.3` via a later
Release PR (this PR bumps the patch + `[Unreleased]` entry only).

## Out of scope (deferred)

- **Ingest / upload API** for adding ARAs at runtime (assumed upstream/offline).
  The immutable `Arc<HashMap>` is chosen deliberately; a hot-ingest API would
  reintroduce `ArcSwap`/`RwLock` — a separate change.
- **Precompressed brotli/gzip assets in the image** — moot under D2 (embedded
  assets aren't content-negotiated); the reverse proxy compresses. Revisit only
  if we later switch the image to `--assets`-served precompressed `dist/`.
- **Auth, horizontal scaling, registry push secrets.**
- **`aarch64-apple-darwin` brew build** (additive CI job, deferred).

## Decisions (resolved by human dev — 2026-07-11)

1. **D1 — Hub routing = path-based `/a/{id}/`.** Viewer fetches relative to
   `document.baseURI`; hub injects `<base href="/a/{id}/">` per ARA; Trunk's
   root-absolute asset URLs stay shared/immutable across ARAs. Rejected:
   host/subdomain (needs wildcard DNS+TLS) and defer-multi-ARA (ships no hub).
   Accepted cost: viewer-source change → embed regen.
2. **D2 — Docker assets = embedded-only.** Ship the self-contained musl binary;
   no `dist/` copy, no `--assets` in the image; compression is the proxy's job.
   Rejected: `--assets`-precompressed (larger image, two asset copies to sync).

## GSTACK REVIEW REPORT

| Review | Trigger | Runs | Status |
|--------|---------|------|--------|
| CEO Review | `/plan-ceo-review` | 0 | — (pending, if requested) |
| Eng Review | `/plan-eng-review` | 0 | — (pending, if requested) |
| Design Review | `/plan-design-review` | 0 | — (n/a — no new UI surface; tree/graph unchanged) |
| DX Review | `/plan-devex-review` | 0 | — (pending, if requested) |

Refreshed from a direct audit of the shipped Stage-4 code; the two gating forks
(D1 routing, D2 assets) are resolved. **Awaiting human review before
implementation.**
