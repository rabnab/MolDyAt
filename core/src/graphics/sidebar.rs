// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared sidebar layout constants.
//!
//! These constants define the icon geometry and the tool list ([`TOOLS`])
//! used by both the renderer (WGPU sidebar pipeline) and the hit-testing
//! logic in `UnifiedRenderState`. Keeping them in one place prevents drift.

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

/// The picture on a tool's sidebar icon. The renderer maps each one to its
/// drawing: `Ball` is drawn procedurally, the others are images in the icon
/// atlas (`icon_image` in the wgpu sidebar pipeline, an exhaustive match, so
/// a new icon cannot be left without one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolIcon {
    Ball,
    Wall,
    Valve,
    Thermostat,
    SourceSphere,
}

/// One operator tool of the sidebar: the tool, its icon and a short name
/// for logs and test messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolDescriptor {
    pub tool: OperatorTool,
    pub icon: ToolIcon,
    pub name: &'static str,
}

/// Every sidebar tool, in icon order: the single table for the icon index,
/// the tool it selects and the icon atlas the renderer builds (#307).
/// Adding, renaming or reordering a tool means editing this list only (plus
/// the tool's `OperatorTool` variant, handler and, for a new picture, its
/// `ToolIcon` and image).
pub const TOOLS: &[ToolDescriptor] = &[
    ToolDescriptor {
        tool: OperatorTool::Ball,
        icon: ToolIcon::Ball,
        name: "ball",
    },
    ToolDescriptor {
        tool: OperatorTool::LineSegment,
        icon: ToolIcon::Wall,
        name: "wall",
    },
    ToolDescriptor {
        tool: OperatorTool::ValveSegment,
        icon: ToolIcon::Valve,
        name: "valve",
    },
    ToolDescriptor {
        tool: OperatorTool::Thermostat,
        icon: ToolIcon::Thermostat,
        name: "thermostat",
    },
    ToolDescriptor {
        tool: OperatorTool::SourceSphere,
        icon: ToolIcon::SourceSphere,
        name: "source sphere",
    },
];

/// Number of icons in the sidebar (derived from [`TOOLS`]).
pub const ICON_COUNT: usize = TOOLS.len();

/// The tool of sidebar icon `index`, or [`OperatorTool::None`] past the end.
pub fn tool_for_icon(index: usize) -> OperatorTool {
    TOOLS
        .get(index)
        .map_or(OperatorTool::None, |descriptor| descriptor.tool)
}

/// The sidebar icon index of `tool`, or `None` for a tool without an icon.
pub fn icon_for_tool(tool: OperatorTool) -> Option<usize> {
    TOOLS.iter().position(|descriptor| descriptor.tool == tool)
}

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
