# Stage 5 — Hub deployment (`ara serve --hub`) + Docker

Design record + ops guide for the read-only multi-ARA **hub** shipped in Stage 5.
Where [`ara serve <dir>`](stage-4-serve.md) watches **one** artifact and live-
reloads on edits, the hub serves **many** ARAs read-only under `/a/{id}/`, parses
each once at startup, and ships as one small static Docker image.

Companion docs: [`stage-4-serve.md`](stage-4-serve.md) (the local server this
extends) and [`manifest-schema.md`](manifest-schema.md) (the wire shape).

## Two modes, one binary

`ara serve` is now mode-selected at parse time by a clap `ArgGroup` (exactly one
of the two is required):

```bash
ara serve ./my-ara                       # LOCAL: single ARA, file-watch live reload
ara serve --hub --ara-root /aras         # HUB:   many ARAs, read-only, parse-once
```

| | Local (`<dir>`) | Hub (`--hub --ara-root`) |
| --- | --- | --- |
| ARAs | one | many (each immediate subdir of the root) |
| Parse | once, **reparse on file change** | once **at startup**, never again |
| Watcher / live reload | yes (`/api/live` WebSocket) | **no** — reads are pure cache hits |
| Routing | `/api/manifest`, `/` | `/a/{id}/api/manifest`, `/a/{id}/`, `/` index |
| Concurrency model | `ArcSwap<CachedAra>` (hot-swappable) | immutable `Arc<HashMap>` (lock-free) |

Common flags: `--port <n>` (default `8080`), `--host <ip>` (default `127.0.0.1`;
set `0.0.0.0` in a container), `--assets <dir>` (serve the viewer from an on-disk
`dist/` instead of the embedded copy). `--poll` is **local-only** (the hub has no
watcher).

## Hub routing (path-based `/a/{id}/`)

The hub is addressed by path, not host/subdomain — no wildcard DNS or per-ARA
TLS. Each ARA `id` is the child directory name, constrained to `[A-Za-z0-9._-]+`
at ingest (see below).

```
GET /                       minimal HTML index of available ARA ids
GET /a/{id}                 308 -> /a/{id}/  if id known; else 404
GET /a/{id}/                viewer index.html with <base href="/a/{id}/"> injected, no-cache
GET /a/{id}/api/manifest    that ARA's cached manifest (ETag + 304); 404 if id unknown
GET /{asset}                shared js/wasm/css if the file exists; else 404 (no SPA fallback)
```

**How one viewer bundle serves every ARA.** The viewer fetches its manifest/live
URLs **relative** to the document base (`api/manifest`, `api/live`). The hub
injects `<base href="/a/{id}/">` into each per-ARA `index.html`, so the same
relative `api/manifest` resolves to `/a/{id}/api/manifest`. Trunk's fingerprinted
bundle URLs (`/ara-viewer-{hash}.js`, `/styles-{hash}.css`) are **root-absolute**,
so they ignore `<base>` and load once from the shared root path — the same
immutable bytes for all ARAs. Under local `ara serve` the page is at `/`, so the
same relative URLs resolve to `/api/manifest` (unchanged behaviour).

**`manifest.json` static fallback is inert on the hub.** The viewer keeps a
relative static fallback (`manifest.json`) for plain static hosts (`trunk serve`,
GitHub Pages). On the hub the primary `api/manifest` always resolves, so the
fallback never fires — and were it to, `manifest.json` under `<base href="/a/{id}/">`
resolves to `/a/{id}/manifest.json`, a route the hub does **not** serve. This is
harmless (a local/static-host path only), not a live static file on the hub.

### Ingest at startup

`--ara-root` is scanned once: each immediate subdirectory is parsed into the same
`CachedAra` the local server uses (positioned manifest JSON + content-hash ETag),
minus the parsed graph, which the hub never reads (dropped to save ~2× resident
memory per ARA). Failures are **logged and skipped**, never fatal:

- A subdir that fails to parse, or whose name is not a valid id (spaces,
  non-ASCII, `/`, `..`), is skipped with a logged reason.
- An **unreadable `--ara-root`** (missing / not a directory) is **fatal** — the
  process exits non-zero, mirroring the local server's fast-fail on a broken
  artifact.
- An **empty root** (or one where every child failed) starts but logs a loud
  `WARNING: hub ingested 0 ARAs …` — a silently-empty hub behind a load balancer
  reads as "up" while serving nothing, so the warning is the ops signal.

Startup prints an ingest summary: `hub: N ARA(s) ingested, M skipped from …`.

Memory scales ~linearly with corpus size × per-ARA serialized manifest size.
Ingest is serial (one-time, off the request path); parallel ingest is deferred
until corpus size warrants it.

## Docker

The image is a multi-stage **musl → distroless** build (see the repo `Dockerfile`).
The viewer bundle is committed and baked into the binary via `include_dir!`, so
the builder is a plain `cargo build --target x86_64-unknown-linux-musl` — there is
**no wasm toolchain in the image** (the embed regen happens on the dev machine and
is enforced by the `viewer-embed-fresh` CI gate). `cargo-chef` caches the
dependency compile as its own layer.

```bash
docker build -t ara-hub .
docker run --rm -p 8080:8080 -v /path/to/aras:/aras:ro ara-hub
# the image's default CMD is: serve --hub --ara-root /aras --host 0.0.0.0 --port 8080
```

The runtime stage is `gcr.io/distroless/static-debian12:nonroot`: no shell, no
libc, runs as a nonroot user. Target compressed image size is **< 20 MB** (static
binary + distroless; the wasm is a few hundred KB inside the binary).

### `compose.yaml`

```yaml
services:
  ara-hub:
    build: .
    ports:
      - "8080:8080"
    volumes:
      - ./aras:/aras:ro
    # The image already binds 0.0.0.0 via its default CMD; override here only to
    # change the root or port.
    restart: unless-stopped
```

## Reverse proxy (TLS + compression)

**The reverse proxy owns compression.** Under the embedded-only image (D2) the
hub serves assets uncompressed on the wire — `embedded_handler` does no
brotli/gzip content negotiation. Front the hub with a proxy that terminates TLS
**and** compresses responses (the wasm bundle especially).

### Caddy (automatic TLS, recommended)

```caddy
ara.example.com {
    encode zstd gzip
    reverse_proxy 127.0.0.1:8080
}
```

### nginx

```nginx
server {
    listen 443 ssl;
    server_name ara.example.com;
    ssl_certificate     /etc/letsencrypt/live/ara.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/ara.example.com/privkey.pem;

    gzip on;
    gzip_types application/wasm application/javascript text/css application/json;

    location / {
        proxy_pass http://127.0.0.1:8080;
    }
}
```

Caching is already right at the origin and needs no proxy tuning: fingerprinted
js/wasm/css are `public, max-age=31536000, immutable`; per-ARA `index.html` and
`api/manifest` are `no-cache` + a strong `ETag`, so a conditional GET is a cheap
`304`.

## systemd (bare-metal binary)

For a non-Docker deploy, run the static binary under systemd as a non-root user:

```ini
# /etc/systemd/system/ara-hub.service
[Unit]
Description=ARA hub
After=network.target

[Service]
User=ara
Environment=ARA_ROOT=/srv/aras
ExecStart=/usr/local/bin/ara serve --hub --ara-root ${ARA_ROOT} --host 127.0.0.1 --port 8080
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Bind `127.0.0.1` and let the reverse proxy (above) face the internet.

## Serving artifact figures

Hub figures use `/a/{id}/api/figure/{*path}` with the same guarded PNG/JPEG handler
as local serving. Keep referenced files beneath each artifact's `evidence/`
directory in the read-only mount. The viewer derives image URLs from the loaded
manifest, so the per-artifact document base does not send images to another ARA.
See [figure-exhibit-images.md](figure-exhibit-images.md) for authoring, allowed
symlinks, and manual static deployment without a running API.

## Out of scope (deferred)

- **Static-export mode** (`ara build <root>` → per-ARA `manifest.json` served by a
  plain file host/CDN) — kept as a post-`0.1.3` scaling play; Stage 5 keeps a
  running server so there is one binary and one `/api` contract, local and hub.
- **Ingest/upload API**, parallel ingest, auth, horizontal scaling, registry-push
  secrets.
