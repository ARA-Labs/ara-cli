# ARA Viewer Runtime (`ara serve`): Rust Implementation Plan

## TL;DR
- **Build `ara serve` as a Cargo workspace with a shared `ara-core` crate compiled to BOTH native and `wasm32-unknown-unknown`; use Leptos 0.8 (client-side rendering) drawing the node-link DAG as SVG (with a `<canvas>`/`web-sys` fast path if node counts grow), NOT egui/eframe** — egui's web build ships a multi-megabyte wasm blob, has no native browser text selection/search, and has no AccessKit accessibility on the web, all disqualifying for a text-heavy drill-down viewer.
- **Parse YAML with `serde-saphyr`** (a maintained, panic-free, serde-native replacement for the deprecated `serde_yaml`), compute layered DAG layout with a Rust `dagre` port, and serve everything from a single **axum 0.8** binary using `tower-http`'s `ServeDir`/`ServeFile` (range requests + precompressed brotli/gzip) — with `notify` file-watching + WebSocket push for local live reload, and parse-once-at-ingest caching on the hub.
- **Build matrix is tiny:** one `wasm32-unknown-unknown` client artifact + one `x86_64-unknown-linux-musl` server binary, and the *same* Linux binary (shipped as a Docker image) serves both the hub and local use. **Windows is dropped (WSL = Linux); macOS/Darwin native builds are out of scope — local users run Docker.** No cross-compilation required.
- **The one hard external precondition:** node "narrative" text must be precomputed upstream and stored on each node in the YAML. The read/serve/view runtime must never call an LLM at view time; if narratives are absent, the viewer renders the raw structured fields only.

## Key Findings

- **Render stack.** The decision hinges on the drill-down pane, not the graph. The pane must render verbatim quotes, tables, and inline figures, be text-selectable/searchable, and be accessible. A DOM-based Rust framework (Leptos) gives all of this for free at a sub-megabyte wasm cost; egui/eframe gives none of it and costs ~4–9 MB uncompressed wasm. Recommend **Leptos 0.8 CSR** as primary; **egui + `egui_graphs`** only as a fallback.
- **DAG layout.** The exploration tree is a typed DAG that wants a layered (Sugiyama) layout — the `dagre`/ELK family. A pure-Rust `dagre` port exists, so layout runs inside `ara-core` and is shared by native and wasm builds. Layered is the right default for a decision tree; force-directed is available via `egui_graphs` in the fallback path.
- **YAML.** `serde_yaml` is deprecated/unmaintained (final release `0.9.34+deprecated`). Current best for typed, messy-tolerant parsing is **`serde-saphyr`** (serde-native, panic-free, YAML 1.2, DoS budgets). `yaml-rust2` is the low-level AST alternative.
- **Serve layer.** **axum 0.8** is the pragmatic 2026 default (Tokio/Tower ecosystem, `tower-http` static serving with range + precompression). Actix-web is marginally faster but unnecessary for a read-only viewer.
- **Live reload.** `notify` (+ `notify-debouncer-full`) watches the ARA dir; on change, re-parse via `ara-core` and push a WebSocket message so the wasm client re-fetches just the manifest JSON and re-renders — preserving pan/zoom/selection, unlike `tower-livereload`'s whole-page reload.
- **Deployment.** Multi-stage Docker → static musl binary → distroless image (~10–20 MB); systemd unit behind nginx/Caddy; content-hashed wasm served `immutable` + precompressed `.br`/`.gz`, per-ARA JSON served `no-cache`/short-TTL with ETag.

## Details

### A. WASM-native client render path

**Candidates (2026 status).**
- **Leptos** (0.8.x) — fine-grained reactive, DOM-based, `view!` macro, no VDOM, CSR/SSR/hydration/islands. Largest active DOM-framework community; prioritizes small wasm.
- **Dioxus** (0.7) — React-like VDOM, cross-platform, `dx serve` hot-patching. Heavier surface aimed at multi-platform.
- **Yew** (0.21) — mature but VDOM-based and slower; momentum moved to Leptos/Dioxus.
- **egui + eframe** — immediate-mode GUI to `<canvas>` via WebGPU/WebGL; `egui_graphs` gives a ready-made interactive graph widget over `petgraph`.

**Bundle size (decision-critical), uncompressed minimal app:**

| Stack | Minimal-app wasm |
|---|---|
| eframe (wgpu default) | ~8.8 MB |
| eframe (glow backend) | ~4.3 MB |
| Leptos CSR (minimal) | ~69–135 KB |
| Dioxus web hello-world | ~100–275 KB optimized |

eframe's bloat is structural (bundles a GPU/shader-transpile stack via `naga`); switching wgpu→glow roughly halves it but it stays multi-MB. Leptos/Dioxus are an order of magnitude smaller because they use the DOM.

**Text, accessibility, interactivity.** egui on the web renders to canvas, so: no native text selection/search (you can't Ctrl-F an egui page), no web accessibility (AccessKit has no web backend; only an experimental built-in screen reader), and figures/tables/rich text must be re-implemented. For a viewer built around verbatim quotes and evidence tables, that's disqualifying. Leptos (DOM) gives selectable/searchable text, native `<img>` figures, `<table>` rendering, real focus/ARIA, and browser zoom for free.

**Recommended primary stack.**
- **Leptos 0.8 CSR**, built with **Trunk**. CSR (not SSR) is correct: the server is read-only static+JSON, so ship a static `index.html` + wasm and fetch the manifest at runtime. Keeps the server trivial and identical across local/hub.
- **Graph = SVG generated declaratively in Leptos.** Node positions come from `ara-core`'s layered layout; Leptos renders `<g>/<rect>/<path>/<text>` bound to signals. Crisp labels, trivial per-element hit-testing, CSS styling per node type (question/experiment/decision/dead_end/pivot/insight) and dead-end highlighting; pan/zoom via `viewBox`.
- **Canvas fast path (same framework):** for hundreds of nodes where SVG DOM counts hurt, draw to `<canvas>` via `web-sys` `CanvasRenderingContext2d` off a Leptos `NodeRef`. Keep the detail pane as DOM. Put the graph behind a trait so SVG↔canvas swaps cleanly.

**Recommended fallback.** egui + eframe + `egui_graphs` (over `petgraph`), Trunk/`wasm-pack`. Fastest path to an interactive graph *if* the team accepts multi-MB bundles and rendering detail text inside egui (losing native selection/accessibility). Prefer the glow backend for size. Note `egui_graphs` upstream flags itself as not in active development — treat as vendored/forkable.

**Verdict:** primary = Leptos + SVG (canvas fallback for scale); fallback = egui + egui_graphs. Both satisfy "Rust/wasm-native, no JS+D3." Leptos wins on the two things this product is about: readable/selectable/accessible drill-down text, and small download.

### B. Shared core (`ara-core`) — one parser, no drift

```
ara/                       # cargo workspace root
├── Cargo.toml             # [workspace] members
├── crates/
│   ├── ara-core/          # lib: parse + normalize + layout + manifest types
│   │   ├── src/
│   │   │   ├── schema.rs   # serde types for exploration_tree.yaml (both dialects)
│   │   │   ├── manifest.rs # normalized Manifest { nodes[], links[], bindings }
│   │   │   ├── parse.rs    # YAML → schema → Manifest (deterministic)
│   │   │   ├── layout.rs   # layered DAG layout (dagre port) -> node positions
│   │   │   └── lib.rs
│   │   └── Cargo.toml      # std by default; feature-gated for wasm
│   ├── ara-cli/           # bin: `ara` (validate, serve)
│   │   └── src/main.rs
│   └── ara-wasm/          # cdylib: wasm-bindgen bindings for the client
│       └── src/lib.rs
└── client/                # Leptos CSR app (Trunk), depends on ara-core + ara-wasm
    └── src/main.rs
```

- `ara-core` holds **all** parsing, normalization, binding resolution, and layout — the single source of truth linked by both server and client. Manifest types `#[derive(Serialize, Deserialize)]` so the same structs cross the wire as JSON and are reused client-side (no hand-written TS types).
- Must build for native and `wasm32-unknown-unknown`. Keep it `std` but wasm-safe: no threads/filesystem/`SystemTime` in the parse/layout path. Gate native-only helpers behind a `native` feature; the wasm client passes in-memory `&str`/`&[u8]`.
  ```toml
  [features]
  default = ["native"]
  native = ["dep:notify", "std-fs"]
  ```
- **Client build:** **Trunk** drives the Leptos wasm build (`cargo build --target wasm32-unknown-unknown` + `wasm-bindgen`, hashed bundle). Expose any hand-written interop from `ara-wasm` via `#[wasm_bindgen]`; otherwise Leptos + `web-sys` covers DOM/canvas. Let Trunk own the wasm build (`trunk build --release`) — don't hand-roll `wasm-pack` for the whole app.
- Client fetches manifest JSON over HTTP and deserializes with `serde_json` straight into `ara-core` types (simpler than a `serde-wasm-bindgen` JS round-trip). One parser implementation, exercised by CLI `validate`, server, and client — it cannot drift.

### C. YAML parsing

**`serde_yaml` is dead** (archived 2024-03-25; final `0.9.34+deprecated`). Don't start on it.

- **`serde-saphyr` (recommended).** Serde-native on the `saphyr` parser; panic-free on malformed input, `#![forbid(unsafe_code)]` in library code, configurable **Budgets** against DoS, YAML 1.2 (avoids the "Norway problem"), passes the full yaml-test-suite, supports untyped `Value` and multi-doc streams. Early-stage (0.0.x, `#[non_exhaustive]` enums) — pin the version.
- **`yaml-rust2`.** Actively maintained pure-Rust AST parser; no serde integration (hand-write typing). Use only for low-level needs.
- **`serde_yml`/`noyalib`.** Avoid — `serde_yml ≤ 0.0.12` hit RUSTSEC-2025-0068 (segfault/unsoundness), project archived.

**ARA dialect tolerance.** Roots appear as either `tree:` (list) or `root:` (single). Normalize immediately:
```rust
#[derive(Deserialize)]
struct RawDoc {
    #[serde(default)] tree: Option<Vec<Node>>,
    #[serde(default)] root: Option<Node>,
    #[serde(flatten)] extra: BTreeMap<String, serde_saphyr::Value>, // tolerate unknown keys
}
```
- Do **not** use `deny_unknown_fields`; capture unknowns with flattened `extra` at each level and surface them as `ara validate` warnings.
- Deserialize quote/evidence text into owned `String`; never re-emit through a YAML serializer in the read path, so verbatim content is byte-preserved. Block scalars pass through unchanged.
- Normalize both forms into one `Manifest { nodes, links, bindings }` so downstream never sees the dialect.

### D. `ara serve` — the serve layer

**Framework: axum 0.8** (Tokio/Tower/hyper; `tower-http` static serving). Actix is ~10–15% faster on synthetic throughput but adds its own runtime for no benefit on a parse/I/O-bound viewer.

**Endpoints:**
- `GET /`, `/assets/*` — serve the wasm client via `tower_http::services::ServeDir` with `precompressed_brotli()`/`precompressed_gzip()` (serves prebuilt `.wasm.br`/`.gz`; emits `Vary: Accept-Encoding`).
- `GET /api/manifest` — normalized manifest as JSON (`ara-core` output via `serde_json`).
- `GET /api/figure/{path}` — stream a PNG from `evidence/figures/`; `ServeFile`/`ServeDir` support **range requests** automatically. Constrain to the figures dir (reject `..`).
- `GET /api/live` (local only) — WebSocket for live-reload.

**Two modes, one `ara-core` call:**
- **LOCAL:** parse-on-change (see E); optimizes edit→refresh latency.
- **HUB:** parse-once-at-ingest; requests are pure cache reads. Same `ara_core::parse(dir)`, different trigger.

**Cache:**
```rust
struct CachedAra {
    manifest: Arc<Manifest>,
    manifest_json: Arc<Bytes>,   // serialize once
    etag: String,                // source hash → HTTP caching + reload signal
    figures_dir: PathBuf,
}
// Hub:   Arc<RwLock<HashMap<AraId, Arc<CachedAra>>>>   (write on ingest)
// Local: Arc<ArcSwap<CachedAra>>                        (swap on file change)
```
Serialize manifest JSON once; hand out `Arc<Bytes>` clones. Source hash is the `ETag` for conditional GETs; bump on reparse.

### E. Live rendering / live reload

Watch the ARA dir with **`notify`** (inotify/FSEvents/ReadDirectoryChangesW) wrapped in **`notify-debouncer-full`** (~200–500 ms) to collapse editor write bursts. On a debounced change to `trace/exploration_tree.yaml` or `evidence/`: re-run `ara_core::parse`, atomically swap the cache + bump ETag, push over `/api/live`.

**Transport — full reload vs fine-grained:**
- `tower-livereload` = zero-effort but whole-page reload → discards pan/zoom/selection/scroll. Jarring during editing.
- **Recommended: WebSocket data refresh.** On signal, the client re-fetches `/api/manifest` and re-renders from the new manifest, preserving view state where node ids persist. Since the client already deserializes `ara-core` types, "apply new manifest" is a signal update, not a reload. WebSocket over SSE only to allow future client→server messages; SSE is fine if push-only.
- Keep `tower-livereload` behind a `--full-reload` dev flag for debugging the wasm bootstrap.

### F.0 Build & distribution matrix

wasm collapses the client to a single OS-agnostic artifact; the only per-target build is the native server, and the decided scope reduces that to **one Linux binary**.

**Decisions:** Windows dropped (WSL users are Linux, covered by the musl build). **macOS/Darwin native builds out of scope — local users run the Docker image.** No cross-compilation required.

| Component | Target(s) | Notes |
|---|---|---|
| wasm client | `wasm32-unknown-unknown` | One artifact; any browser, any OS. No OS split. |
| Server — hub | `x86_64-unknown-linux-musl` | Fully static; distroless container. |
| Server — local (`ara serve`) | **same musl binary, shipped as Docker image** | `docker run ara serve ./my-ara`. One artifact for hub + laptop. |
| Windows | — | Not built. Users run under WSL (= Linux). |
| macOS | — | No native build. Mac users run the Docker image. |

**Net: exactly one wasm artifact + one Linux musl binary** (same binary for hub and local, distributed as a container). No `cross`, no macOS runner, no `-msvc`. A native `brew`-installable macOS binary, if ever wanted, is an additive `aarch64-apple-darwin` CI job (native `macos` runner, no `cross`) — explicitly deferred.

**FS-watch caveat (docs, not code).** `notify` (inotify) does **not** fire reliably for:
- files on the Windows filesystem mounted into WSL2 (`/mnt/c/...`), and
- bind-mounted volumes on non-Linux Docker hosts.

Guidance: keep the ARA dir inside the Linux/WSL filesystem (`~/…`, ext4). As a safety net, `ara serve` exposes a **`--poll` fallback** (polling watcher) for network mounts / cross-boundary bind mounts. Since local `ara serve` ships as Docker, document `--poll` as the standard flag when bind-mounting an ARA from a non-Linux host.

### F. Deployment on a plain Linux container

**Small static binary.** Multi-stage Docker: build stage compiles the client (Trunk → hashed wasm + precompressed `.br`/`.gz`) and the server for `x86_64-unknown-linux-musl` (static, no glibc); runtime stage is distroless static (CA certs + non-root; prefer over bare `scratch`). Use `cargo-chef` to cache dependency builds. Typical musl images land <10 MB compressed.

```dockerfile
# ---- build ----
FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev && rustup target add wasm32-unknown-unknown \
 && cargo install trunk wasm-bindgen-cli
WORKDIR /src
COPY . .
RUN trunk build --release                                   # -> client/dist (hashed wasm + .br/.gz)
RUN cargo build --release --target x86_64-unknown-linux-musl -p ara-cli
# ---- runtime ----
FROM gcr.io/distroless/static
COPY --from=build /src/target/x86_64-unknown-linux-musl/release/ara /ara
COPY --from=build /src/client/dist /assets
ENTRYPOINT ["/ara", "serve", "--assets", "/assets", "--hub"]
```

**MIME + compression.** Serve `.wasm` as `application/wasm` (Trunk sets this; with nginx add `types { application/wasm wasm; }`). Pre-compress wasm at build (brotli 11 / gzip 9); serve via `ServeDir::precompressed_brotli().precompressed_gzip()`. Brotli typically <50% of uncompressed.

**systemd + reverse proxy.** Run under a systemd unit (restart-on-failure, non-privileged user, `--assets`/`--ara-root` via `Environment=`), fronted by nginx or Caddy for TLS. Caddy auto-TLS is the lighter op choice; nginx if already standardized.

**HTTP caching.**
- wasm/JS/CSS (content-hashed): `Cache-Control: public, max-age=31536000, immutable`.
- `index.html`: `no-cache` (revalidate to pick up new bundle hashes).
- `/api/manifest`: `no-cache` + `ETag`; `304` on `If-None-Match`. On the hub (immutable per version), short `max-age` + ETag is fine.
- `/api/figure/*`: long `max-age` + `ETag`.

### G. Risks, unknowns, and phased plan

**Risks / unknowns:**
1. **YAML dialect variance under-specified** — biggest parser risk. Mitigate with tolerant parsing (flattened `extra`) + a real-ARA corpus snapshot-tested early.
2. **`serde-saphyr` maturity** (0.0.x, `#[non_exhaustive]` churn) — pin exact versions; keep an adapter so swapping to `yaml-rust2` is possible.
3. **Graph scale** — SVG comfortable to a few hundred nodes; canvas fast path beyond. Validate on the largest real tree in Phase 2.
4. **`egui_graphs` inactivity** (fallback only) — treat as vendored/forkable.
5. **DAG layout determinism** — pin the `dagre` port's tie-break/rank options so layout is byte-deterministic (matters for snapshot tests and stable diffs).
6. **External precondition (loud):** node **narrative text must be precomputed upstream** and stored on the node. If absent, the viewer degrades to structured-fields-only — it must NOT call an LLM at view time. Upstream dependency, not runtime work.

**Phased implementation:**

- **Phase 1 — `ara-core` parser + manifest schema + CLI `validate`.** serde types for both dialects; `parse(dir) -> Manifest`; binding resolution; layered layout via `dagre`. `ara validate <dir>` parses, resolves, reports unknown fields/broken refs, non-zero on error. **Tests:** per-dialect unit tests; `insta` snapshot of `Manifest` on a real corpus; parse-twice determinism. *Milestone:* `ara validate` green on a real corpus; manifest schema frozen.
- **Phase 2 — minimal wasm viewer from a static manifest.** Leptos CSR: load a checked-in `manifest.json`; render DAG as SVG with pan/zoom, node-type styling, dead-end highlighting, click→drill-down (verbatim quotes/tables/inline `<img>`). Decide SVG vs canvas on the largest corpus tree. **Tests:** wasm-pack headless browser tests for the graph component. *Milestone:* open `index.html`, navigate a real tree.
- **Phase 3 — `ara serve` local mode + live reload.** axum: `ServeDir` assets, `/api/manifest`, `/api/figure/*` (range). `notify` + debouncer → reparse → WebSocket push → in-place re-render. **Tests:** edit a temp YAML → assert new manifest pushed; figure range-request test. *Milestone:* edit YAML, browser updates without losing selection/zoom.
- **Phase 4 — hub deployment + caching.** Parse-once-at-ingest cache; musl Docker → distroless; systemd + reverse proxy; immutable-bundle vs no-cache-manifest headers; precompressed wasm. **Tests:** container smoke test (health, wasm MIME/compression, manifest ETag/304); hub reads are pure cache hits. *Milestone:* `docker run` behind nginx/Caddy; ARA renders over TLS.

## Recommendations

1. **Adopt the primary stack:** workspace with `ara-core` (shared), `ara-cli`, `ara-wasm`, Leptos 0.8 CSR `client`; axum 0.8 + tower-http; `serde-saphyr`; `dagre` layout; `notify` + WebSocket live reload.
2. **Build Phase 1 before the browser.** Freeze the manifest schema against a real corpus first. **Proceed when:** `ara validate` green + snapshots stable.
3. **Pick SVG vs canvas empirically in Phase 2** on your largest real tree. **Switch to canvas if:** SVG pan/zoom drops below ~30 fps or DOM element count exceeds a few thousand.
4. **Default live reload to fine-grained manifest refresh**, not full-page reload; keep `tower-livereload` behind `--full-reload`.
5. **Treat egui/eframe as a documented fallback only** — revisit only if text selection/search/accessibility/size are deprioritized.
6. **Register the narrative-text precondition** as an upstream ticket; implement graceful degradation (structured-fields-only) in Phase 2 so the viewer is never blocked on it.
7. **Keep the build matrix minimal:** one wasm target, one Linux musl binary for hub + local (via Docker), Windows via WSL, macOS via Docker. Pin `serde-saphyr` exactly; vendor `egui_graphs` if the fallback is ever built.

## Caveats
- eframe bundle-size figures (~4.3 MB glow / ~8.8 MB wgpu) are informal, uncompressed, near-empty-app measurements — order-of-magnitude, not exact. Leptos/Dioxus minimal figures (~70–275 KB) vary with `opt-level="z"`, LTO, `wasm-opt`.
- Version currency: Leptos 0.8.x, axum 0.8.x, egui 0.35, tower-http, `notify` 6.x/debouncer, `serde-saphyr` 0.0.x were current mid-2026 — confirm latest patches at implementation, especially fast-moving `serde-saphyr`.
- The `dagre` Rust port is a community crate; validate layout on real trees and pin options for determinism. If inadequate, an ELK-style layered layout may need wrapping.
- Scope is the read/serve/view path only. Narrative precomputation and code-execution/reproduction are out of scope and assumed handled upstream.
- axum vs actix throughput comparisons are synthetic and unrepresentative of this parse/I/O-bound workload; framework choice is driven by ecosystem fit, not throughput.