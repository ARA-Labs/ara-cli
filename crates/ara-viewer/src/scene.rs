//! Pure scene model, `GraphRenderer` trait, and `SvgRenderer`.
//!
//! The scene layer is the bridge between a [`Manifest`] (logical graph) and a
//! concrete renderer.  Everything in this file is free of `web-sys` and
//! browser-only imports so it compiles and is fully tested on native targets.
//!
//! Edge paths use **center-to-center straight lines**: each path goes from the
//! center `(pos.x, pos.y)` of the source node to the center of the target node.
//! Pan/zoom is applied at the SVG `viewBox` level, not here; scene geometry is
//! view-independent.

use std::collections::{HashMap, HashSet};

use ara_core::{LinkKind, Manifest, NodeId, NodeKind, Rect};
use leptos::prelude::*;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys;

use crate::kind::kind_meta;
use crate::state::{PanZoom, safe_viewbox};

// ── Layout parameters ─────────────────────────────────────────────────────────

/// Fixed rendering parameters matching `ara-core`'s `LayoutOptions` defaults.
///
/// Node box size is fixed at 180×60 px — the same values used during layout so
/// the rendered boxes align with the precomputed positions.  Pan/zoom is applied
/// at the SVG `viewBox` level and is not encoded here.
pub struct LayoutView {
    pub node_width: f64,
    pub node_height: f64,
}

impl Default for LayoutView {
    fn default() -> Self {
        Self {
            node_width: 180.0,
            node_height: 60.0,
        }
    }
}

// ── Scene types ───────────────────────────────────────────────────────────────

/// A single node in the scene with all display-relevant fields precomputed.
pub struct SceneNode {
    /// Node identifier — used by the interaction layer for selection/hit-testing.
    pub id: NodeId,
    /// Top-left origin bounding box: `x = pos.x - w/2`, `y = pos.y - h/2`.
    pub rect: Rect,
    /// Node kind — consumed by tests and kind-specific rendering.
    /// Suppressed because the binary path drives rendering from pre-derived
    /// `css_class`/`glyph`/`badge`; the raw kind is only read by tests.
    #[allow(dead_code)]
    pub kind: NodeKind,
    pub glyph: char,
    pub css_class: &'static str,
    pub label: String,
    pub badge: String,
    pub is_dead_end: bool,
}

/// A directed edge in the scene with a precomputed SVG path string.
pub struct SceneEdge {
    /// Source node — consumed by tests; the binary path only uses `path`.
    #[allow(dead_code)]
    pub from: NodeId,
    /// Target node — consumed by tests; the binary path only uses `path`.
    #[allow(dead_code)]
    pub to: NodeId,
    /// SVG path `d` attribute: straight center-to-center line.
    pub path: String,
    pub link_kind: LinkKind,
}

/// The complete scene derived from a [`Manifest`] and [`LayoutView`].
pub struct GraphScene {
    pub nodes: Vec<SceneNode>,
    pub edges: Vec<SceneEdge>,
    /// Bounds for the SVG `viewBox`.  Falls back to `(0,0,100,100)` when the
    /// manifest carries no valid bounds (reuses [`safe_viewbox`] semantics).
    pub bounds: Rect,
}

// ── GraphRenderer trait ───────────────────────────────────────────────────────

/// Seam between the logical graph and a concrete renderer.
///
/// `scene` is a pure function; it never panics and has no side effects.
/// Concrete renderers implement the rendering step (SVG now, canvas if needed).
pub trait GraphRenderer {
    fn scene(&self, manifest: &Manifest, view: &LayoutView) -> GraphScene;
}

// ── SvgRenderer ───────────────────────────────────────────────────────────────

/// Renders a [`GraphScene`] as static, skinned SVG inside a Leptos `view!`.
pub struct SvgRenderer;

impl GraphRenderer for SvgRenderer {
    fn scene(&self, manifest: &Manifest, view: &LayoutView) -> GraphScene {
        // Build NodeId → center-Point map from nodes that have positions.
        let pos_map: HashMap<&NodeId, (f64, f64)> = manifest
            .nodes
            .iter()
            .filter_map(|n| n.pos.map(|p| (&n.id, (p.x, p.y))))
            .collect();

        // Build scene nodes (skip nodes without positions).
        let nodes = manifest
            .nodes
            .iter()
            .filter_map(|n| {
                let (cx, cy) = *pos_map.get(&n.id)?;
                let meta = kind_meta(&n.kind);
                let label = n.label.clone().unwrap_or_else(|| n.id.as_str().to_string());
                Some(SceneNode {
                    id: n.id.clone(),
                    rect: Rect {
                        x: cx - view.node_width / 2.0,
                        y: cy - view.node_height / 2.0,
                        width: view.node_width,
                        height: view.node_height,
                    },
                    is_dead_end: matches!(n.kind, NodeKind::DeadEnd),
                    glyph: meta.glyph,
                    css_class: meta.css_class,
                    badge: meta.badge,
                    kind: n.kind.clone(),
                    label,
                })
            })
            .collect();

        // Build scene edges (skip any edge whose endpoint has no position).
        let edges = manifest
            .links
            .iter()
            .filter_map(|link| {
                let (x1, y1) = *pos_map.get(&link.from)?;
                let (x2, y2) = *pos_map.get(&link.to)?;
                let path = format!("M {x1} {y1} L {x2} {y2}");
                Some(SceneEdge {
                    from: link.from.clone(),
                    to: link.to.clone(),
                    path,
                    link_kind: link.kind,
                })
            })
            .collect();

        // Derive bounds from manifest; fall back to (0,0,100,100) via safe_viewbox.
        let (bx, by, bw, bh) = safe_viewbox(manifest.bounds.as_ref());
        let bounds = Rect {
            x: bx,
            y: by,
            width: bw,
            height: bh,
        };

        GraphScene {
            nodes,
            edges,
            bounds,
        }
    }
}

// ── Owned per-node display struct ────────────────────────────────────────────

/// All display-relevant data for one node, owned for rendering.
struct NodeDisplay {
    id: NodeId,
    rx: f64,
    ry: f64,
    rw: f64,
    rh: f64,
    label: String,
    glyph: char,
    group_class: String,
    badge: String,
    is_dead_end: bool,
    aria_label: String,
}

impl NodeDisplay {
    fn from_scene_node(n: &SceneNode) -> Self {
        let group_class = format!("node {}", n.css_class);
        let aria_label = format!("{}, {}", n.label, n.badge);
        Self {
            id: n.id.clone(),
            rx: n.rect.x,
            ry: n.rect.y,
            rw: n.rect.width,
            rh: n.rect.height,
            label: n.label.clone(),
            glyph: n.glyph,
            group_class,
            badge: n.badge.clone(),
            is_dead_end: n.is_dead_end,
            aria_label,
        }
    }
}

// ── Interactive graph view component ─────────────────────────────────────────

/// Interactive SVG graph: selection, focus, pan/zoom, hit-testing, a11y.
///
/// This is the Step-3b interaction layer.  It replaces the static
/// `render_scene` function.  The pure `scene()` compute is unchanged.
///
/// Selection state is kept in `selected` (shared with the detail pane).
/// Pan/zoom state is kept in `pan_zoom` (survives manifest swaps in Stage 4).
/// `matching` is the reactive set of node ids that pass the toolbar filter;
/// nodes NOT in the set are rendered with the `dimmed` CSS class.
#[component]
pub fn GraphView(
    scene: GraphScene,
    selected: RwSignal<Option<NodeId>>,
    pan_zoom: RwSignal<PanZoom>,
    matching: Memo<HashSet<NodeId>>,
) -> impl IntoView {
    // Unpack scene into owned, Clone-able parts suitable for closures.
    let bounds = scene.bounds;

    // ── Edge display data ─────────────────────────────────────────────────────
    let edges: Vec<(String, &'static str)> = scene
        .edges
        .iter()
        .map(|e| {
            let cls = match e.link_kind {
                LinkKind::Child => "edge edge-child",
                LinkKind::DependsOn => "edge edge-depends",
            };
            (e.path.clone(), cls)
        })
        .collect();

    // ── Node display data ─────────────────────────────────────────────────────
    let nodes: Vec<NodeDisplay> = scene
        .nodes
        .iter()
        .map(NodeDisplay::from_scene_node)
        .collect();

    // ── Reactive viewBox derived from pan_zoom ────────────────────────────────
    // A layered exploration tree is typically wide and short, so a whole-graph
    // fit is limited by width and leaves vertical slack. We pad the base viewBox
    // for breathing room and top-align the SVG (below) so the graph reads as
    // content anchored at the top rather than floating in the middle of the pane.
    let pad = bounds.width.max(bounds.height) * 0.06;
    let viewbox = move || {
        let pz = pan_zoom.get();
        // zoom > 1 = zoomed in → divide viewBox dims by zoom (smaller viewport).
        let vb_w = (bounds.width + pad * 2.0) / pz.zoom;
        let vb_h = (bounds.height + pad * 2.0) / pz.zoom;
        // pan offsets shift the top-left origin.
        let vb_x = bounds.x - pad + pz.x;
        let vb_y = bounds.y - pad + pz.y;
        format!("{vb_x} {vb_y} {vb_w} {vb_h}")
    };

    // ── Drag pan state (in SVG-unit space) ───────────────────────────────────
    // We track whether a drag is in progress and the pointer start position
    // (in SVG units).  Only pointer events on the SVG background start a drag.
    let drag_start: RwSignal<Option<(f64, f64, f64, f64)>> = RwSignal::new(None);
    // drag_start stores (client_x, client_y, pan_x_at_start, pan_y_at_start).

    view! {
        <svg
            class="graph-svg"
            viewBox=viewbox
            xmlns="http://www.w3.org/2000/svg"
            preserveAspectRatio="xMidYMin meet"
            on:wheel=move |ev: web_sys::WheelEvent| {
                ev.prevent_default();
                let delta = ev.delta_y();
                pan_zoom.update(|pz| {
                    // Positive delta_y = scroll down = zoom out.
                    let factor = if delta > 0.0 { 0.9 } else { 1.0 / 0.9 };
                    pz.zoom = (pz.zoom * factor).clamp(0.2, 5.0);
                });
            }
            on:pointerdown=move |ev: web_sys::PointerEvent| {
                // Only start a pan when the press lands on the SVG background, not
                // on a node. A node's sub-elements (glyph chip, label, badge) do
                // not all carry the `node` class, so walk the ancestor chain via
                // closest(".node") instead of testing the target's own class.
                let on_node = ev
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                    .and_then(|el| el.closest(".node").ok().flatten())
                    .is_some();
                if !on_node {
                    let pz = pan_zoom.get();
                    drag_start.set(Some((
                        ev.client_x() as f64,
                        ev.client_y() as f64,
                        pz.x,
                        pz.y,
                    )));
                    // Capture pointer so we receive move/up even when cursor leaves.
                    if let Some(tgt) = ev.current_target()
                        && let Some(el) = tgt.dyn_ref::<web_sys::Element>()
                    {
                        let _ = el.set_pointer_capture(ev.pointer_id());
                    }
                }
            }
            on:pointermove=move |ev: web_sys::PointerEvent| {
                if let Some((start_cx, start_cy, pan_x0, pan_y0)) = drag_start.get() {
                    let dx_screen = ev.client_x() as f64 - start_cx;
                    let dy_screen = ev.client_y() as f64 - start_cy;
                    let pz = pan_zoom.get();
                    // Current viewBox dimensions (mirrors `viewbox` above).
                    let vb_w = (bounds.width + pad * 2.0) / pz.zoom;
                    let vb_h = (bounds.height + pad * 2.0) / pz.zoom;
                    // preserveAspectRatio="…meet" scales BOTH axes by the same
                    // factor s = min(clientW/vb_w, clientH/vb_h), so one screen
                    // pixel maps to 1/s viewBox units on both axes. Read the
                    // rendered SVG size from the handler's own element rather than
                    // hardcoding a per-axis divisor (which panned y ~2-3× too slow).
                    let units_per_px = ev
                        .current_target()
                        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                        .map(|el| {
                            let s = (el.client_width() as f64 / vb_w)
                                .min(el.client_height() as f64 / vb_h);
                            if s > 0.0 { 1.0 / s } else { 1.0 }
                        })
                        .unwrap_or(1.0);
                    // Panning right in screen space moves the viewBox origin left.
                    pan_zoom.update(|pz| {
                        pz.x = pan_x0 - dx_screen * units_per_px;
                        pz.y = pan_y0 - dy_screen * units_per_px;
                    });
                }
            }
            on:pointerup=move |_ev: web_sys::PointerEvent| {
                drag_start.set(None);
            }
            on:pointercancel=move |_ev: web_sys::PointerEvent| {
                drag_start.set(None);
            }
        >
            // ── Edges (rendered under nodes) ──────────────────────────────────
            <g class="edges">
                {edges
                    .into_iter()
                    .map(|(path, cls)| {
                        view! { <path d=path class=cls /> }
                    })
                    .collect_view()}
            </g>
            // ── Nodes (interactive) ───────────────────────────────────────────
            <g class="nodes">
                {nodes
                    .into_iter()
                    .map(|nd| {
                        let node_id = nd.id.clone();
                        let node_id_click = node_id.clone();
                        let node_id_key = node_id.clone();

                        // Reactive class: add "selected" when this node is selected,
                        // and "dimmed" when it does not pass the current filter.
                        let group_class_base = nd.group_class.clone();
                        let node_id_dim = node_id.clone();
                        let group_class = move || {
                            let mut cls = group_class_base.clone();
                            if selected.get().as_ref() == Some(&node_id) {
                                cls.push_str(" selected");
                            }
                            if !matching.get().contains(&node_id_dim) {
                                cls.push_str(" dimmed");
                            }
                            cls
                        };

                        let rx = nd.rx;
                        let ry = nd.ry;
                        let rw = nd.rw;
                        let rh = nd.rh;
                        let label = nd.label.clone();
                        let badge = nd.badge.clone();
                        let glyph = nd.glyph;
                        let is_dead_end = nd.is_dead_end;
                        let aria_label = nd.aria_label.clone();

                        // Chip dimensions: 20×20, top-left corner of the node box.
                        let chip_x = rx + 4.0;
                        let chip_y = ry + 4.0;
                        let chip_size = 20.0;
                        // foreignObject for the 2-line clamped label.
                        let fo_x = rx + chip_size + 10.0;
                        let fo_y = ry + 4.0;
                        let fo_w = rw - chip_size - 18.0;
                        let fo_h = rh - 18.0; // leaves room for badge
                        // Badge: bottom-right corner.
                        let badge_x = rx + rw - 4.0;
                        let badge_y = ry + rh - 5.0;

                        let chip_fill = if is_dead_end { "var(--warn)" } else { "var(--glyph-bg)" };
                        let chip_ink = if is_dead_end { "#fff" } else { "var(--glyph-ink)" };

                        view! {
                            <g
                                class=group_class
                                tabindex="0"
                                role="button"
                                aria-label=aria_label
                                on:click=move |_ev| {
                                    selected.set(Some(node_id_click.clone()));
                                }
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                    let key = ev.key();
                                    if key == "Enter" || key == " " {
                                        ev.prevent_default();
                                        selected.set(Some(node_id_key.clone()));
                                    }
                                }
                            >
                                // Full tooltip text on the <g>
                                <title>{label.clone()}</title>
                                // Node background rect
                                <rect
                                    x=rx
                                    y=ry
                                    width=rw
                                    height=rh
                                    rx="6"
                                    ry="6"
                                    class="node-bg"
                                />
                                // Glyph chip background
                                <rect
                                    x=chip_x
                                    y=chip_y
                                    width=chip_size
                                    height=chip_size
                                    rx="4"
                                    ry="4"
                                    fill=chip_fill
                                />
                                // Glyph character
                                <text
                                    x=chip_x + chip_size / 2.0
                                    y=chip_y + chip_size / 2.0 + 5.0
                                    text-anchor="middle"
                                    fill=chip_ink
                                    font-size="11"
                                    font-weight="700"
                                    font-family="ui-monospace, monospace"
                                >
                                    {glyph.to_string()}
                                </text>
                                // 2-line clamped label via foreignObject + XHTML div
                                <foreignObject x=fo_x y=fo_y width=fo_w height=fo_h>
                                    <div class="node-label">
                                        {label.clone()}
                                    </div>
                                </foreignObject>
                                // Kind badge (bottom-right)
                                <text
                                    x=badge_x
                                    y=badge_y
                                    text-anchor="end"
                                    fill="var(--muted)"
                                    font-size="9"
                                    font-family="ui-sans-serif, system-ui, sans-serif"
                                >
                                    {badge}
                                </text>
                            </g>
                        }
                    })
                    .collect_view()}
            </g>
        </svg>
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::parse_manifest;
    use ara_core::{Link, Manifest, Node, NodeFields, NodeKind};

    fn checked_in_manifest() -> Manifest {
        let json = include_str!("../public/manifest.json");
        parse_manifest(json).expect("checked-in manifest.json must parse")
    }

    // ── scene compute from the checked-in manifest ───────────────────────────

    #[test]
    fn scene_node_count_matches_nodes_with_pos() {
        let manifest = checked_in_manifest();
        let renderer = SvgRenderer;
        let scene = renderer.scene(&manifest, &LayoutView::default());

        // All 15 nodes in the demo have positions, so we expect 15 SceneNodes.
        let expected = manifest.nodes.iter().filter(|n| n.pos.is_some()).count();
        assert_eq!(
            scene.nodes.len(),
            expected,
            "scene node count must equal nodes-with-pos count"
        );
    }

    #[test]
    fn scene_node_rects_are_centered_on_pos() {
        let manifest = checked_in_manifest();
        let view = LayoutView::default();
        let renderer = SvgRenderer;
        let scene = renderer.scene(&manifest, &view);

        for (scene_node, manifest_node) in scene
            .nodes
            .iter()
            .zip(manifest.nodes.iter().filter(|n| n.pos.is_some()))
        {
            let pos = manifest_node.pos.unwrap();
            let expected_x = pos.x - view.node_width / 2.0;
            let expected_y = pos.y - view.node_height / 2.0;
            assert!(
                (scene_node.rect.x - expected_x).abs() < 1e-9,
                "rect.x mismatch for {}: {} vs {}",
                scene_node.id,
                scene_node.rect.x,
                expected_x
            );
            assert!(
                (scene_node.rect.y - expected_y).abs() < 1e-9,
                "rect.y mismatch for {}: {} vs {}",
                scene_node.id,
                scene_node.rect.y,
                expected_y
            );
            assert!((scene_node.rect.width - view.node_width).abs() < 1e-9);
            assert!((scene_node.rect.height - view.node_height).abs() < 1e-9);
        }
    }

    #[test]
    fn scene_node_glyph_and_css_match_kind_meta() {
        let manifest = checked_in_manifest();
        let renderer = SvgRenderer;
        let scene = renderer.scene(&manifest, &LayoutView::default());

        for sn in &scene.nodes {
            let meta = kind_meta(&sn.kind);
            assert_eq!(sn.glyph, meta.glyph, "glyph mismatch for {}", sn.id);
            assert_eq!(
                sn.css_class, meta.css_class,
                "css_class mismatch for {}",
                sn.id
            );
        }
    }

    // ── edge derive ──────────────────────────────────────────────────────────

    #[test]
    fn child_link_produces_child_edge() {
        let manifest = checked_in_manifest();
        let renderer = SvgRenderer;
        let scene = renderer.scene(&manifest, &LayoutView::default());

        let child_edges: Vec<_> = scene
            .edges
            .iter()
            .filter(|e| e.link_kind == LinkKind::Child)
            .collect();
        assert!(
            !child_edges.is_empty(),
            "demo manifest must produce at least one Child edge"
        );
        // Each child edge path must start with "M" and contain "L".
        for e in &child_edges {
            assert!(
                e.path.starts_with('M'),
                "edge path must start with M: {}",
                e.path
            );
            assert!(e.path.contains('L'), "edge path must contain L: {}", e.path);
        }
    }

    #[test]
    fn depends_on_link_produces_depends_on_edge() {
        let manifest = checked_in_manifest();
        let renderer = SvgRenderer;
        let scene = renderer.scene(&manifest, &LayoutView::default());

        let depends_edges: Vec<_> = scene
            .edges
            .iter()
            .filter(|e| e.link_kind == LinkKind::DependsOn)
            .collect();
        assert!(
            !depends_edges.is_empty(),
            "demo manifest must produce at least one DependsOn edge"
        );
    }

    #[test]
    fn child_edge_path_references_endpoint_coords() {
        let manifest = checked_in_manifest();
        let view = LayoutView::default();
        let renderer = SvgRenderer;
        let scene = renderer.scene(&manifest, &view);

        // Find N01→N02 (first child link in the demo).
        let edge = scene
            .edges
            .iter()
            .find(|e| e.from.as_str() == "N01" && e.to.as_str() == "N02")
            .expect("N01→N02 Child edge must exist");

        // N01 pos = (560, 30), N02 pos = (90, 140) per manifest.json.
        assert!(
            edge.path.contains("560"),
            "path must reference N01 x=560: {}",
            edge.path
        );
        assert!(
            edge.path.contains("140"),
            "path must reference N02 y=140: {}",
            edge.path
        );
    }

    // ── pos:None skip — no panic ─────────────────────────────────────────────

    #[test]
    fn node_without_pos_is_skipped_no_panic() {
        let mut manifest = checked_in_manifest();
        // Remove pos from N02.
        let n02 = manifest
            .nodes
            .iter_mut()
            .find(|n| n.id.as_str() == "N02")
            .expect("N02 must exist");
        n02.pos = None;

        let renderer = SvgRenderer;
        let scene = renderer.scene(&manifest, &LayoutView::default());

        // N02 must not appear in scene.nodes.
        assert!(
            scene.nodes.iter().all(|sn| sn.id.as_str() != "N02"),
            "N02 with pos=None must be absent from scene.nodes"
        );
        // Any edge touching N02 must also be absent.
        assert!(
            scene
                .edges
                .iter()
                .all(|e| e.from.as_str() != "N02" && e.to.as_str() != "N02"),
            "edges touching N02 must be absent when N02 has no pos"
        );
    }

    #[test]
    fn link_to_unknown_id_is_skipped_no_panic() {
        let mut manifest = checked_in_manifest();
        // Inject a link to a non-existent node.
        manifest.links.push(Link {
            from: NodeId::new("N01"),
            to: NodeId::new("N99"),
            kind: LinkKind::Child,
        });

        let renderer = SvgRenderer;
        // Must not panic.
        let scene = renderer.scene(&manifest, &LayoutView::default());

        // The bogus edge must be absent.
        assert!(
            scene.edges.iter().all(|e| e.to.as_str() != "N99"),
            "edge to unknown N99 must be skipped"
        );
    }

    #[test]
    fn small_manifest_node_without_pos_and_link_to_it_no_panic() {
        let manifest = Manifest {
            nodes: vec![
                Node {
                    id: NodeId::new("N01"),
                    kind: NodeKind::Question,
                    label: Some("Q".into()),
                    support_level: None,
                    source_refs: vec![],
                    description: None,
                    fields: NodeFields::Question,
                    evidence_notes: vec![],
                    isolated: false,
                    pos: Some(ara_core::Point { x: 90.0, y: 30.0 }),
                },
                Node {
                    id: NodeId::new("N02"),
                    kind: NodeKind::Experiment,
                    label: None,
                    support_level: None,
                    source_refs: vec![],
                    description: None,
                    fields: NodeFields::Experiment { result: None },
                    evidence_notes: vec![],
                    isolated: false,
                    pos: None, // <-- no pos
                },
            ],
            links: vec![Link {
                from: NodeId::new("N01"),
                to: NodeId::new("N02"),
                kind: LinkKind::Child,
            }],
            bindings: vec![],
            claims: vec![],
            bounds: None,
        };

        let renderer = SvgRenderer;
        let scene = renderer.scene(&manifest, &LayoutView::default());

        assert_eq!(scene.nodes.len(), 1, "only N01 (with pos) appears");
        assert!(scene.edges.is_empty(), "edge to posless N02 is skipped");
    }

    // ── bounds fallback ──────────────────────────────────────────────────────

    #[test]
    fn bounds_fallback_when_manifest_has_none() {
        let manifest = Manifest {
            nodes: vec![],
            links: vec![],
            bindings: vec![],
            claims: vec![],
            bounds: None,
        };

        let renderer = SvgRenderer;
        let scene = renderer.scene(&manifest, &LayoutView::default());

        assert!(
            scene.bounds.width > 0.0 && scene.bounds.height > 0.0,
            "fallback bounds must have positive extent"
        );
        // Must be the (0,0,100,100) fallback.
        assert_eq!(scene.bounds.x, 0.0);
        assert_eq!(scene.bounds.y, 0.0);
        assert_eq!(scene.bounds.width, 100.0);
        assert_eq!(scene.bounds.height, 100.0);
    }

    #[test]
    fn bounds_from_checked_in_manifest_are_not_fallback() {
        let manifest = checked_in_manifest();
        let renderer = SvgRenderer;
        let scene = renderer.scene(&manifest, &LayoutView::default());

        // The demo manifest bounds are ~1547×390, not the fallback.
        assert!(
            scene.bounds.width > 100.0,
            "real manifest bounds must be larger than the fallback"
        );
    }
}
