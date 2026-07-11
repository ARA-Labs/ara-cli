//! ARA Viewer — library target.
//!
//! Exposes all viewer modules as public so integration tests and the
//! `wasm-bindgen-test` browser-test layer can import components and helpers.
//! The binary entry point lives in `src/main.rs`.

pub mod canvas;
pub mod detail;
pub mod filter;
pub mod kind;
pub mod scene;
pub mod source;
pub mod state;
pub mod toolbar;

use std::collections::HashSet;

use ara_core::NodeId;
use detail::DetailPane;
use filter::FilterState;
use leptos::prelude::*;
use scene::{GraphRenderer, GraphView, LayoutView, SvgRenderer};
use source::{ManifestSource, connect_live, fetch_manifest};
use state::{LayoutMode, LoadState, MapSurface, PanZoom, map_surface, safe_viewbox};
use toolbar::{LayoutToggle, Toolbar};

/// Mount the [`App`] component to `<body>`.
///
/// Called from `src/main.rs`.  Exposed as `pub` so tests can drive it if
/// needed; for DOM tests prefer mounting sub-components directly.
pub fn mount() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}

/// Root application shell.
///
/// Renders a fixed header with title and toolbar area, and a CSS grid main
/// section containing the `#map` and `#detail` panels. The panes are arranged
/// per the user-selected [`LayoutMode`]: `Stack` (map on top, detail below —
/// the default) or `Split` (map left, detail right).
#[component]
pub fn App() -> impl IntoView {
    // ── Manifest load state ──────────────────────────────────────────────────
    let (load_state, set_load_state) = signal(LoadState::Loading);

    // On mount, start the async fetch, then subscribe to live-reload pushes.
    // Both are cfg'd out on native so `cargo test` compiles without browser
    // deps. `set_load_state` is Copy, so the update closure is Clone — required
    // by `connect_live`, which re-fetches on every WebSocket message.
    let update = move |s| set_load_state.set(s);
    fetch_manifest(ManifestSource::default(), update);
    connect_live(ManifestSource::default(), update);

    // ── Selection state (shared with detail pane) ────────────────────────────
    // Owned here so it survives manifest swaps and can be read by the detail
    // pane without requiring prop-drilling through MapPane.
    let selected: RwSignal<Option<NodeId>> = RwSignal::new(None);

    // ── Pan/zoom state (persists across manifest swaps) ───────────────────────
    let pan_zoom: RwSignal<PanZoom> = RwSignal::new(PanZoom::default());

    // ── Filter state (Step 5: toolbar + dimming; survives manifest swaps) ─────
    let filter: RwSignal<FilterState> = RwSignal::new(FilterState::default());

    // ── Layout mode (stack vs. split; survives manifest swaps) ────────────────
    // Stack (map on top, detail below) is the default — it matches the wide DAG
    // shape and uses the full viewport width. Split is the opt-in side-by-side.
    let layout: RwSignal<LayoutMode> = RwSignal::new(LayoutMode::default());

    view! {
        <header class="app-header">
            <div class="header-title">
                <h1>"ARA Viewer"</h1>
                <span class="header-subtitle">"Agent-Native Research Artifact"</span>
                // INERT: artifact abstract/summary would render here once the
                // Manifest schema carries an `abstract` field.  Until then this
                // block is omitted entirely — no placeholder UI.
            </div>
            // role="toolbar" gives AT users a named landmark for the filter controls.
            <div class="toolbar-area" role="toolbar" aria-label="Filters">
                // Layout mode selector — first so the filter controls stay
                // grouped on the right.
                <LayoutToggle layout=layout />
                // Extract the manifest for the Toolbar kind-options derive.
                // When not loaded, pass None so the select is disabled.
                {move || {
                    let manifest = match load_state.get() {
                        LoadState::Loaded(m) => Some(m),
                        _ => None,
                    };
                    view! {
                        <Toolbar filter=filter manifest=manifest />
                    }
                }}
            </div>
        </header>
        <main class=move || format!("app-main {}", layout.get().css_class())>
            // role="region" + aria-label lets screen-reader users jump between panes.
            <section id="map" class="panel panel-map" role="region" aria-label="Exploration graph">
                <MapPane load_state=load_state selected=selected pan_zoom=pan_zoom filter=filter />
            </section>
            <section id="detail" class="panel panel-detail" role="region" aria-label="Detail">
                <DetailPane load_state=load_state selected=selected />
            </section>
        </main>
    }
}

/// The map pane — renders one of four surfaces based on [`LoadState`].
#[component]
pub fn MapPane(
    load_state: ReadSignal<LoadState>,
    selected: RwSignal<Option<NodeId>>,
    pan_zoom: RwSignal<PanZoom>,
    filter: RwSignal<FilterState>,
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

                // Reactive matching set: ids of nodes passing the current filter.
                // Stored as a Memo so it only recomputes when filter or manifest changes.
                let matching: Memo<HashSet<NodeId>> = {
                    let manifest_for_filter = manifest.clone();
                    Memo::new(move |_| {
                        let f = filter.get();
                        manifest_for_filter
                            .nodes
                            .iter()
                            .filter(|n| filter::node_matches(n, &manifest_for_filter, &f))
                            .map(|n| n.id.clone())
                            .collect::<HashSet<NodeId>>()
                    })
                };

                view! {
                    <GraphView scene=scene selected=selected pan_zoom=pan_zoom matching=matching />
                    // Unobtrusive affordance so the pannable/zoomable canvas is
                    // discoverable. aria-hidden: it's mouse-centric guidance;
                    // keyboard users navigate via the toolbar + Tab.
                    <p class="map-hint" aria-hidden="true">"Scroll to zoom · drag to pan"</p>
                }
                .into_any()
            }
        }
    }
}
