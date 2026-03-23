//! Per-cell thermodynamic value text overlay pipeline for WGPU.
//!
//! Renders a formatted two-decimal label (e.g., "12.34") centred on each grid
//! cell that holds a finite T/P value.  Hidden when `ThermoDisplayMode::Off`.
//!
//! Reuses the existing `text.wgsl` shader (same vertex format) and a compact
//! 8×8 bitmap font atlas built once from `font8x8`.

use bytemuck::{Pod, Zeroable};
use font8x8::UnicodeFonts;

use super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::application::renderer_snapshot::ThermoGridSpec;
use rust_thermo_core::application::thermo_field::ThermoGrid;
use rust_thermo_core::graphics::renderer::state_layers::ThermoDisplayMode;

// ── Font atlas ────────────────────────────────────────────────────────

/// Characters stored in the atlas, in index order.
const ATLAS_CHARS: [char; 12] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', '-'];
const N_ATLAS_CHARS: u32 = 12;
const GLYPH_W: u32 = 8;
const GLYPH_H: u32 = 8;

fn char_atlas_idx(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        '.' => Some(10),
        '-' => Some(11),
        _ => None,
    }
}

fn build_atlas_data() -> Vec<u8> {
    let w = (N_ATLAS_CHARS * GLYPH_W) as usize;
    let h = GLYPH_H as usize;
    let mut data = vec![0u8; w * h];
    for (idx, &ch) in ATLAS_CHARS.iter().enumerate() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
            let x_off = idx * GLYPH_W as usize;
            for (row, &bits) in glyph.iter().enumerate() {
                for col in 0..8usize {
                    if bits & (1 << col) != 0 {
                        data[row * w + x_off + col] = 255;
                    }
                }
            }
        }
    }
    data
}

// ── Vertex ────────────────────────────────────────────────────────────

/// Same layout as `TextPipeline` — compatible with `text.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TextVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

// ── Pipeline ──────────────────────────────────────────────────────────

pub struct ThermoValueOverlayPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Font atlas texture view + bind group; created lazily on first
    /// `prepare()` call so that `new()` doesn't need a `wgpu::Queue`.
    atlas: Option<(wgpu::TextureView, wgpu::BindGroup)>,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: u32,
}

impl ThermoValueOverlayPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Thermo Value Overlay Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text.wgsl").into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Thermo Value Overlay Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Thermo Value Overlay BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Thermo Value Overlay Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Thermo Value Overlay Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TextVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 8,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        const INITIAL_CAPACITY: usize = 1024;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Thermo Value Overlay Vertex Buffer"),
            size: (INITIAL_CAPACITY * std::mem::size_of::<TextVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            atlas: None,
            vertex_buffer,
            vertex_capacity: INITIAL_CAPACITY,
            vertex_count: 0,
        }
    }

    /// Upload the font atlas on first use (needs `queue` which is unavailable in `new()`).
    fn ensure_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.atlas.is_some() {
            return;
        }

        let atlas_w = N_ATLAS_CHARS * GLYPH_W;
        let atlas_h = GLYPH_H;
        let data = build_atlas_data();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Thermo Value Font Atlas"),
            size: wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_w),
                rows_per_image: Some(atlas_h),
            },
            wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Thermo Value Overlay Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.atlas = Some((view, bind_group));
    }

    /// Grow the vertex buffer if `required` vertices exceed current capacity.
    fn ensure_vertex_buffer_capacity(&mut self, device: &wgpu::Device, required: usize) {
        if required <= self.vertex_capacity {
            return;
        }
        let mut cap = self.vertex_capacity;
        while cap < required {
            cap *= 2;
        }
        self.vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Thermo Value Overlay Vertex Buffer"),
            size: (cap * std::mem::size_of::<TextVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.vertex_capacity = cap;
    }

    /// Apply the column-major 4×4 view-projection matrix to a 2D world point.
    ///
    /// Returns the projected NDC xy coordinates.
    fn apply_view_proj(m: [[f32; 4]; 4], wx: f32, wy: f32) -> (f32, f32) {
        // Column-major: m[col][row]; homogeneous point = (wx, wy, 0, 1).
        let x = m[0][0] * wx + m[1][0] * wy + m[3][0];
        let y = m[0][1] * wx + m[1][1] * wy + m[3][1];
        let w = m[0][3] * wx + m[1][3] * wy + m[3][3];
        (x / w, y / w)
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

                let (ndc_x, ndc_y) = Self::apply_view_proj(view_proj, cx, cy);

                let half_text_w = label.len() as f32 * glyph_ndc_w * 0.5;
                let top = ndc_y + glyph_ndc_h * 0.5;
                let bot = ndc_y - glyph_ndc_h * 0.5;

                for (i, ch) in label.chars().enumerate() {
                    let Some(atlas_idx) = char_atlas_idx(ch) else {
                        continue;
                    };
                    let u0 = atlas_idx as f32 / N_ATLAS_CHARS as f32;
                    let u1 = (atlas_idx + 1) as f32 / N_ATLAS_CHARS as f32;
                    let left = ndc_x - half_text_w + i as f32 * glyph_ndc_w;
                    let right = left + glyph_ndc_w;

                    // Two CCW triangles forming a quad.
                    verts.push(TextVertex {
                        position: [left, top],
                        uv: [u0, 0.0],
                    });
                    verts.push(TextVertex {
                        position: [left, bot],
                        uv: [u0, 1.0],
                    });
                    verts.push(TextVertex {
                        position: [right, top],
                        uv: [u1, 0.0],
                    });
                    verts.push(TextVertex {
                        position: [left, bot],
                        uv: [u0, 1.0],
                    });
                    verts.push(TextVertex {
                        position: [right, bot],
                        uv: [u1, 1.0],
                    });
                    verts.push(TextVertex {
                        position: [right, top],
                        uv: [u1, 0.0],
                    });
                }
            }
        }
        verts
    }
}

// ── RenderPipeline impl ───────────────────────────────────────────────

impl RenderPipeline for ThermoValueOverlayPipeline {
    fn name(&self) -> &'static str {
        "thermo_value_overlay"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        if ctx.thermo_display == ThermoDisplayMode::Off {
            self.vertex_count = 0;
            return Ok(());
        }

        let Some(spec) = &ctx.snapshot.thermo_grid_spec else {
            self.vertex_count = 0;
            return Ok(());
        };

        let grid = match ctx.thermo_display {
            ThermoDisplayMode::Temperature => &ctx.snapshot.cell_temperature,
            ThermoDisplayMode::Pressure => &ctx.snapshot.cell_pressure,
            ThermoDisplayMode::Off => unreachable!(),
        };

        let (rows, cols) = grid.shape();
        if rows == 0 || cols == 0 {
            self.vertex_count = 0;
            return Ok(());
        }

        self.ensure_atlas(ctx.device, ctx.queue);

        // Glyph size in NDC units within the inset viewport.
        // Inset dimensions in pixels: (window_w - 2*margin_x) × (window_h - 2*margin_y).
        // NDC range is 2 (-1 to 1), so 1 pixel = 2 / inset_px.
        let inset_w = (ctx.window_size.0 as f32 - 2.0 * ctx.margin.0).max(1.0);
        let inset_h = (ctx.window_size.1 as f32 - 2.0 * ctx.margin.1).max(1.0);
        let glyph_ndc_w = GLYPH_W as f32 * 2.0 / inset_w;
        let glyph_ndc_h = GLYPH_H as f32 * 2.0 / inset_h;

        let verts = Self::build_vertices(grid, spec, ctx.view_proj, glyph_ndc_w, glyph_ndc_h);

        if verts.is_empty() {
            self.vertex_count = 0;
            return Ok(());
        }

        self.ensure_vertex_buffer_capacity(ctx.device, verts.len());
        ctx.queue
            .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        self.vertex_count = verts.len() as u32;

        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        if self.vertex_count == 0 {
            return;
        }
        let Some((_, bind_group)) = &self.atlas else {
            return;
        };
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}
