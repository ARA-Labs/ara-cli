//! `ara serve <dir>` — local axum server with file-watch live reload.
//!
//! Serves the viewer (embedded or `--assets`), the parsed manifest as JSON
//! (`/api/manifest`, ETag/304), range-capable figures (`/api/figure/*`), and a
//! WebSocket (`/api/live`) that pushes the new ETag on every reparse so the
//! client re-fetches and re-renders in place.

mod assets;
mod cache;
mod hub;
mod watch;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::sync::{broadcast, mpsc};
use tower_http::services::{ServeDir, ServeFile};

use assets::{
    Assets, embedded_asset_strict, embedded_handler, embedded_index_html, embedded_is_populated,
};
use cache::CachedAra;
use hub::Aras;

/// `ara serve` command-line arguments.
///
/// Two mutually-exclusive modes, enforced by clap at parse time via the `mode`
/// [`ArgGroup`] so a bad combination fails with a clear message instead of a
/// hand-rolled runtime check:
/// - **local** (default): a positional `dir` → single-ARA watch + live reload.
/// - **hub**: `--hub --ara-root <dir>` → read-only multi-ARA under `/a/{id}/`.
#[derive(clap::Args)]
#[command(group(
    clap::ArgGroup::new("mode")
        .required(true)
        .args(["dir", "hub"])
))]
pub struct ServeArgs {
    /// Path to a single ARA artifact directory (local mode; conflicts with `--hub`).
    #[arg(conflicts_with = "hub")]
    dir: Option<PathBuf>,
    /// Serve many ARAs read-only from `--ara-root` under `/a/{id}/` (hub mode).
    #[arg(long, requires = "ara_root")]
    hub: bool,
    /// Root directory whose immediate subdirectories are each an ARA (hub mode).
    #[arg(long)]
    ara_root: Option<PathBuf>,
    /// Port to bind on.
    #[arg(long, default_value_t = 8080)]
    port: u16,
    /// Bind address. Defaults to loopback for local safety; set `0.0.0.0` in a
    /// container so the port is reachable from the host.
    #[arg(long, default_value = "127.0.0.1")]
    host: std::net::IpAddr,
    /// Serve viewer assets from this on-disk `dist/` instead of the embedded copy.
    #[arg(long)]
    assets: Option<PathBuf>,
    /// Use the polling watcher (local mode only; hub has no watcher).
    #[arg(long)]
    poll: bool,
}

/// Shared server state: the swappable cache + the live-reload broadcast channel.
#[derive(Clone)]
pub struct AppState {
    cache: Arc<ArcSwap<CachedAra>>,
    live_tx: broadcast::Sender<String>,
}

/// Entry point for `ara serve`. Branches on mode, builds the cache(s), then runs
/// the server on a fresh multi-thread Tokio runtime until Ctrl-C.
pub fn run(args: ServeArgs) -> ExitCode {
    if args.assets.is_none() && !embedded_is_populated() {
        eprintln!(
            "warning: no viewer assets are embedded in this binary; pass --assets <dist> \
             (was this built without `trunk build`?)"
        );
    }

    // Clap's `mode` ArgGroup guarantees exactly one of `--hub` / positional
    // `dir` is set, so these branches are exhaustive.
    if args.hub {
        run_hub(args)
    } else {
        run_local(args)
    }
}

/// Local mode: parse one ARA up front (fail fast on a broken artifact), then
/// serve with the file-watch live-reload path.
fn run_local(args: ServeArgs) -> ExitCode {
    let dir = args
        .dir
        .clone()
        .expect("clap ArgGroup guarantees `dir` in local mode");
    let initial = match CachedAra::from_dir(&dir) {
        Ok(c) => c,
        Err(report) => {
            for diagnostic in report.errors() {
                eprintln!("{diagnostic}");
            }
            eprintln!(
                "{}: cannot serve — {} parse error(s)",
                dir.display(),
                report.errors().len()
            );
            return ExitCode::FAILURE;
        }
    };

    with_runtime(|| serve_local(args, dir, initial))
}

/// Hub mode: ingest `--ara-root` once at startup, then serve read-only.
fn run_hub(args: ServeArgs) -> ExitCode {
    let root = args
        .ara_root
        .clone()
        .expect("clap `requires` guarantees --ara-root with --hub");

    // An unreadable root is a fatal misconfiguration → non-zero exit, matching
    // local mode's fast-fail on a broken artifact.
    let ingested = match hub::ingest(&root) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error: cannot read --ara-root {}: {e}", root.display());
            return ExitCode::FAILURE;
        }
    };

    for skip in &ingested.skipped {
        eprintln!("skipped ARA {:?}: {}", skip.name, skip.reason);
    }
    let n = ingested.aras.len();
    let m = ingested.skipped.len();
    // A silently-empty hub behind a load balancer reads as "up" while serving
    // nothing — make that an unmissable ops signal.
    if n == 0 {
        eprintln!(
            "WARNING: hub ingested 0 ARAs from {} ({m} skipped) — the hub will serve nothing",
            root.display()
        );
    } else {
        println!(
            "hub: {n} ARA(s) ingested, {m} skipped from {}",
            root.display()
        );
    }

    with_runtime(|| serve_hub(args, ingested.aras))
}

/// Build a multi-thread Tokio runtime and drive `fut` to completion, mapping the
/// server result to an exit code. Shared by both modes.
fn with_runtime<F, Fut>(make_fut: F) -> ExitCode
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: failed to start async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(make_fut()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Wire the local router, spawn the watcher, and serve until shutdown.
async fn serve_local(
    args: ServeArgs,
    dir: PathBuf,
    initial: CachedAra,
) -> Result<(), Box<dyn std::error::Error>> {
    let figures_dir = initial.figures_dir.clone();
    let cache = Arc::new(ArcSwap::from_pointee(initial));
    let (live_tx, _) = broadcast::channel::<String>(16);
    let state = AppState {
        cache: cache.clone(),
        live_tx: live_tx.clone(),
    };

    let assets = match &args.assets {
        Some(dir) => Assets::Dir(dir.clone()),
        None => Assets::Embedded,
    };
    let app = build_router(state.clone(), assets, figures_dir);

    // File watcher → debounced reparse → cache swap → broadcast new ETag.
    let (watch_tx, mut watch_rx) = mpsc::unbounded_channel();
    watch::spawn(&dir, args.poll, watch_tx)?;
    let reparse_state = state.clone();
    let watch_dir = dir.clone();
    tokio::spawn(async move {
        while watch_rx.recv().await.is_some() {
            reparse_and_swap(&reparse_state, &watch_dir);
        }
    });

    let addr = std::net::SocketAddr::new(args.host, args.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("ara serve: http://{addr}  (watching {})", dir.display());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Wire the hub router and serve read-only until shutdown. No watcher: the hub
/// parses once at ingest and never reparses.
async fn serve_hub(args: ServeArgs, aras: Aras) -> Result<(), Box<dyn std::error::Error>> {
    let assets = match &args.assets {
        Some(dir) => Assets::Dir(dir.clone()),
        None => Assets::Embedded,
    };
    let app = build_hub_router(aras, assets);

    let addr = std::net::SocketAddr::new(args.host, args.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("ara serve --hub: http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Build the route table. Split out so tests can drive it via `oneshot`.
pub fn build_router(state: AppState, assets: Assets, figures_dir: PathBuf) -> Router {
    let figures_dir = Arc::new(figures_dir);
    let router = Router::new()
        .route("/api/manifest", get(manifest))
        .route("/api/live", get(live_ws))
        .route("/api/figure", get(|| async { StatusCode::NOT_FOUND }))
        .route("/api/figure/", get(|| async { StatusCode::NOT_FOUND }))
        .route(
            "/api/figure/{*path}",
            get(
                move |AxumPath(path): AxumPath<String>, request: axum::extract::Request| {
                    let root = figures_dir.clone();
                    async move { serve_figure(&root, &path, request).await }
                },
            ),
        );

    let router = match assets {
        Assets::Dir(dir) => {
            let index = dir.join("index.html");
            let service = ServeDir::new(&dir)
                .precompressed_br()
                .precompressed_gzip()
                .fallback(ServeFile::new(index));
            router.fallback_service(service)
        }
        Assets::Embedded => router.fallback(embedded_handler),
    };

    router.with_state(state)
}

/// Shared state for the hub router: the immutable ARA table + how to serve the
/// shared viewer assets (embedded or on-disk `dist/`).
#[derive(Clone)]
struct HubState {
    aras: Aras,
    assets: Assets,
}

/// Build the hub route table. Split out so tests can drive it via `oneshot`.
///
/// ```text
/// GET /a/{id}              308 -> /a/{id}/ if id known; else 404
/// GET /a/{id}/             index.html with <base href="/a/{id}/"> injected
/// GET /a/{id}/api/manifest cached manifest (ETag/304); 404 if id unknown
/// GET /a/{id}/api/figure/* guarded evidence images, range/conditional aware
/// GET /                    minimal HTML index of available ARA ids
/// GET /{*asset}            shared js/wasm/css if the file exists; else 404
/// ```
///
/// There is no `/api/live` and no watcher — live reload is local-only; the
/// viewer's live socket simply never opens on the hub (it degrades to inert).
pub fn build_hub_router(aras: Aras, assets: Assets) -> Router {
    let state = HubState { aras, assets };
    Router::new()
        .route("/", get(hub_index))
        .route("/a/{id}", get(hub_ara_redirect))
        .route("/a/{id}/", get(hub_ara_index))
        .route("/a/{id}/api/manifest", get(hub_manifest))
        .route(
            "/a/{id}/api/figure",
            get(|| async { StatusCode::NOT_FOUND }),
        )
        .route(
            "/a/{id}/api/figure/",
            get(|| async { StatusCode::NOT_FOUND }),
        )
        .route("/a/{id}/api/figure/{*path}", get(hub_figure))
        // Any other root path is a shared asset lookup (or 404) — NOT an SPA
        // fallback. Placed last so the specific routes above win.
        .fallback(get(hub_asset))
        .with_state(state)
}

/// Paths extracted by axum are decoded once. Preserve the original request
/// headers/method for the file service's range and conditional handling.
async fn serve_figure(root: &Path, path: &str, request: axum::extract::Request) -> Response {
    use tower::ServiceExt;
    let (file, mime) = match ara_core::figure::resolve_image(root, path) {
        Ok(image) => image,
        Err(_) => return (StatusCode::NOT_FOUND, "image not found").into_response(),
    };
    let mut response = match ServeFile::new(file).oneshot(request).await {
        Ok(response) => response.into_response(),
        Err(error) => {
            eprintln!("figure file service failed: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, mime.parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
}

async fn hub_figure(
    State(state): State<HubState>,
    AxumPath((id, path)): AxumPath<(String, String)>,
    request: axum::extract::Request,
) -> Response {
    let Some(cached) = state.aras.get(&id) else {
        return (StatusCode::NOT_FOUND, "unknown ARA").into_response();
    };
    serve_figure(&cached.figures_dir, &path, request).await
}

/// `GET /` — a minimal listing of available ARA ids.
async fn hub_index(State(state): State<HubState>) -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        hub::render_hub_index(&state.aras),
    )
        .into_response()
}

/// `GET /a/{id}` (no trailing slash) — `308` to `/a/{id}/` if the id is known,
/// else `404`. A 308 (not 301) is not permanently browser-cached, and gating on
/// a known id means we never cache a redirect that lands on a 404.
async fn hub_ara_redirect(
    State(state): State<HubState>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    if state.aras.contains_key(&id) {
        (
            StatusCode::PERMANENT_REDIRECT,
            [(header::LOCATION, format!("/a/{id}/"))],
        )
            .into_response()
    } else {
        (StatusCode::NOT_FOUND, "unknown ARA").into_response()
    }
}

/// `GET /a/{id}/` — the viewer index with a per-ARA `<base href="/a/{id}/">`
/// injected, `no-cache`. Unknown id → 404. A template with no `<head>` to
/// splice into → 500 (never a silent base-less page, which would break every
/// relative API URL and render nothing).
async fn hub_ara_index(State(state): State<HubState>, AxumPath(id): AxumPath<String>) -> Response {
    if !state.aras.contains_key(&id) {
        return (StatusCode::NOT_FOUND, "unknown ARA").into_response();
    }

    let template = match hub_index_template(&state.assets) {
        Some(t) => t,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "viewer index.html is not available",
            )
                .into_response();
        }
    };

    match hub::inject_base_href(&template, &id) {
        Some(html) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            html,
        )
            .into_response(),
        None => {
            eprintln!("error: viewer index.html has no <head> to inject <base> into");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "viewer index.html has no <head> to inject a base href into",
            )
                .into_response()
        }
    }
}

/// Read the raw `index.html` template for either asset source.
fn hub_index_template(assets: &Assets) -> Option<String> {
    match assets {
        Assets::Embedded => embedded_index_html().map(str::to_owned),
        Assets::Dir(dir) => std::fs::read_to_string(dir.join("index.html")).ok(),
    }
}

/// `GET /a/{id}/api/manifest` — the per-ARA cached manifest, or `404` if the id
/// is unknown. Uses the shared conditional-GET core.
async fn hub_manifest(
    State(state): State<HubState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    match state.aras.get(&id) {
        Some(cached) => serve_cached_manifest(cached, &headers),
        None => (StatusCode::NOT_FOUND, "unknown ARA").into_response(),
    }
}

/// Fallback: serve a shared root asset if it exists, else `404`. No SPA
/// index-fallback — the hub has no root client-side router.
async fn hub_asset(State(state): State<HubState>, uri: Uri) -> Response {
    match &state.assets {
        Assets::Embedded => embedded_asset_strict(&uri),
        Assets::Dir(dir) => serve_dir_file(dir, &uri).await,
    }
}

/// Serve one file from an on-disk `dist/` by exact path (no fallback). Mirrors
/// `embedded_asset_strict` for the `--assets` hub path so the two asset sources
/// behave identically (dev parity).
async fn serve_dir_file(dir: &std::path::Path, uri: &Uri) -> Response {
    use axum::body::Body;
    use tower::ServiceExt;

    let req = axum::http::Request::builder()
        .uri(uri.clone())
        .body(Body::empty())
        .expect("request from a valid uri");
    // ServeDir with no fallback → 404 for unknown paths, rejects `..` traversal,
    // supports range + precompressed variants (matching local `--assets`).
    // Its Service error is Infallible (IO errors are turned into responses
    // internally), so `oneshot` never yields Err.
    let Ok(resp) = ServeDir::new(dir)
        .precompressed_br()
        .precompressed_gzip()
        .oneshot(req)
        .await;
    resp.into_response()
}

/// `GET /api/manifest` (local) — delegates to the shared conditional-GET core.
async fn manifest(State(state): State<AppState>, headers: HeaderMap) -> Response {
    serve_cached_manifest(&state.cache.load(), &headers)
}

/// The manifest conditional-GET contract, shared by local + hub handlers.
///
/// Returns the cached JSON body with a strong `ETag` and `no-cache`, or `304`
/// when `If-None-Match` matches. One source of truth so the local handler and
/// each per-ARA hub handler cannot drift.
fn serve_cached_manifest(cached: &CachedAra, headers: &HeaderMap) -> Response {
    if let Some(inm) = headers.get(header::IF_NONE_MATCH)
        && inm
            .to_str()
            .map(|v| v.contains(&cached.etag))
            .unwrap_or(false)
    {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, cached.etag.clone()),
                (header::CACHE_CONTROL, "no-cache".to_string()),
            ],
        )
            .into_response();
    }

    let body = (*cached.manifest_json).clone();
    (
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (header::ETAG, cached.etag.clone()),
            (header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        body,
    )
        .into_response()
}

/// `GET /api/live` — WebSocket that emits the new ETag on every reparse.
async fn live_ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    let rx = state.live_tx.subscribe();
    ws.on_upgrade(move |socket| live_socket(socket, rx))
}

/// Forward broadcast ETags to one connected client until either side closes.
async fn live_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<String>) {
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(etag) => {
                    if socket.send(Message::Text(etag.into())).await.is_err() {
                        break;
                    }
                }
                // Slow client fell behind: skip missed etags, keep the socket.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(_)) => {}   // ignore anything the client sends
                _ => break,          // client closed or errored
            },
        }
    }
}

/// Reparse `dir`; on success swap the cache and broadcast the new ETag. On parse
/// failure keep the old cache (so a mid-edit broken file doesn't blank the view).
fn reparse_and_swap(state: &AppState, dir: &Path) -> bool {
    match CachedAra::from_dir(dir) {
        Ok(new) => {
            let etag = new.etag.clone();
            state.cache.store(Arc::new(new));
            // Errors here just mean no live subscribers; that is fine.
            let _ = state.live_tx.send(etag);
            true
        }
        Err(report) => {
            eprintln!(
                "reparse skipped — {} parse error(s); keeping last good manifest",
                report.errors().len()
            );
            false
        }
    }
}

/// Resolve when the process receives Ctrl-C.
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tower::ServiceExt; // oneshot

    use axum::body::Body;
    use axum::http::Request;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ara-core/tests/fixtures/official")
            .join(name)
    }

    fn test_state(dir: &Path) -> (AppState, String) {
        let cached = CachedAra::from_dir(dir).expect("fixture must parse");
        let etag = cached.etag.clone();
        let (live_tx, _) = broadcast::channel(16);
        let state = AppState {
            cache: Arc::new(ArcSwap::from_pointee(cached)),
            live_tx,
        };
        (state, etag)
    }

    fn router(dir: &Path) -> (Router, String) {
        let (state, etag) = test_state(dir);
        let figures = dir.join("evidence");
        (build_router(state, Assets::Embedded, figures), etag)
    }

    #[tokio::test]
    async fn manifest_returns_json_with_etag() {
        let (app, etag) = router(&fixture("resnet-ara-example"));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/manifest")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(resp.headers().get(header::ETAG).unwrap(), etag.as_str());

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(value.get("nodes").is_some());
    }

    #[tokio::test]
    async fn manifest_304_on_matching_if_none_match() {
        let (app, etag) = router(&fixture("resnet-ara-example"));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/manifest")
                    .header(header::IF_NONE_MATCH, &etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert!(body.is_empty(), "304 must carry no body");
    }

    #[tokio::test]
    async fn embedded_fallback_serves_index_html() {
        let (app, _) = router(&fixture("resnet-ara-example"));
        let resp = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get(header::CONTENT_TYPE).unwrap();
        assert!(ct.to_str().unwrap().starts_with("text/html"));
    }

    const PNG: &[u8] = include_bytes!("../../../ara-core/tests/fixtures/images/pixel.png");
    const JPEG: &[u8] = include_bytes!("../../../ara-core/tests/fixtures/images/pixel.jpg");

    fn image_router(root: &Path, hub_mode: bool, assets: Assets) -> Router {
        let artifact = fixture("minimal-artifact");
        if hub_mode {
            let mut cached = CachedAra::from_dir_lean(&artifact).unwrap();
            cached.figures_dir = root.to_path_buf();
            let map = std::collections::HashMap::from([("demo".into(), Arc::new(cached))]);
            build_hub_router(Arc::new(map), assets)
        } else {
            let (state, _) = test_state(&artifact);
            build_router(state, assets, root.to_path_buf())
        }
    }

    #[tokio::test]
    async fn local_and_hub_figures_stream_real_rasters_with_ranges_and_conditionals() {
        let evidence = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(evidence.path().join("figures")).unwrap();
        std::fs::write(evidence.path().join("figures/雪 % # ?.PNG"), PNG).unwrap();
        std::fs::write(evidence.path().join("figures/pixel.JPEG"), JPEG).unwrap();
        let assets = tempfile::TempDir::new().unwrap();
        std::fs::write(
            assets.path().join("index.html"),
            "<html><head></head><body>viewer</body></html>",
        )
        .unwrap();
        for hub_mode in [false, true] {
            for asset_mode in [Assets::Embedded, Assets::Dir(assets.path().into())] {
                let app = image_router(evidence.path(), hub_mode, asset_mode);
                let prefix = if hub_mode {
                    "/a/demo/api/figure"
                } else {
                    "/api/figure"
                };
                for (name, mime, bytes) in [
                    ("%E9%9B%AA%20%25%20%23%20%3F.PNG", "image/png", PNG),
                    ("pixel.JPEG", "image/jpeg", JPEG),
                ] {
                    let uri = format!("{prefix}/figures/{name}");
                    let response = get(&app, &uri).await;
                    assert_eq!(response.status(), StatusCode::OK);
                    assert_eq!(response.headers()[header::CONTENT_TYPE], mime);
                    assert_eq!(
                        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
                        "nosniff"
                    );
                    let modified = response.headers()[header::LAST_MODIFIED].clone();
                    assert_eq!(
                        &response.into_body().collect().await.unwrap().to_bytes()[..],
                        bytes
                    );
                    let partial = app
                        .clone()
                        .oneshot(
                            Request::builder()
                                .uri(&uri)
                                .header(header::RANGE, "bytes=0-7")
                                .body(Body::empty())
                                .unwrap(),
                        )
                        .await
                        .unwrap();
                    assert_eq!(partial.status(), StatusCode::PARTIAL_CONTENT);
                    assert_eq!(
                        partial.headers()[header::CONTENT_RANGE],
                        format!("bytes 0-7/{}", bytes.len())
                    );
                    assert_eq!(
                        &partial.into_body().collect().await.unwrap().to_bytes()[..],
                        &bytes[..8]
                    );
                    let conditional = app
                        .clone()
                        .oneshot(
                            Request::builder()
                                .uri(&uri)
                                .header(header::IF_MODIFIED_SINCE, modified)
                                .body(Body::empty())
                                .unwrap(),
                        )
                        .await
                        .unwrap();
                    assert_eq!(conditional.status(), StatusCode::NOT_MODIFIED);
                    assert!(
                        conditional
                            .into_body()
                            .collect()
                            .await
                            .unwrap()
                            .to_bytes()
                            .is_empty()
                    );
                    let head = app
                        .clone()
                        .oneshot(
                            Request::builder()
                                .method("HEAD")
                                .uri(&uri)
                                .body(Body::empty())
                                .unwrap(),
                        )
                        .await
                        .unwrap();
                    assert_eq!(head.status(), StatusCode::OK);
                    assert_eq!(head.headers()[header::CONTENT_TYPE], mime);
                    assert!(
                        head.into_body()
                            .collect()
                            .await
                            .unwrap()
                            .to_bytes()
                            .is_empty()
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn figure_routes_reject_unsafe_missing_and_disguised_images_without_spa_fallback() {
        let evidence = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(evidence.path().join("figures")).unwrap();
        std::fs::write(
            evidence.path().join("figures/fake.png"),
            "<html>protected</html>",
        )
        .unwrap();
        std::fs::write(
            evidence.path().join("figures/fake.jpg"),
            "<svg>protected</svg>",
        )
        .unwrap();
        std::fs::write(
            evidence.path().join("figures/x.svg"),
            "<svg>protected</svg>",
        )
        .unwrap();
        std::fs::write(evidence.path().join("figures/%2e%2e.png"), PNG).unwrap();
        for hub_mode in [false, true] {
            let app = image_router(evidence.path(), hub_mode, Assets::Embedded);
            let prefix = if hub_mode {
                "/a/demo/api/figure"
            } else {
                "/api/figure"
            };
            for path in [
                "",
                "/figures/missing.png",
                "/figures/fake.png",
                "/figures/fake.jpg",
                "/figures/x.svg",
                "/../secret.png",
                "/%2e%2e/secret.png",
                "/figures/%2e%2e/%2e%2e/secret.png",
                "/%2Ftmp/secret.png",
                "/C%3A/secret.png",
                "/figures%5Csecret.png",
                "/figures/x%0A.png",
            ] {
                let response = get(&app, &format!("{prefix}{path}")).await;
                assert_eq!(response.status(), StatusCode::NOT_FOUND, "{prefix}{path}");
                let body = response.into_body().collect().await.unwrap().to_bytes();
                assert!(!String::from_utf8_lossy(&body).contains("protected"));
                assert!(!String::from_utf8_lossy(&body).contains("<html"));
            }
            // Decode the route once: these are literal percent characters in a filename.
            let response = get(&app, &format!("{prefix}/figures/%252e%252e.png")).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                &response.into_body().collect().await.unwrap().to_bytes()[..],
                PNG
            );
            if hub_mode {
                assert_eq!(
                    get(&app, "/a/unknown/api/figure/figures/%252e%252e.png")
                        .await
                        .status(),
                    StatusCode::NOT_FOUND
                );
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn figure_routes_recheck_containment_and_allow_root_and_contained_symlinks() {
        use std::os::unix::fs::symlink;
        let evidence = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        std::fs::write(evidence.path().join("asset.png"), PNG).unwrap();
        std::fs::write(outside.path().join("secret.png"), PNG).unwrap();
        symlink("asset.png", evidence.path().join("alias.png")).unwrap();
        let selected_root = outside.path().join("selected-evidence");
        symlink(evidence.path(), &selected_root).unwrap();
        for hub_mode in [false, true] {
            let app = image_router(&selected_root, hub_mode, Assets::Embedded);
            let prefix = if hub_mode {
                "/a/demo/api/figure"
            } else {
                "/api/figure"
            };
            assert_eq!(
                get(&app, &format!("{prefix}/alias.png")).await.status(),
                StatusCode::OK
            );
            // Change after router/cache construction: ingest-time checking is insufficient.
            std::fs::remove_file(evidence.path().join("alias.png")).unwrap();
            symlink(
                outside.path().join("secret.png"),
                evidence.path().join("alias.png"),
            )
            .unwrap();
            assert_eq!(
                get(&app, &format!("{prefix}/alias.png")).await.status(),
                StatusCode::NOT_FOUND
            );
            symlink(outside.path(), evidence.path().join("escape")).unwrap();
            assert_eq!(
                get(&app, &format!("{prefix}/escape/secret.png"))
                    .await
                    .status(),
                StatusCode::NOT_FOUND
            );
            std::fs::remove_file(evidence.path().join("escape")).unwrap();
            std::fs::remove_file(evidence.path().join("alias.png")).unwrap();
            symlink("asset.png", evidence.path().join("alias.png")).unwrap();
        }
    }

    #[tokio::test]
    async fn figure_path_escape_is_rejected() {
        let (app, _) = router(&fixture("resnet-ara-example"));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/figure/../../Cargo.toml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // ServeDir normalises the traversal away → never 200 with the file.
        assert_ne!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn reparse_broadcasts_new_etag() {
        use tempfile::tempdir;

        // Copy a fixture into a temp dir so we can mutate the YAML.
        let src = fixture("resnet-ara-example");
        let dir = tempdir().unwrap();
        copy_dir(&src, dir.path());

        let (state, first_etag) = test_state(dir.path());
        let mut sub = state.live_tx.subscribe();

        // Rename a node title — a real manifest change, so the content-hash
        // etag must move (a mere YAML comment would not change the manifest).
        let tree = dir.path().join("trace/exploration_tree.yaml");
        let content = std::fs::read_to_string(&tree).unwrap();
        let edited = content.replace(
            "Is learning better networks as easy as stacking more layers?",
            "Edited title for the live-reload test",
        );
        assert_ne!(edited, content, "the fixture title must be present to edit");
        std::fs::write(&tree, edited).unwrap();

        assert!(reparse_and_swap(&state, dir.path()));
        let pushed = sub.try_recv().expect("a new etag must be broadcast");
        assert_ne!(pushed, first_etag, "etag must change after an edit");
        assert_eq!(state.cache.load().etag, pushed);
    }

    fn copy_dir(src: &Path, dst: &Path) {
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let to = dst.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                std::fs::create_dir_all(&to).unwrap();
                copy_dir(&entry.path(), &to);
            } else {
                std::fs::copy(entry.path(), &to).unwrap();
            }
        }
    }

    // ── Hub router tests (oneshot; mirror the local suite) ────────────────────

    /// Build a hub router over the two official fixtures under ids `resnet` and
    /// `minimal`, returning the router + resnet's expected etag.
    fn hub_router() -> (Router, String) {
        let mut map: std::collections::HashMap<String, Arc<CachedAra>> =
            std::collections::HashMap::new();
        let resnet = CachedAra::from_dir_lean(&fixture("resnet-ara-example")).unwrap();
        let resnet_etag = resnet.etag.clone();
        map.insert("resnet".into(), Arc::new(resnet));
        map.insert(
            "minimal".into(),
            Arc::new(CachedAra::from_dir_lean(&fixture("minimal-artifact")).unwrap()),
        );
        (
            build_hub_router(Arc::new(map), Assets::Embedded),
            resnet_etag,
        )
    }

    async fn get(app: &Router, uri: &str) -> Response {
        app.clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn hub_manifest_returns_json_and_etag() {
        let (app, etag) = hub_router();
        let resp = get(&app, "/a/resnet/api/manifest").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(resp.headers().get(header::ETAG).unwrap(), etag.as_str());
    }

    #[tokio::test]
    async fn hub_manifest_304_on_if_none_match() {
        let (app, etag) = hub_router();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/a/resnet/api/manifest")
                    .header(header::IF_NONE_MATCH, &etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn hub_manifest_reads_are_pure_cache_hits() {
        // The milestone's core property: after startup, hub reads never reparse.
        // Two sequential reads must return the SAME etag (a reparse would change
        // it only if the content changed, but there's no watcher — so identical
        // etag proves the same cached bytes are served both times).
        let (app, etag) = hub_router();
        let first = get(&app, "/a/resnet/api/manifest").await;
        let second = get(&app, "/a/resnet/api/manifest").await;
        assert_eq!(first.headers().get(header::ETAG).unwrap(), etag.as_str());
        assert_eq!(
            first.headers().get(header::ETAG),
            second.headers().get(header::ETAG),
            "sequential hub reads must return the same etag (no reparse)"
        );
    }

    #[tokio::test]
    async fn hub_distinct_aras_have_distinct_manifests() {
        let (app, _) = hub_router();
        let a = get(&app, "/a/resnet/api/manifest").await;
        let b = get(&app, "/a/minimal/api/manifest").await;
        assert_ne!(
            a.headers().get(header::ETAG),
            b.headers().get(header::ETAG),
            "two ARAs must serve different manifests at their own paths"
        );
    }

    #[tokio::test]
    async fn hub_unknown_manifest_is_404() {
        let (app, _) = hub_router();
        let resp = get(&app, "/a/nope/api/manifest").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn hub_ara_index_has_base_href_and_no_cache() {
        let (app, _) = hub_router();
        let resp = get(&app, "/a/resnet/").await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            resp.headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-cache"
        );
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            html.contains(r#"<base href="/a/resnet/">"#),
            "index must carry the per-ARA base href"
        );
    }

    #[tokio::test]
    async fn hub_known_bare_id_redirects_308() {
        let (app, _) = hub_router();
        let resp = get(&app, "/a/resnet").await;
        assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(resp.headers().get(header::LOCATION).unwrap(), "/a/resnet/");
    }

    #[tokio::test]
    async fn hub_unknown_bare_id_is_404_not_redirect() {
        // Never a 308 to a 404 (which a browser could permanently cache).
        let (app, _) = hub_router();
        let resp = get(&app, "/a/nope").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn hub_unknown_root_path_is_404_not_spa() {
        // No SPA fallback: an unknown non-asset path must 404, not the viewer index.
        let (app, _) = hub_router();
        let resp = get(&app, "/typo").await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert!(
            !String::from_utf8_lossy(&body).contains("<base"),
            "unknown root path must not serve the viewer index"
        );
    }

    #[tokio::test]
    async fn hub_root_index_lists_aras() {
        let (app, _) = hub_router();
        let resp = get(&app, "/").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains(r#"href="/a/resnet/""#));
        assert!(html.contains(r#"href="/a/minimal/""#));
    }

    #[tokio::test]
    async fn hub_shared_asset_is_served() {
        // A real embedded root asset must load at root so per-ARA pages can
        // fetch the shared immutable bundle. index.html is always embedded.
        let (app, _) = hub_router();
        let resp = get(&app, "/index.html").await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── CLI parse tests (try_parse_from) ──────────────────────────────────────

    /// Parse a full `ara serve …` arg vector through the top-level clap parser
    /// so the `mode` ArgGroup + `--host` default are exercised as shipped.
    fn parse(args: &[&str]) -> Result<ServeArgs, clap::Error> {
        use clap::Parser;
        #[derive(clap::Parser)]
        struct Cli {
            #[command(subcommand)]
            cmd: Cmd,
        }
        #[derive(clap::Subcommand)]
        enum Cmd {
            Serve(ServeArgs),
        }
        let mut full = vec!["ara", "serve"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full).map(|c| match c.cmd {
            Cmd::Serve(s) => s,
        })
    }

    #[test]
    fn parse_local_dir_ok() {
        let a = parse(&["some/dir"]).expect("local dir must parse");
        assert_eq!(a.dir.as_deref(), Some(Path::new("some/dir")));
        assert!(!a.hub);
    }

    #[test]
    fn parse_hub_with_root_ok() {
        let a = parse(&["--hub", "--ara-root", "/aras"]).expect("hub+root must parse");
        assert!(a.hub);
        assert_eq!(a.ara_root.as_deref(), Some(Path::new("/aras")));
    }

    #[test]
    fn parse_hub_without_root_errs() {
        assert!(parse(&["--hub"]).is_err(), "--hub requires --ara-root");
    }

    #[test]
    fn parse_both_modes_errs() {
        assert!(
            parse(&["some/dir", "--hub", "--ara-root", "/aras"]).is_err(),
            "positional dir conflicts with --hub"
        );
    }

    #[test]
    fn parse_neither_mode_errs() {
        assert!(parse(&[]).is_err(), "one of dir / --hub is required");
    }

    #[test]
    fn parse_host_defaults_to_loopback() {
        let a = parse(&["some/dir"]).unwrap();
        assert_eq!(a.host, std::net::IpAddr::from([127, 0, 0, 1]));
    }

    #[test]
    fn parse_host_override_is_honored() {
        let a = parse(&["--hub", "--ara-root", "/aras", "--host", "0.0.0.0"]).unwrap();
        assert_eq!(a.host, std::net::IpAddr::from([0, 0, 0, 0]));
    }

    #[tokio::test]
    async fn hub_no_head_template_errors_not_baseless() {
        // If the index template has no <head>, the handler must 500, never serve
        // a base-less page. Drive it through the on-disk `--assets` path with a
        // fixture index.html lacking <head>.
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("index.html"),
            "<!DOCTYPE html><html><body></body></html>",
        )
        .unwrap();

        let mut map: std::collections::HashMap<String, Arc<CachedAra>> =
            std::collections::HashMap::new();
        map.insert(
            "resnet".into(),
            Arc::new(CachedAra::from_dir_lean(&fixture("resnet-ara-example")).unwrap()),
        );
        let app = build_hub_router(Arc::new(map), Assets::Dir(dir.path().to_path_buf()));

        let resp = get(&app, "/a/resnet/").await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
