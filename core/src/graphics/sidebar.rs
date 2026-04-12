//! Shared sidebar layout constants.
//!
//! These constants define the icon geometry and tool ordering used by
//! both the renderer (WGPU sidebar pipeline) and the hit-testing logic
//! in `UnifiedRenderState`.  Keeping them in one place prevents drift.

use crate::graphics::renderer::event_loop::OperatorTool;

/// Display size for each sidebar icon in pixels (square).
pub const ICON_DISPLAY_SIZE: f32 = 48.0;

/// Vertical spacing between icons in pixels.
pub const ICON_SPACING: f32 = 12.0;

/// Sidebar icon order — single source of truth for icon index ↔ tool.
///
/// The WGPU sidebar pipeline's icon atlas **must** match this order.
pub const SIDEBAR_TOOLS: [OperatorTool; 4] = [
    OperatorTool::LineSegment,  // 0 — wall icon
    OperatorTool::ValveSegment, // 1 — gate icon
    OperatorTool::Thermostat,   // 2 — temperature icon
    OperatorTool::SourceSphere, // 3 — emitter icon
];

/// Number of icons in the sidebar (derived from [`SIDEBAR_TOOLS`]).
pub const ICON_COUNT: usize = SIDEBAR_TOOLS.len();

/// Compute how many sidebar icons are visible given window height and margins.
///
/// Icons that would extend below `win_h - margin_bottom` are clipped.
pub fn visible_icon_count(win_h: f32, margin_top: f32, margin_bottom: f32) -> usize {
    let first_icon_top = margin_top + ICON_SPACING;
    let mut count = 0usize;
    for i in 0..ICON_COUNT {
        let icon_bottom =
            first_icon_top + i as f32 * (ICON_DISPLAY_SIZE + ICON_SPACING) + ICON_DISPLAY_SIZE;
        if icon_bottom > win_h - margin_bottom {
            break;
        }
        count += 1;
    }
    count
}
