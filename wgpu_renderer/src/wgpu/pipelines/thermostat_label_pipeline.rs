// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Thermostat label overlay pipeline.
//!
//! Renders local and target temperature labels near each thermostat in the
//! inset simulation viewport. Each label gets a semi-transparent white capsule
//! behind it so it stays readable on top of the thermostat zone, balls and
//! walls (see `gpu/label_background.rs`).

use rust_thermo_core::application::AppResult;
use rust_thermo_core::application::scene_layers::SceneLayers;
use rust_thermo_core::objects2d::SimThermostat;
use std::time::Instant;

use crate::wgpu::gpu::geometry::apply_view_proj;
use crate::wgpu::gpu::label_background::{CapsuleInstance, LabelBackground, capsule_for_label};
use crate::wgpu::gpu::text::{
    BLACK_TEXT_COLOR, TextBatch, TextVertex, inset_glyph_ndc_size, push_label_vertices,
};
use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

const ATLAS_CHARS: [char; 17] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', '-', ':', 'L', 'T', 'O', 'F',
];

/// Added to the local temperature label of a deactivated thermostat: the
/// value is still measured live, but nothing regulates it.
const INACTIVE_SUFFIX: &str = " OFF";

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
    text: TextBatch,
    background: LabelBackground,
    /// One smoother per thermostat (index-aligned with the snapshot list)
    smoothers: Vec<Smoother>,
    last_update: Instant,
    last_display: Instant,
}

impl ThermostatLabelPipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            text: TextBatch::new(
                device,
                queue,
                format,
                "Thermostat Label",
                &ATLAS_CHARS,
                BLACK_TEXT_COLOR,
            ),
            background: LabelBackground::new(device, format),
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

    /// The local temperature label; a deactivated thermostat is marked.
    pub(crate) fn build_local_label(thermostat: &SimThermostat, local_temp: f32) -> String {
        let mut label = Self::build_label_text('L', local_temp);
        if !thermostat.is_active() {
            label.push_str(INACTIVE_SUFFIX);
        }
        label
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
            let local_label = Self::build_local_label(thermostat, local_temp);
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

    pub(crate) fn vertex_count_for_thermostats(
        thermostats: &[SimThermostat],
        view_proj: [[f32; 4]; 4],
        glyph_ndc_w: f32,
        glyph_ndc_h: f32,
    ) -> usize {
        let raw: Vec<f32> = thermostats.iter().map(|t| t.current_local_temp).collect();
        Self::build_geometry(thermostats, &raw, view_proj, glyph_ndc_w, glyph_ndc_h)
            .0
            .len()
    }
}

impl RenderPipeline for ThermostatLabelPipeline {
    fn name(&self) -> &'static str {
        "thermostat_labels"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn layer(&self) -> SceneLayers {
        SceneLayers::READOUTS
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let thermostats = &ctx.snapshot.operators.thermostats;
        if thermostats.is_empty() {
            self.text.clear();
            self.background.upload(ctx.device, ctx.queue, &[]);
            self.smoothers.clear(); // start fresh when thermostats are added again
            return Ok(());
        }

        let (glyph_ndc_w, glyph_ndc_h) =
            inset_glyph_ndc_size(ctx.window_size, ctx.margin, ctx.inset);
        let local_temps = self.smoothed_local_temps(thermostats);
        let (verts, capsules) = Self::build_geometry(
            thermostats,
            &local_temps,
            ctx.view_proj,
            glyph_ndc_w,
            glyph_ndc_h,
        );

        self.text.upload(ctx.device, ctx.queue, &verts);
        self.background.upload(ctx.device, ctx.queue, &capsules);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        // Capsules first, text on top.
        self.background.draw(render_pass);
        self.text.draw(render_pass);
    }
}
