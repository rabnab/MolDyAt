//! Shared sidebar layout constants.
//!
//! These constants define the icon geometry and tool ordering used by
//! both the renderer (WGPU sidebar pipeline) and the hit-testing logic
//! in `UnifiedRenderState`.  Keeping them in one place prevents drift.

use crate::graphics::renderer::event_loop::OperatorTool;

/// Base left sidebar margin in pixels at 100% UI scale.
pub const SIDEBAR_MARGIN_LEFT: f32 = 110.0;

/// Display size for each sidebar icon in pixels (square).
pub const ICON_DISPLAY_SIZE: f32 = 48.0;

/// Vertical spacing between icons in pixels.
pub const ICON_SPACING: f32 = 12.0;

pub fn sidebar_ui_scale(margin_left: f32) -> f32 {
    (margin_left / SIDEBAR_MARGIN_LEFT).max(1.0)
}

pub fn icon_display_size_for_margin(margin_left: f32) -> f32 {
    ICON_DISPLAY_SIZE * sidebar_ui_scale(margin_left)
}

pub fn icon_spacing_for_margin(margin_left: f32) -> f32 {
    ICON_SPACING * sidebar_ui_scale(margin_left)
}

/// Sidebar icon order — single source of truth for icon index ↔ tool.
///
/// The WGPU sidebar pipeline's icon atlas **must** match this order.
pub const SIDEBAR_TOOLS: [OperatorTool; 5] = [
    OperatorTool::Ball,         // 0 — standard-sized ball icon
    OperatorTool::LineSegment,  // 1 — wall icon
    OperatorTool::ValveSegment, // 2 — gate icon
    OperatorTool::Thermostat,   // 3 — temperature icon
    OperatorTool::SourceSphere, // 4 — emitter icon
];

/// Number of icons in the sidebar (derived from [`SIDEBAR_TOOLS`]).
pub const ICON_COUNT: usize = SIDEBAR_TOOLS.len();

/// Compute how many sidebar icons are visible given window height and margins.
///
/// Icons that would extend below `win_h - margin_bottom` are clipped.
pub fn visible_icon_count(win_h: f32, margin_top: f32, margin_bottom: f32) -> usize {
    visible_icon_count_for_margin(win_h, margin_top, margin_bottom, SIDEBAR_MARGIN_LEFT)
}

pub fn visible_icon_count_for_margin(
    win_h: f32,
    margin_top: f32,
    margin_bottom: f32,
    margin_left: f32,
) -> usize {
    let icon_display_size = icon_display_size_for_margin(margin_left);
    let icon_spacing = icon_spacing_for_margin(margin_left);
    let first_icon_top = margin_top + icon_spacing;
    let mut count = 0usize;
    for i in 0..ICON_COUNT {
        let icon_bottom =
            first_icon_top + i as f32 * (icon_display_size + icon_spacing) + icon_display_size;
        if icon_bottom > win_h - margin_bottom {
            break;
        }
        count += 1;
    }
    count
}
