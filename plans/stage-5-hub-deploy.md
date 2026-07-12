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
  **Refactor the URL-building core out of the `web_sys` call** so it is unit-
  testable without a browser context.
- Native stubs unchanged.
- **Regression risk (issue 4):** this rewrite changes the ONE feature Stage 4
  shipped (local live reload). A subtly-wrong base resolution or scheme-swap makes
  the local WebSocket silently fail to open — and that failure is intentionally
  swallowed as "no live server, inert" (`source.rs:133-136`), so a regression is
  **invisible**. Guard it:
- **Tests:**
  - (native) assert the relative defaults (`"api/manifest"`, `"api/live"`,
    `"manifest.json"`) — cheap wire-contract guard.
  - (**wasm**, issue 4 — the viewer already has a `wasm-pack` job, `ci.yml:122`)
    assert `absolute_ws_url("api/live")` resolves correctly for BOTH a root base
    (→ `ws://host/api/live`, local-serve unchanged) and a `/a/{id}/` base
    (→ `ws://host/a/{id}/api/live`, hub).
  - (**headless-Chrome**, issue 12 — extend the existing `viewer-web-test` job)
    load a page with `<base href="/a/x/">` and assert the viewer's relative fetch
    resolves `api/manifest` → `/a/x/api/manifest`. This is the ONLY test that
    proves D1's load-bearing assumption (`<base>` + relative fetch) in a real
    browser — the native string test and the curl grep do not.
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
- **Id charset (issue 13c):** restrict ids to `[A-Za-z0-9._-]+`; reject anything
  else (spaces, non-ASCII, `/`, `..`) with a logged skip. This single guard
  covers both the path-segment safety AND the URL/HTML-encoding concerns (a dir
  named `my ara` or a non-ASCII name would otherwise need percent-encoding in the
  `<base href>` and route matching — see issue 2 + 13c). No encoding needed once
  the charset is constrained.
- **Id collision (issue 6.3):** if two subdirs map to the same id, **log and skip
  the duplicate** — do not silently overwrite in the map.
- **Startup behavior (issue 6):**
  - `--ara-root` missing / not a directory / unreadable → **fatal**, exit non-zero
    with a clear message (consistent with local `run`'s fast-fail, `mod.rs:56-68`).
  - empty root, or every child failed to parse → **start, but log a loud WARN**
    (`0 ARAs ingested`). A silently-empty hub behind a load balancer reads as "up"
    while serving nothing — the WARN is the ops signal.
- Log an ingest summary at startup: `N ARAs ingested, M skipped`.
- **Memory (issue 9):** the hub serves only `manifest_json` + `figures_dir` and
  never reads `CachedAra.manifest` (the parsed graph, `#[allow(dead_code)]` at
  `cache.rs:18`). Drop/omit the parsed `Arc<Manifest>` on the hub path so the hub
  doesn't pay ~2x resident memory (parsed graph + serialized bytes) for data it
  never uses. Document the memory-vs-N profile. Serial ingest stays (one-time,
  off the request path); parallel ingest is a TODO only if corpus size warrants
  (see `T-STATIC-EXPORT` for the larger scaling question).
- **Unit tests (native):** ingest a temp root with two fixture ARAs → map has
  both ids; one good + one intentionally-broken dir → good id present, broken id
  skipped, summary counts correct; empty root → empty map (+ WARN); nonexistent
  root → `Err`/fatal; a rejected-charset dir name (`bad id`, `..`) → skipped; two
  dirs colliding on id → duplicate skipped, not overwritten.

### 3. Hub routing — `crates/ara-cli/src/serve/mod.rs`

New `ServeArgs` fields (clap) — **mode selection via `ArgGroup` (issue 7):**

- `--hub` (`bool`) — enable hub mode.
- `--ara-root <dir>` (`Option<PathBuf>`) — the dir scanned by `ingest`.
- Model local-vs-hub as a **required, single-member `ArgGroup`** (or
  `conflicts_with` / `requires` pairing) so clap rejects bad combos **at parse
  time** with a clear message — do NOT scatter a manual cross-field check into
  `run` (untested, imperative). `--hub` requires `--ara-root`; the positional
  `dir` is for local mode only and conflicts with `--hub`.
- **`--assets` in hub mode (issue 14):** `--assets` is wired **end-to-end** in hub
  mode — the shared js/wasm/css come from an on-disk `ServeDir` AND the per-ARA
  index is read from disk (with `<base>` injection), so there is no half-wired
  embedded/disk split. `build_hub_router(aras, assets)` honors `Assets::Dir`
  consistently across both the `/a/{id}/` index and the `/{asset}` shared route.
  (The shipped Docker image still uses embedded — D2 — so this is a dev-parity
  path, tested.)
- **Tests (`try_parse_from`, issue 7):** `--hub --ara-root x` ok; local `dir` ok;
  `--hub` without `--ara-root` → err; both modes → err; neither → err.

Hub route table (a `build_hub_router(aras: Aras, assets: Assets) -> Router`
sibling to `build_router`):

```
GET  /a/{id}                 -> 308 to /a/{id}/ IF id known; else 404   (issue 13a)
GET  /a/{id}/                -> index.html with <base href="/a/{id}/"> injected, no-cache
GET  /a/{id}/api/manifest    -> cache[id] manifest (ETag/304); 404 if id unknown
GET  /                       -> minimal HTML/JSON index of available ARA ids
GET  /{asset}                -> shared js/wasm/css IF the embedded/dist file
                                exists; else 404 (NOT SPA-fallback — issue 3)
```

**Figures are OUT of scope for this PR (issue 11 + T-HUB-FIGURES).** Stage 4's
`nest_service("/api/figure", ServeDir::new(dir))` (`mod.rs:144`) is a static
prefix bound to ONE dir; axum cannot `nest_service` a `ServeDir` under a `{id}`
path parameter, so the plan's original "`ServeDir(cache[id].figures_dir)` — same
traversal safety as Stage 4" line was **factually wrong** (that mechanism does
not survive the routing change). The viewer also renders figures inert today
(`detail.rs:386`), so the endpoint would serve nothing. Deferred to the PR that
lights up figure rendering, where the traversal-safe handler + the relative
figure-`src` URL contract get designed and tested together.

- **No `/api/live`, no watcher in hub mode** — live reload is local-only. The
  `watch::spawn` + `reparse_and_swap` + broadcast path is not wired in hub mode;
  the viewer's live WebSocket simply never opens (it already degrades to inert,
  `source.rs:133-136`).
- **`/a/{id}/` handler — hardened base-href injection (issue 2):** insert
  `<base href="/a/{id}/">` after `<head>`, serve `text/html; charset=utf-8`,
  `Cache-Control: no-cache`. Because ids are constrained to `[A-Za-z0-9._-]+` at
  ingest (step 2), the id is already safe to interpolate into the `href="..."`
  attribute — no HTML-injection sink. **Guard the splice:** if `<head>` is not
  found (e.g. a future Trunk reformats it), return an error / 500 and log — do
  **not** silently serve a base-less page (every relative API URL would break and
  the viewer would render nothing). Tests: a malicious-looking id is rejected at
  ingest so never reaches the splice; a no-`<head>` fixture → error, not a silent
  base-less page.
- **Per-ARA `api/manifest` — shared helper (issue 5):** extract the ETag/304/body
  core from the Stage-4 handler (`mod.rs:162-191`) into one free function
  `serve_cached_manifest(&CachedAra, &HeaderMap) -> Response`. Both the Stage-4
  handler (`State<AppState>` → `cache.load()`) and the hub per-id handler
  (`cache[id]`, else 404) call it — one source of truth for the conditional-GET
  contract, no copy-paste of the 30-line block.
- **`/a/{id}` bare (issue 13a):** redirect to `/a/{id}/` with **308** (not 301 —
  301 is permanently browser-cacheable) **only if `id` is known**; unknown id →
  404 at this route too, so we never cache a redirect to a 404.
- **Root `/{asset}` — no SPA fallback in hub (issue 3):** serve an embedded/dist
  file only if it exists; unknown non-asset paths (`/typo`, `/favicon.ico`) → 404.
  Do **not** reuse `embedded_handler`'s index.html fallback at root — the hub has
  no client-side router at root, and a base-less viewer index would fetch
  `/api/manifest` (no such hub route) and show a load error. This matches the
  `/a/{unknown}/` → 404 rule. Root-absolute Trunk asset URLs
  (`/ara-viewer-{hash}.js`) still resolve here — that's how per-ARA pages load the
  shared bundle. Test: `/typo` → 404.
- **`manifest.json` fallback under `<base>` (issue 13b):** the viewer's fallback
  URL stays relative (`"manifest.json"`, `source.rs:44`), so under
  `<base href="/a/{id}/">` it resolves to `/a/{id}/manifest.json` — a route the
  hub does not have. This is **harmless** (the primary `api/manifest` always
  exists on the hub, so the fallback never fires) but the plan's earlier "fallback
  stays working" wording was wrong: on the hub the fallback is inert, not a live
  static file. Correct the docs accordingly; no hub alias needed.
- The Stage-4 single-ARA path (`build_router` + `run`) is unchanged; `run`
  branches to the hub path when `--hub` is set.

**Router tests (native, `oneshot` — mirror the Stage-4 suite):**

- `/a/{id}/api/manifest` → 200, `application/json`, correct per-ARA `ETag`;
  `If-None-Match` → `304`.
- **Pure cache hit (issue 8):** two sequential `/a/{id}/api/manifest` requests
  return the **same** etag with no reparse — pins the milestone's core property
  (hub reads never re-parse after startup).
- Two distinct ARAs return **different** manifests + etags at their own paths.
- `/a/{unknown}/api/manifest` → `404` (not the other ARA, not index.html).
- `/a/{id}/` → 200 `text/html`, body contains `<base href="/a/{id}/">`,
  `no-cache`.
- `/a/{id}` (no slash, **known** id) → `308` → `/a/{id}/` (issue 13a).
- `/a/{unknown}` (no slash) → `404`, **not** a 308 to a 404 (issue 13a).
- Unknown root path `/typo` → `404`, **not** the SPA index (issue 3).
- shared asset (`/` index / a real embedded root asset) → served.

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

- **Builder stage** (`rust:1` + musl target): **no wasm toolchain.** The viewer
  bundle is already committed under `crates/ara-cli/assets/viewer/` and baked in
  at compile time via `include_dir!` (`assets.rs:24`), so the builder is a plain
  `cargo build --release --target x86_64-unknown-linux-musl -p ara-cli` — it
  consumes the committed bytes. Do **not** install `trunk` / `wasm-bindgen` /
  `wasm32` or run `scripts/embed-viewer.sh` in the image (issue 1): the regen
  already happens on the dev machine and is enforced by the `viewer-embed-fresh`
  CI gate (step 8), and a wasm rebuild inside the image is not `cargo-chef`-
  cacheable, defeating the dependency-layer cache. Use `cargo-chef` to cache the
  Rust dependency compile as its own layer.
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
**Test (issue 8.2):** assert `--host` defaults to `127.0.0.1` and parses an
override, so the loopback-only local default can't silently regress to `0.0.0.0`
(a security regression) and the container's `0.0.0.0` is honored.

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
- **Correct the fallback wording (issue 13b):** document that the viewer's
  `manifest.json` static fallback is a local/static-host path and is **inert on
  the hub** (the primary `api/manifest` always resolves there), not a live static
  file served under `/a/{id}/`.

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
| viewer (wasm) | `absolute_ws_url("api/live")` resolves for base `/` AND `/a/{id}/` (issue 4) |
| viewer (headless-Chrome) | under `<base href="/a/x/">`, relative fetch → `/a/x/api/manifest` (issue 12) |
| CLI parse (native) | `try_parse_from`: hub+root ok, local dir ok, hub-no-root err, both err, neither err (issue 7); `--host` default `127.0.0.1` + override (issue 8) |
| hub cache (native) | ingest 2 good → both; 1 good + 1 broken → skipped; empty → empty+WARN; missing root → err; rejected-charset id → skipped; id collision → duplicate skipped (issue 6) |
| hub router (native, `oneshot`) | per-ARA manifest 200/etag/304; two sequential reads same etag no-reparse (issue 8); two ARAs differ; unknown id → 404; `/a/{id}/` has `<base>` + no-cache; no-`<head>` fixture → error not base-less (issue 2); known `/a/{id}` → 308; unknown `/a/{id}` → 404 not 308 (issue 13a); unknown root `/typo` → 404 not SPA (issue 3) |
| container (CI) | `docker run` smoke: manifest 200/etag/304, `/a/{id}/` html+base, wasm `application/wasm`, image size budget |

`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, and `wasm-pack test --headless --chrome
crates/ara-viewer` all green; `scripts/embed-viewer.sh --check` passes after regen.

## Milestone / acceptance

`docker run` behind Caddy/nginx serves multiple ARAs over TLS at `/a/{id}/`; hub
reads are pure cache hits (no reparse after startup); the image is small and
static; local `ara serve <dir>` is unchanged. Ships as `0.1.3` via a later
Release PR (this PR bumps the patch + `[Unreleased]` entry only).

## What already exists (reused verbatim, verified against shipped code)

| Existing code | Hub reuse |
|---------------|-----------|
| `cache::CachedAra` (`cache.rs:16-48`) | parse + layout + JSON + content-hash ETag + `figures_dir`. Already per-ARA; hub holds many. Hub drops the unused `.manifest` (issue 9). |
| `manifest` handler ETag/304 (`mod.rs:162-191`) | extracted to `serve_cached_manifest(&CachedAra, &HeaderMap)` and shared by local + hub (issue 5). |
| `embedded_handler` + committed `include_dir!` bundle (`assets.rs`) | shared immutable assets; the committed bytes mean Docker needs no wasm build (issue 1). Hub does NOT reuse its SPA index-fallback at root (issue 3). |
| `AppState` / `build_router` split + `oneshot` harness | extended with `build_hub_router` + hub `oneshot` tests, not rewritten. |
| `viewer-web-test` headless-Chrome job (`ci.yml:122`) | extended with a sub-path base-resolution test (issue 12), not duplicated. |
| `Assets::Dir` on-disk serving (`mod.rs:146-156`) | wired end-to-end into hub `--assets` (issue 14). |

Not reused (would have been a mistake to): Stage 4's
`nest_service("/api/figure", ServeDir)` — a static single-dir mount that cannot
sit under a `{id}` param, so per-ARA figures are deferred (issue 11).

## Out of scope (deferred)

- **Per-ARA figure serving** (`/a/{id}/api/figure/*`) — deferred to the
  figure-rendering PR; the viewer renders figures inert today (`detail.rs:386`)
  and the traversal-safe per-id handler + relative figure-`src` contract belong
  together (issue 11, `T-HUB-FIGURES`).
- **Static-export mode** (`ara build <root>`) — a running server is kept for
  Stage 5 (D3); static export tracked as `T-STATIC-EXPORT` for post-0.1.3.
- **Ingest / upload API** for adding ARAs at runtime (assumed upstream/offline).
  The immutable `Arc<HashMap>` is chosen deliberately; a hot-ingest API would
  reintroduce `ArcSwap`/`RwLock` — a separate change.
- **Parallel ingest** — serial parse at startup is kept (one-time, off the request
  path); revisit only if corpus size warrants (issue 9).
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
3. **D3 — Hub = running server, not static export (eng review 2026-07-12).**
   Rejected the static-export alternative (`ara build <root>` → per-ARA
   `manifest.json` + viewer served by a plain file host/CDN) for Stage 5. Reason:
   the running server keeps ONE artifact (same binary local + hub) and an
   identical `/api` contract local vs hub, so the viewer's live-with-fallback path
   is exercised the same way, at a smaller conceptual surface than a second
   subcommand + a static-hosting story. Static export is the better long-term
   scaling play (CDN, zero-runtime) but a larger product decision — tracked as
   `T-STATIC-EXPORT` for post-0.1.3.

## Implementation Tasks
Synthesized from this review's findings (eng review 2026-07-12). Each task
derives from a specific finding above. Run with Claude Code or Codex; checkbox as
you ship. P1 blocks ship; P2 lands same branch.

- [ ] **T1 (P1, human: ~1h / CC: ~10min)** — Dockerfile — builder is `cargo build --release --target musl` only, no wasm toolchain / no embed-viewer.sh in the image
  - Surfaced by: Architecture issue 1 — bundle is committed + baked via `include_dir!`; a wasm rebuild is redundant and breaks cargo-chef caching
  - Files: `Dockerfile`
  - Verify: image builds; `docker run` serves the viewer; build has no trunk/wasm step
- [ ] **T2 (P1, human: ~2h / CC: ~20min)** — serve/hub.rs — constrain ids to `[A-Za-z0-9._-]+` at ingest; log+skip rejects and collisions
  - Surfaced by: Architecture issue 2 + 13c + code-quality issue 6.3 — one guard covers HTML-escape, URL-encode, and silent collision
  - Files: `crates/ara-cli/src/serve/hub.rs`
  - Verify: unit tests for `bad id`, `..`, non-ASCII → skipped; two dirs same id → duplicate skipped
- [ ] **T3 (P1, human: ~1h / CC: ~10min)** — serve/mod.rs — guard base-href splice: error if `<head>` not found, never serve a base-less page
  - Surfaced by: Architecture issue 2 — a silent base-less page breaks every relative API URL
  - Files: `crates/ara-cli/src/serve/mod.rs`
  - Verify: no-`<head>` fixture → error response, not 200 base-less html
- [ ] **T4 (P1, human: ~1h / CC: ~10min)** — serve/mod.rs — root `/{asset}` serves real files only, else 404 (no SPA fallback in hub)
  - Surfaced by: Architecture issue 3 — base-less viewer index shows a load error for unknown root paths
  - Files: `crates/ara-cli/src/serve/mod.rs`
  - Verify: `/typo` → 404; real `/ara-viewer-{hash}.js` → 200
- [ ] **T5 (P1, human: ~2h / CC: ~20min)** — viewer/source.rs — refactor `absolute_ws_url` core out of `web_sys`; wasm-test both base cases
  - Surfaced by: Architecture issue 4 — silent regression to the shipped local live-reload
  - Files: `crates/ara-viewer/src/source.rs`
  - Verify: wasm test `api/live` resolves for base `/` and `/a/{id}/`
- [ ] **T6 (P1, human: ~2h / CC: ~20min)** — viewer CI — extend `viewer-web-test` with a `<base href>` sub-path relative-fetch resolution test
  - Surfaced by: Test issue 12 — D1's load-bearing browser assumption is otherwise unverified
  - Files: `crates/ara-viewer/tests/web.rs`, `.github/workflows/ci.yml`
  - Verify: headless-Chrome asserts relative `api/manifest` → `/a/x/api/manifest`
- [ ] **T7 (P1, human: ~1h / CC: ~10min)** — serve/mod.rs — extract `serve_cached_manifest(&CachedAra, &HeaderMap)`; share local + hub
  - Surfaced by: Code-quality issue 5 — avoid duplicating the 30-line ETag/304 block
  - Files: `crates/ara-cli/src/serve/mod.rs`
  - Verify: both handlers call it; existing Stage-4 manifest tests still pass
- [ ] **T8 (P1, human: ~1h / CC: ~10min)** — serve/hub.rs — define startup: bad root fatal, empty warns; test each
  - Surfaced by: Code-quality issue 6 — a silently-empty hub is a bad ops signal
  - Files: `crates/ara-cli/src/serve/hub.rs`
  - Verify: missing root → non-zero exit; empty root → start + WARN
- [ ] **T9 (P1, human: ~1h / CC: ~10min)** — serve/mod.rs — clap `ArgGroup` for local/hub mode + `--host`; parse tests
  - Surfaced by: Code-quality issue 7 — untested manual cross-field validation knot
  - Files: `crates/ara-cli/src/serve/mod.rs`
  - Verify: `try_parse_from` cases: hub+root ok, hub-no-root err, both err, neither err; `--host` default `127.0.0.1`
- [ ] **T10 (P1, human: ~30min / CC: ~5min)** — serve/mod.rs — known `/a/{id}` → 308; unknown → 404 (not 308-to-404)
  - Surfaced by: Architecture issue 13a — a 301/redirect-to-404 is permanently browser-cacheable
  - Files: `crates/ara-cli/src/serve/mod.rs`
  - Verify: known id → 308; unknown → 404
- [ ] **T11 (P1, human: ~30min / CC: ~5min)** — serve/hub.rs — test two sequential manifest reads return same etag, no reparse
  - Surfaced by: Test/Perf issue 8 — the milestone's pure-cache-hit property is unverified
  - Files: `crates/ara-cli/src/serve/hub.rs`
  - Verify: `oneshot` two reads → identical etag
- [ ] **T12 (P2, human: ~30min / CC: ~5min)** — serve/hub.rs + cache.rs — drop unused `Arc<Manifest>` on hub path; document memory-vs-N
  - Surfaced by: Performance issue 9 — ~2x resident memory for data the hub never reads
  - Files: `crates/ara-cli/src/serve/hub.rs`, `crates/ara-cli/src/serve/cache.rs`
  - Verify: hub path never references `.manifest`; memory note in plan/docs
- [ ] **T13 (P2, human: ~2h / CC: ~20min)** — serve/mod.rs — wire `--assets` end-to-end in hub (shared `ServeDir` + on-disk index)
  - Surfaced by: Code-quality issue 14 — half-wired embedded/disk split, untested
  - Files: `crates/ara-cli/src/serve/mod.rs`
  - Verify: `--hub --assets dist/` serves shared assets AND base-injected index from disk
- [ ] **T14 (P2, human: ~1h / CC: ~10min)** — docs/deploy.md — proxy owns compression; `manifest.json` fallback inert on hub
  - Surfaced by: Architecture issue 13b — the plan's "fallback stays working" wording is wrong for hub
  - Files: `docs/deploy.md`
  - Verify: docs state the fallback is local/static-host-only

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | not run (optional) |
| Codex Review | `/codex review` | Independent 2nd opinion | 1 | issues_found | outside voice via Claude subagent (Codex not authed) — 5 new findings folded |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR | 14 issues, 0 critical gaps, all folded |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | n/a — no new UI surface (viewer routing only) |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | not run (optional) |

- **CODEX:** Codex CLI installed but not authenticated → outside voice ran via Claude subagent. Surfaced 5 findings beyond the review's 9: the ServeDir-under-`{id}` feasibility gap (issue 11, load-bearing), untested D1 browser assumption (12), routing edge cases (13a/b/c), the `--assets`/`--hub` half-state (14), and the static-export strategic question (10).
- **CROSS-MODEL:** No tension. The outside voice and the review agreed everywhere they overlapped (voice #5→issue 3, #7→issues 6/9, #9→issue 13c). Consensus strengthened issues 3, 6, 9. All 14 accepted; server-vs-static resolved as D3 (keep server).
- **VERDICT:** ENG CLEARED — ready to implement. 14 findings all folded into the plan + 14 build tasks (T1–T14); figures and static-export deferred as TODOs (`T-HUB-FIGURES`, `T-STATIC-EXPORT`).

NO UNRESOLVED DECISIONS
