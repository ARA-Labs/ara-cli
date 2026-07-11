//! ARA Viewer — Leptos CSR entry point.
//!
//! Mounts the [`App`] component to `<body>`. All application logic lives in
//! sub-components; this file is intentionally minimal.

mod canvas;
mod kind;
mod scene;
mod source;
mod state;

use ara_core::NodeId;
use leptos::prelude::*;
use scene::{GraphRenderer, GraphView, LayoutView, SvgRenderer};
use source::{ManifestSource, fetch_manifest};
use state::{LoadState, MapSurface, PanZoom, map_surface, safe_viewbox};

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}

/// Root application shell.
///
/// Renders the two-pane layout: a fixed header with title and toolbar area,
/// and a CSS grid main section containing the `#map` (left) and `#detail`
/// (right) panels.
#[component]
fn App() -> impl IntoView {
    // ── Manifest load state ──────────────────────────────────────────────────
    let (load_state, set_load_state) = signal(LoadState::Loading);

    // On mount, start the async fetch.  The fetch is cfg'd out on native so
    // `cargo test` compiles without browser deps.
    let source = ManifestSource::default();
    fetch_manifest(source, move |s| set_load_state.set(s));

    // ── Selection state (shared with future detail pane, Step 4) ─────────────
    // Owned here so it survives manifest swaps and can be read by the detail
    // pane without requiring prop-drilling through MapPane.
    let selected: RwSignal<Option<NodeId>> = RwSignal::new(None);

    // ── Pan/zoom state (persists across manifest swaps) ───────────────────────
    let pan_zoom: RwSignal<PanZoom> = RwSignal::new(PanZoom::default());

    view! {
        <header class="app-header">
            <div class="header-title">
                <h1>"ARA Viewer"</h1>
                <span class="header-subtitle">"Agent-Native Research Artifact"</span>
            </div>
            <div class="toolbar-area">
                // Toolbar placeholder — populated in a later step.
            </div>
        </header>
        <main class="app-main">
            <section id="map" class="panel panel-map">
                <MapPane load_state=load_state selected=selected pan_zoom=pan_zoom />
            </section>
            <section id="detail" class="panel panel-detail">
                // Step 4 will build the real detail pane.
                // For now, show the selected node id or a placeholder.
                {move || match selected.get() {
                    None => view! {
                        <p class="placeholder-text">"Select a step on the left."</p>
                    }.into_any(),
                    Some(id) => view! {
                        <p class="placeholder-text">"Selected: " {id.as_str().to_string()}</p>
                    }.into_any(),
                }}
            </section>
        </main>
    }
}

/// The map pane — renders one of four surfaces based on [`LoadState`].
#[component]
fn MapPane(
    load_state: ReadSignal<LoadState>,
    selected: RwSignal<Option<NodeId>>,
    pan_zoom: RwSignal<PanZoom>,
) -> impl IntoView {
    move || {
        let state = load_state.get();
        match map_surface(&state) {
            MapSurface::Loading => view! {
                <div class="skeleton" aria-busy="true" aria-label="Loading artifact">
                    <p class="skeleton-text">"Loading artifact\u{2026}"</p>
                </div>
            }
            .into_any(),

            MapSurface::Error(reason) => view! {
                <div class="error-card" role="alert">
                    <h2 class="error-card-title">"Couldn\u{2019}t load manifest"</h2>
                    <p class="error-card-reason">{reason}</p>
                </div>
            }
            .into_any(),

            MapSurface::Empty => {
                let _vb = safe_viewbox(None);
                view! {
                    <p class="placeholder-text">"No nodes in this artifact."</p>
                }
                .into_any()
            }

            MapSurface::Graph => {
                let manifest = match load_state.get() {
                    LoadState::Loaded(m) => m,
                    _ => return view! { <p class="placeholder-text">"Loading…"</p> }.into_any(),
                };

                let view_params = LayoutView::default();
                let renderer = SvgRenderer;
                let scene = renderer.scene(&manifest, &view_params);

                view! {
                    <GraphView scene=scene selected=selected pan_zoom=pan_zoom />
                }
                .into_any()
            }
        }
    }
}
