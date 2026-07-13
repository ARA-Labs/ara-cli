//! Pure resize-math helpers for the resizable split/stack panel divider.
//!
//! This module contains **only** constants and pure functions — no `web-sys`,
//! no Leptos components. It compiles and is testable on native targets with
//! `cargo test -p ara-viewer`. The Leptos `<Splitter>` component that drives
//! these helpers from pointer/keyboard events lives in a separate module added
//! in a later implementation step.

use crate::state::LayoutMode;

// ── Default ratios ────────────────────────────────────────────────────────────

/// Default map-pane fraction of the full axis in **Split** (side-by-side) mode.
///
/// The map occupies 38 % of the viewport width; the detail pane takes the rest.
pub const SPLIT_DEFAULT_RATIO: f64 = 0.38;

/// Default map-pane fraction of the full axis in **Stack** (top/bottom) mode.
///
/// The map occupies ≈ 2/3 of the viewport height; the detail pane takes the
/// rest. Chosen to give the wide DAG more vertical room than the detail panel.
pub const STACK_DEFAULT_RATIO: f64 = 0.667;

// ── Pane floor constants ──────────────────────────────────────────────────────

/// Minimum pixel width of the **map pane** in Split (side-by-side) mode
/// (the Split axis is horizontal, so the floor governs width).
pub const SPLIT_MAP_MIN_PX: f64 = 320.0;

/// Minimum pixel width of the **detail pane** in Split (side-by-side) mode
/// (the Split axis is horizontal, so the floor governs width).
pub const SPLIT_DETAIL_MIN_PX: f64 = 240.0;

/// Minimum pixel height of the **map pane** in Stack (top/bottom) mode.
pub const STACK_MAP_MIN_PX: f64 = 180.0;

/// Minimum pixel height of the **detail pane** in Stack (top/bottom) mode.
pub const STACK_DETAIL_MIN_PX: f64 = 180.0;

// ── Keyboard step ─────────────────────────────────────────────────────────────

/// Fixed fractional step applied per arrow-key press on the divider.
///
/// Each press shifts the split ratio by ±2 % of the full axis.
pub const KEYBOARD_STEP: f64 = 0.02;

// ── Mode helpers ──────────────────────────────────────────────────────────────

/// Return the shipped default ratio for `layout`.
///
/// - [`LayoutMode::Split`] → [`SPLIT_DEFAULT_RATIO`] (0.38)
/// - [`LayoutMode::Stack`] → [`STACK_DEFAULT_RATIO`] (0.667)
pub fn default_ratio(layout: LayoutMode) -> f64 {
    match layout {
        LayoutMode::Split => SPLIT_DEFAULT_RATIO,
        LayoutMode::Stack => STACK_DEFAULT_RATIO,
    }
}

/// Return the structural pane floors `(pane1_min_px, pane2_min_px)` for
/// `layout`, where pane 1 is the map pane and pane 2 is the detail pane.
///
/// - [`LayoutMode::Split`] → `(`[`SPLIT_MAP_MIN_PX`]`, `[`SPLIT_DETAIL_MIN_PX`]`)`
///   i.e. `(320.0, 240.0)`
/// - [`LayoutMode::Stack`] → `(`[`STACK_MAP_MIN_PX`]`, `[`STACK_DETAIL_MIN_PX`]`)`
///   i.e. `(180.0, 180.0)`
pub fn floors_for(layout: LayoutMode) -> (f64, f64) {
    match layout {
        LayoutMode::Split => (SPLIT_MAP_MIN_PX, SPLIT_DETAIL_MIN_PX),
        LayoutMode::Stack => (STACK_MAP_MIN_PX, STACK_DETAIL_MIN_PX),
    }
}

// ── Core clamp ───────────────────────────────────────────────────────────────

/// Clamp a desired map-pane fraction to the valid window imposed by pane floors
/// and the gutter.
///
/// # Arguments
///
/// - `raw` — desired map fraction of the **full** axis (0.0–1.0).
/// - `axis_px` — total axis size in pixels (viewport width for Split, height
///   for Stack).
/// - `gutter_px` — width/height of the fixed divider handle, subtracted from
///   the shareable space.
/// - `pane1_min_px` — minimum pixel size of the map (first) pane.
/// - `pane2_min_px` — minimum pixel size of the detail (second) pane.
///
/// # Return value
///
/// `raw` clamped into `[lo, hi]` where:
///
/// ```text
/// lo = pane1_min_px / axis_px
/// hi = (axis_px - gutter_px - pane2_min_px) / axis_px
/// ```
///
/// The two panes share `axis_px - gutter_px` of space.
///
/// # Robustness and fallback behaviour
///
/// This function **never panics** and **never returns NaN or ±inf** regardless
/// of the inputs:
///
/// - If `axis_px` is not finite or `<= 0`, the bounds cannot be computed; the
///   function returns `raw.clamp(0.0, 1.0)` (sanitising non-finite `raw` to
///   `0.5` first), so the caller always gets a valid fraction.
/// - If the window is degenerate (`lo > hi`) — which happens when the axis is
///   too small to honour both floors simultaneously — `lo` is preferred (map
///   floor wins) but clamped into `[0.0, 1.0]` to stay in range.
/// - A non-finite `raw` is sanitised to `0.5` before clamping.
pub fn clamp_split_ratio(
    raw: f64,
    axis_px: f64,
    gutter_px: f64,
    pane1_min_px: f64,
    pane2_min_px: f64,
) -> f64 {
    // Sanitise raw: non-finite input becomes the neutral midpoint.
    let raw = if raw.is_finite() { raw } else { 0.5 };

    // Guard: degenerate axis — cannot derive meaningful bounds.
    if !axis_px.is_finite() || axis_px <= 0.0 {
        return raw.clamp(0.0, 1.0);
    }

    let lo = pane1_min_px / axis_px;
    let hi = (axis_px - gutter_px - pane2_min_px) / axis_px;

    if !lo.is_finite() || !hi.is_finite() {
        // Degenerate floors/gutter — fall back to sanitised raw in [0,1].
        return raw.clamp(0.0, 1.0);
    }

    if lo > hi {
        // Collapsed window: axis too small for both floors. Map floor wins;
        // clamp into [0,1] so the fraction stays valid.
        return lo.clamp(0.0, 1.0);
    }

    raw.clamp(lo, hi)
}

// ── Keyboard step (pointer-parity wrapper) ────────────────────────────────────

/// Apply a keyboard step to the current split ratio.
///
/// Computes `current + delta` and delegates to [`clamp_split_ratio`] with the
/// same axis/gutter/floor arguments. This structural delegation guarantees that
/// the keyboard path and the pointer path produce **identical** results for the
/// same target ratio — parity is structural, not coincidental.
///
/// # Arguments
///
/// - `current` — current map-pane fraction.
/// - `delta` — signed step to apply (e.g. [`KEYBOARD_STEP`] or
///   `-`[`KEYBOARD_STEP`]).
/// - remaining args — forwarded verbatim to [`clamp_split_ratio`].
pub fn step_ratio(
    current: f64,
    delta: f64,
    axis_px: f64,
    gutter_px: f64,
    pane1_min_px: f64,
    pane2_min_px: f64,
) -> f64 {
    clamp_split_ratio(current + delta, axis_px, gutter_px, pane1_min_px, pane2_min_px)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Shared test parameters — realistic viewport, 6 px gutter.
    const AXIS: f64 = 1000.0;
    const GUTTER: f64 = 6.0;

    // ── default_ratio and floors_for ──────────────────────────────────────────

    #[test]
    fn default_ratio_returns_per_mode_values() {
        assert_eq!(default_ratio(LayoutMode::Split), SPLIT_DEFAULT_RATIO);
        assert_eq!(default_ratio(LayoutMode::Stack), STACK_DEFAULT_RATIO);
    }

    #[test]
    fn floors_for_returns_per_mode_values() {
        assert_eq!(
            floors_for(LayoutMode::Split),
            (SPLIT_MAP_MIN_PX, SPLIT_DETAIL_MIN_PX)
        );
        assert_eq!(
            floors_for(LayoutMode::Split),
            (320.0, 240.0),
            "Split floors must be (320, 240)"
        );
        assert_eq!(
            floors_for(LayoutMode::Stack),
            (STACK_MAP_MIN_PX, STACK_DETAIL_MIN_PX)
        );
        assert_eq!(
            floors_for(LayoutMode::Stack),
            (180.0, 180.0),
            "Stack floors must be (180, 180)"
        );
    }

    // ── Split floors respected at both ends ───────────────────────────────────

    #[test]
    fn split_floors_lower_bound_respected() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        let result = clamp_split_ratio(0.0, AXIS, GUTTER, map_min, detail_min);
        let lo = map_min / AXIS;
        assert_eq!(result, lo, "result must equal lo when raw=0");
        let map_px = result * AXIS;
        assert!(
            map_px >= map_min,
            "map pane ({map_px:.1} px) must be >= map floor ({map_min:.1} px)"
        );
    }

    #[test]
    fn split_floors_upper_bound_respected() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        let result = clamp_split_ratio(1.0, AXIS, GUTTER, map_min, detail_min);
        let hi = (AXIS - GUTTER - detail_min) / AXIS;
        assert_eq!(result, hi, "result must equal hi when raw=1");
        let detail_px = AXIS - GUTTER - result * AXIS;
        assert!(
            detail_px >= detail_min,
            "detail pane ({detail_px:.1} px) must be >= detail floor ({detail_min:.1} px)"
        );
    }

    // ── Stack floors respected at both ends ───────────────────────────────────

    #[test]
    fn stack_floors_lower_bound_respected() {
        let (map_min, detail_min) = floors_for(LayoutMode::Stack);
        let result = clamp_split_ratio(0.0, AXIS, GUTTER, map_min, detail_min);
        let lo = map_min / AXIS;
        assert_eq!(result, lo, "result must equal lo when raw=0 (Stack)");
        let map_px = result * AXIS;
        assert!(
            map_px >= map_min,
            "map pane ({map_px:.1} px) must be >= stack map floor ({map_min:.1} px)"
        );
    }

    #[test]
    fn stack_floors_upper_bound_respected() {
        let (map_min, detail_min) = floors_for(LayoutMode::Stack);
        let result = clamp_split_ratio(1.0, AXIS, GUTTER, map_min, detail_min);
        let hi = (AXIS - GUTTER - detail_min) / AXIS;
        assert_eq!(result, hi, "result must equal hi when raw=1 (Stack)");
        let detail_px = AXIS - GUTTER - result * AXIS;
        assert!(
            detail_px >= detail_min,
            "detail pane ({detail_px:.1} px) must be >= stack detail floor ({detail_min:.1} px)"
        );
    }

    // ── Gutter correctly lowers the upper bound ───────────────────────────────

    #[test]
    fn larger_gutter_lowers_hi() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        let hi_small_gutter = (AXIS - GUTTER - detail_min) / AXIS;
        let hi_large_gutter = (AXIS - 24.0 - detail_min) / AXIS;
        assert!(
            hi_large_gutter < hi_small_gutter,
            "hi must decrease as gutter grows: {hi_large_gutter:.4} < {hi_small_gutter:.4}"
        );
        // Verify both bounds are what clamp_split_ratio returns at raw=1.
        let r_small = clamp_split_ratio(1.0, AXIS, GUTTER, map_min, detail_min);
        let r_large = clamp_split_ratio(1.0, AXIS, 24.0, map_min, detail_min);
        assert_eq!(r_small, hi_small_gutter);
        assert_eq!(r_large, hi_large_gutter);
        assert!(r_large < r_small);
    }

    // ── Midpoint pass-through ─────────────────────────────────────────────────

    #[test]
    fn midpoint_inside_window_passes_through() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        // 0.5 is well within the Split window on a 1000 px axis (lo≈0.32, hi≈0.754)
        let result = clamp_split_ratio(0.5, AXIS, GUTTER, map_min, detail_min);
        assert_eq!(result, 0.5, "0.5 must pass through unchanged");
    }

    #[test]
    fn stack_midpoint_passes_through() {
        let (map_min, detail_min) = floors_for(LayoutMode::Stack);
        // lo=0.18, hi≈0.814 for Stack on 1000 px axis with 6 px gutter.
        let result = clamp_split_ratio(0.5, AXIS, GUTTER, map_min, detail_min);
        assert_eq!(result, 0.5, "0.5 must pass through unchanged (Stack)");
    }

    // ── Degenerate axis ───────────────────────────────────────────────────────

    #[test]
    fn degenerate_axis_zero_is_finite_and_in_range() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        let result = clamp_split_ratio(0.38, 0.0, GUTTER, map_min, detail_min);
        assert!(result.is_finite(), "result must be finite for axis=0");
        assert!((0.0..=1.0).contains(&result), "result must be in [0,1]");
    }

    #[test]
    fn degenerate_axis_tiny_smaller_than_floors_is_finite_and_in_range() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        // axis=10 is much smaller than map_min(320)+detail_min(240)+gutter(6).
        let result = clamp_split_ratio(0.38, 10.0, GUTTER, map_min, detail_min);
        assert!(result.is_finite(), "result must be finite for tiny axis");
        assert!((0.0..=1.0).contains(&result), "result must be in [0,1]");
    }

    #[test]
    fn degenerate_axis_negative_is_finite_and_in_range() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        let result = clamp_split_ratio(0.38, -500.0, GUTTER, map_min, detail_min);
        assert!(result.is_finite(), "result must be finite for axis<0");
        assert!((0.0..=1.0).contains(&result), "result must be in [0,1]");
    }

    #[test]
    fn degenerate_raw_nan_is_finite_and_in_range() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        let result = clamp_split_ratio(f64::NAN, AXIS, GUTTER, map_min, detail_min);
        assert!(result.is_finite(), "result must be finite for raw=NaN");
        assert!((0.0..=1.0).contains(&result), "result must be in [0,1]");
    }

    #[test]
    fn degenerate_raw_inf_is_finite_and_in_range() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        let result = clamp_split_ratio(f64::INFINITY, AXIS, GUTTER, map_min, detail_min);
        assert!(result.is_finite(), "result must be finite for raw=+inf");
        assert!((0.0..=1.0).contains(&result), "result must be in [0,1]");
    }

    #[test]
    fn degenerate_raw_neg_inf_is_finite_and_in_range() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        let result = clamp_split_ratio(f64::NEG_INFINITY, AXIS, GUTTER, map_min, detail_min);
        assert!(result.is_finite(), "result must be finite for raw=-inf");
        assert!((0.0..=1.0).contains(&result), "result must be in [0,1]");
    }

    #[test]
    fn degenerate_axis_nan_is_finite_and_in_range() {
        let (map_min, detail_min) = floors_for(LayoutMode::Stack);
        let result = clamp_split_ratio(0.5, f64::NAN, GUTTER, map_min, detail_min);
        assert!(result.is_finite(), "result must be finite for axis=NaN");
        assert!((0.0..=1.0).contains(&result), "result must be in [0,1]");
    }

    // ── Key-step / pointer parity ─────────────────────────────────────────────

    #[test]
    fn step_ratio_parity_split_floors_forward() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        let c = 0.38_f64;
        assert_eq!(
            step_ratio(c, KEYBOARD_STEP, AXIS, GUTTER, map_min, detail_min),
            clamp_split_ratio(c + KEYBOARD_STEP, AXIS, GUTTER, map_min, detail_min),
            "step_ratio(+KEYBOARD_STEP) must equal clamp(c+delta) for Split"
        );
    }

    #[test]
    fn step_ratio_parity_split_floors_backward() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        let c = 0.38_f64;
        assert_eq!(
            step_ratio(c, -KEYBOARD_STEP, AXIS, GUTTER, map_min, detail_min),
            clamp_split_ratio(c - KEYBOARD_STEP, AXIS, GUTTER, map_min, detail_min),
            "step_ratio(-KEYBOARD_STEP) must equal clamp(c-delta) for Split"
        );
    }

    #[test]
    fn step_ratio_parity_stack_floors_forward() {
        let (map_min, detail_min) = floors_for(LayoutMode::Stack);
        let c = 0.667_f64;
        assert_eq!(
            step_ratio(c, KEYBOARD_STEP, AXIS, GUTTER, map_min, detail_min),
            clamp_split_ratio(c + KEYBOARD_STEP, AXIS, GUTTER, map_min, detail_min),
            "step_ratio(+KEYBOARD_STEP) must equal clamp(c+delta) for Stack"
        );
    }

    #[test]
    fn step_ratio_parity_stack_floors_backward() {
        let (map_min, detail_min) = floors_for(LayoutMode::Stack);
        let c = 0.667_f64;
        assert_eq!(
            step_ratio(c, -KEYBOARD_STEP, AXIS, GUTTER, map_min, detail_min),
            clamp_split_ratio(c - KEYBOARD_STEP, AXIS, GUTTER, map_min, detail_min),
            "step_ratio(-KEYBOARD_STEP) must equal clamp(c-delta) for Stack"
        );
    }

    /// Parity also holds at the boundary: stepping beyond lo/hi returns the same
    /// clamped value from both paths.
    #[test]
    fn step_ratio_parity_at_lower_boundary() {
        let (map_min, detail_min) = floors_for(LayoutMode::Split);
        // Start at lo and step down: both paths must clamp to lo.
        let lo = map_min / AXIS;
        assert_eq!(
            step_ratio(lo, -KEYBOARD_STEP, AXIS, GUTTER, map_min, detail_min),
            clamp_split_ratio(lo - KEYBOARD_STEP, AXIS, GUTTER, map_min, detail_min)
        );
    }
}
