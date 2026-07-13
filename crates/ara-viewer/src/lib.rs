//! ARA Viewer — library target.
//!
//! Exposes all viewer modules as public so integration tests and the
//! `wasm-bindgen-test` browser-test layer can import components and helpers.
//! The binary entry point lives in `src/main.rs`.

pub mod canvas;
pub mod detail;
pub mod filter;
pub mod kind;
pub mod replay;
pub mod scene;
pub mod source;
pub mod splitter;
pub mod state;
pub mod toolbar;
pub mod tree;

use std::collections::HashSet;

use ara_core::NodeId;
use detail::DetailPane;
use filter::FilterState;
use leptos::prelude::*;
use replay::{ReplayBar, ReplayState};
use scene::{GraphRenderer, GraphView, LayoutView, SvgRenderer};
use source::{ManifestSource, connect_live, fetch_manifest};
use splitter::Splitter;
use state::{DisplayMode, LayoutMode, LoadState, MapSurface, PanZoom, map_surface, safe_viewbox};
use toolbar::{DisplayToggle, LayoutToggle, Toolbar};
use tree::{TreeView, tree_model};

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

    // Per-mode split fractions (map fraction of the main axis). In-memory only;
    // reset to defaults on reload. Two signals so the column (split) and row
    // (stack) fractions don't bleed into each other when toggling modes.
    let split_ratio: RwSignal<f64> = RwSignal::new(splitter::SPLIT_DEFAULT_RATIO);
    let stack_ratio: RwSignal<f64> = RwSignal::new(splitter::STACK_DEFAULT_RATIO);
    // Drag-active flag, folded into the <main> class closure (Leptos owns that
    // attribute — do NOT toggle it imperatively, a re-render would wipe it).
    let dragging: RwSignal<bool> = RwSignal::new(false);
    // The ratio signal for the active mode (what <main>'s --split reads).
    let active_ratio = move || match layout.get() {
        LayoutMode::Split => split_ratio,
        LayoutMode::Stack => stack_ratio,
    };

    // ── Display mode (graph vs. tree; survives manifest swaps) ────────────────
    // Graph (SVG DAG) is the default; Tree is the published DOM tree-list.
    let display: RwSignal<DisplayMode> = RwSignal::new(DisplayMode::default());

    // ── Shared derived state (lifted into App — the single owner) ─────────────
    // node_order + the filter `matching` set + the `#rstat` readout live here so
    // both the header Toolbar (which renders #rstat) and the map/replay (sibling
    // subtrees) read one stable instance. `matching` used to be rebuilt inside
    // MapPane's render closure; lifting it removes that and gives the header a
    // handle to the same set.
    let node_order: Memo<Vec<NodeId>> = Memo::new(move |_| match load_state.get() {
        LoadState::Loaded(m) => replay::node_order(&m),
        _ => Vec::new(),
    });
    let matching: Memo<HashSet<NodeId>> = Memo::new(move |_| match load_state.get() {
        LoadState::Loaded(m) => {
            let f = filter.get();
            m.nodes
                .iter()
                .filter(|n| filter::node_matches(n, &m, &f))
                .map(|n| n.id.clone())
                .collect()
        }
        _ => HashSet::new(),
    });
    // The shared `#rstat` readout: replay form when a node is selected, else the
    // filter form. Both modes show it, exactly as the reference does.
    let rstat: Memo<String> = Memo::new(move |_| {
        let order = node_order.get();
        replay::rstat_text(&order, selected.get().as_ref(), matching.get().len())
    });

    // ── Replay runtime state (owned by App; shared with ReplayBar + keys) ─────
    let replay_state = ReplayState::default();

    // Tear the interval down on App unmount so it can't outlive the component.
    on_cleanup(move || replay::stop_replay(replay_state));

    // ── Document-level ←/→ key listener (wasm-only) ───────────────────────────
    // Installs the reference arrow-key stepper with its INPUT/SELECT guard.
    #[cfg(target_arch = "wasm32")]
    replay::install_arrow_key_listener(node_order, selected, replay_state);

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
                // Display + layout mode selectors — first so the filter controls
                // stay grouped on the right.
                <DisplayToggle display=display />
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
                // Shared #rstat readout: step count / filtered count. Shown in
                // both display modes, as the reference does.
                <span class="count" id="rstat">{move || rstat.get()}</span>
            </div>
        </header>
        <main
            class=move || format!(
                "app-main {}{}",
                layout.get().css_class(),
                if dragging.get() { " is-dragging" } else { "" },
            )
            style=move || format!("--split: {}%;", (active_ratio().get() * 100.0).clamp(0.0, 100.0))
        >
            // role="region" + aria-label lets screen-reader users jump between panes.
            <section id="map" class="panel panel-map" role="region" aria-label="Exploration graph">
                <MapPane
                    load_state=load_state
                    selected=selected
                    pan_zoom=pan_zoom
                    display=display
                    matching=matching
                    node_order=node_order
                    replay_state=replay_state
                />
            </section>
            <Splitter layout=layout split_ratio=split_ratio stack_ratio=stack_ratio dragging=dragging />
            <section id="detail" class="panel panel-detail" role="region" aria-label="Detail">
                <DetailPane load_state=load_state selected=selected />
            </section>
        </main>
    }
}

/// The map pane — renders one of four surfaces based on [`LoadState`].
///
/// When a manifest with nodes is loaded, renders the shared [`ReplayBar`] above
/// the map surface, then switches on [`DisplayMode`]: `Graph` → the SVG
/// [`GraphView`] (+ pan/zoom hint), `Tree` → the DOM [`TreeView`]. The
/// `matching` Memo + `node_order` are owned by `App` and passed in (so the
/// header `#rstat` and the map read one instance).
#[component]
pub fn MapPane(
    load_state: ReadSignal<LoadState>,
    selected: RwSignal<Option<NodeId>>,
    pan_zoom: RwSignal<PanZoom>,
    display: RwSignal<DisplayMode>,
    matching: Memo<HashSet<NodeId>>,
    node_order: Memo<Vec<NodeId>>,
    replay_state: ReplayState,
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

                // The map surface swaps reactively on `display`; the ReplayBar
                // sits above it in both modes. The heavy scene/tree compute is
                // done per-mode inside the reactive closure so only the active
                // renderer's model is built.
                let manifest_for_surface = manifest.clone();
                let surface = move || match display.get() {
                    DisplayMode::Graph => {
                        let scene =
                            SvgRenderer.scene(&manifest_for_surface, &LayoutView::default());
                        view! {
                            <GraphView
                                scene=scene
                                selected=selected
                                pan_zoom=pan_zoom
                                matching=matching
                            />
                            // Unobtrusive affordance so the pannable/zoomable
                            // canvas is discoverable. aria-hidden: mouse-centric
                            // guidance; keyboard users navigate via toolbar + Tab.
                            <p class="map-hint" aria-hidden="true">"Scroll to zoom · drag to pan"</p>
                        }
                        .into_any()
                    }
                    DisplayMode::Tree => {
                        let model = tree_model(&manifest_for_surface);
                        view! {
                            <TreeView model=model selected=selected matching=matching />
                        }
                        .into_any()
                    }
                };

                view! {
                    <ReplayBar order=node_order selected=selected state=replay_state />
                    {surface}
                }
                .into_any()
            }
        }
    }
}
