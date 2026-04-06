//! Shared sidebar layout constants.
//!
//! These constants define the icon geometry used by both the renderer
//! (WGPU sidebar pipeline) and the hit-testing logic in
//! `UnifiedRenderState`.  Keeping them in one place prevents drift.

/// Display size for each sidebar icon in pixels (square).
pub const ICON_DISPLAY_SIZE: f32 = 48.0;

/// Vertical spacing between icons in pixels.
pub const ICON_SPACING: f32 = 12.0;

/// Number of icons in the sidebar.
pub const ICON_COUNT: usize = 4;

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
