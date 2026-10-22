// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Sidebar icon hit-testing and operator tool mapping.
//!
//! Extends [`UnifiedRenderState`] with methods for detecting which
//! sidebar icon the user clicked and mapping icon indices to
//! [`OperatorTool`] variants.

use super::input_handler::OperatorTool;
use super::unified_render_state::UnifiedRenderState;

impl UnifiedRenderState {
    /// Returns the sidebar icon index (0-based) at the given screen pixel
    /// position, or `None` if the click is outside all icons.
    ///
    /// Icons that would be clipped below the bottom margin are excluded
    /// (matching the WGPU sidebar pipeline's `visible_count` logic).
    pub fn sidebar_icon_at(&self, pos_px: [f32; 2]) -> Option<usize> {
        use crate::graphics::sidebar::{
            icon_display_size_for_margin, icon_spacing_for_margin, visible_icon_count_for_margin,
        };

        let margins = self.render.viewport.margins();
        let icon_display_size = icon_display_size_for_margin(margins.left);
        let icon_spacing = icon_spacing_for_margin(margins.left);
        let pad_x = (margins.left - icon_display_size) * 0.5;
        let first_icon_top = margins.top + icon_spacing;

        // Horizontal bounds check
        if pos_px[0] < pad_x || pos_px[0] > pad_x + icon_display_size {
            return None;
        }

        let (_, win_h) = self.render.viewport.window_size_px();
        let visible =
            visible_icon_count_for_margin(win_h, margins.top, margins.bottom, margins.left);

        for i in 0..visible {
            let icon_top = first_icon_top + i as f32 * (icon_display_size + icon_spacing);
            let icon_bottom = icon_top + icon_display_size;

            if pos_px[1] >= icon_top && pos_px[1] <= icon_bottom {
                return Some(i);
            }
        }
        None
    }

    /// Maps a sidebar icon index to an `OperatorTool` ([`TOOLS`](crate::graphics::sidebar::TOOLS)).
    pub(super) fn operator_tool_for_icon(index: usize) -> OperatorTool {
        crate::graphics::sidebar::tool_for_icon(index)
    }

    /// Returns the sidebar icon index of the currently active tool, if any
    /// ([`TOOLS`](crate::graphics::sidebar::TOOLS)).
    pub fn selected_sidebar_icon(&self) -> Option<usize> {
        crate::graphics::sidebar::icon_for_tool(self.render.active_tool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
    use crate::graphics::renderer::event_loop::metrics::MetricsCollector;
    use crate::graphics::renderer::event_loop::unified_render_state::RenderStateData;
    use crate::graphics::renderer::event_loop::{EnergyWindowManager, InputHandler};
    use crate::graphics::renderer::state_layers::RendererConfig;
    use crate::graphics::renderer::{Margins, ViewportTransform};
    use crate::objects2d::{Rect, Vec2D};
    use std::sync::{Arc, Mutex};

    use super::super::PhysicsCoordinator;

    fn make_disconnected_coordinator() -> PhysicsCoordinator {
        PhysicsCoordinator::new_disconnected()
    }

    fn sidebar_viewport() -> ViewportTransform {
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let margins = Margins::new(110.0, 50.0, 10.0, 50.0);
        ViewportTransform::new(world, (1280.0, 720.0), margins)
    }

    fn build_test_state(
        viewport: ViewportTransform,
        physics: PhysicsCoordinator,
    ) -> UnifiedRenderState {
        let energy_data: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));
        UnifiedRenderState {
            input: InputHandler::new(),
            physics,
            render: RenderStateData {
                viewport,
                energy_manager: EnergyWindowManager::new(energy_data),
                config: RendererConfig::default(),
                view_dirty: false,
                last_snapshot: None,
                cursor_world_pos: Vec2D::zero(),
                metrics: MetricsCollector::new(),
                active_tool: OperatorTool::None,
                tool_handler: None,
                default_operator_snapshot: Default::default(),
                resize_pending: false,
                requested_window_size: None,
                pending_window_request: None,
            },
        }
    }

    #[test]
    fn sidebar_icon_at_returns_index_for_first_icon() {
        use crate::graphics::sidebar::{icon_display_size_for_margin, icon_spacing_for_margin};

        let vp = sidebar_viewport();
        let margins = vp.margins();
        let icon_display_size = icon_display_size_for_margin(margins.left);
        let icon_spacing = icon_spacing_for_margin(margins.left);
        let pad_x = (margins.left - icon_display_size) * 0.5;
        let first_top = margins.top + icon_spacing;
        let cx = pad_x + icon_display_size * 0.5;
        let cy = first_top + icon_display_size * 0.5;

        let physics = make_disconnected_coordinator();
        let state = build_test_state(vp, physics);
        assert_eq!(state.sidebar_icon_at([cx, cy]), Some(0));
    }

    #[test]
    fn sidebar_icon_at_returns_none_outside_icons() {
        let vp = sidebar_viewport();
        let physics = make_disconnected_coordinator();
        let state = build_test_state(vp, physics);
        assert_eq!(state.sidebar_icon_at([1000.0, 100.0]), None);
    }

    #[test]
    fn sidebar_icon_at_clips_when_window_too_short() {
        use crate::graphics::sidebar::{icon_display_size_for_margin, icon_spacing_for_margin};

        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let margins = Margins::new(110.0, 50.0, 10.0, 50.0);
        let vp = ViewportTransform::new(world, (1280.0, 170.0), margins);
        let physics = make_disconnected_coordinator();
        let state = build_test_state(vp, physics);

        let icon_display_size = icon_display_size_for_margin(margins.left);
        let icon_spacing = icon_spacing_for_margin(margins.left);
        let pad_x = (margins.left - icon_display_size) * 0.5;
        let first_top = margins.top + icon_spacing;

        let cy0 = first_top + icon_display_size * 0.5;
        assert_eq!(state.sidebar_icon_at([pad_x + 24.0, cy0]), Some(0));

        let cy1 = first_top + (icon_display_size + icon_spacing) + icon_display_size * 0.5;
        assert_eq!(
            state.sidebar_icon_at([pad_x + 24.0, cy1]),
            None,
            "Icon 1 should be clipped in short window"
        );
    }

    #[test]
    fn operator_tool_toggle() {
        assert_eq!(
            UnifiedRenderState::operator_tool_for_icon(0),
            OperatorTool::Ball
        );
        assert_eq!(
            UnifiedRenderState::operator_tool_for_icon(1),
            OperatorTool::LineSegment
        );
        assert_eq!(
            UnifiedRenderState::operator_tool_for_icon(2),
            OperatorTool::ValveSegment
        );
        assert_eq!(
            UnifiedRenderState::operator_tool_for_icon(99),
            OperatorTool::None
        );
    }

    #[test]
    fn visible_icon_count_all_fit() {
        use crate::graphics::sidebar::visible_icon_count;
        assert_eq!(visible_icon_count(720.0, 50.0, 50.0), 5);
    }

    #[test]
    fn visible_icon_count_none_fit() {
        use crate::graphics::sidebar::visible_icon_count;
        assert_eq!(visible_icon_count(100.0, 50.0, 50.0), 0);
    }

    /// Every tool appears once, and icon index and tool map to each other
    /// through the one list (#307).
    #[test]
    fn tool_list_maps_icons_and_tools_both_ways() {
        use crate::graphics::sidebar::{ICON_COUNT, TOOLS, icon_for_tool, tool_for_icon};
        for (index, descriptor) in TOOLS.iter().enumerate() {
            assert_eq!(tool_for_icon(index), descriptor.tool, "{}", descriptor.name);
            assert_eq!(
                icon_for_tool(descriptor.tool),
                Some(index),
                "{}",
                descriptor.name
            );
            assert_eq!(
                UnifiedRenderState::operator_tool_for_icon(index),
                descriptor.tool
            );
        }
        assert_eq!(tool_for_icon(ICON_COUNT), OperatorTool::None);
        assert_eq!(icon_for_tool(OperatorTool::None), None);
        let mut names: Vec<_> = TOOLS.iter().map(|descriptor| descriptor.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ICON_COUNT, "tool names are unique");
    }
}
