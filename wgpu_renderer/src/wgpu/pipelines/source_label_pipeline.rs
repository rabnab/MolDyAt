//! Source sphere label overlay pipeline for WGPU.
//!
//! Draws two labels per source sphere in the inset simulation viewport:
//! emission speed (`S:`) above the sphere and emission rate (`R:`) below it,
//! each on a semi-transparent white capsule (see `label_background.rs`).
//! Mirrors `ThermostatLabelPipeline`.

use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use crate::wgpu::util::label_background::{CapsuleInstance, LabelBackground, capsule_for_label};
use crate::wgpu::world_text::{
    BLACK_TEXT_COLOR, TextVertex, WorldTextPipeline, apply_view_proj, inset_glyph_ndc_size,
    push_label_vertices,
};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::objects2d::{SOURCE_SPHERE_RADIUS, SimSourceSphere};

const ATLAS_CHARS: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', '-', ':', '%', 'S', 'R',
];

/// Distance of the labels from the sphere centre, in sphere radii. The arc
/// indicators sit at 1.15 and 1.35 radii, so this keeps the labels outside them.
const LABEL_RADIUS_FACTOR: f32 = 1.35;

pub struct SourceLabelPipeline {
    text: WorldTextPipeline,
    background: LabelBackground,
    capsules: Vec<CapsuleInstance>,
}

/// One label such as `S:65%`; shows `--` for a value that isn't a number.
fn percent_label(prefix: char, fraction: f32) -> String {
    if fraction.is_finite() {
        format!("{prefix}:{:.0}%", fraction * 100.0)
    } else {
        format!("{prefix}:--")
    }
}

/// Texts for the (speed, rate) labels of one sphere.
///
/// NOTE: I haven't seen `SimSourceSphere`, so this shows the same fractions the
/// arc indicators use, as percentages. To show the real emission speed / rate
/// instead, change only this function (and add any new characters, such as a
/// unit, to `ATLAS_CHARS`).
fn label_texts(sphere: &SimSourceSphere) -> (String, String) {
    (
        percent_label('S', sphere.speed_fraction()),
        percent_label('R', sphere.rate_fraction()),
    )
}

impl SourceLabelPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            text: WorldTextPipeline::new(
                device,
                format,
                "Source Sphere Label",
                &ATLAS_CHARS,
                256,
                BLACK_TEXT_COLOR,
            ),
            background: LabelBackground::new(device, format),
            capsules: Vec::new(),
        }
    }

    /// Text vertices plus one capsule per label, from the same layout pass so
    /// the capsule always matches the text exactly.
    fn build_geometry(
        spheres: &[SimSourceSphere],
        view_proj: [[f32; 4]; 4],
        glyph_ndc_w: f32,
        glyph_ndc_h: f32,
    ) -> (Vec<TextVertex>, Vec<CapsuleInstance>) {
        let mut verts = Vec::new();
        let mut capsules = Vec::new();
        let label_radius = SOURCE_SPHERE_RADIUS * LABEL_RADIUS_FACTOR;

        for sphere in spheres {
            let (speed_label, rate_label) = label_texts(sphere);

            let (top_x, top_y) =
                apply_view_proj(view_proj, sphere.center.x, sphere.center.y + label_radius);
            let (bottom_x, bottom_y) =
                apply_view_proj(view_proj, sphere.center.x, sphere.center.y - label_radius);

            let labels = [
                (&speed_label, top_x, top_y + glyph_ndc_h),
                (&rate_label, bottom_x, bottom_y - glyph_ndc_h),
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
}

impl RenderPipeline for SourceLabelPipeline {
    fn name(&self) -> &'static str {
        "source_labels"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let spheres = &ctx.snapshot.operators.source_spheres;
        if spheres.is_empty() {
            self.text.clear();
            self.background.update(ctx.device, ctx.queue, &[]);
            return Ok(());
        }

        let (glyph_ndc_w, glyph_ndc_h) = inset_glyph_ndc_size(ctx.window_size, ctx.margin);

        let (verts, capsules) =
            Self::build_geometry(spheres, ctx.view_proj, glyph_ndc_w, glyph_ndc_h);

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
