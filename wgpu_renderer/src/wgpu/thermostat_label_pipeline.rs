//! Thermostat label overlay pipeline for WGPU.
//!
//! Renders local and target temperature labels near each thermostat in the
//! inset simulation viewport using the shared text shader.

use super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use super::world_text::{
    BLACK_TEXT_COLOR, TextVertex, WorldTextPipeline, apply_view_proj, inset_glyph_ndc_size,
    push_label_vertices,
};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::objects2d::SimThermostat;

const ATLAS_CHARS: [char; 15] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', '-', ':', 'L', 'T',
];

pub struct ThermostatLabelPipeline {
    text: WorldTextPipeline,
}

impl ThermostatLabelPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            text: WorldTextPipeline::new(
                device,
                format,
                "Thermostat Label",
                &ATLAS_CHARS,
                256,
                BLACK_TEXT_COLOR,
            ),
        }
    }

    pub(crate) fn build_label_text(prefix: char, value: f32) -> String {
        if value.is_finite() {
            format!("{prefix}:{value:.2}")
        } else {
            format!("{prefix}:--")
        }
    }

    fn build_vertices(
        thermostats: &[SimThermostat],
        view_proj: [[f32; 4]; 4],
        glyph_ndc_w: f32,
        glyph_ndc_h: f32,
    ) -> Vec<TextVertex> {
        let mut verts = Vec::new();

        for thermostat in thermostats {
            let local_label = Self::build_label_text('L', thermostat.current_local_temp);
            let target_label = Self::build_label_text('T', thermostat.target_temperature);

            let (top_x, top_y) = apply_view_proj(
                view_proj,
                thermostat.position.x,
                thermostat.position.y + thermostat.radius,
            );
            let (bottom_x, bottom_y) = apply_view_proj(
                view_proj,
                thermostat.position.x,
                thermostat.position.y - thermostat.radius,
            );

            push_label_vertices(
                &mut verts,
                &ATLAS_CHARS,
                &local_label,
                top_x,
                top_y + glyph_ndc_h,
                glyph_ndc_w,
                glyph_ndc_h,
            );
            push_label_vertices(
                &mut verts,
                &ATLAS_CHARS,
                &target_label,
                bottom_x,
                bottom_y - glyph_ndc_h,
                glyph_ndc_w,
                glyph_ndc_h,
            );
        }

        verts
    }

    pub(crate) fn vertex_count_for_thermostats(
        thermostats: &[SimThermostat],
        view_proj: [[f32; 4]; 4],
        glyph_ndc_w: f32,
        glyph_ndc_h: f32,
    ) -> usize {
        Self::build_vertices(thermostats, view_proj, glyph_ndc_w, glyph_ndc_h).len()
    }
}

impl RenderPipeline for ThermostatLabelPipeline {
    fn name(&self) -> &'static str {
        "thermostat_labels"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        if ctx.snapshot.operators.thermostats.is_empty() {
            self.text.clear();
            return Ok(());
        }

        let (glyph_ndc_w, glyph_ndc_h) = inset_glyph_ndc_size(ctx.window_size, ctx.margin);

        let verts = Self::build_vertices(
            &ctx.snapshot.operators.thermostats,
            ctx.view_proj,
            glyph_ndc_w,
            glyph_ndc_h,
        );

        self.text.update_vertices(ctx.device, ctx.queue, &verts);

        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.text.render(render_pass);
    }
}
