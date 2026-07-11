//! Load-state model, view state, and pure reducer helpers.
//!
//! Everything in this module is **free of wasm-only dependencies** so it
//! compiles and is testable on native (`cargo test -p ara-viewer`).

use ara_core::{Manifest, NodeId, Rect};

// ── Parse helper ─────────────────────────────────────────────────────────────

/// Deserialise a JSON string into a [`Manifest`].
///
/// Maps `serde_json` errors to a human-readable [`String`] so the rest of the
/// code never has to depend on `serde_json`'s error types directly.
///
/// Used directly by the wasm fetch path and by native tests; not called from
/// the native binary entry point.
#[allow(dead_code)]
pub fn parse_manifest(json: &str) -> Result<Manifest, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

// ── Load state ────────────────────────────────────────────────────────────────

/// The lifecycle of a manifest fetch.
#[derive(Debug, Clone)]
pub enum LoadState {
    /// Fetch in-flight; no manifest available yet.
    Loading,
    /// Fetch + parse succeeded.
    Loaded(Manifest),
    /// Fetch or parse failed; `reason` is shown to the user verbatim.
    Failed(String),
}

// ── Map surface selector ──────────────────────────────────────────────────────

/// Which surface the `#map` pane should display.
#[derive(Debug, PartialEq)]
pub enum MapSurface {
    /// Show a "Loading artifact…" skeleton.
    Loading,
    /// Show an error card: "Couldn't load manifest" + reason.
    Error(String),
    /// Manifest loaded but `nodes` is empty.
    Empty,
    /// Manifest loaded with at least one node — render the graph.
    Graph,
}

/// Map a [`LoadState`] to the [`MapSurface`] the UI should show.
///
/// Rules:
/// - `Loading` → `Loading`
/// - `Failed(r)` → `Error(r)`
/// - `Loaded(m)` where `m.nodes` is empty → `Empty`
/// - `Loaded(m)` with nodes → `Graph`
pub fn map_surface(state: &LoadState) -> MapSurface {
    match state {
        LoadState::Loading => MapSurface::Loading,
        LoadState::Failed(reason) => MapSurface::Error(reason.clone()),
        LoadState::Loaded(m) if m.nodes.is_empty() => MapSurface::Empty,
        LoadState::Loaded(_) => MapSurface::Graph,
    }
}

// ── Safe viewBox ──────────────────────────────────────────────────────────────

/// Returns a safe SVG `viewBox` tuple `(min_x, min_y, width, height)`.
///
/// When `bounds` is `None` or has a non-positive extent, returns the fallback
/// `(0, 0, 100, 100)` to prevent divide-by-zero on an empty graph.
pub fn safe_viewbox(bounds: Option<&Rect>) -> (f64, f64, f64, f64) {
    const FALLBACK: (f64, f64, f64, f64) = (0.0, 0.0, 100.0, 100.0);
    match bounds {
        None => FALLBACK,
        Some(r) if r.width <= 0.0 || r.height <= 0.0 => FALLBACK,
        Some(r) => (r.x, r.y, r.width, r.height),
    }
}

// ── View state ────────────────────────────────────────────────────────────────

/// Minimal pan/zoom state.  Steps 3b/5 will extend this; it is a plain value
/// type so it can be stored in a Leptos signal and tested on native.
#[derive(Debug, Clone, PartialEq)]
pub struct PanZoom {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl Default for PanZoom {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

/// Per-session view state that must survive a manifest swap.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewState {
    /// Currently selected node, if any.
    pub selection: Option<NodeId>,
    /// Pan and zoom applied to the graph canvas.
    pub pan_zoom: PanZoom,
}

// ── apply_manifest reducer ────────────────────────────────────────────────────

/// Produce a new [`ViewState`] after replacing the active manifest.
///
/// The reducer clones the existing `selection` and `pan_zoom` into the returned
/// state, preserving the user's context across a live-reload or manual manifest
/// swap (the Stage-4 live-reload survival promise).
///
/// `_new` is accepted (and explicitly ignored) to make the signature concrete
/// and ready for Stage 4 without breaking callers.
///
/// Called by native tests; not yet wired to a hot-reload trigger.
#[allow(dead_code)]
pub fn apply_manifest(view: &ViewState, _new: &Manifest) -> ViewState {
    view.clone()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ara_core::NodeId;

    // ── manifest round-trip ───────────────────────────────────────────────────

    /// The checked-in `public/manifest.json` must parse without error and
    /// have the expected node count (15, for the ResNet demo).
    #[test]
    fn manifest_round_trip_checked_in_file() {
        let json = include_str!("../public/manifest.json");
        let manifest = parse_manifest(json).expect("checked-in manifest.json must parse");
        assert_eq!(
            manifest.nodes.len(),
            15,
            "ResNet demo manifest has 15 nodes"
        );
        assert!(
            manifest.bounds.is_some(),
            "checked-in manifest must carry bounds"
        );
    }

    // ── map_surface state selection ───────────────────────────────────────────

    #[test]
    fn map_surface_loading() {
        assert_eq!(map_surface(&LoadState::Loading), MapSurface::Loading);
    }

    #[test]
    fn map_surface_failed() {
        let reason = "404 Not Found".to_string();
        assert_eq!(
            map_surface(&LoadState::Failed(reason.clone())),
            MapSurface::Error(reason)
        );
    }

    #[test]
    fn map_surface_loaded_non_empty() {
        let json = include_str!("../public/manifest.json");
        let manifest = parse_manifest(json).unwrap();
        assert_eq!(map_surface(&LoadState::Loaded(manifest)), MapSurface::Graph);
    }

    #[test]
    fn map_surface_empty_nodes() {
        let json = include_str!("../public/manifest.json");
        let mut manifest = parse_manifest(json).unwrap();
        manifest.nodes.clear();
        assert_eq!(map_surface(&LoadState::Loaded(manifest)), MapSurface::Empty);
    }

    // ── safe_viewbox ──────────────────────────────────────────────────────────

    #[test]
    fn safe_viewbox_none_returns_default() {
        assert_eq!(safe_viewbox(None), (0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn safe_viewbox_zero_width_returns_default() {
        let r = Rect {
            x: 10.0,
            y: 10.0,
            width: 0.0,
            height: 50.0,
        };
        assert_eq!(safe_viewbox(Some(&r)), (0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn safe_viewbox_zero_height_returns_default() {
        let r = Rect {
            x: 10.0,
            y: 10.0,
            width: 50.0,
            height: 0.0,
        };
        assert_eq!(safe_viewbox(Some(&r)), (0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn safe_viewbox_negative_extent_returns_default() {
        let r = Rect {
            x: 10.0,
            y: 10.0,
            width: -5.0,
            height: 100.0,
        };
        assert_eq!(safe_viewbox(Some(&r)), (0.0, 0.0, 100.0, 100.0));
    }

    #[test]
    fn safe_viewbox_positive_extent_passes_through() {
        let r = Rect {
            x: 5.0,
            y: 15.0,
            width: 200.0,
            height: 100.0,
        };
        assert_eq!(safe_viewbox(Some(&r)), (5.0, 15.0, 200.0, 100.0));
    }

    /// Specifically covers the bounds from the checked-in manifest (non-zero,
    /// so it must NOT return the fallback).
    #[test]
    fn safe_viewbox_real_manifest_bounds() {
        let json = include_str!("../public/manifest.json");
        let manifest = parse_manifest(json).unwrap();
        let bounds = manifest.bounds.as_ref().unwrap();
        let vb = safe_viewbox(Some(bounds));
        // Must not be the fallback — the real bounds have positive extent.
        assert_ne!(vb, (0.0, 0.0, 100.0, 100.0));
        assert!(
            vb.2 > 0.0 && vb.3 > 0.0,
            "viewBox must have positive extent"
        );
    }

    // ── apply_manifest ────────────────────────────────────────────────────────

    /// Swapping the manifest must preserve selection + pan_zoom.
    #[test]
    fn apply_manifest_preserves_selection_and_pan_zoom() {
        let json = include_str!("../public/manifest.json");
        let manifest = parse_manifest(json).unwrap();

        let original = ViewState {
            selection: Some(NodeId::new("N07")),
            pan_zoom: PanZoom {
                x: 42.0,
                y: -10.0,
                zoom: 2.5,
            },
        };

        let next = apply_manifest(&original, &manifest);

        assert_eq!(
            next.selection,
            Some(NodeId::new("N07")),
            "selection must survive manifest swap"
        );
        assert_eq!(
            next.pan_zoom, original.pan_zoom,
            "pan_zoom must survive manifest swap"
        );
    }
}
