//! ARA Viewer — Leptos CSR entry point.
//!
//! Mounts the [`App`] component to `<body>`. All application logic lives in
//! sub-components; this file is intentionally minimal.

mod kind;
mod source;
mod state;

use leptos::prelude::*;
use source::{ManifestSource, fetch_manifest};
use state::{LoadState, MapSurface, ViewState, map_surface, safe_viewbox};

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

    // ── View state (selection + pan/zoom) ─────────────────────────────────────
    let (_view_state, _set_view_state) = signal(ViewState::default());

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
                <MapPane load_state=load_state />
            </section>
            <section id="detail" class="panel panel-detail">
                <p class="placeholder-text">"Select a step on the left."</p>
            </section>
        </main>
    }
}

/// The map pane — renders one of four surfaces based on [`LoadState`].
#[component]
fn MapPane(load_state: ReadSignal<LoadState>) -> impl IntoView {
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
                // When nodes is empty, bounds is None; safe_viewbox guards divide-by-zero.
                let _vb = safe_viewbox(None);
                view! {
                    <p class="placeholder-text">"No nodes in this artifact."</p>
                }
                .into_any()
            }

            MapSurface::Graph => {
                // Manifest is loaded with nodes.  Full SVG graph is Step 3.
                // For now show a placeholder that confirms node count.
                let count = match load_state.get() {
                    LoadState::Loaded(m) => m.nodes.len(),
                    _ => 0,
                };
                let vb = match load_state.get() {
                    LoadState::Loaded(m) => safe_viewbox(m.bounds.as_ref()),
                    _ => safe_viewbox(None),
                };
                view! {
                    <p class="placeholder-text">
                        {format!("{count} nodes loaded (graph renders in Step\u{a0}3)")}
                    </p>
                    // SVG scaffold — safe viewBox, no divide-by-zero.
                    <svg
                        class="graph-svg"
                        viewBox={format!("{} {} {} {}", vb.0, vb.1, vb.2, vb.3)}
                        xmlns="http://www.w3.org/2000/svg"
                    >
                        // Graph scene built in Step 3.
                    </svg>
                }
                .into_any()
            }
        }
    }
}
