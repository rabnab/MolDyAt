// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Per-cell thermodynamic value text overlay pipeline.
//!
//! Renders a formatted two-decimal label (e.g., "12.34") centred on each grid
//! cell that holds a finite value of the scalar field shown. Hidden unless a
//! scalar field overlay is active.

use thermo_physics::application::AppResult;
use thermo_physics::application::thermo_field::ThermoGrid;
use thermo_runtime::application::renderer_snapshot::ThermoGridSpec;
use thermo_runtime::application::scene_layers::SceneLayers;

use crate::wgpu::render::gpu::geometry::apply_view_proj;
use crate::wgpu::render::gpu::text::{
    BLACK_TEXT_COLOR, GlyphAtlas, TextBatch, TextVertex, inset_glyph_ndc_size, push_label_vertices,
};
use crate::wgpu::render::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

pub struct ThermoValueOverlayPipeline {
    text: TextBatch,
}

impl ThermoValueOverlayPipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        glyphs: &GlyphAtlas,
    ) -> Self {
        Self {
            text: TextBatch::new(
                device,
                queue,
                format,
                "Thermo Value Overlay",
                glyphs,
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

                // Cell centre in world space.
                let cx = spec.origin_x + (c as f32 + 0.5) * spec.cell_width;
                let cy = spec.origin_y + (r as f32 + 0.5) * spec.cell_height;

                let (ndc_x, ndc_y) = apply_view_proj(view_proj, cx, cy);
                push_label_vertices(
                    &mut verts,
                    &format!("{v:.2}"),
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

    fn layer(&self) -> SceneLayers {
        SceneLayers::THERMO
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let Some(field) = ctx.thermo_display.scalar_field() else {
            self.text.clear();
            return Ok(());
        };
        let grid = field.grid(&ctx.fields.cells);
        let Some(spec) = &ctx.fields.thermo_grid else {
            self.text.clear();
            return Ok(());
        };

        let (glyph_ndc_w, glyph_ndc_h) =
            inset_glyph_ndc_size(ctx.window_size, ctx.margin, ctx.inset);
        let verts = Self::build_vertices(&grid, spec, ctx.view_proj, glyph_ndc_w, glyph_ndc_h);
        self.text.upload(ctx.device, ctx.queue, &verts);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.text.draw(render_pass);
    }
}
