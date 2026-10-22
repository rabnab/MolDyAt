// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Sidebar icon rendering pipeline.
//!
//! Draws the tool icons in the left margin of the window on a rounded-rect
//! background, plus a highlight frame behind the selected tool. Icons are
//! arranged vertically from top to bottom in the order of core's tool list
//! (`TOOLS`: ball, drawn procedurally, then the atlas icons). The name of the species new balls get is written
//! centred on the ball icon. Geometry is built in NDC and only rebuilt when
//! the window size, margins, selection or species change. See `sidebar.wgsl`.

use bytemuck::{Pod, Zeroable};
use moldyat_core::application::AppResult;
use moldyat_core::graphics::renderer::Margins;
use moldyat_core::graphics::renderer::event_loop::OperatorTool;
use moldyat_core::graphics::sidebar::{
    ICON_COUNT, TOOLS, ToolIcon, icon_display_size_for_margin, icon_for_tool,
    icon_spacing_for_margin, sidebar_ui_scale, visible_icon_count_for_margin,
};

use crate::wgpu::MARGINS;
use crate::wgpu::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::gpu::geometry::px_to_ndc;
use crate::wgpu::gpu::shaders;
use crate::wgpu::gpu::text::{
    ASCII_CHARS, BLACK_TEXT_COLOR, GLYPH_W, TextBatch, TextVertex, push_label_vertices,
    window_glyph_ndc_size,
};
use crate::wgpu::gpu::textures::{
    ICON_SOURCE_OPEN, ICON_THERMO, ICON_VALVE, ICON_WALL, icon_atlas,
};
use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// The image of a sidebar icon, or `None` for the procedural ball. The atlas
/// and its row lookup are both derived from this match and core's tool list
/// (`TOOLS`), so they cannot drift apart (#307).
pub(crate) fn icon_image(icon: ToolIcon) -> Option<&'static [u8]> {
    match icon {
        ToolIcon::Ball => None,
        ToolIcon::Wall => Some(ICON_WALL),
        ToolIcon::Valve => Some(ICON_VALVE),
        ToolIcon::Thermostat => Some(ICON_THERMO),
        ToolIcon::SourceSphere => Some(ICON_SOURCE_OPEN),
    }
}

/// The images of the textured icons, in atlas row order: the tools of
/// `TOOLS` that have one, in icon order.
pub(crate) fn sidebar_atlas_icons() -> Vec<&'static [u8]> {
    TOOLS
        .iter()
        .filter_map(|descriptor| icon_image(descriptor.icon))
        .collect()
}

/// The atlas row of sidebar icon `index`, or `None` for a procedural icon:
/// the number of textured icons before it.
pub(crate) fn atlas_row_for_icon(index: usize) -> Option<u32> {
    let descriptor = TOOLS.get(index)?;
    icon_image(descriptor.icon)?;
    let row = TOOLS[..index]
        .iter()
        .filter(|earlier| icon_image(earlier.icon).is_some())
        .count();
    Some(row as u32)
}

/// Texture resolution per atlas icon.
const ATLAS_CELL: u32 = 64;

/// Padding around the icon group inside the background rectangle.
pub(crate) const RECT_PADDING: f32 = 8.0;
/// Corner radius for the background rounded rectangle.
const CORNER_RADIUS: f32 = 12.0;
/// Border width for the background rounded rectangle outline.
const BORDER_WIDTH: f32 = 2.0;
/// 6 vertices per quad: 1 background + 1 highlight + ICON_COUNT icon quads.
pub(crate) const MAX_VERTICES: usize = (ICON_COUNT + 2) * 6;

const MODE_BACKGROUND: f32 = 0.0;
const MODE_ATLAS_ICON: f32 = 1.0;
const MODE_HIGHLIGHT: f32 = 2.0;
const MODE_BALL_ICON: f32 = 3.0;

/// Glyph size of the species label on the ball icon, in 8×8 glyphs at UI
/// scale 1 (16 px per glyph).
const BALL_LABEL_GLYPH_SCALE: f32 = 2.0;
/// Widest the species label may be, as a share of the icon width. Longer
/// names get smaller glyphs.
const BALL_LABEL_MAX_WIDTH: f32 = 0.8;

/// Sidebar vertex (NDC). Layout must match `Vertex` in sidebar.wgsl.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct SidebarVertex {
    pub(crate) position: [f32; 2],
    pub(crate) uv: [f32; 2],
    /// 0 = background rounded rect, 1 = atlas icon, 2 = highlight,
    /// 3 = procedural ball icon.
    pub(crate) mode: f32,
}

const SIDEBAR_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32];

/// Rounded-rect background parameters, passed to sidebar.wgsl as `u.params`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct RoundedRectParams {
    pub(crate) size_px: [f32; 2],
    pub(crate) corner_radius: f32,
    pub(crate) border_width: f32,
}

impl RoundedRectParams {
    fn as_params(&self) -> [f32; 4] {
        [
            self.size_px[0],
            self.size_px[1],
            self.corner_radius,
            self.border_width,
        ]
    }
}

/// Sidebar icon rendering pipeline.
///
/// Since icons are static, geometry is only rebuilt when the window size,
/// margins or selection change. No per-frame work is done otherwise.
pub struct SidebarPipeline {
    gpu: GpuBatch<SidebarVertex>,
    /// Name of the species of new balls, drawn on the ball icon.
    ball_label: TextBatch,
    /// Cached window size to skip redundant vertex rebuilds.
    last_window_size: (u32, u32),
    /// Cached selected icon to detect selection changes.
    last_selected_icon: Option<usize>,
    /// Cached margins to detect DPI-scale changes.
    last_margin: Margins,
    /// Cached name of the species of new balls, to rebuild the label when it
    /// changes (by name: a loaded world may bring another species table).
    last_species: Option<String>,
}

impl SidebarPipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let atlas = icon_atlas(
            device,
            queue,
            "Sidebar Icon Atlas",
            &sidebar_atlas_icons(),
            ATLAS_CELL,
        );
        Self {
            gpu: GpuBatch::new(
                device,
                format,
                BatchDesc::vertices(
                    "Sidebar Pipeline",
                    shaders::SIDEBAR,
                    &SIDEBAR_ATTRS,
                    wgpu::PrimitiveTopology::TriangleList,
                )
                .texture(&atlas, wgpu::FilterMode::Linear),
            ),
            ball_label: TextBatch::new(
                device,
                queue,
                format,
                "Sidebar Ball Label",
                &ASCII_CHARS,
                BLACK_TEXT_COLOR,
            ),
            last_window_size: (0, 0),
            last_selected_icon: None,
            last_margin: MARGINS,
            last_species: None,
        }
    }
}

/// Screen-space rectangle in window pixels (y down).
#[derive(Clone, Copy)]
struct PxRect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

/// Append one quad as two triangles (TL, BL, TR, TR, BL, BR). U spans 0..1,
/// V spans `v_top..v_bottom`.
fn push_quad(
    verts: &mut Vec<SidebarVertex>,
    rect: PxRect,
    (v_top, v_bottom): (f32, f32),
    mode: f32,
    (win_w, win_h): (f32, f32),
) {
    let vertex = |x: f32, y: f32, uv: [f32; 2]| SidebarVertex {
        position: px_to_ndc(x, y, win_w, win_h),
        uv,
        mode,
    };
    let tl = vertex(rect.left, rect.top, [0.0, v_top]);
    let bl = vertex(rect.left, rect.bottom, [0.0, v_bottom]);
    let tr = vertex(rect.right, rect.top, [1.0, v_top]);
    let br = vertex(rect.right, rect.bottom, [1.0, v_bottom]);
    verts.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
}

/// Pixel rectangle of sidebar icon `i` (window pixels, y down).
fn icon_px_rect(i: u32, margin: Margins) -> PxRect {
    let icon_size = icon_display_size_for_margin(margin.left);
    let icon_spacing = icon_spacing_for_margin(margin.left);
    let left = (margin.left - icon_size) * 0.5;
    let top = margin.top + icon_spacing + i as f32 * (icon_size + icon_spacing);
    PxRect {
        left,
        top,
        right: left + icon_size,
        bottom: top + icon_size,
    }
}

/// Glyph quads (NDC) for `name` centred on the ball icon. Empty when the ball
/// icon does not fit in the window. Glyphs shrink so the name stays within
/// [`BALL_LABEL_MAX_WIDTH`] of the icon.
pub(crate) fn ball_label_vertices(
    name: &str,
    win_w: f32,
    win_h: f32,
    margin: Margins,
) -> Vec<TextVertex> {
    let mut verts = Vec::new();
    let Some(ball_idx) = icon_for_tool(OperatorTool::Ball) else {
        return verts;
    };
    let visible = visible_icon_count_for_margin(win_h, margin.top, margin.bottom, margin.left);
    let len = name.chars().count();
    if ball_idx >= visible || len == 0 {
        return verts;
    }

    let icon = icon_px_rect(ball_idx as u32, margin);
    let nominal = BALL_LABEL_GLYPH_SCALE * sidebar_ui_scale(margin.left);
    let fit = (icon.right - icon.left) * BALL_LABEL_MAX_WIDTH / (len as f32 * GLYPH_W as f32);
    let scale = nominal.min(fit);
    let [cx, cy] = px_to_ndc(
        (icon.left + icon.right) * 0.5,
        (icon.top + icon.bottom) * 0.5,
        win_w,
        win_h,
    );
    let (glyph_ndc_w, glyph_ndc_h) = window_glyph_ndc_size((win_w, win_h), scale);
    push_label_vertices(
        &mut verts,
        &ASCII_CHARS,
        name,
        cx,
        cy,
        glyph_ndc_w,
        glyph_ndc_h,
    );
    verts
}

/// Build sidebar geometry: a background rounded-rect quad followed by icon quads.
///
/// Returns the vertex list and the rounded-rect uniform params.
/// Icons are centered horizontally in the left margin, stacked vertically
/// starting below the top margin. Icons that would exceed the bottom margin
/// are omitted. The background rectangle encloses all visible icons with
/// [`RECT_PADDING`] on each side.
#[cfg(test)]
pub(crate) fn build_sidebar_geometry(
    icon_count: u32,
    win_w: f32,
    win_h: f32,
    selected: Option<usize>,
) -> (Vec<SidebarVertex>, RoundedRectParams) {
    build_sidebar_geometry_with_margins(icon_count, win_w, win_h, MARGINS, selected)
}

pub(crate) fn build_sidebar_geometry_with_margins(
    icon_count: u32,
    win_w: f32,
    win_h: f32,
    margin: Margins,
    selected: Option<usize>,
) -> (Vec<SidebarVertex>, RoundedRectParams) {
    let mut verts = Vec::with_capacity(MAX_VERTICES);
    let win = (win_w, win_h);

    let ui_scale = (margin.left / MARGINS.left).max(1.0);
    let rect_padding = RECT_PADDING * ui_scale;
    let icon_size = icon_display_size_for_margin(margin.left);
    let icon_spacing = icon_spacing_for_margin(margin.left);
    let icon_left = (margin.left - icon_size) * 0.5;
    let first_icon_top = margin.top + icon_spacing;
    let icon_rect = |i: u32| icon_px_rect(i, margin);

    // Icons that fit, clamped to the number of tools in the shared ordering.
    let visible_count =
        (visible_icon_count_for_margin(win_h, margin.top, margin.bottom, margin.left) as u32)
            .min(ICON_COUNT as u32);

    let mut rect_params = RoundedRectParams {
        size_px: [0.0, 0.0],
        corner_radius: CORNER_RADIUS * ui_scale,
        border_width: BORDER_WIDTH * ui_scale,
    };

    if visible_count == 0 {
        return (verts, rect_params);
    }

    // ── Background rounded rectangle ───────────────────────────────
    let last_icon = icon_rect(visible_count - 1);
    let background = PxRect {
        left: icon_left - rect_padding,
        top: first_icon_top - rect_padding,
        right: icon_left + icon_size + rect_padding,
        bottom: last_icon.bottom + rect_padding,
    };
    rect_params.size_px = [
        background.right - background.left,
        background.bottom - background.top,
    ];
    push_quad(&mut verts, background, (0.0, 1.0), MODE_BACKGROUND, win);

    // ── Highlight quad behind selected icon ──────────────────────────
    if let Some(sel) = selected.map(|s| s as u32).filter(|&s| s < visible_count) {
        let icon = icon_rect(sel);
        let pad = rect_padding * 0.5;
        let highlight = PxRect {
            left: icon.left - pad,
            top: icon.top - pad,
            right: icon.right + pad,
            bottom: icon.bottom + pad,
        };
        push_quad(&mut verts, highlight, (0.0, 1.0), MODE_HIGHLIGHT, win);
    }

    // ── Icon quads ─────────────────────────────────────────────────
    let rows = icon_count.max(1) as f32;
    for i in 0..visible_count {
        let (mode, v_range) = match atlas_row_for_icon(i as usize) {
            Some(row) => (
                MODE_ATLAS_ICON,
                (row as f32 / rows, (row + 1) as f32 / rows),
            ),
            // Procedural icon (ball) uses local UV space [0,1].
            None => (MODE_BALL_ICON, (0.0, 1.0)),
        };
        push_quad(&mut verts, icon_rect(i), v_range, mode, win);
    }

    (verts, rect_params)
}

impl RenderPipeline for SidebarPipeline {
    fn name(&self) -> &'static str {
        "sidebar"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::FullWindow
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let selected = ctx.operator_previews.selected_sidebar_icon;
        let name = ctx
            .snapshot
            .species
            .get(ctx.snapshot.cursor_species)
            .map_or("", |s| s.name.as_str());
        if ctx.window_size == self.last_window_size
            && selected == self.last_selected_icon
            && ctx.margin == self.last_margin
            && self.last_species.as_deref() == Some(name)
        {
            return Ok(());
        }
        self.last_window_size = ctx.window_size;
        self.last_selected_icon = selected;
        self.last_margin = ctx.margin;
        self.last_species = Some(name.to_string());

        let (w, h) = ctx.window_size;
        let (verts, rect_params) = build_sidebar_geometry_with_margins(
            sidebar_atlas_icons().len() as u32,
            w as f32,
            h as f32,
            ctx.margin,
            selected,
        );
        self.gpu
            .set_uniforms(ctx.queue, SceneUniforms::ndc(rect_params.as_params()));
        self.gpu.flush(ctx.device, ctx.queue, &verts);

        let label = ball_label_vertices(name, w as f32, h as f32, ctx.margin);
        self.ball_label.upload(ctx.device, ctx.queue, &label);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
        self.ball_label.draw(render_pass);
    }
}
