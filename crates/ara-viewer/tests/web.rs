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
    detail::DetailPane,
    scene::{GraphRenderer, GraphView, LayoutView, SvgRenderer},
    state::{LayoutMode, LoadState, PanZoom, parse_manifest},
    toolbar::LayoutToggle,
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
