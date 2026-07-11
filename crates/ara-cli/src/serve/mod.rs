//! `ara serve <dir>` — local axum server with file-watch live reload.
//!
//! Serves the viewer (embedded or `--assets`), the parsed manifest as JSON
//! (`/api/manifest`, ETag/304), range-capable figures (`/api/figure/*`), and a
//! WebSocket (`/api/live`) that pushes the new ETag on every reparse so the
//! client re-fetches and re-renders in place.

mod assets;
mod cache;
mod watch;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::sync::{broadcast, mpsc};
use tower_http::services::{ServeDir, ServeFile};

use assets::{Assets, embedded_handler, embedded_is_populated};
use cache::CachedAra;

/// `ara serve` command-line arguments.
#[derive(clap::Args)]
pub struct ServeArgs {
    /// Path to the ARA artifact directory (containing `trace/` and `logic/`).
    dir: PathBuf,
    /// Port to bind on `127.0.0.1`.
    #[arg(long, default_value_t = 8080)]
    port: u16,
    /// Serve viewer assets from this on-disk `dist/` instead of the embedded copy.
    #[arg(long)]
    assets: Option<PathBuf>,
    /// Use the polling watcher (for network mounts / bind mounts).
    #[arg(long)]
    poll: bool,
}

/// Shared server state: the swappable cache + the live-reload broadcast channel.
#[derive(Clone)]
pub struct AppState {
    cache: Arc<ArcSwap<CachedAra>>,
    live_tx: broadcast::Sender<String>,
}

/// Entry point for `ara serve`. Builds the initial cache, then runs the server
/// on a fresh multi-thread Tokio runtime until Ctrl-C.
pub fn run(args: ServeArgs) -> ExitCode {
    // Parse once up front so a broken artifact fails fast with diagnostics.
    let initial = match CachedAra::from_dir(&args.dir) {
        Ok(c) => c,
        Err(report) => {
            for diagnostic in report.errors() {
                eprintln!("{diagnostic}");
            }
            eprintln!(
                "{}: cannot serve — {} parse error(s)",
                args.dir.display(),
                report.errors().len()
            );
            return ExitCode::FAILURE;
        }
    };

    if args.assets.is_none() && !embedded_is_populated() {
        eprintln!(
            "warning: no viewer assets are embedded in this binary; pass --assets <dist> \
             (was this built without `trunk build`?)"
        );
    }

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

    match runtime.block_on(serve(args, initial)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Wire the router, spawn the watcher, and serve until shutdown.
async fn serve(args: ServeArgs, initial: CachedAra) -> Result<(), Box<dyn std::error::Error>> {
    let figures_dir = initial.figures_dir.clone();
    let cache = Arc::new(ArcSwap::from_pointee(initial));
    let (live_tx, _) = broadcast::channel::<String>(16);
    let state = AppState {
        cache: cache.clone(),
        live_tx: live_tx.clone(),
    };

    let assets = match args.assets {
        Some(dir) => Assets::Dir(dir),
        None => Assets::Embedded,
    };
    let app = build_router(state.clone(), assets, figures_dir);

    // File watcher → debounced reparse → cache swap → broadcast new ETag.
    let (watch_tx, mut watch_rx) = mpsc::unbounded_channel();
    watch::spawn(&args.dir, args.poll, watch_tx)?;
    let reparse_state = state.clone();
    let watch_dir = args.dir.clone();
    tokio::spawn(async move {
        while watch_rx.recv().await.is_some() {
            reparse_and_swap(&reparse_state, &watch_dir);
        }
    });

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], args.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!(
        "ara serve: http://{addr}  (watching {})",
        args.dir.display()
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Build the route table. Split out so tests can drive it via `oneshot`.
pub fn build_router(state: AppState, assets: Assets, figures_dir: PathBuf) -> Router {
    let router = Router::new()
        .route("/api/manifest", get(manifest))
        .route("/api/live", get(live_ws))
        // ServeDir handles range requests and rejects `..` traversal.
        .nest_service("/api/figure", ServeDir::new(figures_dir));

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

/// `GET /api/manifest` — cached JSON with a strong `ETag`; `304` on match.
async fn manifest(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let cached = state.cache.load();

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
}
