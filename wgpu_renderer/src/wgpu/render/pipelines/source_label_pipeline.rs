// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Source sphere label overlay pipeline.
//!
//! Draws three labels per source sphere in the inset simulation viewport:
//! the species it emits and, below that, the emission speed (`S:`) above the
//! sphere, and the emission rate (`R:`) below it, each on a semi-transparent
//! white capsule (see `gpu/label_background.rs`). Mirrors
//! `ThermostatLabelPipeline`.

use thermo_physics::application::AppResult;
use thermo_physics::objects2d::ball_impl::SpeciesTable;
use thermo_physics::objects2d::{SOURCE_SPHERE_RADIUS, SimSourceSphere};

use crate::wgpu::render::gpu::geometry::apply_view_proj;
use crate::wgpu::render::gpu::label_background::{
    CapsuleInstance, LabelBackground, capsule_for_label,
};
use crate::wgpu::render::gpu::text::{
    ASCII_CHARS, BLACK_TEXT_COLOR, TextBatch, TextVertex, inset_glyph_ndc_size, push_label_vertices,
};
use crate::wgpu::render::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

/// Printable ASCII: species names are user-defined (`[[species]]`).
const ATLAS_CHARS: [char; 95] = ASCII_CHARS;

/// Distance of the labels from the sphere centre, in sphere radii. The arc
/// indicators sit at 1.15 and 1.35 radii, so this keeps the labels outside them.
const LABEL_RADIUS_FACTOR: f32 = 1.35;

pub struct SourceLabelPipeline {
    text: TextBatch,
    background: LabelBackground,
}

/// One label such as `S:65%`; shows `--` for a value that isn't a number.
fn percent_label(prefix: char, fraction: f32) -> String {
    if fraction.is_finite() {
        format!("{prefix}:{:.0}%", fraction * 100.0)
    } else {
        format!("{prefix}:--")
    }
}

/// Texts for the (species, speed, rate) labels of one sphere: the name of
/// the species it emits (from the world's table, `?` if unknown) and the
/// same fractions the arc indicators show, as percentages.
pub(crate) fn label_texts(
    sphere: &SimSourceSphere,
    species: &SpeciesTable,
) -> (String, String, String) {
    let name = species
        .get(sphere.ball_species)
        .map_or_else(|| "?".to_string(), |s| s.name.clone());
    (
        name,
        percent_label('S', sphere.speed_fraction()),
        percent_label('R', sphere.rate_fraction()),
    )
}

impl SourceLabelPipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            text: TextBatch::new(
                device,
                queue,
                format,
                "Source Sphere Label",
                &ATLAS_CHARS,
                BLACK_TEXT_COLOR,
            ),
            background: LabelBackground::new(device, format),
        }
    }

    /// Text vertices plus one capsule per label, from the same layout pass so
    /// the capsule always matches the text exactly.
    fn build_geometry(
        spheres: &[SimSourceSphere],
        species: &SpeciesTable,
        view_proj: [[f32; 4]; 4],
        glyph_ndc_w: f32,
        glyph_ndc_h: f32,
    ) -> (Vec<TextVertex>, Vec<CapsuleInstance>) {
        let mut verts = Vec::new();
        let mut capsules = Vec::new();
        let label_radius = SOURCE_SPHERE_RADIUS * LABEL_RADIUS_FACTOR;

        for sphere in spheres {
            let (species_label, speed_label, rate_label) = label_texts(sphere, species);

            let (top_x, top_y) =
                apply_view_proj(view_proj, sphere.center.x, sphere.center.y + label_radius);
            let (bottom_x, bottom_y) =
                apply_view_proj(view_proj, sphere.center.x, sphere.center.y - label_radius);

            // The species label sits one capsule height above the speed label.
            let labels = [
                (&species_label, top_x, top_y + 2.6 * glyph_ndc_h),
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
            self.background.upload(ctx.device, ctx.queue, &[]);
            return Ok(());
        }

        let (glyph_ndc_w, glyph_ndc_h) =
            inset_glyph_ndc_size(ctx.window_size, ctx.margin, ctx.inset);
        let (verts, capsules) = Self::build_geometry(
            spheres,
            &ctx.snapshot.species,
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
