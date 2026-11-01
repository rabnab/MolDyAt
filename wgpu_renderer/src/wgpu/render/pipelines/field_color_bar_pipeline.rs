// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Colour bar of the scalar field overlay (#398).
//!
//! While a scalar field (temperature, pressure, density, flow speed) is
//! shown, a vertical bar with the heatmap's rainbow colour map stands in the
//! left margin below the tool icons, bottom-aligned with the simulation area:
//! the field's name above it, the high end of its fixed range
//! (`SCALAR_FIELDS`) at the top, the low end at the bottom. The field itself
//! shows no numbers. Screen space (NDC, full window); the layout is in window
//! pixels ([`color_bar_layout`]). See `color_bar.wgsl`.

use bytemuck::{Pod, Zeroable};
use thermo_physics::application::AppResult;
use thermo_runtime::application::scene_layers::SceneLayers;
use thermo_ui::graphics::renderer::margins::Margins;
use thermo_ui::graphics::sidebar::{
    ICON_COUNT, icon_display_size_for_margin, icon_spacing_for_margin, sidebar_ui_scale,
};

use crate::wgpu::render::gpu::batch::{BatchDesc, GpuBatch, SceneUniforms};
use crate::wgpu::render::gpu::geometry::px_to_ndc;
use crate::wgpu::render::gpu::shaders;
use crate::wgpu::render::gpu::text::{
    GLYPH_H, GlyphAtlas, TextBatch, TextVertex, push_label_vertices, window_glyph_ndc_size,
};
use crate::wgpu::render::pipelines::crosshair_pipeline::RectInstance;
use crate::wgpu::render::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Text and outline colour, as the other margin text.
const LABEL_COLOR: [f32; 4] = [0.92, 0.9, 0.84, 1.0];
/// Half the bar width, in pixels at UI scale 1.
const BAR_HALF_WIDTH_PX: f32 = 8.0;
/// Outline around the bar, in pixels at UI scale 1.
const OUTLINE_PX: f32 = 1.0;
/// Vertical gap between the icons, the labels and the bar, in pixels at UI
/// scale 1.
const GAP_PX: f32 = 8.0;
/// Shortest bar worth drawing, in pixels at UI scale 1; below it (a very
/// short window) the colour bar is hidden.
const MIN_BAR_PX: f32 = 40.0;

/// Bar instance. Layout must match `Inst` in color_bar.wgsl.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
struct BarInstance {
    center: [f32; 2],
    half_size: [f32; 2],
}

const BAR_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];
const RECT_ATTRS: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

/// Where the colour bar and its labels go, in window pixels (y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ColorBarLayout {
    /// Horizontal centre of the bar and the labels (middle of the margin).
    pub center_x: f32,
    pub bar_top: f32,
    pub bar_bottom: f32,
    pub bar_half_width: f32,
    pub outline: f32,
    /// Vertical centres of the field name, the high and the low value.
    pub name_y: f32,
    pub high_y: f32,
    pub low_y: f32,
    /// UI scale of the margin (glyph pixels per font pixel).
    pub ui_scale: f32,
}

/// The colour bar's layout for a window of `window_size` pixels with
/// `margin`: below the last tool icon, bottom-aligned with the simulation
/// area. `None` when the margin below the icons is too short for it.
pub(crate) fn color_bar_layout(window_size: (u32, u32), margin: Margins) -> Option<ColorBarLayout> {
    let s = sidebar_ui_scale(margin.left);
    let (icon, spacing) = (
        icon_display_size_for_margin(margin.left),
        icon_spacing_for_margin(margin.left),
    );
    let glyph = GLYPH_H as f32 * s;
    let gap = GAP_PX * s;
    // The sidebar starts its first icon one spacing below the top margin.
    let icons_bottom = margin.top + spacing + ICON_COUNT as f32 * (icon + spacing);

    let name_y = icons_bottom + gap + glyph * 0.5;
    let high_y = name_y + glyph + gap * 0.5;
    let bar_top = high_y + glyph * 0.5 + gap * 0.5;
    let low_y = window_size.1 as f32 - margin.bottom - glyph * 0.5;
    let bar_bottom = low_y - glyph * 0.5 - gap * 0.5;
    (bar_bottom - bar_top >= MIN_BAR_PX * s).then_some(ColorBarLayout {
        center_x: margin.left * 0.5,
        bar_top,
        bar_bottom,
        bar_half_width: BAR_HALF_WIDTH_PX * s,
        outline: OUTLINE_PX * s,
        name_y,
        high_y,
        low_y,
        ui_scale: s,
    })
}

/// A range end as the bar shows it: `5`, `-4`, `0.5`.
pub(crate) fn range_label(value: f32) -> String {
    format!("{value}")
}

/// NDC centre and half size of the pixel rectangle `[left, right] x [top, bottom]`.
fn ndc_rect(
    (left, top, right, bottom): (f32, f32, f32, f32),
    (win_w, win_h): (f32, f32),
) -> ([f32; 2], [f32; 2]) {
    let center = px_to_ndc((left + right) * 0.5, (top + bottom) * 0.5, win_w, win_h);
    let half = [(right - left) / win_w, (bottom - top) / win_h];
    (center, half)
}

/// Glyph quads of the field name and the two range ends.
pub(crate) fn color_bar_text(
    layout: &ColorBarLayout,
    name: &str,
    (low, high): (f32, f32),
    (win_w, win_h): (f32, f32),
) -> Vec<TextVertex> {
    let (glyph_w, glyph_h) = window_glyph_ndc_size((win_w, win_h), layout.ui_scale);
    let mut verts = Vec::new();
    for (text, y) in [
        (name.to_string(), layout.name_y),
        (range_label(high), layout.high_y),
        (range_label(low), layout.low_y),
    ] {
        let [x, y] = px_to_ndc(layout.center_x, y, win_w, win_h);
        push_label_vertices(&mut verts, &text, x, y, glyph_w, glyph_h);
    }
    verts
}

pub struct FieldColorBarPipeline {
    outline: GpuBatch<RectInstance>,
    bar: GpuBatch<BarInstance>,
    text: TextBatch,
}

impl FieldColorBarPipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        glyphs: &GlyphAtlas,
    ) -> Self {
        Self {
            outline: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Color Bar Outline", shaders::SOLID_RECT, &RECT_ATTRS),
            ),
            bar: GpuBatch::new(
                device,
                format,
                BatchDesc::quads("Color Bar", shaders::COLOR_BAR, &BAR_ATTRS),
            ),
            text: TextBatch::new(
                device,
                queue,
                format,
                "Color Bar Labels",
                glyphs,
                LABEL_COLOR,
            ),
        }
    }

    fn clear(&mut self) {
        self.outline.clear();
        self.bar.clear();
        self.text.clear();
    }
}

impl RenderPipeline for FieldColorBarPipeline {
    fn name(&self) -> &'static str {
        "field_color_bar"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::FullWindow
    }

    fn layer(&self) -> SceneLayers {
        SceneLayers::THERMO
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let (Some(field), Some(layout)) = (
            ctx.thermo_display.scalar_field(),
            color_bar_layout(ctx.window_size, ctx.margin),
        ) else {
            self.clear();
            return Ok(());
        };
        let descriptor = field.descriptor();
        let win = (ctx.window_size.0 as f32, ctx.window_size.1 as f32);
        let uniforms = SceneUniforms::ndc([0.0; 4]);

        let (l, t, r, b) = (
            layout.center_x - layout.bar_half_width,
            layout.bar_top,
            layout.center_x + layout.bar_half_width,
            layout.bar_bottom,
        );
        let o = layout.outline;
        let (center, half_size) = ndc_rect((l - o, t - o, r + o, b + o), win);
        self.outline.set_uniforms(ctx.queue, uniforms);
        self.outline.flush(
            ctx.device,
            ctx.queue,
            &[RectInstance {
                center,
                half_size,
                color: LABEL_COLOR,
            }],
        );
        let (center, half_size) = ndc_rect((l, t, r, b), win);
        self.bar.set_uniforms(ctx.queue, uniforms);
        self.bar
            .flush(ctx.device, ctx.queue, &[BarInstance { center, half_size }]);

        let verts = color_bar_text(&layout, descriptor.name, descriptor.range, win);
        self.text.upload(ctx.device, ctx.queue, &verts);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.outline.draw(render_pass);
        self.bar.draw(render_pass);
        self.text.draw(render_pass);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wgpu::MARGINS;

    const WINDOW: (u32, u32) = (1200, 800);

    #[test]
    fn color_bar_sits_below_the_icons_and_inside_the_margin() {
        let layout = color_bar_layout(WINDOW, MARGINS).expect("room for the bar");
        let icons_bottom = MARGINS.top
            + icon_spacing_for_margin(MARGINS.left)
            + ICON_COUNT as f32
                * (icon_display_size_for_margin(MARGINS.left)
                    + icon_spacing_for_margin(MARGINS.left));
        assert!(layout.name_y > icons_bottom, "label overlaps the icons");
        assert!(layout.name_y < layout.high_y && layout.high_y < layout.bar_top);
        assert!(layout.bar_top < layout.bar_bottom && layout.bar_bottom < layout.low_y);
        // Bottom-aligned with the simulation area.
        assert!(layout.low_y <= WINDOW.1 as f32 - MARGINS.bottom);
        // Inside the left margin.
        assert!(layout.center_x - layout.bar_half_width - layout.outline > 0.0);
        assert!(layout.center_x + layout.bar_half_width + layout.outline < MARGINS.left);
    }

    #[test]
    fn color_bar_is_hidden_when_the_window_is_too_short() {
        assert!(color_bar_layout((1200, 420), MARGINS).is_none());
        assert!(color_bar_layout((1200, 800), MARGINS).is_some());
    }

    #[test]
    fn color_bar_scales_with_the_ui() {
        let at_1x = color_bar_layout((1200, 1600), MARGINS).unwrap();
        let margin_2x = crate::wgpu::margins_for_scale_factor(2.0);
        let at_2x = color_bar_layout((1200, 1600), margin_2x).unwrap();
        assert_eq!(at_2x.bar_half_width, 2.0 * at_1x.bar_half_width);
        assert_eq!(at_2x.center_x, 2.0 * at_1x.center_x);
    }

    #[test]
    fn range_ends_are_printed_without_trailing_zeros() {
        assert_eq!(range_label(5.0), "5");
        assert_eq!(range_label(-4.0), "-4");
        assert_eq!(range_label(0.0), "0");
        assert_eq!(range_label(0.5), "0.5");
    }

    #[test]
    fn labels_hold_the_name_and_both_range_ends() {
        let layout = color_bar_layout(WINDOW, MARGINS).unwrap();
        let verts = color_bar_text(&layout, "Temperature", (0.0, 5.0), (1200.0, 800.0));
        // 6 vertices per glyph: "Temperature" + "5" + "0".
        assert_eq!(verts.len(), 6 * (11 + 1 + 1));
        // Every glyph lies in the left margin (NDC x below the margin edge).
        let margin_edge = -1.0 + 2.0 * MARGINS.left / 1200.0;
        assert!(verts.iter().all(|v| v.position[0] < margin_edge));
    }
}
