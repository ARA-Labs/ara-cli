//! Replay stepper — pure step/counter helpers + the `ReplayBar` component.
//!
//! The replay works in **both** display modes: it steps the shared `selected`
//! signal through node order, matching the published `research-visualizer`
//! `step` / `play` / `stop`. The pure helpers here are native-testable; the
//! `ReplayBar` component (and its 1300 ms interval) lands with the wiring.

use ara_core::{Manifest, NodeId};
use leptos::prelude::*;

// ── Pure helpers ──────────────────────────────────────────────────────────────

/// Direction of a single replay step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Next,
    Prev,
}

/// Node traversal order for replay: `manifest.nodes` order.
///
/// The manifest contract guarantees `nodes` is pre-order DFS, which equals the
/// reference's "`DATA.order` if present, else DFS-from-roots" for our manifests.
pub fn node_order(manifest: &Manifest) -> Vec<NodeId> {
    manifest.nodes.iter().map(|n| n.id.clone()).collect()
}

/// Advance / retreat the selection by one step, reproducing the reference
/// `step(delta)` semantics exactly:
///
/// `i = clamp(0, N-1, indexOf(current) + delta)`, with `indexOf(None) = -1` and
/// an unknown id treated the same as `None` (`-1`).
///
/// Consequences (all matching the reference):
/// - `Next` from `None` → `order[0]`.
/// - `Prev` from `None` → `order[0]` too (`-1 + -1 = -2` clamps to `0`) — **not**
///   the last node.
/// - Clamps at both ends (no wrap).
///
/// Returns `None` only when `order` is empty.
pub fn step(order: &[NodeId], current: Option<&NodeId>, dir: Step) -> Option<NodeId> {
    if order.is_empty() {
        return None;
    }
    let n = order.len() as isize;
    // indexOf(current); None / unknown → -1.
    let idx = current
        .and_then(|c| order.iter().position(|id| id == c))
        .map(|p| p as isize)
        .unwrap_or(-1);
    let delta = match dir {
        Step::Next => 1,
        Step::Prev => -1,
    };
    let clamped = (idx + delta).clamp(0, n - 1);
    Some(order[clamped as usize].clone())
}

/// Replay counter `(i, N)`: `i` is the **1-based** position of `current` in
/// `order`, or `0` when there is no selection (or an unknown id). `N` is the
/// total node count.
pub fn counter(order: &[NodeId], current: Option<&NodeId>) -> (usize, usize) {
    let n = order.len();
    let i = current
        .and_then(|c| order.iter().position(|id| id == c))
        .map(|p| p + 1)
        .unwrap_or(0);
    (i, n)
}

/// The shared `#rstat` readout string.
///
/// Two forms, matching the reference's single shared span:
/// - **replay form** (a node is selected): `"step {i} / {N}"`.
/// - **filter form** (nothing selected): `"{shown} / {N} steps"`, where `shown`
///   is the number of nodes passing the current filter.
///
/// When a node is selected the replay form wins (the reference's `rstat` write
/// on select overrides the filter write).
pub fn rstat_text(order: &[NodeId], current: Option<&NodeId>, shown: usize) -> String {
    let (i, n) = counter(order, current);
    if i > 0 {
        format!("step {i} / {n}")
    } else {
        format!("{shown} / {n} steps")
    }
}

// ── Replay runtime state + control helpers ───────────────────────────────────

/// Replay interval, matching the reference (`1300 ms`).
#[cfg(target_arch = "wasm32")]
const REPLAY_INTERVAL_MS: u64 = 1300;

/// Imperative runtime state for the replay interval, owned by `App` so both the
/// [`ReplayBar`] buttons and the document-level arrow-key listener share one
/// timer. `playing` drives the button label; `handle` holds the live interval
/// (a `StoredValue`, since it is imperative timer state, not reactive view
/// state). Both are `Copy`.
#[derive(Clone, Copy)]
pub struct ReplayState {
    pub playing: RwSignal<bool>,
    pub handle: StoredValue<Option<IntervalHandle>>,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            playing: RwSignal::new(false),
            handle: StoredValue::new(None),
        }
    }
}

/// Clear the interval (if any) and reset the playing flag. Safe on native
/// (the handle is always `None` there).
pub fn stop_replay(state: ReplayState) {
    state.handle.update_value(|h| {
        if let Some(h) = h.take() {
            h.clear();
        }
    });
    state.playing.set(false);
}

/// Advance / retreat the selection by one step through `order`, clamping.
pub fn advance(order: &[NodeId], selected: RwSignal<Option<NodeId>>, dir: Step) {
    if let Some(next) = step(order, selected.get().as_ref(), dir) {
        selected.set(Some(next));
    }
}

// ── Arrow-key listener (wasm-only) ────────────────────────────────────────────

/// Install a document-level `keydown` listener that steps the replay with the
/// `←` / `→` keys, mirroring the reference guard **exactly**: it ignores the
/// event when focus is in an `INPUT` or `SELECT` (so arrows don't hijack the
/// search field). `ArrowLeft` → `stop_replay` + `step(-1)`; `ArrowRight` →
/// `stop_replay` + `step(+1)`.
///
/// Extracted from `App` so the guard is testable against the real code. The
/// closure is leaked (`forget`) so it lives for the app's lifetime — the
/// listener is document-scoped and outlives no shorter than the viewer.
#[cfg(target_arch = "wasm32")]
pub fn install_arrow_key_listener(
    order: Memo<Vec<NodeId>>,
    selected: RwSignal<Option<NodeId>>,
    state: ReplayState,
) {
    use leptos::wasm_bindgen::JsCast;
    use leptos::wasm_bindgen::prelude::Closure;

    let handler = Closure::<dyn FnMut(leptos::web_sys::KeyboardEvent)>::new(
        move |ev: leptos::web_sys::KeyboardEvent| {
            // Reference guard: skip when typing in a form control.
            if let Some(target) = ev.target()
                && let Some(el) = target.dyn_ref::<leptos::web_sys::Element>()
            {
                let tag = el.tag_name();
                if tag == "INPUT" || tag == "SELECT" {
                    return;
                }
            }
            let dir = match ev.key().as_str() {
                "ArrowLeft" => Some(Step::Prev),
                "ArrowRight" => Some(Step::Next),
                _ => None,
            };
            if let Some(dir) = dir {
                stop_replay(state);
                advance(&order.get(), selected, dir);
            }
        },
    );
    if let Some(doc) = leptos::web_sys::window().and_then(|w| w.document()) {
        let _ =
            doc.add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref());
    }
    handler.forget();
}

// ── ReplayBar component ───────────────────────────────────────────────────────

/// The replay controls: `‹` (prev) / `▶ Replay`⇄`⏸ Pause` (play) / `›` (next).
///
/// Works in **both** display modes; steps the shared `selected` signal through
/// `order`. Prev/next call [`stop_replay`] first (per the reference). Play
/// toggles a 1300 ms interval: if nothing is selected it selects `order[0]`,
/// then each tick advances; at the last node it auto-stops (no wrap, no loop).
/// The timer is wasm-only; on native the play button is inert. `state` is owned
/// by `App`, which tears the interval down on unmount and shares it with the
/// arrow-key listener.
#[component]
pub fn ReplayBar(
    order: Memo<Vec<NodeId>>,
    selected: RwSignal<Option<NodeId>>,
    state: ReplayState,
) -> impl IntoView {
    let playing = state.playing;

    // Prev / next: stop() first, then a single step (reference order).
    let on_prev = move |_| {
        stop_replay(state);
        advance(&order.get(), selected, Step::Prev);
    };
    let on_next = move |_| {
        stop_replay(state);
        advance(&order.get(), selected, Step::Next);
    };

    // Play toggle. The interval is wasm-only; on native this just no-ops the
    // timer (the button still renders but does not animate).
    let on_play = move |_| {
        if playing.get() {
            stop_replay(state);
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let ord = order.get();
            if ord.is_empty() {
                return;
            }
            // If nothing is selected, start at the first node.
            if selected.get().is_none() {
                selected.set(Some(ord[0].clone()));
            }
            playing.set(true);
            let tick = move || {
                let ord = order.get();
                let cur = selected.get();
                let (i, n) = counter(&ord, cur.as_ref());
                // At (or past) the last node → auto-stop, no wrap.
                if n == 0 || i >= n {
                    stop_replay(state);
                    return;
                }
                if let Some(next) = step(&ord, cur.as_ref(), Step::Next) {
                    selected.set(Some(next));
                }
                // If that step landed on the last node, stop after it.
                let (i2, n2) = counter(&ord, selected.get().as_ref());
                if i2 >= n2 {
                    stop_replay(state);
                }
            };
            if let Ok(h) = set_interval_with_handle(
                tick,
                std::time::Duration::from_millis(REPLAY_INTERVAL_MS),
            ) {
                state.handle.set_value(Some(h));
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Native: no timer. Mark playing so the label reflects intent in
            // native component tests; a subsequent click stops it.
            playing.set(true);
        }
    };

    // Belt-and-suspenders teardown: if the bar itself unmounts (e.g. the map
    // surface swaps), clear the interval here too. App also tears it down on its
    // own unmount, sharing the same handle — clearing twice is safe (idempotent).
    on_cleanup(move || stop_replay(state));

    let play_label = move || {
        if playing.get() {
            "\u{23f8} Pause"
        } else {
            "\u{25b6} Replay"
        }
    };

    view! {
        <div class="replay-controls" role="group" aria-label="Replay">
            <button type="button" class="btn" id="rprev" aria-label="Previous step" on:click=on_prev>
                "\u{2039}"
            </button>
            <button type="button" class="btn primary" id="rplay" on:click=on_play>
                {play_label}
            </button>
            <button type="button" class="btn" id="rnext" aria-label="Next step" on:click=on_next>
                "\u{203a}"
            </button>
        </div>
    }
}

// ── Tests (native) ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::parse_manifest;

    fn ids(strs: &[&str]) -> Vec<NodeId> {
        strs.iter().map(|s| NodeId::new(*s)).collect()
    }

    // ── node_order ──────────────────────────────────────────────────────────────

    #[test]
    fn node_order_equals_manifest_nodes_order() {
        let json = include_str!("../public/manifest.json");
        let manifest = parse_manifest(json).unwrap();
        let order = node_order(&manifest);
        let expected: Vec<NodeId> = manifest.nodes.iter().map(|n| n.id.clone()).collect();
        assert_eq!(order, expected);
        assert_eq!(order.first(), Some(&NodeId::new("N01")));
        assert_eq!(order.len(), 15);
    }

    // ── step: clamp at both ends ────────────────────────────────────────────────

    #[test]
    fn step_next_advances() {
        let order = ids(&["N01", "N02", "N03"]);
        assert_eq!(
            step(&order, Some(&NodeId::new("N01")), Step::Next),
            Some(NodeId::new("N02"))
        );
    }

    #[test]
    fn step_prev_retreats() {
        let order = ids(&["N01", "N02", "N03"]);
        assert_eq!(
            step(&order, Some(&NodeId::new("N02")), Step::Prev),
            Some(NodeId::new("N01"))
        );
    }

    #[test]
    fn step_next_clamps_at_last() {
        let order = ids(&["N01", "N02", "N03"]);
        assert_eq!(
            step(&order, Some(&NodeId::new("N03")), Step::Next),
            Some(NodeId::new("N03")),
            "Next at last node stays (no wrap)"
        );
    }

    #[test]
    fn step_prev_clamps_at_first() {
        let order = ids(&["N01", "N02", "N03"]);
        assert_eq!(
            step(&order, Some(&NodeId::new("N01")), Step::Prev),
            Some(NodeId::new("N01")),
            "Prev at first node stays (no wrap)"
        );
    }

    // ── step: from None (reference quirk) ────────────────────────────────────────

    #[test]
    fn step_next_from_none_selects_first() {
        let order = ids(&["N01", "N02", "N03"]);
        assert_eq!(step(&order, None, Step::Next), Some(NodeId::new("N01")));
    }

    #[test]
    fn step_prev_from_none_selects_first_not_last() {
        // Reference quirk: indexOf(None) = -1, -1 + -1 = -2 clamps to 0 → first.
        let order = ids(&["N01", "N02", "N03"]);
        assert_eq!(
            step(&order, None, Step::Prev),
            Some(NodeId::new("N01")),
            "Prev from no selection selects the FIRST node (reference clamp)"
        );
    }

    #[test]
    fn step_unknown_id_treated_as_none() {
        let order = ids(&["N01", "N02", "N03"]);
        assert_eq!(
            step(&order, Some(&NodeId::new("N99")), Step::Next),
            Some(NodeId::new("N01")),
            "unknown id → indexOf -1, same as None"
        );
    }

    #[test]
    fn step_empty_order_yields_none() {
        assert_eq!(step(&[], None, Step::Next), None);
        assert_eq!(step(&[], Some(&NodeId::new("N01")), Step::Prev), None);
    }

    // ── counter ─────────────────────────────────────────────────────────────────

    #[test]
    fn counter_is_one_based() {
        let order = ids(&["N01", "N02", "N03"]);
        assert_eq!(counter(&order, Some(&NodeId::new("N01"))), (1, 3));
        assert_eq!(counter(&order, Some(&NodeId::new("N03"))), (3, 3));
    }

    #[test]
    fn counter_zero_when_unselected() {
        let order = ids(&["N01", "N02", "N03"]);
        assert_eq!(counter(&order, None), (0, 3));
        assert_eq!(counter(&order, Some(&NodeId::new("N99"))), (0, 3), "unknown → 0");
    }

    // ── rstat string forms ───────────────────────────────────────────────────────

    #[test]
    fn rstat_replay_form_when_selected() {
        let order = ids(&["N01", "N02", "N03"]);
        assert_eq!(
            rstat_text(&order, Some(&NodeId::new("N02")), 3),
            "step 2 / 3"
        );
    }

    #[test]
    fn rstat_filter_form_when_unselected() {
        let order = ids(&["N01", "N02", "N03"]);
        // 2 of 3 nodes shown by the filter, nothing selected.
        assert_eq!(rstat_text(&order, None, 2), "2 / 3 steps");
    }
}
