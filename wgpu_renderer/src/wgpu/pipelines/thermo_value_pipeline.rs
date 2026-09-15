//! Per-cell thermodynamic value text overlay pipeline for WGPU.
//!
//! Renders a formatted two-decimal label (e.g., "12.34") centred on each grid
//! cell that holds a finite T/P value.  Hidden when `ThermoDisplayMode::Off`.
//!
//! Reuses the existing `text.wgsl` shader (same vertex format) and a compact
//! 8×8 bitmap font atlas built once from `font8x8`.

use super::super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use super::super::world_text::{
    BLACK_TEXT_COLOR, TextVertex, WorldTextPipeline, apply_view_proj, inset_glyph_ndc_size,
    push_label_vertices,
};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::application::renderer_snapshot::ThermoGridSpec;
use rust_thermo_core::application::thermo_field::ThermoGrid;
use rust_thermo_core::graphics::renderer::state_layers::ThermoDisplayMode;

/// Characters stored in the atlas, in index order.
const ATLAS_CHARS: [char; 12] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', '-'];

pub struct ThermoValueOverlayPipeline {
    text: WorldTextPipeline,
}

impl ThermoValueOverlayPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            text: WorldTextPipeline::new(
                device,
                format,
                "Thermo Value Overlay",
                &ATLAS_CHARS,
                1024,
                BLACK_TEXT_COLOR,
            ),
        }
    }

    /// Build a flat vertex list for all cells with finite values.
    fn build_vertices(
        grid: &ThermoGrid,
        spec: &ThermoGridSpec,
        view_proj: [[f32; 4]; 4],
        glyph_ndc_w: f32,
        glyph_ndc_h: f32,
    ) -> Vec<TextVertex> {
        let (rows, cols) = grid.shape();
        let mut verts = Vec::new();

        for r in 0..rows {
            for c in 0..cols {
                let v = grid.get(r, c);
                if !v.is_finite() {
                    continue;
                }

                let label = format!("{:.2}", v);

                // Cell centre in world space.
                let cx = spec.origin_x + (c as f32 + 0.5) * spec.cell_width;
                let cy = spec.origin_y + (r as f32 + 0.5) * spec.cell_height;

                let (ndc_x, ndc_y) = apply_view_proj(view_proj, cx, cy);
                push_label_vertices(
                    &mut verts,
                    &ATLAS_CHARS,
                    &label,
                    ndc_x,
                    ndc_y,
                    glyph_ndc_w,
                    glyph_ndc_h,
                );
            }
        }
        verts
    }
}

impl RenderPipeline for ThermoValueOverlayPipeline {
    fn name(&self) -> &'static str {
        "thermo_value_overlay"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        if !ctx.thermo_display.is_thermo_active() {
            self.text.clear();
            return Ok(());
        }

        let Some(spec) = &ctx.snapshot.thermo_grid_spec else {
            self.text.clear();
            return Ok(());
        };

        let grid = match ctx.thermo_display {
            ThermoDisplayMode::Temperature => &ctx.snapshot.cell_temperature,
            ThermoDisplayMode::Pressure => &ctx.snapshot.cell_pressure,
            ThermoDisplayMode::Off | ThermoDisplayMode::Grid => unreachable!(),
        };

        let (rows, cols) = grid.shape();
        if rows == 0 || cols == 0 {
            self.text.clear();
            return Ok(());
        }

        let (glyph_ndc_w, glyph_ndc_h) = inset_glyph_ndc_size(ctx.window_size, ctx.margin);

        let verts = Self::build_vertices(grid, spec, ctx.view_proj, glyph_ndc_w, glyph_ndc_h);

        self.text.update_vertices(ctx.device, ctx.queue, &verts);

        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.text.render(render_pass);
    }
}
