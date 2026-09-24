//! Bitmap text: an 8×8 glyph atlas built from `font8x8` plus a [`GpuBatch`]
//! of glyph quads in NDC. Used by every text overlay (cursor readout, cell
//! values, thermostat and source-sphere labels). See `text.wgsl`.

use bytemuck::{Pod, Zeroable};
use font8x8::UnicodeFonts;
use rust_thermo_core::graphics::renderer::margins::Margins;
use rust_thermo_core::graphics::sidebar::sidebar_ui_scale;

use super::batch::{BatchDesc, GpuBatch, SceneUniforms};
use super::{shaders, textures};

pub(crate) const GLYPH_W: u32 = 8;
pub(crate) const GLYPH_H: u32 = 8;
pub(crate) const BLACK_TEXT_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

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
pub(crate) fn inset_glyph_ndc_size(window_size: (u32, u32), margin: Margins) -> (f32, f32) {
    let inset_w = (window_size.0 as f32 - margin.horizontal()).max(1.0);
    let inset_h = (window_size.1 as f32 - margin.vertical()).max(1.0);
    let ui_scale = sidebar_ui_scale(margin.left);
    (
        GLYPH_W as f32 * ui_scale * 2.0 / inset_w,
        GLYPH_H as f32 * ui_scale * 2.0 / inset_h,
    )
}

/// Append one label (6 vertices per glyph) centred on `(center_x, center_y)`
/// in NDC. Characters missing from `atlas_chars` are skipped but keep their
/// space.
pub(crate) fn push_label_vertices(
    verts: &mut Vec<TextVertex>,
    atlas_chars: &[char],
    label: &str,
    center_x: f32,
    center_y: f32,
    glyph_ndc_w: f32,
    glyph_ndc_h: f32,
) {
    let half_text_w = label.chars().count() as f32 * glyph_ndc_w * 0.5;
    let top = center_y + glyph_ndc_h * 0.5;
    let bottom = center_y - glyph_ndc_h * 0.5;
    let atlas_len = atlas_chars.len() as f32;

    for (i, ch) in label.chars().enumerate() {
        let Some(atlas_idx) = atlas_chars.iter().position(|&glyph| glyph == ch) else {
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

/// Glyph atlas + vertex batch for one text overlay in a single colour.
/// Lay out labels with [`push_label_vertices`] using the same `atlas_chars`.
pub(crate) struct TextBatch {
    gpu: GpuBatch<TextVertex>,
    /// Kept alive for the batch's bind group
    _atlas: wgpu::Texture,
}

impl TextBatch {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        label: &'static str,
        atlas_chars: &[char],
        color: [f32; 4],
    ) -> Self {
        let atlas_w = atlas_chars.len() as u32 * GLYPH_W;
        let (atlas, view) = textures::r8_texture(device, label, atlas_w, GLYPH_H);
        textures::write_r8(
            queue,
            &atlas,
            &build_atlas_data(atlas_chars),
            atlas_w,
            GLYPH_H,
        );

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
            .texture(&view, wgpu::FilterMode::Nearest),
        );
        gpu.set_uniforms(queue, SceneUniforms::ndc(color));

        Self { gpu, _atlas: atlas }
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
fn build_atlas_data(atlas_chars: &[char]) -> Vec<u8> {
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
