//! Deterministic layered DAG layout (Sugiyama method) for `Manifest`.
//!
//! Produces node positions + bounding rect via `dagre-dgl-rs`. All computation
//! is pure and wasm-safe (no threads, filesystem, randomness, or `SystemTime`).
//! The same input yields byte-identical JSON on native and wasm32 targets.

use dagre_dgl_rs::{EdgeLabel, Graph, GraphLabel, NodeLabel as DagreNodeLabel};

use std::collections::BTreeMap;

use crate::manifest::{Link, Manifest, Node, NodeId};

/// A 2D point (center of a node).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// An axis-aligned rectangle (bounding box).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Configuration knobs for layout. All values are pinned for determinism.
#[derive(Debug, Clone)]
pub struct LayoutOptions {
    /// Width of every node box (px). Default: 180.
    pub node_width: f64,
    /// Height of every node box (px). Default: 60.
    pub node_height: f64,
    /// Minimum separation between adjacent nodes in the same rank. Default: 50.
    pub node_sep: f64,
    /// Minimum separation between adjacent ranks. Default: 50.
    pub rank_sep: f64,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            node_width: 180.0,
            node_height: 60.0,
            node_sep: 50.0,
            rank_sep: 50.0,
        }
    }
}

/// Result of running layout on a manifest.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LayoutResult {
    pub positions: Vec<NodePosition>,
    pub bounds: Rect,
}

/// The computed position for a single node.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NodePosition {
    pub id: NodeId,
    pub pos: Point,
}

/// Computes a layered DAG layout for `manifest` with the given options.
///
/// Nodes are inserted in sorted `NodeId` order (the fixed tie-break) so
/// equal-rank ordering is stable regardless of input shuffling.
///
/// # Panics
///
/// Debug-asserts that the graph has no cycles (the parse layer already
/// rejects cycles, so this is a defensive invariant check).
pub fn layout(manifest: &Manifest, opts: &LayoutOptions) -> LayoutResult {
    if manifest.nodes.is_empty() {
        return LayoutResult {
            positions: Vec::new(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
        };
    }

    let mut g = Graph::default();
    g.set_graph(GraphLabel {
        rankdir: Some("TB".to_string()),
        nodesep: Some(opts.node_sep),
        ranksep: Some(opts.rank_sep),
        ..Default::default()
    });

    // Insert nodes in sorted NodeId order for a stable tie-break.
    let mut sorted_ids: Vec<&NodeId> = manifest.nodes.iter().map(|n| &n.id).collect();
    sorted_ids.sort();
    for id in &sorted_ids {
        g.set_node(
            id.as_str(),
            DagreNodeLabel {
                width: opts.node_width,
                height: opts.node_height,
                ..Default::default()
            },
        );
    }

    // Add edges. Only Child + DependsOn links exist; both are used for ranking.
    debug_assert!(
        !has_cycle(&manifest.nodes, &manifest.links),
        "cycle reached layout — parse should have rejected this"
    );
    for link in &manifest.links {
        g.set_edge(
            link.from.as_str(),
            link.to.as_str(),
            EdgeLabel::default(),
            None,
        );
    }

    dagre_dgl_rs::layout(&mut g);

    // Extract positions and compute bounds.
    let mut positions = Vec::with_capacity(manifest.nodes.len());
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for node in &manifest.nodes {
        let nl = g.node(node.id.as_str());
        let x = canonicalize(nl.x.unwrap_or(0.0));
        let y = canonicalize(nl.y.unwrap_or(0.0));
        positions.push(NodePosition {
            id: node.id.clone(),
            pos: Point { x, y },
        });

        let half_w = opts.node_width / 2.0;
        let half_h = opts.node_height / 2.0;
        min_x = min_x.min(x - half_w);
        min_y = min_y.min(y - half_h);
        max_x = max_x.max(x + half_w);
        max_y = max_y.max(y + half_h);
    }

    let bounds = Rect {
        x: canonicalize(min_x),
        y: canonicalize(min_y),
        width: canonicalize(max_x - min_x),
        height: canonicalize(max_y - min_y),
    };

    LayoutResult { positions, bounds }
}

/// Canonicalize an f64: round to 6 decimal places and normalize -0.0 to 0.0.
fn canonicalize(v: f64) -> f64 {
    let rounded = (v * 1_000_000.0).round() / 1_000_000.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// Cheap cycle check (DFS three-color). Only used in debug_assert.
fn has_cycle(nodes: &[Node], links: &[Link]) -> bool {
    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for link in links {
        adj.entry(link.from.as_str())
            .or_default()
            .push(link.to.as_str());
    }
    let mut color: BTreeMap<&str, u8> = BTreeMap::new();
    for node in nodes {
        if color.get(node.id.as_str()).copied().unwrap_or(0) == 0
            && visit_cycle(node.id.as_str(), &adj, &mut color)
        {
            return true;
        }
    }
    false
}

fn visit_cycle<'a>(
    u: &'a str,
    adj: &BTreeMap<&'a str, Vec<&'a str>>,
    color: &mut BTreeMap<&'a str, u8>,
) -> bool {
    color.insert(u, 1);
    if let Some(neighbors) = adj.get(u) {
        for &v in neighbors {
            match color.get(v).copied().unwrap_or(0) {
                0 => {
                    if visit_cycle(v, adj, color) {
                        return true;
                    }
                }
                1 => return true,
                _ => {}
            }
        }
    }
    color.insert(u, 2);
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Link, LinkKind, Manifest, Node, NodeFields, NodeId, NodeKind};

    fn simple_manifest() -> Manifest {
        Manifest {
            nodes: vec![
                Node {
                    id: NodeId::new("N01"),
                    kind: NodeKind::Question,
                    label: Some("Q?".into()),
                    support_level: None,
                    source_refs: vec![],
                    description: None,
                    fields: NodeFields::Question,
                    evidence_notes: vec![],
                    isolated: false,
                    pos: None,
                },
                Node {
                    id: NodeId::new("N02"),
                    kind: NodeKind::Experiment,
                    label: Some("Exp".into()),
                    support_level: None,
                    source_refs: vec![],
                    description: None,
                    fields: NodeFields::Experiment { result: None },
                    evidence_notes: vec![],
                    isolated: false,
                    pos: None,
                },
                Node {
                    id: NodeId::new("N03"),
                    kind: NodeKind::Decision,
                    label: Some("Dec".into()),
                    support_level: None,
                    source_refs: vec![],
                    description: None,
                    fields: NodeFields::Decision {
                        choice: None,
                        alternatives: vec![],
                        rationale: None,
                    },
                    evidence_notes: vec![],
                    isolated: false,
                    pos: None,
                },
            ],
            links: vec![
                Link {
                    from: NodeId::new("N01"),
                    to: NodeId::new("N02"),
                    kind: LinkKind::Child,
                },
                Link {
                    from: NodeId::new("N01"),
                    to: NodeId::new("N03"),
                    kind: LinkKind::Child,
                },
            ],
            bindings: vec![],
            claims: vec![],
            bounds: None,
            paper: None,
            related_work: vec![],
            concepts: vec![],
            problem: None,
            recipes: vec![],
            exhibits: vec![],
            built_on: vec![],
            node_exhibits: vec![],
        }
    }

    #[test]
    fn all_positions_finite() {
        let m = simple_manifest();
        let result = layout(&m, &LayoutOptions::default());
        for np in &result.positions {
            assert!(np.pos.x.is_finite(), "NaN/inf x for {}", np.id);
            assert!(np.pos.y.is_finite(), "NaN/inf y for {}", np.id);
        }
    }

    #[test]
    fn ranks_monotonic_along_child_edges() {
        let m = simple_manifest();
        let result = layout(&m, &LayoutOptions::default());
        let pos_map: std::collections::HashMap<&str, &Point> = result
            .positions
            .iter()
            .map(|np| (np.id.as_str(), &np.pos))
            .collect();
        for link in &m.links {
            if link.kind == LinkKind::Child {
                let from_y = pos_map[link.from.as_str()].y;
                let to_y = pos_map[link.to.as_str()].y;
                assert!(
                    from_y < to_y,
                    "rank not monotonic: {} (y={}) -> {} (y={})",
                    link.from,
                    from_y,
                    link.to,
                    to_y
                );
            }
        }
    }

    #[test]
    fn tie_break_stable_across_input_order() {
        let m1 = simple_manifest();
        let mut m2 = simple_manifest();
        m2.nodes.reverse(); // shuffle input order
        let r1 = layout(&m1, &LayoutOptions::default());
        let r2 = layout(&m2, &LayoutOptions::default());
        // Positions are returned in manifest.nodes order, so sort by id.
        let mut p1: Vec<_> = r1.positions.clone();
        let mut p2: Vec<_> = r2.positions.clone();
        p1.sort_by(|a, b| a.id.cmp(&b.id));
        p2.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(p1, p2);
        assert_eq!(r1.bounds, r2.bounds);
    }

    #[test]
    fn empty_manifest_produces_zero_bounds() {
        let m = Manifest {
            nodes: vec![],
            links: vec![],
            bindings: vec![],
            claims: vec![],
            bounds: None,
            paper: None,
            related_work: vec![],
            concepts: vec![],
            problem: None,
            recipes: vec![],
            exhibits: vec![],
            built_on: vec![],
            node_exhibits: vec![],
        };
        let result = layout(&m, &LayoutOptions::default());
        assert!(result.positions.is_empty());
        assert_eq!(result.bounds.width, 0.0);
        assert_eq!(result.bounds.height, 0.0);
    }

    #[test]
    fn single_node_has_finite_pos_and_enclosing_bounds() {
        let m = Manifest {
            nodes: vec![Node {
                id: NodeId::new("N01"),
                kind: NodeKind::Question,
                label: None,
                support_level: None,
                source_refs: vec![],
                description: None,
                fields: NodeFields::Question,
                evidence_notes: vec![],
                isolated: false,
                pos: None,
            }],
            links: vec![],
            bindings: vec![],
            claims: vec![],
            bounds: None,
            paper: None,
            related_work: vec![],
            concepts: vec![],
            problem: None,
            recipes: vec![],
            exhibits: vec![],
            built_on: vec![],
            node_exhibits: vec![],
        };
        let opts = LayoutOptions::default();
        let result = layout(&m, &opts);
        assert_eq!(result.positions.len(), 1);
        assert!(result.positions[0].pos.x.is_finite());
        assert!(result.positions[0].pos.y.is_finite());
        assert!(result.bounds.width >= opts.node_width);
        assert!(result.bounds.height >= opts.node_height);
    }

    #[test]
    fn bounds_enclose_all_node_rects() {
        let m = simple_manifest();
        let opts = LayoutOptions::default();
        let result = layout(&m, &opts);
        let half_w = opts.node_width / 2.0;
        let half_h = opts.node_height / 2.0;
        for np in &result.positions {
            assert!(np.pos.x - half_w >= result.bounds.x - 1e-9);
            assert!(np.pos.y - half_h >= result.bounds.y - 1e-9);
            assert!(np.pos.x + half_w <= result.bounds.x + result.bounds.width + 1e-9);
            assert!(np.pos.y + half_h <= result.bounds.y + result.bounds.height + 1e-9);
        }
    }

    #[test]
    fn canonicalize_normalizes_negative_zero() {
        assert_eq!(canonicalize(-0.0), 0.0);
        assert_eq!(canonicalize(-0.0).to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn canonicalize_rounds_to_six_decimals() {
        let v = 1.23456789;
        assert_eq!(canonicalize(v), 1.234568);
    }

    #[test]
    fn layout_twice_identical() {
        let m = simple_manifest();
        let opts = LayoutOptions::default();
        let r1 = layout(&m, &opts);
        let r2 = layout(&m, &opts);
        let j1 = serde_json::to_string(&r1).unwrap();
        let j2 = serde_json::to_string(&r2).unwrap();
        assert_eq!(j1, j2);
    }
}
