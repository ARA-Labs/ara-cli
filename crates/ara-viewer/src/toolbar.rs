//! Toolbar component — search, kind filter, dead-ends-only checkbox.
//!
//! The component reads/writes a shared [`FilterState`] signal owned by [`App`].
//! It derives the kind options from the loaded manifest so the select shows only
//! kinds actually present.  When the manifest is not yet loaded the select is
//! rendered with only the "all" option.

use ara_core::{Manifest, NodeKind};
use leptos::prelude::*;

use crate::filter::FilterState;
use crate::kind::kind_meta;
use crate::state::{DisplayMode, LayoutMode};

// ── Kind option ───────────────────────────────────────────────────────────────

/// A single option in the kind `<select>`.
#[derive(Debug, Clone, PartialEq)]
struct KindOption {
    /// Value sent to the `<select>` (`kind_meta.css_class`).
    value: &'static str,
    /// Human-readable label in the dropdown.
    label: String,
}

/// Derive the distinct kind options present in `manifest.nodes` in canonical
/// first-seen order.  The canonical order is:
/// question → experiment → decision → dead_end → insight → other
/// (matches the plan's "stable canonical order").
fn kind_options(manifest: &Manifest) -> Vec<KindOption> {
    // Canonical ordering index — lower = earlier.
    fn order(kind: &NodeKind) -> u8 {
        match kind {
            NodeKind::Question => 0,
            NodeKind::Experiment => 1,
            NodeKind::Decision => 2,
            NodeKind::DeadEnd => 3,
            NodeKind::Insight => 4,
            NodeKind::Other(_) => 5,
        }
    }

    let mut seen: Vec<(&'static str, String)> = Vec::new();
    let mut seen_css: std::collections::HashSet<&'static str> = std::collections::HashSet::new();

    // Collect in canonical order by sorting nodes by their kind's order index.
    let mut kinds: Vec<&NodeKind> = manifest.nodes.iter().map(|n| &n.kind).collect();
    kinds.sort_by_key(|k| order(k));

    for kind in kinds {
        let meta = kind_meta(kind);
        if seen_css.insert(meta.css_class) {
            seen.push((meta.css_class, meta.badge.clone()));
        }
    }

    seen.into_iter()
        .map(|(value, badge)| KindOption {
            value,
            label: badge,
        })
        .collect()
}

// ── Generic segmented toggle ─────────────────────────────────────────────────

/// A mode usable as a segment in [`seg_toggle`]. Implemented by the small
/// `Copy` view-state enums (`LayoutMode`, `DisplayMode`). Provides the segment
/// list (token + visible label) and the wire token for the active segment.
pub trait SegMode: Copy + PartialEq + Send + Sync + 'static {
    /// Segments in display order: `(mode, visible label)`.
    fn segments() -> &'static [(Self, &'static str)];
    /// Stable wire token for `data-mode` (used by tests + round-tripping).
    fn token(self) -> &'static str;
}

impl SegMode for LayoutMode {
    fn segments() -> &'static [(Self, &'static str)] {
        &[(LayoutMode::Stack, "stack"), (LayoutMode::Split, "split")]
    }
    fn token(self) -> &'static str {
        self.as_token()
    }
}

impl SegMode for DisplayMode {
    fn segments() -> &'static [(Self, &'static str)] {
        &[(DisplayMode::Graph, "graph"), (DisplayMode::Tree, "tree")]
    }
    fn token(self) -> &'static str {
        self.as_token()
    }
}

/// The single segmented two-button control shared by [`LayoutToggle`] and
/// [`DisplayToggle`].
///
/// One `<button>` per `M::segments()`, in display order. The active segment
/// carries `is-active` + `aria-pressed="true"`; `data-mode` carries the wire
/// token for tests. Clicking a segment sets the backing `signal`. Reuses the
/// generic `.layout-toggle*` skin (the class names already read generically), so
/// no per-toggle CSS is needed. `group_label` names the control for AT users.
pub fn seg_toggle<M: SegMode>(signal: RwSignal<M>, group_label: &'static str) -> impl IntoView {
    view! {
        <div class="layout-toggle" role="group" aria-label=group_label>
            {M::segments()
                .iter()
                .copied()
                .map(|(mode, label)| {
                    view! {
                        <button
                            type="button"
                            class="layout-toggle-btn"
                            class:is-active=move || signal.get() == mode
                            aria-pressed=move || (signal.get() == mode).to_string()
                            data-mode=mode.token()
                            on:click=move |_| signal.set(mode)
                        >
                            {label}
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}

// ── Layout toggle component ─────────────────────────────────────────────────

/// Segmented two-button control for choosing the `.app-main` [`LayoutMode`].
///
/// A thin caller of [`seg_toggle`]. `layout` is owned by [`crate::App`]; clicking
/// a segment sets it, which flips the reactive modifier class on `.app-main`.
/// The active segment carries `is-active` + `aria-pressed="true"`. `Stack` is
/// the default.
#[component]
pub fn LayoutToggle(layout: RwSignal<LayoutMode>) -> impl IntoView {
    seg_toggle(layout, "Layout")
}

// ── Display toggle component ─────────────────────────────────────────────────

/// Segmented two-button control for choosing the `#map` [`DisplayMode`].
///
/// A thin caller of [`seg_toggle`]. `display` is owned by [`crate::App`];
/// clicking a segment swaps the rendered map surface (SVG graph ⇄ DOM tree).
/// `Graph` is the default.
#[component]
pub fn DisplayToggle(display: RwSignal<DisplayMode>) -> impl IntoView {
    seg_toggle(display, "Display")
}

// ── Toolbar component ─────────────────────────────────────────────────────────

/// Renders the filter toolbar placed in the header `.toolbar-area`.
///
/// `filter` is owned by `App`; mutations here propagate to the graph (dimming)
/// and remain alive across manifest swaps.
///
/// `manifest` is `None` while still loading — in that case the kind `<select>`
/// is disabled.
#[component]
pub fn Toolbar(filter: RwSignal<FilterState>, manifest: Option<Manifest>) -> impl IntoView {
    let opts = manifest.as_ref().map(kind_options).unwrap_or_default();

    let has_manifest = manifest.is_some();

    view! {
        // ── Search input ──────────────────────────────────────────────────
        <input
            type="search"
            class="toolbar-search"
            aria-label="Search nodes"
            placeholder="Search\u{2026}"
            prop:value=move || filter.get().query.clone()
            on:input=move |ev| {
                let val = event_target_value(&ev);
                filter.update(|f| f.query = val);
            }
        />

        // ── Kind filter <select> ──────────────────────────────────────────
        <select
            class="toolbar-select"
            aria-label="Filter by type"
            disabled=!has_manifest
            on:change=move |ev| {
                let val = event_target_value(&ev);
                filter.update(|f| {
                    f.kind = if val.is_empty() { None } else { Some(val) };
                });
            }
        >
            <option value="">"all kinds"</option>
            {opts
                .into_iter()
                .map(|opt| {
                    let value = opt.value;
                    let label = opt.label.clone();
                    view! {
                        <option value=value>{label}</option>
                    }
                })
                .collect_view()}
        </select>

        // ── Dead-ends-only checkbox ───────────────────────────────────────
        <label class="toolbar-checkbox-label">
            <input
                type="checkbox"
                class="toolbar-checkbox"
                prop:checked=move || filter.get().dead_ends_only
                on:change=move |ev| {
                    use leptos::wasm_bindgen::JsCast;
                    let checked = ev
                        .target()
                        .and_then(|t| t.dyn_into::<leptos::web_sys::HtmlInputElement>().ok())
                        .map(|el| el.checked())
                        .unwrap_or(false);
                    filter.update(|f| f.dead_ends_only = checked);
                }
            />
            <span class="toolbar-checkbox-text">"dead ends only"</span>
        </label>
    }
}
