// Browser-test layer for ara-viewer.
//
// Gated to wasm32 so `cargo test --workspace` (native host build) skips this
// file entirely.  The CI job `viewer-web-test` runs these via:
//   wasm-pack test --headless --chrome crates/ara-viewer
//
// Mounting strategy: we use `leptos::mount::mount_to` to mount sub-components
// (GraphView, DetailPane) directly with in-test signals and a synthetic
// manifest, rather than mounting the full App.  This avoids the fetch-on-mount
// in App (which would 404 in the test harness and stay Loading forever).
//
// Manifest construction: we parse a small JSON string via
// `ara_viewer::state::parse_manifest` — the simplest path that exercises the
// full code path and lets us control every field.
#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

use std::collections::HashSet;

use ara_viewer::{
    PaperHeader,
    deps::DependenciesPanel,
    detail::DetailPane,
    modal::Modal,
    panels::{ContextPanel, GlossaryPanel, RecipesPanel},
    replay::{ReplayBar, ReplayState, install_arrow_key_listener, node_order},
    scene::{GraphRenderer, GraphView, LayoutView, SvgRenderer},
    source::ws_url_from_base,
    state::{DisplayMode, LayoutMode, LoadState, PanZoom, parse_manifest},
    toolbar::{DisplayToggle, LayoutToggle},
    tree::{TreeView, tree_model},
};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlElement};

// ── Manifest JSON fixture ─────────────────────────────────────────────────────
//
// Covers:
//  N01  Decision  — choice + rationale + alternatives + description
//  N02  DeadEnd   — why_failed (primary field); is_dead_end == true
//  N03  Question  — only id/kind (no description, no fields) → "Nothing recorded"
//  N04  Insight   — description only, no typed fields
//  C01  Claim     — bound to N04 via B01; status "supported"
//
// All nodes carry `pos` so they appear in the scene.
// N01 → N02 via DependsOn; N01 → N03 via Child.
const FIXTURE_JSON: &str = r#"{
  "nodes": [
    {
      "id": "N01",
      "kind": "decision",
      "label": "Use sinusoidal encoding",
      "description": "Decision about positional encoding strategy.",
      "source_refs": [],
      "evidence_notes": [],
      "fields": {
        "decision": {
          "choice": "sinusoidal",
          "alternatives": ["learned", "relative"],
          "rationale": "Better on long sequences."
        }
      },
      "pos": { "x": 100.0, "y": 100.0 }
    },
    {
      "id": "N02",
      "kind": "dead_end",
      "label": "Gradient collapse",
      "description": "This path failed.",
      "source_refs": [],
      "evidence_notes": [],
      "fields": {
        "dead_end": {
          "why_failed": "Gradients vanished at depth 12."
        }
      },
      "pos": { "x": 300.0, "y": 100.0 }
    },
    {
      "id": "N03",
      "kind": "question",
      "source_refs": [],
      "evidence_notes": [],
      "fields": "question",
      "isolated": false,
      "pos": { "x": 100.0, "y": 300.0 }
    },
    {
      "id": "N04",
      "kind": "insight",
      "label": "Attention is all you need",
      "description": "Core insight of the transformer.",
      "source_refs": [],
      "evidence_notes": [],
      "fields": "insight",
      "pos": { "x": 300.0, "y": 300.0 }
    }
  ],
  "links": [
    { "from": "N01", "to": "N02", "kind": "depends_on" },
    { "from": "N01", "to": "N03", "kind": "child" }
  ],
  "bindings": [
    { "node": "N04", "claim": "C01", "role": "evidence" }
  ],
  "claims": [
    {
      "id": "C01",
      "title": "Transformer convergence",
      "statement": "The model converges in 50 epochs.",
      "status": "supported",
      "proof": [],
      "deps": []
    }
  ],
  "bounds": { "x": 0.0, "y": 0.0, "width": 500.0, "height": 500.0 }
}"#;

// ── Helper: create a div attached to document.body ────────────────────────────

fn body_div(doc: &Document) -> HtmlElement {
    let div = doc.create_element("div").unwrap();
    doc.body().unwrap().append_child(&div).unwrap();
    div.unchecked_into::<HtmlElement>()
}

// ── Test: live-socket URL resolves relative to the document base (D1) ─────────
//
// The load-bearing hub assumption: the viewer's relative `api/live` must resolve
// against the page base. This test drives `ws_url_from_base` with a synthetic
// base for BOTH the local-serve root (`/`) and the hub sub-path (`/a/{id}/`), so
// a regression in the relative resolution or the http→ws scheme swap is caught
// without a live server. A silent break here would make local live-reload fail
// invisibly (the socket error is intentionally swallowed as "static host").

#[wasm_bindgen_test]
fn ws_url_resolves_relative_to_document_base() {
    // Local serve: page at origin root → api/live is same as an absolute
    // /api/live. http base → ws scheme.
    assert_eq!(
        ws_url_from_base("http://localhost:8080/", "api/live").as_deref(),
        Some("ws://localhost:8080/api/live"),
        "root base must resolve api/live to /api/live (local serve unchanged)"
    );

    // Hub: page at /a/{id}/ → api/live resolves under that sub-path.
    assert_eq!(
        ws_url_from_base("http://example.com/a/resnet/", "api/live").as_deref(),
        Some("ws://example.com/a/resnet/api/live"),
        "sub-path base must resolve api/live under /a/{{id}}/ (hub)"
    );

    // https base → wss scheme, sub-path preserved.
    assert_eq!(
        ws_url_from_base("https://example.com/a/resnet/", "api/live").as_deref(),
        Some("wss://example.com/a/resnet/api/live"),
        "https base must swap to wss and keep the sub-path"
    );
}

// ── Test: relative fetch resolves under <base href> in a real browser (D1) ────
//
// The ONE test that proves D1's load-bearing assumption in an actual browser:
// with `<base href="/a/x/">` in the document head, a relative `api/manifest`
// (the viewer's default manifest URL) must resolve to `/a/x/api/manifest`. The
// native string test and the ws_url_from_base test cover the logic; only this
// exercises the browser's real `<base>` + URL resolution the viewer relies on.

#[wasm_bindgen_test]
fn base_href_makes_relative_fetch_resolve_under_subpath() {
    let doc = web_sys::window().unwrap().document().unwrap();

    // Inject <base href="/a/x/"> into the document head, as the hub does.
    let head = doc.head().expect("document must have a <head>");
    let base = doc.create_element("base").unwrap();
    base.set_attribute("href", "/a/x/").unwrap();
    head.append_child(&base).unwrap();

    // The viewer's default manifest URL is the relative "api/manifest".
    let manifest_url = match ara_viewer::source::ManifestSource::default() {
        ara_viewer::source::ManifestSource::Api { manifest_url, .. } => manifest_url,
        _ => panic!("default must be the Api variant"),
    };

    // Resolve it exactly as the browser's fetch would: against document.baseURI.
    let base_uri = doc.base_uri().unwrap().expect("baseURI present");
    let resolved = web_sys::Url::new_with_base(&manifest_url, &base_uri).unwrap();
    assert_eq!(
        resolved.pathname(),
        "/a/x/api/manifest",
        "relative api/manifest must resolve under the injected <base href>"
    );

    // Clean up so the base tag doesn't leak into sibling tests.
    head.remove_child(&base).unwrap();
}

// ── Test: node count equals nodes-with-pos count ──────────────────────────────

/// Mounts GraphView with the fixture manifest and asserts that the number of
/// rendered `<g class="node …">` elements equals the number of nodes with `pos`
/// (all 4 in the fixture).
#[wasm_bindgen_test]
fn graph_view_node_count_equals_nodes_with_pos() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(FIXTURE_JSON).expect("fixture must parse");
    let renderer = SvgRenderer;
    let scene = renderer.scene(&manifest, &LayoutView::default());

    let expected_count = manifest.nodes.iter().filter(|n| n.pos.is_some()).count();
    assert_eq!(expected_count, 4, "fixture has 4 nodes with pos");

    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let pan_zoom: RwSignal<PanZoom> = RwSignal::new(PanZoom::default());
    let all_ids: HashSet<ara_core::NodeId> = manifest.nodes.iter().map(|n| n.id.clone()).collect();
    let matching = Memo::new(move |_| all_ids.clone());

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <GraphView scene=scene selected=selected pan_zoom=pan_zoom matching=matching /> }
    });

    // Query all <g> elements that carry the "node" CSS class.
    let svg = container
        .query_selector("svg.graph-svg")
        .unwrap()
        .expect("graph-svg must be present");
    let node_gs = svg.query_selector_all("g[role='button']").unwrap();
    assert_eq!(
        node_gs.length(),
        expected_count as u32,
        "rendered node <g> count must equal nodes-with-pos count"
    );
}

// ── Test: dead_end node carries expected class and chip colour ────────────────

/// N02 is a DeadEnd.  Its `<g>` must contain "dead_end" in its class, and the
/// chip fill rect must use "var(--warn)" (checked via the `fill` attribute, not
/// computed style, which is flaky in headless).
#[wasm_bindgen_test]
fn dead_end_node_has_dead_end_class() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(FIXTURE_JSON).expect("fixture must parse");
    let renderer = SvgRenderer;
    let scene = renderer.scene(&manifest, &LayoutView::default());

    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let pan_zoom: RwSignal<PanZoom> = RwSignal::new(PanZoom::default());
    let all_ids: HashSet<ara_core::NodeId> = manifest.nodes.iter().map(|n| n.id.clone()).collect();
    let matching = Memo::new(move |_| all_ids.clone());

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <GraphView scene=scene selected=selected pan_zoom=pan_zoom matching=matching /> }
    });

    // Find all node <g> elements and locate the one for N02 by aria-label
    // containing "Gradient collapse" (the label field of N02).
    let svg = container
        .query_selector("svg.graph-svg")
        .unwrap()
        .expect("graph-svg must be present");

    // N02's <g> aria-label is "Gradient collapse, dead end"
    let dead_end_g = svg
        .query_selector("g[aria-label*='Gradient collapse']")
        .unwrap()
        .expect("dead_end node g must be present");

    // NB: `<g>` is an SVG element, whose `.className` is an `SVGAnimatedString`
    // object, not a string — `Element::class_name()` would throw. Read the raw
    // `class` attribute instead.
    let class = dead_end_g.get_attribute("class").unwrap_or_default();
    assert!(
        class.contains("dead_end"),
        "dead_end node <g> class must contain 'dead_end', got: {class}"
    );
}

// ── Test: DependsOn edge carries edge-depends class ───────────────────────────

/// N01 → N02 is a DependsOn link.  The rendered `<path>` must carry the
/// "edge-depends" CSS class.  N01 → N03 is Child → "edge-child".
#[wasm_bindgen_test]
fn edge_classes_match_link_kind() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(FIXTURE_JSON).expect("fixture must parse");
    let renderer = SvgRenderer;
    let scene = renderer.scene(&manifest, &LayoutView::default());

    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let pan_zoom: RwSignal<PanZoom> = RwSignal::new(PanZoom::default());
    let all_ids: HashSet<ara_core::NodeId> = manifest.nodes.iter().map(|n| n.id.clone()).collect();
    let matching = Memo::new(move |_| all_ids.clone());

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <GraphView scene=scene selected=selected pan_zoom=pan_zoom matching=matching /> }
    });

    let svg = container
        .query_selector("svg.graph-svg")
        .unwrap()
        .expect("graph-svg must be present");

    let depends_edges = svg.query_selector_all("path.edge-depends").unwrap();
    assert!(
        depends_edges.length() >= 1,
        "at least one edge-depends path must exist (N01→N02)"
    );

    let child_edges = svg.query_selector_all("path.edge-child").unwrap();
    assert!(
        child_edges.length() >= 1,
        "at least one edge-child path must exist (N01→N03)"
    );
}

// ── Test: node <g> a11y attributes ───────────────────────────────────────────

/// Every node `<g>` must have `tabindex="0"`, `role="button"`, and a non-empty
/// `aria-label`.
#[wasm_bindgen_test]
fn node_g_has_a11y_attributes() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(FIXTURE_JSON).expect("fixture must parse");
    let renderer = SvgRenderer;
    let scene = renderer.scene(&manifest, &LayoutView::default());

    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let pan_zoom: RwSignal<PanZoom> = RwSignal::new(PanZoom::default());
    let all_ids: HashSet<ara_core::NodeId> = manifest.nodes.iter().map(|n| n.id.clone()).collect();
    let matching = Memo::new(move |_| all_ids.clone());

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <GraphView scene=scene selected=selected pan_zoom=pan_zoom matching=matching /> }
    });

    let svg = container
        .query_selector("svg.graph-svg")
        .unwrap()
        .expect("graph-svg must be present");

    let node_gs = svg.query_selector_all("g[role='button']").unwrap();
    assert!(node_gs.length() > 0, "must have at least one node <g>");

    for i in 0..node_gs.length() {
        let g = node_gs.item(i).unwrap();
        let el = g.dyn_ref::<web_sys::Element>().unwrap();

        let tabindex = el.get_attribute("tabindex").unwrap_or_default();
        assert_eq!(
            tabindex, "0",
            "node <g> [{i}] must have tabindex='0', got: {tabindex:?}"
        );

        let aria_label = el.get_attribute("aria-label").unwrap_or_default();
        assert!(
            !aria_label.is_empty(),
            "node <g> [{i}] must have non-empty aria-label"
        );
    }
}

// ── Test: click node → detail pane shows node content ────────────────────────

/// Clicking a node <g> sets `selected` signal → the DetailPane renders that
/// node's content.  We verify that after a click on N01's <g>, the detail pane
/// shows "Use sinusoidal encoding" (N01's label).
#[wasm_bindgen_test]
async fn click_node_updates_detail_pane() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let graph_container = body_div(&doc);
    let detail_container = body_div(&doc);

    let manifest = parse_manifest(FIXTURE_JSON).expect("fixture must parse");
    let renderer = SvgRenderer;
    let scene = renderer.scene(&manifest, &LayoutView::default());

    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let pan_zoom: RwSignal<PanZoom> = RwSignal::new(PanZoom::default());
    let all_ids: HashSet<ara_core::NodeId> = manifest.nodes.iter().map(|n| n.id.clone()).collect();
    let matching = Memo::new(move |_| all_ids.clone());

    let manifest_clone = manifest.clone();

    // Mount the graph
    let _gh = leptos::mount::mount_to(graph_container.clone(), move || {
        view! { <GraphView scene=scene selected=selected pan_zoom=pan_zoom matching=matching /> }
    });

    // Mount the detail pane: inject the manifest directly via LoadState::Loaded
    let (load_state, _set_ls) = signal(LoadState::Loaded(manifest_clone));
    let _dh = leptos::mount::mount_to(detail_container.clone(), move || {
        view! { <DetailPane load_state=load_state selected=selected /> }
    });

    // Before click: detail pane shows placeholder
    let detail_text = detail_container.inner_text();
    assert!(
        detail_text.contains("Select a step"),
        "detail pane before selection must show placeholder, got: {detail_text:?}"
    );

    // Click the <g> for N01 (aria-label contains "Use sinusoidal encoding")
    let svg = graph_container
        .query_selector("svg.graph-svg")
        .unwrap()
        .expect("graph-svg must be present");
    let n01_g = svg
        .query_selector("g[aria-label*='Use sinusoidal encoding']")
        .unwrap()
        .expect("N01 node g must be present");

    // `<g>` is an SVG element, not an `HtmlElement`, so `.click()` is
    // unavailable. Dispatch a synthetic click that *bubbles* — Leptos 0.8
    // delegates `on:click` to the mount root, so a non-bubbling event would
    // never reach the handler.
    let init = web_sys::MouseEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    let click_ev = web_sys::MouseEvent::new_with_mouse_event_init_dict("click", &init).unwrap();
    n01_g.dispatch_event(&click_ev).unwrap();

    // Leptos 0.8 flushes reactive effects on the async executor's next tick, so
    // the detail pane's DOM is not updated synchronously with the signal set.
    // Yield one tick before reading it back.
    leptos::task::tick().await;

    // After click: detail pane must show N01's title
    let detail_text_after = detail_container.inner_text();
    assert!(
        detail_text_after.contains("Use sinusoidal encoding"),
        "detail pane after N01 click must show N01's label, got: {detail_text_after:?}"
    );
}

// ── Test: Decision node detail hierarchy (choice → rationale → alternatives) ──

/// For N01 (a Decision), the detail pane must render block-labels in order:
/// "choice" appears before "rationale" appears before "alternatives".
#[wasm_bindgen_test]
fn decision_detail_hierarchy_order() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(FIXTURE_JSON).expect("fixture must parse");
    let selected: RwSignal<Option<ara_core::NodeId>> =
        RwSignal::new(Some(ara_core::NodeId::new("N01")));
    let (load_state, _) = signal(LoadState::Loaded(manifest));

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DetailPane load_state=load_state selected=selected /> }
    });

    let text = container.inner_text();
    let pos_choice = text
        .find("choice")
        .expect("'choice' block-label must appear");
    let pos_rationale = text
        .find("rationale")
        .expect("'rationale' block-label must appear");
    let pos_alternatives = text
        .find("alternatives")
        .expect("'alternatives' block-label must appear");

    assert!(
        pos_choice < pos_rationale,
        "choice must appear before rationale in detail pane"
    );
    assert!(
        pos_rationale < pos_alternatives,
        "rationale must appear before alternatives in detail pane"
    );
}

// ── Test: DeadEnd detail — why_failed appears first ───────────────────────────

/// For N02 (a DeadEnd), the detail pane must render "why failed" as the first
/// typed-field block (primary accent).
#[wasm_bindgen_test]
fn dead_end_detail_why_failed_is_primary() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(FIXTURE_JSON).expect("fixture must parse");
    let selected: RwSignal<Option<ara_core::NodeId>> =
        RwSignal::new(Some(ara_core::NodeId::new("N02")));
    let (load_state, _) = signal(LoadState::Loaded(manifest));

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DetailPane load_state=load_state selected=selected /> }
    });

    // The primary field gets class "block reason" in render_detail.
    let reason_block = container
        .query_selector("div.reason")
        .unwrap()
        .expect("dead_end node detail must have a .reason block");

    let block_text = reason_block
        .dyn_ref::<web_sys::HtmlElement>()
        .unwrap()
        .inner_text();
    assert!(
        block_text.contains("why failed"),
        "DeadEnd .reason block must contain 'why failed', got: {block_text:?}"
    );
    assert!(
        block_text.contains("Gradients vanished"),
        "DeadEnd .reason block must contain the why_failed value"
    );
}

// ── Test: bound claim renders title + status pill ─────────────────────────────

/// N04 has a binding to C01 ("Transformer convergence", status "supported").
/// The detail pane for N04 must render the claim title and a status pill
/// containing "supported".
#[wasm_bindgen_test]
fn bound_claim_renders_title_and_status_pill() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(FIXTURE_JSON).expect("fixture must parse");
    let selected: RwSignal<Option<ara_core::NodeId>> =
        RwSignal::new(Some(ara_core::NodeId::new("N04")));
    let (load_state, _) = signal(LoadState::Loaded(manifest));

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DetailPane load_state=load_state selected=selected /> }
    });

    let text = container.inner_text();
    assert!(
        text.contains("Transformer convergence"),
        "claim title must render, got: {text:?}"
    );

    // Status pill: class "status-pill status-supported"
    let pill = container
        .query_selector("span.status-supported")
        .unwrap()
        .expect("supported status pill must be present");
    let pill_text = pill.dyn_ref::<web_sys::HtmlElement>().unwrap().inner_text();
    assert!(
        pill_text.contains("supported"),
        "status pill must contain 'supported', got: {pill_text:?}"
    );
}

// ── Test: empty node (only id/kind) renders "Nothing recorded" ─────────────────

/// N03 has only `id` and `kind` (no description, no typed fields, no claims,
/// no source_refs).  The detail pane must render "Nothing recorded" without
/// panicking.
#[wasm_bindgen_test]
fn empty_node_renders_nothing_recorded() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(FIXTURE_JSON).expect("fixture must parse");
    let selected: RwSignal<Option<ara_core::NodeId>> =
        RwSignal::new(Some(ara_core::NodeId::new("N03")));
    let (load_state, _) = signal(LoadState::Loaded(manifest));

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DetailPane load_state=load_state selected=selected /> }
    });

    let text = container.inner_text();
    assert!(
        text.contains("Nothing recorded"),
        "empty node must render 'Nothing recorded', got: {text:?}"
    );
}

// ── Test: node with description but no typed fields ────────────────────────────

/// N04 (Insight) has a description but no typed fields.  The detail pane
/// must render the description text and NOT render any .block.reason element.
#[wasm_bindgen_test]
fn insight_node_shows_description_no_typed_fields() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(FIXTURE_JSON).expect("fixture must parse");
    let selected: RwSignal<Option<ara_core::NodeId>> =
        RwSignal::new(Some(ara_core::NodeId::new("N04")));
    let (load_state, _) = signal(LoadState::Loaded(manifest));

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DetailPane load_state=load_state selected=selected /> }
    });

    let text = container.inner_text();
    assert!(
        text.contains("Core insight of the transformer"),
        "description must render, got: {text:?}"
    );

    // No .reason block — Insight has no typed fields
    let reason_block = container.query_selector("div.reason").unwrap();
    assert!(
        reason_block.is_none(),
        "insight node detail must NOT have a .reason block"
    );
}

// ── Built-on / Result linkage fixture ─────────────────────────────────────────
//
// N01 (Experiment) carries an evidence note + a built_on edge (→ RW01) + a
// node_exhibit edge (→ E01). N02 (Insight) carries a description only, no
// linkage. The new manifest fields are serde-defaulted, so we include only what
// these tests exercise.
const LINKAGE_FIXTURE_JSON: &str = r#"{
  "nodes": [
    {
      "id": "N01",
      "kind": "experiment",
      "label": "Train the transformer",
      "description": "Ran the training experiment.",
      "source_refs": [],
      "evidence_notes": ["Logged in run 42"],
      "fields": { "experiment": { "result": "28.4 BLEU" } },
      "pos": { "x": 100.0, "y": 100.0 }
    },
    {
      "id": "N02",
      "kind": "insight",
      "label": "A bare insight",
      "description": "No linkage here.",
      "source_refs": [],
      "evidence_notes": [],
      "fields": "insight",
      "pos": { "x": 300.0, "y": 100.0 }
    }
  ],
  "links": [],
  "bindings": [],
  "claims": [],
  "related_work": [
    { "id": "RW01", "cite": "Vaswani et al., 2017", "doi": null, "kind": null,
      "what_changed": null, "why": null, "adopted": null, "claims_affected": [] }
  ],
  "exhibits": [
    { "id": "E01", "file": "evidence/fig1.md", "kind": "figure",
      "source": "Fig. 1", "description": null, "claims": [], "body": "" }
  ],
  "built_on": [
    { "node": "N01", "related_work": "RW01" }
  ],
  "node_exhibits": [
    { "node": "N01", "exhibit": "E01" }
  ],
  "bounds": { "x": 0.0, "y": 0.0, "width": 500.0, "height": 500.0 }
}"#;

// ── Test: built-on + result blocks render, in order after evidence ────────────

/// N01 has an evidence note, a built_on edge (→ RW01) and a node_exhibit
/// (→ E01). The detail pane must render a `.built-on-block` (with the RW chip)
/// and a `.result-block` (with the exhibit chip), in that DOM order after the
/// evidence block.
#[wasm_bindgen_test]
fn built_on_and_result_blocks_render_after_evidence() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(LINKAGE_FIXTURE_JSON).expect("linkage fixture must parse");
    let selected: RwSignal<Option<ara_core::NodeId>> =
        RwSignal::new(Some(ara_core::NodeId::new("N01")));
    let (load_state, _) = signal(LoadState::Loaded(manifest));

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DetailPane load_state=load_state selected=selected /> }
    });

    // Both new blocks are present.
    let built_on = container
        .query_selector("div.built-on-block")
        .unwrap()
        .expect("built-on-block must render")
        .dyn_into::<HtmlElement>()
        .unwrap();
    let result = container
        .query_selector("div.result-block")
        .unwrap()
        .expect("result-block must render")
        .dyn_into::<HtmlElement>()
        .unwrap();

    // Chips carry the resolved linkage.
    assert!(
        built_on.inner_text().contains("RW01") && built_on.inner_text().contains("Vaswani"),
        "built-on chip must show the RW id + cite, got: {:?}",
        built_on.inner_text()
    );
    assert!(
        result.inner_text().contains("E01"),
        "result chip must show the exhibit id, got: {:?}",
        result.inner_text()
    );

    // DOM order: evidence < built on < result.
    let text = container.inner_text();
    let pos_evidence = text.find("evidence").expect("'evidence' label must appear");
    let pos_built_on = text.find("built on").expect("'built on' label must appear");
    let pos_result = text.find("result").expect("'result' label must appear");
    assert!(
        pos_evidence < pos_built_on,
        "built-on block must appear after the evidence block"
    );
    assert!(
        pos_built_on < pos_result,
        "result block must appear after the built-on block"
    );
}

// ── Test: node with no linkage renders neither block ──────────────────────────

/// N02 has no built_on and no node_exhibit edges. The detail pane must render
/// neither `.built-on-block` nor `.result-block`.
#[wasm_bindgen_test]
fn node_without_linkage_renders_neither_block() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(LINKAGE_FIXTURE_JSON).expect("linkage fixture must parse");
    let selected: RwSignal<Option<ara_core::NodeId>> =
        RwSignal::new(Some(ara_core::NodeId::new("N02")));
    let (load_state, _) = signal(LoadState::Loaded(manifest));

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DetailPane load_state=load_state selected=selected /> }
    });

    assert!(
        container
            .query_selector("div.built-on-block")
            .unwrap()
            .is_none(),
        "node with no built_on must NOT render a built-on-block"
    );
    assert!(
        container
            .query_selector("div.result-block")
            .unwrap()
            .is_none(),
        "node with no exhibits must NOT render a result-block"
    );
}

// ── Test: layout toggle flips the active segment + drives the signal ──────────

/// Mounts `LayoutToggle` bound to a `layout` signal. Asserts:
///  - two segment buttons render (stack, split);
///  - "stack" is active initially (the default), "split" is not;
///  - clicking "split" flips the signal to `Split`, moving `is-active` +
///    `aria-pressed="true"` onto the split button.
#[wasm_bindgen_test]
async fn layout_toggle_flips_active_segment() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let layout: RwSignal<LayoutMode> = RwSignal::new(LayoutMode::default());

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <LayoutToggle layout=layout /> }
    });

    let stack_btn = container
        .query_selector("button[data-mode='stack']")
        .unwrap()
        .expect("stack segment button must be present")
        .dyn_into::<HtmlElement>()
        .unwrap();
    let split_btn = container
        .query_selector("button[data-mode='split']")
        .unwrap()
        .expect("split segment button must be present")
        .dyn_into::<HtmlElement>()
        .unwrap();

    // Initial state: stack active, split not.
    assert!(
        stack_btn
            .get_attribute("class")
            .unwrap_or_default()
            .contains("is-active"),
        "stack must be the initially active segment"
    );
    assert_eq!(
        stack_btn.get_attribute("aria-pressed").as_deref(),
        Some("true"),
        "stack must be aria-pressed initially"
    );
    assert!(
        !split_btn
            .get_attribute("class")
            .unwrap_or_default()
            .contains("is-active"),
        "split must not be active initially"
    );

    // Click "split" — it's an HtmlElement so .click() is available.
    split_btn.click();
    leptos::task::tick().await;

    assert_eq!(
        layout.get_untracked(),
        LayoutMode::Split,
        "signal must flip to Split"
    );
    assert!(
        split_btn
            .get_attribute("class")
            .unwrap_or_default()
            .contains("is-active"),
        "split must become active after click"
    );
    assert_eq!(
        split_btn.get_attribute("aria-pressed").as_deref(),
        Some("true"),
        "split must be aria-pressed after click"
    );
    assert!(
        !stack_btn
            .get_attribute("class")
            .unwrap_or_default()
            .contains("is-active"),
        "stack must no longer be active after selecting split"
    );
}

// ── Test: search query dims non-matching nodes ────────────────────────────────

/// Set filter.query = "Gradient" (matches N02 only).  After computing the
/// `matching` set, all non-matching nodes must carry the "dimmed" CSS class.
/// We drive the Memo directly in-test rather than through the Toolbar DOM.
#[wasm_bindgen_test]
fn search_query_dims_non_matching_nodes() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(FIXTURE_JSON).expect("fixture must parse");
    let renderer = SvgRenderer;
    let scene = renderer.scene(&manifest, &LayoutView::default());

    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let pan_zoom: RwSignal<PanZoom> = RwSignal::new(PanZoom::default());

    // Only N02 ("Gradient collapse") matches the query "Gradient".
    let n02_id = ara_core::NodeId::new("N02");
    let matching_set: HashSet<ara_core::NodeId> = std::iter::once(n02_id).collect();
    let matching = Memo::new(move |_| matching_set.clone());

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <GraphView scene=scene selected=selected pan_zoom=pan_zoom matching=matching /> }
    });

    let svg = container
        .query_selector("svg.graph-svg")
        .unwrap()
        .expect("graph-svg must be present");

    // N02's <g> must NOT be dimmed (it matches).
    let n02_g = svg
        .query_selector("g[aria-label*='Gradient collapse']")
        .unwrap()
        .expect("N02 node g must be present");
    // `<g>` is SVG: read the raw `class` attribute (its `.className` is an
    // `SVGAnimatedString` object, not a string).
    let n02_class = n02_g.get_attribute("class").unwrap_or_default();
    assert!(
        !n02_class.contains("dimmed"),
        "matching node N02 must NOT be dimmed, got class: {n02_class}"
    );

    // N01's <g> must be dimmed (it does not match).
    let n01_g = svg
        .query_selector("g[aria-label*='Use sinusoidal encoding']")
        .unwrap()
        .expect("N01 node g must be present");
    let n01_class = n01_g.get_attribute("class").unwrap_or_default();
    assert!(
        n01_class.contains("dimmed"),
        "non-matching node N01 must be dimmed, got class: {n01_class}"
    );
}

// ── Paper-header fixtures ─────────────────────────────────────────────────────
//
// A manifest carrying a full PaperMeta (title/authors/venue/year/abstract), and
// one with no `paper` key at all (→ paper defaults to None). Nodes are empty:
// the header reads only `manifest.paper`, so the node list is irrelevant here.
const PAPER_FIXTURE_JSON: &str = r#"{
  "nodes": [],
  "links": [],
  "bindings": [],
  "claims": [],
  "paper": {
    "title": "Attention Is All You Need",
    "authors": ["A. Vaswani", "N. Shazeer"],
    "year": "2017",
    "venue": "NeurIPS",
    "doi": null,
    "abstract": "The dominant sequence transduction models are based on complex recurrent or convolutional neural networks.",
    "keywords": []
  }
}"#;

const NO_PAPER_FIXTURE_JSON: &str = r#"{
  "nodes": [],
  "links": [],
  "bindings": [],
  "claims": []
}"#;

const NO_ABSTRACT_PAPER_FIXTURE_JSON: &str = r#"{
  "nodes": [],
  "links": [],
  "bindings": [],
  "claims": [],
  "paper": {
    "title": "A Terse Artifact",
    "authors": ["A. Author"],
    "year": "2024",
    "venue": "ArXiv",
    "doi": null,
    "abstract": null,
    "keywords": []
  }
}"#;

// ── Test: paper header renders title, byline, and a collapsed Abstract ─────────

/// Mounts `PaperHeader` with a `LoadState::Loaded` manifest carrying a full
/// `PaperMeta`. The header must show the title `<h1>`, the joined
/// authors/venue/year byline, and a collapsed `<details class="paper-abstract">`
/// (no `open` attribute) with an "Abstract" summary.
#[wasm_bindgen_test]
fn paper_header_renders_title_byline_and_collapsed_abstract() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(PAPER_FIXTURE_JSON).expect("paper fixture must parse");
    let (load_state, _) = signal(LoadState::Loaded(manifest));

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <PaperHeader load_state=load_state /> }
    });

    // Title in the <h1>.
    let h1 = container
        .query_selector("h1")
        .unwrap()
        .expect("paper header must render an <h1>");
    assert_eq!(
        h1.dyn_ref::<HtmlElement>().unwrap().inner_text(),
        "Attention Is All You Need",
        "h1 must carry the paper title"
    );

    // Byline: authors joined with ", ", then "· Venue Year".
    let meta = container
        .query_selector("span.paper-meta")
        .unwrap()
        .expect("byline .paper-meta must be present")
        .dyn_into::<HtmlElement>()
        .unwrap();
    let meta_text = meta.inner_text();
    assert!(
        meta_text.contains("A. Vaswani, N. Shazeer"),
        "byline must join authors with ', ', got: {meta_text:?}"
    );
    assert!(
        meta_text.contains("NeurIPS 2017"),
        "byline must show 'Venue Year', got: {meta_text:?}"
    );

    // Abstract lives in a <details> that is collapsed (no `open` attribute).
    let details = container
        .query_selector("details.paper-abstract")
        .unwrap()
        .expect("abstract must render in a <details class='paper-abstract'>");
    assert!(
        details.get_attribute("open").is_none(),
        "abstract <details> must be collapsed by default (no `open`)"
    );
    let summary = details
        .query_selector("summary")
        .unwrap()
        .expect("abstract <details> must have a <summary>")
        .dyn_into::<HtmlElement>()
        .unwrap();
    assert_eq!(
        summary.inner_text(),
        "Abstract",
        "summary must read 'Abstract'"
    );
}

// ── Test: manifest without a paper falls back to the ARA Viewer brand ─────────

/// With `paper = None`, `PaperHeader` must fall back to the brand: an "ARA
/// Viewer" `<h1>` + the `.header-subtitle`, and render NO paper byline/abstract.
#[wasm_bindgen_test]
fn paper_header_falls_back_to_brand_without_paper() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(NO_PAPER_FIXTURE_JSON).expect("no-paper fixture must parse");
    assert!(manifest.paper.is_none(), "fixture must have no paper");
    let (load_state, _) = signal(LoadState::Loaded(manifest));

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <PaperHeader load_state=load_state /> }
    });

    let h1 = container
        .query_selector("h1")
        .unwrap()
        .expect("brand header must render an <h1>");
    assert_eq!(
        h1.dyn_ref::<HtmlElement>().unwrap().inner_text(),
        "ARA Viewer",
        "fallback h1 must read 'ARA Viewer'"
    );
    assert!(
        container
            .query_selector("span.header-subtitle")
            .unwrap()
            .is_some(),
        "fallback must render the .header-subtitle brand tagline"
    );
    // No paper-specific chrome.
    assert!(
        container
            .query_selector("span.paper-meta")
            .unwrap()
            .is_none(),
        "brand fallback must NOT render a paper byline"
    );
    assert!(
        container
            .query_selector("details.paper-abstract")
            .unwrap()
            .is_none(),
        "brand fallback must NOT render an abstract"
    );
}

// ── Test: titled paper with no abstract renders header but no <details> ────────

/// A titled `PaperMeta` whose `abstract` is absent must still render the paper
/// header (title + byline) but NO `<details class="paper-abstract">`.
#[wasm_bindgen_test]
fn paper_header_without_abstract_omits_details() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest =
        parse_manifest(NO_ABSTRACT_PAPER_FIXTURE_JSON).expect("no-abstract fixture must parse");
    let (load_state, _) = signal(LoadState::Loaded(manifest));

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <PaperHeader load_state=load_state /> }
    });

    let h1 = container
        .query_selector("h1")
        .unwrap()
        .expect("paper header must render an <h1>");
    assert_eq!(
        h1.dyn_ref::<HtmlElement>().unwrap().inner_text(),
        "A Terse Artifact",
        "h1 must carry the paper title"
    );
    assert!(
        container
            .query_selector("span.paper-meta")
            .unwrap()
            .is_some(),
        "byline must still render"
    );
    assert!(
        container
            .query_selector("details.paper-abstract")
            .unwrap()
            .is_none(),
        "no abstract → no <details>"
    );
}

// ── Tree-list mode fixture ────────────────────────────────────────────────────
//
// A tree with an isolated root so the `.isobox` renders:
//   N01 (question, root) ──child──▶ N02 (experiment) ──child──▶ N03 (dead_end)
//                         └─depends_on─▶ N02
//   N10 (question, isolated: true) ──child──▶ N11 (insight)
const TREE_FIXTURE_JSON: &str = r#"{
  "nodes": [
    { "id": "N01", "kind": "question", "label": "Root question",
      "source_refs": [], "evidence_notes": [], "fields": "question" },
    { "id": "N02", "kind": "experiment", "label": "An experiment",
      "source_refs": [], "evidence_notes": [],
      "fields": { "experiment": { "result": null } } },
    { "id": "N03", "kind": "dead_end", "label": "A dead end",
      "source_refs": [], "evidence_notes": [],
      "fields": { "dead_end": { "why_failed": "nope" } } },
    { "id": "N10", "kind": "question", "label": "Isolated root", "isolated": true,
      "source_refs": [], "evidence_notes": [], "fields": "question" },
    { "id": "N11", "kind": "insight", "label": "Isolated child",
      "source_refs": [], "evidence_notes": [], "fields": "insight" }
  ],
  "links": [
    { "from": "N01", "to": "N02", "kind": "child" },
    { "from": "N02", "to": "N03", "kind": "child" },
    { "from": "N01", "to": "N02", "kind": "depends_on" },
    { "from": "N10", "to": "N11", "kind": "child" }
  ],
  "bindings": [],
  "claims": [],
  "bounds": { "x": 0.0, "y": 0.0, "width": 500.0, "height": 500.0 }
}"#;

fn all_matching(manifest: &ara_core::Manifest) -> Memo<HashSet<ara_core::NodeId>> {
    let all: HashSet<ara_core::NodeId> = manifest.nodes.iter().map(|n| n.id.clone()).collect();
    Memo::new(move |_| all.clone())
}

// ── Test: tree rows render with nesting + .kid containers ─────────────────────

#[wasm_bindgen_test]
fn tree_view_renders_rows_and_nesting() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(TREE_FIXTURE_JSON).expect("tree fixture must parse");
    let model = tree_model(&manifest);
    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let matching = all_matching(&manifest);

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <TreeView model=model selected=selected matching=matching /> }
    });

    let tree = container
        .query_selector("div.tree-map")
        .unwrap()
        .expect("tree-map container must be present");

    // 5 rows total (N01, N02, N03, N10, N11).
    let rows = tree.query_selector_all("div.node").unwrap();
    assert_eq!(rows.length(), 5, "all 5 nodes render a .node row");

    // Nesting: at least one .kid container (N01's children, N02's, N10's).
    let kids = tree.query_selector_all("div.kid").unwrap();
    assert!(
        kids.length() >= 1,
        "child rows live in sibling .kid containers"
    );

    // Each row carries a .glyph chip and an .ntitle.
    assert!(tree.query_selector("span.glyph").unwrap().is_some());
    assert!(tree.query_selector("div.ntitle").unwrap().is_some());
}

// ── Test: dead-end row gets .dead class (strikethrough) ───────────────────────

#[wasm_bindgen_test]
fn tree_dead_end_row_has_dead_class() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(TREE_FIXTURE_JSON).expect("tree fixture must parse");
    let model = tree_model(&manifest);
    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let matching = all_matching(&manifest);

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <TreeView model=model selected=selected matching=matching /> }
    });

    // N03 is the dead end (aria-label "A dead end, dead_end").
    let dead = container
        .query_selector("div.node[aria-label*='A dead end']")
        .unwrap()
        .expect("dead-end row must be present")
        .dyn_into::<HtmlElement>()
        .unwrap();
    let class = dead.get_attribute("class").unwrap_or_default();
    assert!(
        class.contains("dead"),
        "dead-end row class must contain 'dead', got: {class}"
    );
}

// ── Test: isolated root renders inside .isobox ────────────────────────────────

#[wasm_bindgen_test]
fn tree_isolated_root_renders_in_isobox() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(TREE_FIXTURE_JSON).expect("tree fixture must parse");
    let model = tree_model(&manifest);
    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let matching = all_matching(&manifest);

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <TreeView model=model selected=selected matching=matching /> }
    });

    let isobox = container
        .query_selector("div.isobox")
        .unwrap()
        .expect("isolated root must render inside .isobox");
    // The isobox header + the isolated root row (N10) live inside it.
    assert!(
        isobox.query_selector("div.isohdr").unwrap().is_some(),
        "isobox must have an .isohdr"
    );
    let iso_row = isobox
        .query_selector("div.node[aria-label*='Isolated root']")
        .unwrap();
    assert!(
        iso_row.is_some(),
        "isolated root row must live inside .isobox"
    );
}

// ── Test: dep marker ⇠ renders; hover applies .deptarget ──────────────────────

#[wasm_bindgen_test]
async fn tree_dep_marker_and_hover_deptarget() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(TREE_FIXTURE_JSON).expect("tree fixture must parse");
    let model = tree_model(&manifest);
    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let matching = all_matching(&manifest);

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <TreeView model=model selected=selected matching=matching /> }
    });

    // N01 depends_on N02 → a single .dep marker with the ⇠ glyph.
    let dep = container
        .query_selector("span.dep")
        .unwrap()
        .expect("dep marker must render for N01→N02 depends_on")
        .dyn_into::<HtmlElement>()
        .unwrap();
    assert!(
        dep.inner_text().contains('\u{21e0}') && dep.inner_text().contains("N02"),
        "dep marker must show ⇠ and the target id, got: {:?}",
        dep.inner_text()
    );

    // Hover N01's row → its dep target N02 gets .deptarget.
    let n01_row = container
        .query_selector("div.node[aria-label*='Root question']")
        .unwrap()
        .expect("N01 row must be present");
    let init = web_sys::PointerEventInit::new();
    init.set_bubbles(true);
    let enter = web_sys::PointerEvent::new_with_event_init_dict("pointerenter", &init).unwrap();
    n01_row.dispatch_event(&enter).unwrap();
    leptos::task::tick().await;

    let n02_row = container
        .query_selector("div.node[aria-label*='An experiment']")
        .unwrap()
        .expect("N02 row must be present");
    let n02_class = n02_row.get_attribute("class").unwrap_or_default();
    assert!(
        n02_class.contains("deptarget"),
        "N02 must get .deptarget while N01 is hovered, got: {n02_class}"
    );
}

// ── Test: selecting a tree row updates the detail pane ────────────────────────

#[wasm_bindgen_test]
async fn tree_row_click_updates_selection() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(TREE_FIXTURE_JSON).expect("tree fixture must parse");
    let model = tree_model(&manifest);
    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let matching = all_matching(&manifest);

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <TreeView model=model selected=selected matching=matching /> }
    });

    let n02_row = container
        .query_selector("div.node[aria-label*='An experiment']")
        .unwrap()
        .expect("N02 row must be present")
        .dyn_into::<HtmlElement>()
        .unwrap();
    n02_row.click();
    leptos::task::tick().await;

    assert_eq!(
        selected.get_untracked(),
        Some(ara_core::NodeId::new("N02")),
        "clicking a tree row sets the shared selected signal"
    );
    // Selected row gets .sel.
    let n02_class = n02_row.get_attribute("class").unwrap_or_default();
    assert!(
        n02_class.contains("sel"),
        "selected tree row must get .sel, got: {n02_class}"
    );
}

// ── Test: DisplayToggle flips + carries data-mode / is-active / aria-pressed ──

#[wasm_bindgen_test]
async fn display_toggle_flips_active_segment() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let display: RwSignal<DisplayMode> = RwSignal::new(DisplayMode::default());

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DisplayToggle display=display /> }
    });

    let graph_btn = container
        .query_selector("button[data-mode='graph']")
        .unwrap()
        .expect("graph segment button must be present")
        .dyn_into::<HtmlElement>()
        .unwrap();
    let tree_btn = container
        .query_selector("button[data-mode='tree']")
        .unwrap()
        .expect("tree segment button must be present")
        .dyn_into::<HtmlElement>()
        .unwrap();

    // Initial: graph active, tree not.
    assert!(
        graph_btn
            .get_attribute("class")
            .unwrap_or_default()
            .contains("is-active"),
        "graph must be the initially active segment"
    );
    assert_eq!(
        graph_btn.get_attribute("aria-pressed").as_deref(),
        Some("true"),
        "graph must be aria-pressed initially"
    );
    assert!(
        !tree_btn
            .get_attribute("class")
            .unwrap_or_default()
            .contains("is-active"),
        "tree must not be active initially"
    );

    // Click "tree".
    tree_btn.click();
    leptos::task::tick().await;

    assert_eq!(
        display.get_untracked(),
        DisplayMode::Tree,
        "signal must flip to Tree"
    );
    assert!(
        tree_btn
            .get_attribute("class")
            .unwrap_or_default()
            .contains("is-active"),
        "tree must become active after click"
    );
    assert_eq!(
        tree_btn.get_attribute("aria-pressed").as_deref(),
        Some("true"),
        "tree must be aria-pressed after click"
    );
    assert!(
        !graph_btn
            .get_attribute("class")
            .unwrap_or_default()
            .contains("is-active"),
        "graph must no longer be active after selecting tree"
    );
}

// ── Test: replay next / prev step the selection ───────────────────────────────

#[wasm_bindgen_test]
async fn replay_next_prev_step_selection() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(TREE_FIXTURE_JSON).expect("tree fixture must parse");
    let order = {
        let ids = node_order(&manifest);
        Memo::new(move |_| ids.clone())
    };
    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let state = ReplayState::default();

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <ReplayBar order=order selected=selected state=state /> }
    });

    let next = container
        .query_selector("button#rnext")
        .unwrap()
        .expect("next button")
        .dyn_into::<HtmlElement>()
        .unwrap();
    let prev = container
        .query_selector("button#rprev")
        .unwrap()
        .expect("prev button")
        .dyn_into::<HtmlElement>()
        .unwrap();

    // Next from no selection → first node (N01).
    next.click();
    leptos::task::tick().await;
    assert_eq!(selected.get_untracked(), Some(ara_core::NodeId::new("N01")));

    // Next again → N02.
    next.click();
    leptos::task::tick().await;
    assert_eq!(selected.get_untracked(), Some(ara_core::NodeId::new("N02")));

    // Prev → back to N01.
    prev.click();
    leptos::task::tick().await;
    assert_eq!(selected.get_untracked(), Some(ara_core::NodeId::new("N01")));
}

// ── Test: arrow keys step selection; INPUT focus guards them ──────────────────

#[wasm_bindgen_test]
async fn arrow_keys_step_and_input_guard() {
    let doc = web_sys::window().unwrap().document().unwrap();

    let manifest = parse_manifest(TREE_FIXTURE_JSON).expect("tree fixture must parse");
    let order = {
        let ids = node_order(&manifest);
        Memo::new(move |_| ids.clone())
    };
    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let state = ReplayState::default();

    install_arrow_key_listener(order, selected, state);

    // Dispatch ArrowRight on <body> (focus outside any input) → advances to N01.
    let body = doc.body().unwrap();
    let init = web_sys::KeyboardEventInit::new();
    init.set_bubbles(true);
    init.set_key("ArrowRight");
    let ev = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
    body.dispatch_event(&ev).unwrap();
    leptos::task::tick().await;
    assert_eq!(
        selected.get_untracked(),
        Some(ara_core::NodeId::new("N01")),
        "ArrowRight outside inputs advances the selection"
    );

    // ArrowRight again → N02.
    let ev2 = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
    body.dispatch_event(&ev2).unwrap();
    leptos::task::tick().await;
    assert_eq!(selected.get_untracked(), Some(ara_core::NodeId::new("N02")));

    // Now dispatch ArrowLeft from a focused <input> → the guard must ignore it,
    // so the selection stays at N02.
    let input = doc
        .create_element("input")
        .unwrap()
        .dyn_into::<web_sys::HtmlInputElement>()
        .unwrap();
    body.append_child(&input).unwrap();
    let left_init = web_sys::KeyboardEventInit::new();
    left_init.set_bubbles(true);
    left_init.set_key("ArrowLeft");
    let left_ev =
        web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &left_init).unwrap();
    // Dispatch the event *on the input* so ev.target() is the INPUT element.
    input.dispatch_event(&left_ev).unwrap();
    leptos::task::tick().await;
    assert_eq!(
        selected.get_untracked(),
        Some(ara_core::NodeId::new("N02")),
        "ArrowLeft while an <input> is the target must be ignored (INPUT guard)"
    );
}

// ── Test: replay play from mid-list ticks to last node and auto-stops ─────────

#[wasm_bindgen_test]
async fn replay_play_auto_stops_at_last() {
    use std::time::Duration;

    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    // Small 3-node order: N01, N02, N03 (from the tree fixture's main branch).
    let manifest = parse_manifest(TREE_FIXTURE_JSON).expect("tree fixture must parse");
    let order = {
        let ids = node_order(&manifest);
        Memo::new(move |_| ids.clone())
    };
    let n = order.get_untracked().len();
    // Start selection at the second-to-last node so play only needs a tick or two.
    let start = order.get_untracked()[n - 2].clone();
    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(Some(start));
    let state = ReplayState::default();

    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <ReplayBar order=order selected=selected state=state /> }
    });

    let play = container
        .query_selector("button#rplay")
        .unwrap()
        .expect("play button")
        .dyn_into::<HtmlElement>()
        .unwrap();

    // Start playing.
    play.click();
    leptos::task::tick().await;
    assert!(state.playing.get_untracked(), "play sets the playing flag");

    // Wait long enough for the 1300ms interval to tick past the last node.
    // 2 ticks max (start is second-to-last), so ~3s is a safe ceiling.
    gloo_timers_sleep(Duration::from_millis(3200)).await;

    let last = order.get_untracked()[n - 1].clone();
    assert_eq!(
        selected.get_untracked(),
        Some(last),
        "play advances to and stops at the last node"
    );
    assert!(
        !state.playing.get_untracked(),
        "replay auto-stops at the last node (no wrap, no loop)"
    );
    // Interval handle must be cleared (no leaked timer).
    assert!(
        state.handle.get_value().is_none(),
        "the interval handle must be cleared after auto-stop"
    );
}

/// Minimal async sleep for the replay-interval test (avoids adding a gloo-timers
/// dep — spins a Promise-backed setTimeout via wasm-bindgen-futures).
async fn gloo_timers_sleep(dur: std::time::Duration) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let win = web_sys::window().unwrap();
        win.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, dur.as_millis() as i32)
            .unwrap();
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

// ── Test: unmounting ReplayBar tears the interval down (on_cleanup) ───────────

#[wasm_bindgen_test]
async fn replay_interval_cleared_on_unmount() {
    use std::time::Duration;

    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = parse_manifest(TREE_FIXTURE_JSON).expect("tree fixture must parse");
    let order = {
        let ids = node_order(&manifest);
        Memo::new(move |_| ids.clone())
    };
    let selected: RwSignal<Option<ara_core::NodeId>> = RwSignal::new(None);
    let state = ReplayState::default();

    let handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <ReplayBar order=order selected=selected state=state /> }
    });

    // Start playing so an interval exists.
    let play = container
        .query_selector("button#rplay")
        .unwrap()
        .expect("play button")
        .dyn_into::<HtmlElement>()
        .unwrap();
    play.click();
    leptos::task::tick().await;
    assert!(
        state.handle.get_value().is_some(),
        "an interval handle must exist while playing"
    );

    // Unmount the bar → on_cleanup must clear the interval.
    drop(handle);
    leptos::task::tick().await;
    // Give the runtime a beat to run cleanup.
    gloo_timers_sleep(Duration::from_millis(50)).await;
    assert!(
        state.handle.get_value().is_none(),
        "on_cleanup must clear the interval handle on unmount (no leaked timer)"
    );
    assert!(
        !state.playing.get_untracked(),
        "playing flag reset on unmount cleanup"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Splitter (draggable gutter) browser interaction tests
// ══════════════════════════════════════════════════════════════════════════════
//
// The `Splitter` component measures geometry via `.closest(".app-main")`, so
// every test mounts the gutter inside a wrapper `<main class="app-main …">`. The
// test harness does NOT load `public/styles.css`, so an unstyled `.panel-gutter`
// div would have width = container width and height = 0 — unrealistic geometry
// that makes the pointer math meaningless. To fix that we inject a deterministic
// `<style>` block into `document.head` before mounting (see `inject_split_style`).
//
// Expected ratios are never hardcoded: after dispatching a synthetic event we
// recompute the expected value with the SAME public pure function
// (`clamp_split_ratio`) against the SAME measured geometry. This keeps the
// assertions self-consistent (robust to any harness offset) while still proving
// the component wires the pointer → ratio → signal path and the clamp correctly.

use ara_viewer::splitter::{
    KEYBOARD_STEP, SPLIT_DEFAULT_RATIO, STACK_DEFAULT_RATIO, Splitter, clamp_split_ratio,
    default_ratio, floors_for,
};

// ── Helper: inject deterministic gutter geometry ──────────────────────────────
//
// Sizes the `.app-main` wrapper to a fixed 1000×600 box and gives the gutter a
// 6 px cross-axis thickness in each layout. Appended once per test to
// `document.head`; leftover style tags across tests are harmless because the
// selectors are keyed on the wrapper's `layout-*` class.
fn inject_split_style(doc: &Document) {
    let style = doc.create_element("style").unwrap();
    style.set_text_content(Some(
        "
        .app-main { position: absolute; top: 0; left: 0; box-sizing: border-box; }
        .app-main.layout-split  { width: 1000px; height: 600px; }
        .app-main.layout-split  .panel-gutter { width: 6px;  height: 600px; }
        .app-main.layout-stack  { width: 1000px; height: 600px; }
        .app-main.layout-stack  .panel-gutter { height: 6px; width: 1000px; }
        ",
    ));
    doc.head().unwrap().append_child(&style).unwrap();
}

// ── Helper: a `<main class="app-main layout-…">` wrapper attached to body ─────
fn app_main_wrapper(doc: &Document, layout_class: &str) -> HtmlElement {
    let main = doc.create_element("main").unwrap();
    main.set_class_name(&format!("app-main {layout_class}"));
    doc.body().unwrap().append_child(&main).unwrap();
    main.unchecked_into::<HtmlElement>()
}

// ── Helper: synthetic pointer event dispatched on `el` ────────────────────────
fn dispatch_pointer(el: &web_sys::Element, kind: &str, client_x: i32, client_y: i32) {
    let init = web_sys::PointerEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_pointer_id(1);
    init.set_client_x(client_x);
    init.set_client_y(client_y);
    let ev = web_sys::PointerEvent::new_with_event_init_dict(kind, &init).unwrap();
    el.dispatch_event(&ev).unwrap();
}

// ── Helper: synthetic keydown dispatched on `el` ──────────────────────────────
fn dispatch_keydown(el: &web_sys::Element, key: &str) {
    let init = web_sys::KeyboardEventInit::new();
    init.set_key(key);
    init.set_bubbles(true);
    init.set_cancelable(true);
    let ev = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
    el.dispatch_event(&ev).unwrap();
}

// ── Helper: synthetic dblclick dispatched on `el` ─────────────────────────────
fn dispatch_dblclick(el: &web_sys::Element) {
    let init = web_sys::MouseEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    let ev = web_sys::MouseEvent::new_with_mouse_event_init_dict("dblclick", &init).unwrap();
    el.dispatch_event(&ev).unwrap();
}

// ── Helper: body has (or lacks) a class ───────────────────────────────────────
fn body_has_class(doc: &Document, class: &str) -> bool {
    doc.body().unwrap().class_list().contains(class)
}

// ── Test 1: pointer drag in Split updates the ratio + clamp holds both floors ─

/// Split mode: a pointerdown+pointermove writes the clamped fraction into
/// `split_ratio`; a mid-range drag lands strictly inside the two floor ratios;
/// extreme-left clamps to the map floor and extreme-right to the detail floor.
/// Every expected value is recomputed from the measured geometry via the public
/// `clamp_split_ratio`, so the assertion proves the wiring, not a magic number.
#[wasm_bindgen_test]
async fn splitter_pointer_drag_split_updates_ratio_and_clamps() {
    let doc = web_sys::window().unwrap().document().unwrap();
    inject_split_style(&doc);
    let wrapper = app_main_wrapper(&doc, "layout-split");

    let layout: RwSignal<LayoutMode> = RwSignal::new(LayoutMode::Split);
    let split_ratio: RwSignal<f64> = RwSignal::new(SPLIT_DEFAULT_RATIO);
    let stack_ratio: RwSignal<f64> = RwSignal::new(STACK_DEFAULT_RATIO);
    let dragging: RwSignal<bool> = RwSignal::new(false);

    let _handle = leptos::mount::mount_to(wrapper.clone().unchecked_into(), move || {
        view! {
            <Splitter
                layout=layout
                split_ratio=split_ratio
                stack_ratio=stack_ratio
                dragging=dragging
            />
        }
    });

    let gutter = wrapper
        .query_selector(".panel-gutter")
        .unwrap()
        .expect("gutter must render");

    // Measure the real geometry the component will see.
    let rect = wrapper.get_bounding_client_rect();
    let grect = gutter.get_bounding_client_rect();
    let axis = rect.width();
    let gutter_px = grect.width();
    let (m1, m2) = floors_for(LayoutMode::Split);
    let lo = clamp_split_ratio(0.0, axis, gutter_px, m1, m2);
    let hi = clamp_split_ratio(1.0, axis, gutter_px, m1, m2);

    // Mid-range drag: pointerdown (arms `dragging`) then a pointermove at X=500.
    dispatch_pointer(&gutter, "pointerdown", 500, 300);
    dispatch_pointer(&gutter, "pointermove", 500, 300);
    leptos::task::tick().await;

    let raw_mid = (500.0 - rect.left() - gutter_px / 2.0) / axis;
    let expected_mid = clamp_split_ratio(raw_mid, axis, gutter_px, m1, m2);
    assert!(
        (split_ratio.get_untracked() - expected_mid).abs() < 1e-9,
        "mid drag must equal clamp of measured raw ({expected_mid}), got {}",
        split_ratio.get_untracked()
    );
    assert!(
        split_ratio.get_untracked() > lo && split_ratio.get_untracked() < hi,
        "mid drag must land strictly between the floor ratios (lo={lo}, hi={hi})"
    );

    // Extreme-left drag → clamps to the map floor (lo).
    dispatch_pointer(&gutter, "pointermove", 0, 300);
    leptos::task::tick().await;
    assert!(
        (split_ratio.get_untracked() - lo).abs() < 1e-9,
        "extreme-left drag must clamp to the map floor ({lo}), got {}",
        split_ratio.get_untracked()
    );

    // Extreme-right drag → clamps to the detail floor (hi).
    dispatch_pointer(&gutter, "pointermove", 2000, 300);
    leptos::task::tick().await;
    assert!(
        (split_ratio.get_untracked() - hi).abs() < 1e-9,
        "extreme-right drag must clamp to the detail floor ({hi}), got {}",
        split_ratio.get_untracked()
    );
}

// ── Test 2: pointer drag in Stack updates the row ratio + clamp holds ─────────

/// Stack mode mirrors test 1 but drives the vertical axis: clientY + heights
/// feed the clamp, and the value lands in `stack_ratio` (never `split_ratio`).
#[wasm_bindgen_test]
async fn splitter_pointer_drag_stack_updates_ratio_and_clamps() {
    let doc = web_sys::window().unwrap().document().unwrap();
    inject_split_style(&doc);
    let wrapper = app_main_wrapper(&doc, "layout-stack");

    let layout: RwSignal<LayoutMode> = RwSignal::new(LayoutMode::Stack);
    let split_ratio: RwSignal<f64> = RwSignal::new(SPLIT_DEFAULT_RATIO);
    let stack_ratio: RwSignal<f64> = RwSignal::new(STACK_DEFAULT_RATIO);
    let dragging: RwSignal<bool> = RwSignal::new(false);

    let _handle = leptos::mount::mount_to(wrapper.clone().unchecked_into(), move || {
        view! {
            <Splitter
                layout=layout
                split_ratio=split_ratio
                stack_ratio=stack_ratio
                dragging=dragging
            />
        }
    });

    let gutter = wrapper
        .query_selector(".panel-gutter")
        .unwrap()
        .expect("gutter must render");

    let rect = wrapper.get_bounding_client_rect();
    let grect = gutter.get_bounding_client_rect();
    let axis = rect.height();
    let gutter_px = grect.height();
    let (m1, m2) = floors_for(LayoutMode::Stack);
    let lo = clamp_split_ratio(0.0, axis, gutter_px, m1, m2);
    let hi = clamp_split_ratio(1.0, axis, gutter_px, m1, m2);

    // Mid-range drag at Y=300.
    dispatch_pointer(&gutter, "pointerdown", 500, 300);
    dispatch_pointer(&gutter, "pointermove", 500, 300);
    leptos::task::tick().await;

    let raw_mid = (300.0 - rect.top() - gutter_px / 2.0) / axis;
    let expected_mid = clamp_split_ratio(raw_mid, axis, gutter_px, m1, m2);
    assert!(
        (stack_ratio.get_untracked() - expected_mid).abs() < 1e-9,
        "mid drag must equal clamp of measured raw ({expected_mid}), got {}",
        stack_ratio.get_untracked()
    );
    assert!(
        stack_ratio.get_untracked() > lo && stack_ratio.get_untracked() < hi,
        "mid stack drag must land strictly between the floor ratios (lo={lo}, hi={hi})"
    );
    // The Split ratio must not have moved.
    assert!(
        (split_ratio.get_untracked() - SPLIT_DEFAULT_RATIO).abs() < 1e-9,
        "stack drag must not touch split_ratio"
    );

    // Extreme-top → map floor.
    dispatch_pointer(&gutter, "pointermove", 500, 0);
    leptos::task::tick().await;
    assert!(
        (stack_ratio.get_untracked() - lo).abs() < 1e-9,
        "extreme-top drag must clamp to the map floor ({lo})"
    );

    // Extreme-bottom → detail floor.
    dispatch_pointer(&gutter, "pointermove", 500, 2000);
    leptos::task::tick().await;
    assert!(
        (stack_ratio.get_untracked() - hi).abs() < 1e-9,
        "extreme-bottom drag must clamp to the detail floor ({hi})"
    );
}

// ── Test 3: keyboard Arrow/Home/End update the ratio + aria-valuenow ──────────

/// Split mode: ArrowRight steps the ratio up by one `KEYBOARD_STEP` and the
/// `aria-valuenow` attribute reflects it; Home drops to the reachable min and
/// End climbs to the reachable max. `aria-valuemin`/`aria-valuemax` must be the
/// real clamped bounds (not the pre-measurement 0/100 fallback).
#[wasm_bindgen_test]
async fn splitter_keyboard_updates_ratio_and_aria() {
    let doc = web_sys::window().unwrap().document().unwrap();
    inject_split_style(&doc);
    let wrapper = app_main_wrapper(&doc, "layout-split");

    let layout: RwSignal<LayoutMode> = RwSignal::new(LayoutMode::Split);
    let split_ratio: RwSignal<f64> = RwSignal::new(SPLIT_DEFAULT_RATIO);
    let stack_ratio: RwSignal<f64> = RwSignal::new(STACK_DEFAULT_RATIO);
    let dragging: RwSignal<bool> = RwSignal::new(false);

    let _handle = leptos::mount::mount_to(wrapper.clone().unchecked_into(), move || {
        view! {
            <Splitter
                layout=layout
                split_ratio=split_ratio
                stack_ratio=stack_ratio
                dragging=dragging
            />
        }
    });

    let gutter = wrapper
        .query_selector(".panel-gutter")
        .unwrap()
        .expect("gutter must render");

    let rect = wrapper.get_bounding_client_rect();
    let grect = gutter.get_bounding_client_rect();
    let axis = rect.width();
    let gutter_px = grect.width();
    let (m1, m2) = floors_for(LayoutMode::Split);
    let lo = clamp_split_ratio(0.0, axis, gutter_px, m1, m2);
    let hi = clamp_split_ratio(1.0, axis, gutter_px, m1, m2);

    let before = split_ratio.get_untracked();
    dispatch_keydown(&gutter, "ArrowRight");
    leptos::task::tick().await;
    let after = split_ratio.get_untracked();
    assert!(
        (after - (before + KEYBOARD_STEP)).abs() < 1e-9,
        "ArrowRight must raise the ratio by one KEYBOARD_STEP ({before} -> {after})"
    );
    // aria-valuenow tracks the ratio (rounded percent).
    let expected_now = (after * 100.0).round() as i64;
    assert_eq!(
        gutter.get_attribute("aria-valuenow").as_deref(),
        Some(expected_now.to_string().as_str()),
        "aria-valuenow must follow the ratio after ArrowRight"
    );

    // Home → reachable min.
    dispatch_keydown(&gutter, "Home");
    leptos::task::tick().await;
    assert!(
        (split_ratio.get_untracked() - lo).abs() < 1e-9,
        "Home must clamp to the reachable min ({lo})"
    );

    // End → reachable max.
    dispatch_keydown(&gutter, "End");
    leptos::task::tick().await;
    assert!(
        (split_ratio.get_untracked() - hi).abs() < 1e-9,
        "End must clamp to the reachable max ({hi})"
    );

    // aria-valuemin / aria-valuemax must be the measured bounds, not 0 / 100.
    let expected_min = (lo * 100.0).round() as i64;
    let expected_max = (hi * 100.0).round() as i64;
    assert_eq!(
        gutter.get_attribute("aria-valuemin").as_deref(),
        Some(expected_min.to_string().as_str()),
        "aria-valuemin must equal the reachable map floor percent ({expected_min})"
    );
    assert_eq!(
        gutter.get_attribute("aria-valuemax").as_deref(),
        Some(expected_max.to_string().as_str()),
        "aria-valuemax must equal the reachable detail floor percent ({expected_max})"
    );
    assert!(
        expected_min > 0 && expected_max < 100,
        "measured bounds must be strictly inside 0..100 (min={expected_min}, max={expected_max})"
    );
}

// ── Test 4: double-click resets to the mode default ───────────────────────────

/// A non-default `split_ratio` returns to `SPLIT_DEFAULT_RATIO` on dblclick.
#[wasm_bindgen_test]
async fn splitter_dblclick_resets_to_default() {
    let doc = web_sys::window().unwrap().document().unwrap();
    inject_split_style(&doc);
    let wrapper = app_main_wrapper(&doc, "layout-split");

    let layout: RwSignal<LayoutMode> = RwSignal::new(LayoutMode::Split);
    let split_ratio: RwSignal<f64> = RwSignal::new(0.6);
    let stack_ratio: RwSignal<f64> = RwSignal::new(STACK_DEFAULT_RATIO);
    let dragging: RwSignal<bool> = RwSignal::new(false);

    let _handle = leptos::mount::mount_to(wrapper.clone().unchecked_into(), move || {
        view! {
            <Splitter
                layout=layout
                split_ratio=split_ratio
                stack_ratio=stack_ratio
                dragging=dragging
            />
        }
    });

    let gutter = wrapper
        .query_selector(".panel-gutter")
        .unwrap()
        .expect("gutter must render");

    assert!(
        (split_ratio.get_untracked() - 0.6).abs() < 1e-9,
        "precondition: ratio starts non-default"
    );
    dispatch_dblclick(&gutter);
    leptos::task::tick().await;
    assert!(
        (split_ratio.get_untracked() - SPLIT_DEFAULT_RATIO).abs() < 1e-9,
        "dblclick must reset split_ratio to SPLIT_DEFAULT_RATIO, got {}",
        split_ratio.get_untracked()
    );
    assert!(
        (split_ratio.get_untracked() - default_ratio(LayoutMode::Split)).abs() < 1e-9,
        "reset value must equal default_ratio(Split)"
    );
}

// ── Test 5: per-mode preservation across layout flips ─────────────────────────

/// A single `layout` signal drives both a drag in Split and a drag in Stack.
/// After flipping back to Split, each mode's ratio must retain its own dragged
/// value — the two ratios never bleed into each other. The wrapper's `layout-*`
/// class is kept in sync so the injected geometry matches the active mode.
#[wasm_bindgen_test]
async fn splitter_per_mode_ratios_are_preserved() {
    let doc = web_sys::window().unwrap().document().unwrap();
    inject_split_style(&doc);
    let wrapper = app_main_wrapper(&doc, "layout-split");

    let layout: RwSignal<LayoutMode> = RwSignal::new(LayoutMode::Split);
    let split_ratio: RwSignal<f64> = RwSignal::new(SPLIT_DEFAULT_RATIO);
    let stack_ratio: RwSignal<f64> = RwSignal::new(STACK_DEFAULT_RATIO);
    let dragging: RwSignal<bool> = RwSignal::new(false);

    let _handle = leptos::mount::mount_to(wrapper.clone().unchecked_into(), move || {
        view! {
            <Splitter
                layout=layout
                split_ratio=split_ratio
                stack_ratio=stack_ratio
                dragging=dragging
            />
        }
    });

    let gutter = wrapper
        .query_selector(".panel-gutter")
        .unwrap()
        .expect("gutter must render");

    // Drag in Split at X=450.
    dispatch_pointer(&gutter, "pointerdown", 450, 300);
    dispatch_pointer(&gutter, "pointermove", 450, 300);
    dispatch_pointer(&gutter, "pointerup", 450, 300);
    leptos::task::tick().await;
    let split_val = split_ratio.get_untracked();
    assert!(
        (split_val - SPLIT_DEFAULT_RATIO).abs() > 1e-6,
        "split drag must have moved split_ratio off its default"
    );

    // Flip to Stack: update BOTH the signal (handler logic) and the wrapper class
    // (injected geometry).
    layout.set(LayoutMode::Stack);
    wrapper.set_class_name("app-main layout-stack");
    leptos::task::tick().await;

    // Drag in Stack at Y=250.
    dispatch_pointer(&gutter, "pointerdown", 500, 250);
    dispatch_pointer(&gutter, "pointermove", 500, 250);
    dispatch_pointer(&gutter, "pointerup", 500, 250);
    leptos::task::tick().await;
    let stack_val = stack_ratio.get_untracked();
    assert!(
        (stack_val - STACK_DEFAULT_RATIO).abs() > 1e-6,
        "stack drag must have moved stack_ratio off its default"
    );
    // Split ratio must be untouched by the stack drag.
    assert!(
        (split_ratio.get_untracked() - split_val).abs() < 1e-9,
        "stack drag must not disturb split_ratio"
    );

    // Flip back to Split.
    layout.set(LayoutMode::Split);
    wrapper.set_class_name("app-main layout-split");
    leptos::task::tick().await;

    assert!(
        (split_ratio.get_untracked() - split_val).abs() < 1e-9,
        "split_ratio must survive the round-trip unchanged ({split_val})"
    );
    assert!(
        (stack_ratio.get_untracked() - stack_val).abs() < 1e-9,
        "stack_ratio must survive the round-trip unchanged ({stack_val})"
    );
}

// ── Test 6: body-lock cleanup on BOTH pointerup and pointercancel ─────────────

/// pointerdown adds the global body lock (`is-resizing` + `resizing-col` in
/// Split). pointerUP must clear all three lock classes. The regression case: a
/// pointerCANCEL (fresh mount) must ALSO fully clear the lock — a cancelled drag
/// that skipped cleanup would freeze the cursor/selection for the whole document.
#[wasm_bindgen_test]
async fn splitter_body_lock_cleared_on_pointerup_and_cancel() {
    let doc = web_sys::window().unwrap().document().unwrap();
    inject_split_style(&doc);

    // ── pointerup path ──
    {
        let wrapper = app_main_wrapper(&doc, "layout-split");
        let layout: RwSignal<LayoutMode> = RwSignal::new(LayoutMode::Split);
        let split_ratio: RwSignal<f64> = RwSignal::new(SPLIT_DEFAULT_RATIO);
        let stack_ratio: RwSignal<f64> = RwSignal::new(STACK_DEFAULT_RATIO);
        let dragging: RwSignal<bool> = RwSignal::new(false);

        let _handle = leptos::mount::mount_to(wrapper.clone().unchecked_into(), move || {
            view! {
                <Splitter
                    layout=layout
                    split_ratio=split_ratio
                    stack_ratio=stack_ratio
                    dragging=dragging
                />
            }
        });
        let gutter = wrapper.query_selector(".panel-gutter").unwrap().unwrap();

        dispatch_pointer(&gutter, "pointerdown", 500, 300);
        leptos::task::tick().await;
        assert!(
            body_has_class(&doc, "is-resizing"),
            "pointerdown must add is-resizing to body"
        );
        assert!(
            body_has_class(&doc, "resizing-col"),
            "pointerdown in Split must add resizing-col to body"
        );

        dispatch_pointer(&gutter, "pointerup", 500, 300);
        leptos::task::tick().await;
        assert!(
            !body_has_class(&doc, "is-resizing")
                && !body_has_class(&doc, "resizing-col")
                && !body_has_class(&doc, "resizing-row"),
            "pointerup must clear all three body lock classes"
        );
    }

    // ── pointercancel path (the key regression) ──
    {
        let wrapper = app_main_wrapper(&doc, "layout-split");
        let layout: RwSignal<LayoutMode> = RwSignal::new(LayoutMode::Split);
        let split_ratio: RwSignal<f64> = RwSignal::new(SPLIT_DEFAULT_RATIO);
        let stack_ratio: RwSignal<f64> = RwSignal::new(STACK_DEFAULT_RATIO);
        let dragging: RwSignal<bool> = RwSignal::new(false);

        let _handle = leptos::mount::mount_to(wrapper.clone().unchecked_into(), move || {
            view! {
                <Splitter
                    layout=layout
                    split_ratio=split_ratio
                    stack_ratio=stack_ratio
                    dragging=dragging
                />
            }
        });
        let gutter = wrapper.query_selector(".panel-gutter").unwrap().unwrap();

        dispatch_pointer(&gutter, "pointerdown", 500, 300);
        leptos::task::tick().await;
        assert!(
            body_has_class(&doc, "is-resizing"),
            "pointerdown must add is-resizing before cancel"
        );

        dispatch_pointer(&gutter, "pointercancel", 500, 300);
        leptos::task::tick().await;
        assert!(
            !body_has_class(&doc, "is-resizing")
                && !body_has_class(&doc, "resizing-col")
                && !body_has_class(&doc, "resizing-row"),
            "pointercancel must fully clear the body lock (no stuck global lock)"
        );
    }
}

// ── Test 7: <800px mobile collapse regression ─────────────────────────────────
//
// Approach: iframe with the REAL stylesheet.  We build a 375 px-wide `<iframe>`,
// write the actual `public/styles.css` into it (via `include_str!`) plus minimal
// `.app-main` markup, then read the *computed* style. This exercises the genuine
// `@media (max-width: 800px)` cascade in a real layout engine, so the test fails
// if EITHER mobile rule (single-column grid OR gutter `display:none`) is deleted.
//
// The iframe/computed-style path works reliably under
// `wasm-pack test --headless --chrome`; no fallback was needed.
#[wasm_bindgen_test]
async fn splitter_mobile_collapse_hides_gutter_and_single_column() {
    let doc = web_sys::window().unwrap().document().unwrap();

    // 375 px-wide iframe → triggers the max-width:800px media query.
    let iframe = doc
        .create_element("iframe")
        .unwrap()
        .dyn_into::<web_sys::HtmlIFrameElement>()
        .unwrap();
    iframe.set_attribute("width", "375").unwrap();
    iframe.set_attribute("height", "600").unwrap();
    iframe
        .set_attribute("style", "width:375px;height:600px;border:0;")
        .unwrap();
    doc.body().unwrap().append_child(&iframe).unwrap();

    let idoc = iframe
        .content_document()
        .expect("iframe must expose a content document");

    // Inject the real stylesheet + minimal viewer markup by writing into the
    // iframe's root element. `set_inner_html` on <html> is the reliable
    // cross-browser path in the headless harness (vs `document.write`).
    let styles = include_str!("../public/styles.css");
    let idoc_html = idoc.document_element().unwrap();
    idoc_html.set_inner_html(&format!(
        "<head><style>{styles}</style></head><body>\
         <main class=\"app-main layout-split\">\
           <section class=\"panel panel-map\"></section>\
           <div class=\"panel-gutter\"></div>\
           <section class=\"panel panel-detail\"></section>\
         </main></body>"
    ));

    // Let layout/style resolution settle.
    leptos::task::tick().await;

    let iwin = iframe.content_window().expect("iframe window");
    let iwin: web_sys::Window = iwin.unchecked_into();

    let gutter = idoc
        .query_selector(".panel-gutter")
        .unwrap()
        .expect("gutter must exist in iframe");
    let main = idoc
        .query_selector(".app-main")
        .unwrap()
        .expect("app-main must exist in iframe");

    // Assert 1: the gutter is display:none under the mobile media query.
    let gutter_cs = iwin
        .get_computed_style(&gutter)
        .unwrap()
        .expect("computed style for gutter");
    let display = gutter_cs.get_property_value("display").unwrap();
    assert_eq!(
        display, "none",
        "at <800px the gutter must be display:none (got {display:?})"
    );

    // Assert 2: .app-main collapses to a single grid column. A single-track
    // computed `grid-template-columns` has no internal space (multi-track values
    // are space-separated, e.g. "320px 6px 434px").
    let main_cs = iwin
        .get_computed_style(&main)
        .unwrap()
        .expect("computed style for app-main");
    let cols = main_cs.get_property_value("grid-template-columns").unwrap();
    let track_count = cols.split_whitespace().count();
    assert_eq!(
        track_count, 1,
        "at <800px .app-main must be a single grid column (got {cols:?})"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Modal (shared a11y dialog) + Dependencies panel — Slice 5
//
// These exercise the full accessibility contract of `modal::Modal` in a real
// browser: focus-in on open, the Tab/Shift+Tab focus trap (wrapping both ends),
// Esc-to-close, focus restore to the invoking element, and scrim-vs-content
// click behaviour. Plus the Dependencies panel: live count, hidden at 0, and
// the in-modal filter.
// ══════════════════════════════════════════════════════════════════════════════

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Dispatch a bubbling `keydown` on `document` (the modal listens there).
fn dispatch_modal_keydown(doc: &Document, key: &str, shift: bool) {
    let init = web_sys::KeyboardEventInit::new();
    init.set_key(key);
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_shift_key(shift);
    let ev = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
    doc.dispatch_event(&ev).unwrap();
}

/// Move focus to the first element matching `sel` within `doc`.
fn focus_sel(doc: &Document, sel: &str) {
    let el = doc
        .query_selector(sel)
        .unwrap()
        .unwrap_or_else(|| panic!("selector {sel:?} must match an element"));
    el.unchecked_ref::<HtmlElement>().focus().unwrap();
}

/// True when `document.activeElement` is the same node as the first `sel` match.
fn active_is(doc: &Document, sel: &str) -> bool {
    let el = doc.query_selector(sel).unwrap().expect("sel must match");
    doc.active_element()
        .map(|a| a.is_same_node(Some(el.unchecked_ref::<web_sys::Node>())))
        .unwrap_or(false)
}

/// Mount a harness: an `#opener` button (outside the modal) plus a `Modal` with
/// two buttons + an input as focusable content. Returns the shared `open`
/// signal AND the mount handle (boxed) — the caller MUST keep the handle alive
/// for the test's duration and drop it at the end. Dropping it unmounts and
/// runs the modal's cleanup, detaching its document keydown listener; leaking it
/// (or dropping it early) would leave listeners from prior tests interfering.
/// `open` starts closed.
fn mount_modal_harness() -> (RwSignal<bool>, Box<dyn std::any::Any>) {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);
    let open = RwSignal::new(false);
    let handle = leptos::mount::mount_to(container, move || {
        view! {
            <button id="opener" on:click=move |_| open.set(true)>"Open"</button>
            <Modal open=open title="Test Dialog">
                <button id="btn-a">"A"</button>
                <button id="btn-b">"B"</button>
                <input id="input-c" type="text" />
            </Modal>
        }
    });
    (open, Box::new(handle))
}

// ── Test: opening moves focus into the modal ──────────────────────────────────

#[wasm_bindgen_test]
async fn modal_open_moves_focus_inside() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let (open, _handle) = mount_modal_harness();

    open.set(true);
    leptos::task::tick().await;

    let modal = doc
        .query_selector(".modal")
        .unwrap()
        .expect("modal must be rendered when open");
    let active = doc
        .active_element()
        .expect("something must be focused after open");
    assert!(
        modal.contains(Some(active.unchecked_ref::<web_sys::Node>())),
        "on open, focus must move into the .modal (active element inside it)"
    );
}

// ── Test: role/aria contract ──────────────────────────────────────────────────

#[wasm_bindgen_test]
async fn modal_has_dialog_aria_contract() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let (open, _handle) = mount_modal_harness();
    open.set(true);
    leptos::task::tick().await;

    let dialog = doc
        .query_selector(".modal")
        .unwrap()
        .expect("modal present");
    assert_eq!(dialog.get_attribute("role").as_deref(), Some("dialog"));
    assert_eq!(
        dialog.get_attribute("aria-modal").as_deref(),
        Some("true"),
        "dialog must be aria-modal"
    );
    let labelled = dialog
        .get_attribute("aria-labelledby")
        .expect("aria-labelledby must be set");
    let title = doc
        .get_element_by_id(&labelled)
        .expect("aria-labelledby must point at an existing element");
    assert!(
        title
            .text_content()
            .unwrap_or_default()
            .contains("Test Dialog"),
        "aria-labelledby target must be the title"
    );
}

// ── Test: Tab at the last focusable wraps to the first ────────────────────────

#[wasm_bindgen_test]
async fn modal_tab_wraps_last_to_first() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let (open, _handle) = mount_modal_harness();
    open.set(true);
    leptos::task::tick().await;

    // Focus the last focusable (the input), then Tab forward → wraps to first
    // focusable, which is the header close button.
    focus_sel(&doc, "#input-c");
    dispatch_modal_keydown(&doc, "Tab", false);
    leptos::task::tick().await;

    assert!(
        active_is(&doc, ".modal-close"),
        "Tab at the last focusable must wrap to the first (close button)"
    );
}

// ── Test: Shift+Tab at the first focusable wraps to the last ──────────────────

#[wasm_bindgen_test]
async fn modal_shift_tab_wraps_first_to_last() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let (open, _handle) = mount_modal_harness();
    open.set(true);
    leptos::task::tick().await;

    // Focus the first focusable (close button), Shift+Tab → wraps to last (input).
    focus_sel(&doc, ".modal-close");
    dispatch_modal_keydown(&doc, "Tab", true);
    leptos::task::tick().await;

    assert!(
        active_is(&doc, "#input-c"),
        "Shift+Tab at the first focusable must wrap to the last (input)"
    );
}

// ── Test: Escape closes the modal ─────────────────────────────────────────────

#[wasm_bindgen_test]
async fn modal_escape_closes() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let (open, _handle) = mount_modal_harness();
    open.set(true);
    leptos::task::tick().await;
    assert!(doc.query_selector(".modal").unwrap().is_some());

    dispatch_modal_keydown(&doc, "Escape", false);
    leptos::task::tick().await;

    assert!(
        doc.query_selector(".modal").unwrap().is_none(),
        "Escape must close the modal (.modal removed)"
    );
    assert!(!open.get(), "Escape must set open=false");
}

// ── Test: on close, focus returns to the invoking element ─────────────────────

#[wasm_bindgen_test]
async fn modal_returns_focus_on_close() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let (open, _handle) = mount_modal_harness();

    // Focus the opener, THEN open — the modal must capture it as the return target.
    focus_sel(&doc, "#opener");
    open.set(true);
    leptos::task::tick().await;
    // Focus is now inside the modal (not the opener).
    assert!(
        !active_is(&doc, "#opener"),
        "focus should have left the opener"
    );

    // Close via Escape → focus must return to the opener.
    dispatch_modal_keydown(&doc, "Escape", false);
    leptos::task::tick().await;

    assert!(
        active_is(&doc, "#opener"),
        "on close, focus must return to the invoking element (#opener)"
    );
}

// ── Test: scrim click closes; content click does not ──────────────────────────

#[wasm_bindgen_test]
async fn modal_content_click_does_not_close() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let (open, _handle) = mount_modal_harness();
    open.set(true);
    leptos::task::tick().await;

    // Click a button INSIDE the modal → must NOT close.
    let btn = doc
        .query_selector("#btn-a")
        .unwrap()
        .expect("btn-a present");
    btn.unchecked_ref::<HtmlElement>().click();
    leptos::task::tick().await;
    assert!(
        doc.query_selector(".modal").unwrap().is_some(),
        "clicking inside .modal must not close it"
    );
}

#[wasm_bindgen_test]
async fn modal_scrim_click_closes() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let (open, _handle) = mount_modal_harness();
    open.set(true);
    leptos::task::tick().await;

    // Dispatch a bubbling click whose target is the scrim backdrop itself.
    let scrim = doc
        .query_selector(".modal-scrim")
        .unwrap()
        .expect("scrim present");
    let init = web_sys::MouseEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    let ev = web_sys::MouseEvent::new_with_mouse_event_init_dict("click", &init).unwrap();
    scrim.dispatch_event(&ev).unwrap();
    leptos::task::tick().await;

    assert!(
        doc.query_selector(".modal").unwrap().is_none(),
        "clicking the scrim backdrop must close the modal"
    );
}

// ── Dependencies panel ────────────────────────────────────────────────────────

/// Build a Manifest carrying `n` related-work entries (RW01..RWn), each with a
/// distinct cite so the filter has something to narrow on.
fn manifest_with_related_work(n: usize) -> ara_core::Manifest {
    let related_work = (1..=n)
        .map(|i| ara_core::RelatedWork {
            id: format!("RW{i:02}"),
            cite: format!("Author{i} et al., 20{i:02}"),
            doi: None,
            kind: Some("baseline".to_string()),
            what_changed: None,
            why: None,
            adopted: None,
            claims_affected: vec![],
        })
        .collect();
    ara_core::Manifest {
        nodes: vec![],
        links: vec![],
        bindings: vec![],
        claims: vec![],
        bounds: None,
        paper: None,
        related_work,
        concepts: vec![],
        problem: None,
        recipes: vec![],
        exhibits: vec![],
        built_on: vec![],
        node_exhibits: vec![],
    }
}

#[wasm_bindgen_test]
async fn dependencies_button_shows_count_and_opens_modal() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = manifest_with_related_work(3);
    let (load_state, _) = signal(LoadState::Loaded(manifest));
    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DependenciesPanel load_state=load_state /> }
    });

    // Launcher shows the live count.
    let btn = container
        .query_selector(".panel-launch-btn")
        .unwrap()
        .expect("Dependencies launcher must render with related work");
    let btn_text = btn.unchecked_ref::<HtmlElement>().inner_text();
    assert!(
        btn_text.contains("Dependencies"),
        "launcher labelled Dependencies"
    );
    assert!(btn_text.contains('3'), "launcher shows the live count (3)");

    // Click opens the modal, which lists every RW id.
    btn.unchecked_ref::<HtmlElement>().click();
    leptos::task::tick().await;

    let modal = doc
        .query_selector(".modal")
        .unwrap()
        .expect("clicking the launcher opens the modal");
    let modal_text = modal.unchecked_ref::<HtmlElement>().inner_text();
    assert!(modal_text.contains("RW01"), "modal lists RW01");
    assert!(modal_text.contains("RW02"), "modal lists RW02");
    assert!(modal_text.contains("RW03"), "modal lists RW03");
}

#[wasm_bindgen_test]
fn dependencies_button_hidden_at_zero() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = manifest_with_related_work(0);
    let (load_state, _) = signal(LoadState::Loaded(manifest));
    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DependenciesPanel load_state=load_state /> }
    });

    assert!(
        container
            .query_selector(".panel-launch-btn")
            .unwrap()
            .is_none(),
        "a 0 related-work count must hide the Dependencies launcher entirely"
    );
}

#[wasm_bindgen_test]
async fn dependencies_filter_narrows_the_list() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);

    let manifest = manifest_with_related_work(3);
    let (load_state, _) = signal(LoadState::Loaded(manifest));
    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <DependenciesPanel load_state=load_state /> }
    });

    // Open the modal.
    let btn = container
        .query_selector(".panel-launch-btn")
        .unwrap()
        .unwrap();
    btn.unchecked_ref::<HtmlElement>().click();
    leptos::task::tick().await;

    // Type "RW02" into the filter → only RW02 survives.
    let input = doc
        .query_selector(".panel-filter")
        .unwrap()
        .expect("filter input present");
    let input: web_sys::HtmlInputElement = input.unchecked_into();
    input.set_value("RW02");
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    let ev = web_sys::Event::new_with_event_init_dict("input", &init).unwrap();
    input.dispatch_event(&ev).unwrap();
    leptos::task::tick().await;

    let modal = doc.query_selector(".modal").unwrap().unwrap();
    let text = modal.unchecked_ref::<HtmlElement>().inner_text();
    assert!(text.contains("RW02"), "filter 'RW02' keeps RW02");
    assert!(!text.contains("RW01"), "filter 'RW02' drops RW01");
    assert!(!text.contains("RW03"), "filter 'RW02' drops RW03");
}

// ── Context / Glossary / Recipes panels ───────────────────────────────────────

/// A Manifest carrying a problem framing, `n` concepts (one with `$…$` LaTeX and
/// a related-term cross-reference), and `n` recipes.
fn manifest_with_panels(n: usize) -> ara_core::Manifest {
    let concepts = (1..=n)
        .map(|i| ara_core::Concept {
            term: format!("Concept{i}"),
            notation: Some(format!("$\\pi^{{({i})}}$")),
            definition: Some(format!("definition of concept {i}")),
            boundary: None,
            related: vec!["ProgressiveNet".to_string()],
        })
        .collect();
    let recipes = (1..=n)
        .map(|i| ara_core::Recipe {
            name: format!("recipe{i}"),
            title: Some(format!("Recipe {i}")),
            body: format!("step body for recipe {i}"),
        })
        .collect();
    ara_core::Manifest {
        nodes: vec![],
        links: vec![],
        bindings: vec![],
        claims: vec![],
        bounds: None,
        paper: None,
        related_work: vec![],
        concepts,
        problem: Some(ara_core::Problem {
            statement: Some("the problem statement".to_string()),
            observations: vec!["O1: observed thing".to_string()],
            gaps: vec!["G1: the gap".to_string()],
            insights: vec!["I1: the insight".to_string()],
        }),
        recipes,
        exhibits: vec![],
        built_on: vec![],
        node_exhibits: vec![],
    }
}

#[wasm_bindgen_test]
async fn glossary_shows_count_opens_with_latex_and_xref() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);
    let (load_state, _) = signal(LoadState::Loaded(manifest_with_panels(2)));
    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <GlossaryPanel load_state=load_state /> }
    });

    let btn = container
        .query_selector(".panel-launch-btn")
        .unwrap()
        .expect("Glossary launcher present with concepts");
    let btn_text = btn.unchecked_ref::<HtmlElement>().inner_text();
    assert!(btn_text.contains("Glossary"), "labelled Glossary");
    assert!(btn_text.contains('2'), "shows the live count (2)");

    btn.unchecked_ref::<HtmlElement>().click();
    leptos::task::tick().await;

    let modal = doc.query_selector(".modal").unwrap().expect("modal opens");
    assert_eq!(
        modal.get_attribute("role").as_deref(),
        Some("dialog"),
        "panel opens a role=dialog"
    );
    let text = modal.unchecked_ref::<HtmlElement>().inner_text();
    assert!(text.contains("Concept1"), "lists Concept1");
    // Inert LaTeX span rendered verbatim as monospace, never interpreted (D3).
    let latex = modal
        .query_selector("code.latex-inert")
        .unwrap()
        .expect("notation rendered as inert LaTeX");
    assert!(
        latex
            .unchecked_ref::<HtmlElement>()
            .inner_text()
            .contains('$'),
        "inert LaTeX keeps the $…$ delimiters verbatim"
    );
    // Related term is a dotted cross-reference chip.
    assert!(
        modal.query_selector(".concept-xref").unwrap().is_some(),
        "related term rendered as a cross-reference chip"
    );
}

#[wasm_bindgen_test]
fn glossary_hidden_at_zero() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);
    let (load_state, _) = signal(LoadState::Loaded(manifest_with_panels(0)));
    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <GlossaryPanel load_state=load_state /> }
    });
    assert!(
        container
            .query_selector(".panel-launch-btn")
            .unwrap()
            .is_none(),
        "0 concepts hides the Glossary launcher"
    );
}

#[wasm_bindgen_test]
async fn context_opens_and_hidden_without_problem() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);
    let (load_state, _) = signal(LoadState::Loaded(manifest_with_panels(1)));
    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <ContextPanel load_state=load_state /> }
    });

    let btn = container
        .query_selector(".panel-launch-btn")
        .unwrap()
        .expect("Context launcher present when a problem exists");
    btn.unchecked_ref::<HtmlElement>().click();
    leptos::task::tick().await;
    let modal = doc
        .query_selector(".modal")
        .unwrap()
        .expect("context modal opens");
    let text = modal.unchecked_ref::<HtmlElement>().inner_text();
    assert!(
        text.contains("the problem statement"),
        "shows the statement"
    );
    assert!(text.contains("O1: observed thing"), "shows observations");

    // A manifest with no problem hides the launcher.
    let container2 = body_div(&doc);
    let mut no_problem = manifest_with_panels(1);
    no_problem.problem = None;
    let (ls2, _) = signal(LoadState::Loaded(no_problem));
    let _h2 = leptos::mount::mount_to(container2.clone(), move || {
        view! { <ContextPanel load_state=ls2 /> }
    });
    assert!(
        container2
            .query_selector(".panel-launch-btn")
            .unwrap()
            .is_none(),
        "no problem → no Context launcher"
    );
}

#[wasm_bindgen_test]
async fn recipes_shows_count_and_opens() {
    let doc = web_sys::window().unwrap().document().unwrap();
    let container = body_div(&doc);
    let (load_state, _) = signal(LoadState::Loaded(manifest_with_panels(4)));
    let _handle = leptos::mount::mount_to(container.clone(), move || {
        view! { <RecipesPanel load_state=load_state /> }
    });

    let btn = container
        .query_selector(".panel-launch-btn")
        .unwrap()
        .expect("Solution files launcher present");
    let btn_text = btn.unchecked_ref::<HtmlElement>().inner_text();
    assert!(
        btn_text.contains("Solution files"),
        "labelled Solution files"
    );
    assert!(
        btn_text.contains('4'),
        "count = one per solution file (E8 fallback)"
    );

    btn.unchecked_ref::<HtmlElement>().click();
    leptos::task::tick().await;
    let modal = doc
        .query_selector(".modal")
        .unwrap()
        .expect("recipes modal opens");
    let text = modal.unchecked_ref::<HtmlElement>().inner_text();
    assert!(text.contains("Recipe 1"), "lists Recipe 1 by title");
    assert!(
        text.contains("step body for recipe 1"),
        "shows the raw body"
    );
}
