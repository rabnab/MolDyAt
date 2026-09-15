//! Thermostat label overlay pipeline for WGPU.
//!
//! Renders local and target temperature labels near each thermostat in the
//! inset simulation viewport using the shared text shader. Each label gets a
//! semi-transparent white capsule behind it so it stays readable on top of
//! the thermostat zone, balls and walls (see `label_background.rs`).

use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use crate::wgpu::util::label_background::{CapsuleInstance, LabelBackground, capsule_for_label};
use crate::wgpu::world_text::{
    BLACK_TEXT_COLOR, TextVertex, WorldTextPipeline, apply_view_proj, inset_glyph_ndc_size,
    push_label_vertices,
};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::objects2d::SimThermostat;
use std::time::Instant;

const ATLAS_CHARS: [char; 15] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', '-', ':', 'L', 'T',
];

/// Time constant of the moving average applied to the local temperature label
/// (seconds). Larger = calmer but slower to follow real changes.
const LOCAL_TEMP_SMOOTHING_SECS: f32 = 1.0;
/// How often the displayed local temperature is refreshed (seconds), so the
/// digits don't flicker every frame.
const DISPLAY_INTERVAL_SECS: f32 = 0.25;

/// Exponential moving average of one thermostat's local temperature.
#[derive(Debug, Clone, Copy, Default)]
struct Smoother {
    /// Running average, updated every frame
    avg: f32,
    /// Value currently shown on the label (refreshed at DISPLAY_INTERVAL_SECS)
    shown: f32,
    initialized: bool,
}

impl Smoother {
    fn update(&mut self, value: f32, alpha: f32, refresh: bool) {
        if !value.is_finite() {
            // Not a number: show "--" and start fresh once a real value returns.
            self.initialized = false;
            self.shown = value;
            return;
        }
        if !self.initialized {
            self.avg = value;
            self.shown = value;
            self.initialized = true;
            return;
        }
        self.avg += (value - self.avg) * alpha;
        if refresh {
            self.shown = self.avg;
        }
    }
}

pub struct ThermostatLabelPipeline {
    text: WorldTextPipeline,
    background: LabelBackground,
    capsules: Vec<CapsuleInstance>,
    /// One smoother per thermostat (index-aligned with the snapshot list)
    smoothers: Vec<Smoother>,
    last_update: Instant,
    last_display: Instant,
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
            background: LabelBackground::new(device, format),
            capsules: Vec::new(),
            smoothers: Vec::new(),
            last_update: Instant::now(),
            last_display: Instant::now(),
        }
    }

    /// Moving-average local temperatures, refreshed at a readable rate.
    fn smoothed_local_temps(&mut self, thermostats: &[SimThermostat]) -> Vec<f32> {
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        let refresh = now.duration_since(self.last_display).as_secs_f32() >= DISPLAY_INTERVAL_SECS;
        if refresh {
            self.last_display = now;
        }

        // Keep one smoother per thermostat. (Deleting a thermostat from the
        // middle of the list shifts the averages for about a second.)
        self.smoothers
            .resize(thermostats.len(), Smoother::default());

        // Weight derived from elapsed time, so it doesn't depend on frame rate.
        let alpha = 1.0 - (-dt / LOCAL_TEMP_SMOOTHING_SECS).exp();

        thermostats
            .iter()
            .zip(self.smoothers.iter_mut())
            .map(|(thermostat, smoother)| {
                smoother.update(thermostat.current_local_temp, alpha, refresh);
                smoother.shown
            })
            .collect()
    }

    pub(crate) fn build_label_text(prefix: char, value: f32) -> String {
        if value.is_finite() {
            format!("{prefix}:{value:.2}")
        } else {
            format!("{prefix}:--")
        }
    }

    /// Text vertices plus one capsule per label, from the same layout pass so
    /// the capsule always matches the text exactly.
    fn build_geometry(
        thermostats: &[SimThermostat],
        local_temps: &[f32],
        view_proj: [[f32; 4]; 4],
        glyph_ndc_w: f32,
        glyph_ndc_h: f32,
    ) -> (Vec<TextVertex>, Vec<CapsuleInstance>) {
        let mut verts = Vec::new();
        let mut capsules = Vec::new();

        for (thermostat, &local_temp) in thermostats.iter().zip(local_temps) {
            let local_label = Self::build_label_text('L', local_temp);
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

            let labels = [
                (&local_label, top_x, top_y + glyph_ndc_h),
                (&target_label, bottom_x, bottom_y - glyph_ndc_h),
            ];

            for (label, x, y) in labels {
                let start = verts.len();
                push_label_vertices(
                    &mut verts,
                    &ATLAS_CHARS,
                    label,
                    x,
                    y,
                    glyph_ndc_w,
                    glyph_ndc_h,
                );

                if let Some(capsule) = capsule_for_label(&verts[start..], glyph_ndc_w, glyph_ndc_h)
                {
                    capsules.push(capsule);
                }
            }
        }

        (verts, capsules)
    }

    fn build_vertices(
        thermostats: &[SimThermostat],
        view_proj: [[f32; 4]; 4],
        glyph_ndc_w: f32,
        glyph_ndc_h: f32,
    ) -> Vec<TextVertex> {
        let raw: Vec<f32> = thermostats.iter().map(|t| t.current_local_temp).collect();
        Self::build_geometry(thermostats, &raw, view_proj, glyph_ndc_w, glyph_ndc_h).0
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
            self.background.update(ctx.device, ctx.queue, &[]);
            self.smoothers.clear(); // start fresh when thermostats are added again
            return Ok(());
        }

        let (glyph_ndc_w, glyph_ndc_h) = inset_glyph_ndc_size(ctx.window_size, ctx.margin);

        let local_temps = self.smoothed_local_temps(&ctx.snapshot.operators.thermostats);

        let (verts, capsules) = Self::build_geometry(
            &ctx.snapshot.operators.thermostats,
            &local_temps,
            ctx.view_proj,
            glyph_ndc_w,
            glyph_ndc_h,
        );

        self.text.update_vertices(ctx.device, ctx.queue, &verts);

        self.capsules = capsules;
        self.background
            .update(ctx.device, ctx.queue, &self.capsules);

        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        // Capsules first, text on top.
        self.background.draw(render_pass);
        self.text.render(render_pass);
    }
}
