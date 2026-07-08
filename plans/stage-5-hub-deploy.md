# Stage 5 — Hub Deployment + Caching (Docker)

**PR target:** `stage5-hub-deploy` → `main`. **Depends on:** Stage 4 (and the
`0.1.0` release cut that follows it). **Version bump:** `0.1.0 → 0.1.1` — hub
mode / Docker is deployment/ops, not new public API, so it ships as a patch on
top of the `0.1.0` release.

## Problem background

Local serving works (Stage 4). The hub serves many ARAs read-only, where
requests should be pure cache reads (parse-once-at-ingest), and the whole thing
must ship as one small static image. The build matrix is intentionally tiny: one
`wasm32` client artifact + one `x86_64-unknown-linux-musl` server binary, the
same binary for hub and local (distributed as a Docker image). Windows → WSL,
macOS → Docker; no cross-compilation.

## Proposed solution

Add a `--hub` mode with a parse-once-at-ingest cache keyed by ARA id, a
multi-stage Docker build (Trunk client + musl server → distroless static), and
correct HTTP caching headers (immutable hashed bundles vs no-cache manifest).

## Implementation steps

1. **Hub cache:** `Arc<RwLock<HashMap<AraId, Arc<CachedAra>>>>`, written on
   ingest, read on request. Reuse `CachedAra` from Stage 4; only the trigger
   differs (ingest vs file-watch). `--hub --ara-root <dir>` scans/ingests ARAs.
2. **Routing:** per-ARA paths (e.g. `/a/{ara_id}/api/manifest`,
   `/a/{ara_id}/api/figure/*`); shared immutable client assets at `/assets/*`.
   Live-reload (`/api/live`) is **local-only**; disabled in hub mode.
3. **HTTP caching headers:**
   - content-hashed wasm/js/css → `Cache-Control: public, max-age=31536000, immutable`.
   - `index.html` → `no-cache`.
   - `/api/manifest` → `ETag` + short `max-age` (immutable per version on hub);
     `304` on `If-None-Match`.
   - `/api/figure/*` → long `max-age` + `ETag`.
4. **Precompressed assets:** Trunk build emits hashed wasm; pre-compress
   brotli-11 / gzip-9 at build; `ServeDir::precompressed_brotli().precompressed_gzip()`.
5. **Dockerfile (multi-stage):** build stage `rust:1-alpine` + `musl-dev` +
   `wasm32` target + `trunk`/`wasm-bindgen-cli` → `trunk build --release` then
   `cargo build --release --target x86_64-unknown-linux-musl -p ara-cli`; runtime
   stage `gcr.io/distroless/static` (CA certs, non-root). Use `cargo-chef` to
   cache dependency builds. Target image <20 MB.
6. **Ops docs (`docs/deploy.md`):** systemd unit (restart-on-failure, non-root,
   `--assets`/`--ara-root` via `Environment=`), nginx/Caddy TLS front (Caddy
   auto-TLS recommended), and the `--poll` guidance for non-Linux bind mounts.
7. **CI:** add a Docker build job; optionally push to a registry on tags.

## Tests / verification

- Container smoke test (CI): `docker run` the image, hit health/asset endpoints;
  assert `.wasm` served as `application/wasm`, brotli negotiated, `index.html`
  `no-cache`, an asset `immutable`.
- Manifest `ETag`/`304` conditional-GET test against the running container.
- Hub read path: after ingest, requests do not re-parse (assert via a parse
  counter/metric or log absence).
- Image size check: fail the job if the compressed image exceeds the budget.

## Milestone / acceptance

`docker run` behind nginx/Caddy serves an ARA over TLS; hub reads are pure cache
hits; image is small and static. Publish `0.1.1` (`ara-core` → `ara-wasm` →
`ara-cli`, in dependency order) and tag `v0.1.1`. (`0.1.0` was already cut after
Stage 4.)

## Out of scope (deferred)

Ingest pipeline / upload API for new ARAs (assumed upstream), auth, horizontal
scaling, a native `aarch64-apple-darwin` brew build (additive CI job, deferred).

## CHANGELOG (Unreleased → Added)

- `ara serve --hub` parse-once-at-ingest cache; multi-stage musl→distroless
  Docker image; content-hashed immutable bundles + no-cache manifest headers;
  deployment docs (systemd + reverse proxy).
