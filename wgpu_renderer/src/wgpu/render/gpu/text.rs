// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bitmap text: one 8×8 glyph atlas of printable ASCII built from
//! `font8x8` ([`GlyphAtlas`], one per renderer, #303) plus a [`GpuBatch`]
//! of glyph quads in NDC per overlay. Used by every text overlay (cursor
//! readout, field colour bar labels, thermostat and source-sphere labels, the sidebar
//! label) and the plot window. See `text.wgsl`.

use bytemuck::{Pod, Zeroable};
use font8x8::UnicodeFonts;
use thermo_ui::graphics::renderer::margins::Margins;
use thermo_ui::graphics::sidebar::sidebar_ui_scale;

use super::batch::{BatchDesc, GpuBatch, SceneUniforms};
use super::{shaders, textures};

pub(crate) const GLYPH_W: u32 = 8;
pub(crate) const GLYPH_H: u32 = 8;
pub(crate) const BLACK_TEXT_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// Printable ASCII, in atlas order.
pub(crate) const ASCII_CHARS: [char; 95] = {
    let mut chars = [' '; 95];
    let mut i = 0;
    while i < chars.len() {
        chars[i] = (b' ' + i as u8) as char;
        i += 1;
    }
    chars
};

/// One glyph-quad corner. Layout must match `Vertex` in text.wgsl.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct TextVertex {
    /// NDC
    pub(crate) position: [f32; 2],
    pub(crate) uv: [f32; 2],
}

const TEXT_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

/// Glyph size in NDC for the inset simulation viewport, at the current UI scale.
/// `margin` sets the UI scale, `inset` the drawing area (letterboxed).
pub(crate) fn inset_glyph_ndc_size(
    window_size: (u32, u32),
    margin: Margins,
    inset: Margins,
) -> (f32, f32) {
    let inset_w = (window_size.0 as f32 - inset.horizontal()).max(1.0);
    let inset_h = (window_size.1 as f32 - inset.vertical()).max(1.0);
    let ui_scale = sidebar_ui_scale(margin.left);
    (
        GLYPH_W as f32 * ui_scale * 2.0 / inset_w,
        GLYPH_H as f32 * ui_scale * 2.0 / inset_h,
    )
}

/// Glyph size in NDC for a full-window overlay, with each glyph pixel drawn
/// `scale` window pixels wide (the UI scale, or a larger factor).
pub(crate) fn window_glyph_ndc_size((win_w, win_h): (f32, f32), scale: f32) -> (f32, f32) {
    (
        GLYPH_W as f32 * scale * 2.0 / win_w.max(1.0),
        GLYPH_H as f32 * scale * 2.0 / win_h.max(1.0),
    )
}

/// Index of `ch` in the ASCII atlas (code order), `None` outside it.
pub(crate) fn ascii_index(ch: char) -> Option<usize> {
    (ch as usize)
        .checked_sub(' ' as usize)
        .filter(|&idx| idx < ASCII_CHARS.len())
}

/// Append one label (6 vertices per glyph of the [`GlyphAtlas`]) centred on
/// `(center_x, center_y)` in NDC. A space emits its blank glyph, so the
/// vertices span the whole label (label backgrounds are sized from them);
/// characters outside printable ASCII emit no quad but keep their width.
pub(crate) fn push_label_vertices(
    verts: &mut Vec<TextVertex>,
    label: &str,
    center_x: f32,
    center_y: f32,
    glyph_ndc_w: f32,
    glyph_ndc_h: f32,
) {
    let half_text_w = label.chars().count() as f32 * glyph_ndc_w * 0.5;
    let top = center_y + glyph_ndc_h * 0.5;
    let bottom = center_y - glyph_ndc_h * 0.5;
    let atlas_len = ASCII_CHARS.len() as f32;

    for (i, ch) in label.chars().enumerate() {
        let Some(atlas_idx) = ascii_index(ch) else {
            continue;
        };

        let u0 = atlas_idx as f32 / atlas_len;
        let u1 = (atlas_idx + 1) as f32 / atlas_len;
        let left = center_x - half_text_w + i as f32 * glyph_ndc_w;
        let right = left + glyph_ndc_w;

        let tl = TextVertex {
            position: [left, top],
            uv: [u0, 0.0],
        };
        let bl = TextVertex {
            position: [left, bottom],
            uv: [u0, 1.0],
        };
        let tr = TextVertex {
            position: [right, top],
            uv: [u1, 0.0],
        };
        let br = TextVertex {
            position: [right, bottom],
            uv: [u1, 1.0],
        };
        verts.extend_from_slice(&[tl, bl, tr, bl, br, tr]);
    }
}

/// The printable-ASCII glyph texture, built once per renderer and shared by
/// every text batch and the plot window (#303): a batch keeps only its
/// colour and vertices.
pub(crate) struct GlyphAtlas {
    /// Kept alive for the bind groups that sample `view`.
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl GlyphAtlas {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let width = ASCII_CHARS.len() as u32 * GLYPH_W;
        let (texture, view) = textures::r8_texture(device, "Glyph Atlas", width, GLYPH_H);
        textures::write_r8(
            queue,
            &texture,
            &build_atlas_data(&ASCII_CHARS),
            width,
            GLYPH_H,
        );
        Self {
            _texture: texture,
            view,
        }
    }

    /// The view the text shaders sample.
    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

/// Vertex batch for one text overlay in a single colour, drawn from the
/// shared [`GlyphAtlas`]. Lay out labels with [`push_label_vertices`].
pub(crate) struct TextBatch {
    gpu: GpuBatch<TextVertex>,
}

impl TextBatch {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        label: &'static str,
        glyphs: &GlyphAtlas,
        color: [f32; 4],
    ) -> Self {
        let gpu = GpuBatch::new(
            device,
            format,
            BatchDesc::vertices(
                label,
                shaders::TEXT,
                &TEXT_ATTRS,
                wgpu::PrimitiveTopology::TriangleList,
            )
            // Nearest keeps the 8×8 bitmap glyphs crisp.
            .texture(glyphs.view(), wgpu::FilterMode::Nearest),
        );
        gpu.set_uniforms(queue, SceneUniforms::ndc(color));

        Self { gpu }
    }

    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        verts: &[TextVertex],
    ) {
        self.gpu.flush(device, queue, verts);
    }

    pub(crate) fn clear(&mut self) {
        self.gpu.clear();
    }

    pub(crate) fn draw<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        self.gpu.draw(render_pass);
    }
}

/// Row-major R8 atlas: glyph `i` of `atlas_chars` at x = `i * GLYPH_W`.
pub(crate) fn build_atlas_data(atlas_chars: &[char]) -> Vec<u8> {
    let width = atlas_chars.len() * GLYPH_W as usize;
    let mut data = vec![0u8; width * GLYPH_H as usize];

    for (idx, &ch) in atlas_chars.iter().enumerate() {
        let Some(glyph) = font8x8::BASIC_FONTS.get(ch) else {
            continue;
        };
        let x_off = idx * GLYPH_W as usize;
        for (row, &bits) in glyph.iter().enumerate() {
            for col in 0..GLYPH_W as usize {
                if bits & (1 << col) != 0 {
                    data[row * width + x_off + col] = 255;
                }
            }
        }
    }
    data
}
