// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cursor readout text pipeline.
//!
//! Shows the cursor's world position, the gravity multiple and the species
//! of new balls at the bottom-right of the simulation pane, inside the bottom
//! margin. Uses the
//! shared bitmap text renderer (`gpu/text.rs`).

use thermo_physics::application::AppResult;
use thermo_runtime::application::renderer_snapshot::RendererSnapshot;
use thermo_ui::graphics::renderer::margins::Margins;
use thermo_ui::graphics::sidebar::sidebar_ui_scale;

use crate::wgpu::gpu::geometry::px_to_ndc;
use crate::wgpu::gpu::text::{
    ASCII_CHARS, GLYPH_H, GLYPH_W, TextBatch, TextVertex, push_label_vertices,
    window_glyph_ndc_size,
};
use crate::wgpu::render_graph::{PrepareContext, RenderPipeline, ViewportMode};

const CURSOR_TEXT_COLOR: [f32; 4] = [0.92, 0.9, 0.84, 1.0];

/// Gap between the text and the pane edges, in pixels at UI scale 1.
const PADDING_PX: f32 = 4.0;

pub struct TextPipeline {
    text: TextBatch,
}

impl TextPipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            text: TextBatch::new(
                device,
                queue,
                format,
                "Cursor Text",
                &ASCII_CHARS,
                CURSOR_TEXT_COLOR,
            ),
        }
    }
}

/// The readout text: cursor position, gravity multiple and the species of
/// new balls (from the world's table in the snapshot).
pub(crate) fn cursor_readout(cursor: (f32, f32), snapshot: &RendererSnapshot) -> String {
    let species = snapshot
        .species
        .get(snapshot.cursor_species)
        .map_or("?", |species| species.name.as_str());
    format!(
        "({:.2}, {:.2})  {}g  {species}",
        cursor.0, cursor.1, snapshot.gravity_force_multiple
    )
}

/// Glyph quads (NDC, full window) for `text`, right-aligned to the
/// simulation pane and just below it.
pub(crate) fn cursor_text_vertices(
    text: &str,
    window_size: (u32, u32),
    margin: Margins,
) -> Vec<TextVertex> {
    let (win_w, win_h) = (window_size.0 as f32, window_size.1 as f32);
    let ui_scale = sidebar_ui_scale(margin.left);
    let glyph_w = GLYPH_W as f32 * ui_scale;
    let glyph_h = GLYPH_H as f32 * ui_scale;
    let text_w = text.chars().count().max(1) as f32 * glyph_w;
    let padding = PADDING_PX * ui_scale;

    let left = (win_w - margin.right - text_w - padding).max(margin.left);
    let top = (win_h - margin.bottom + padding).max(0.0);
    let [cx, cy] = px_to_ndc(left + text_w * 0.5, top + glyph_h * 0.5, win_w, win_h);
    let (glyph_ndc_w, glyph_ndc_h) = window_glyph_ndc_size((win_w, win_h), ui_scale);

    let mut verts = Vec::new();
    push_label_vertices(
        &mut verts,
        &ASCII_CHARS,
        text,
        cx,
        cy,
        glyph_ndc_w,
        glyph_ndc_h,
    );
    verts
}

impl RenderPipeline for TextPipeline {
    fn name(&self) -> &'static str {
        "text"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::FullWindow
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let text = cursor_readout(ctx.cursor_world_pos, ctx.snapshot);
        let verts = cursor_text_vertices(&text, ctx.window_size, ctx.margin);
        self.text.upload(ctx.device, ctx.queue, &verts);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.text.draw(render_pass);
    }
}
