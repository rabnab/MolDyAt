//! Sidebar icon rendering pipeline.
//!
//! Loads PNG icons from embedded bytes and renders them as textured quads
//! in the left margin of the window. Icons are arranged vertically from
//! top to bottom: wall, valve, thermo, sourceOpen.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use crate::wgpu::{MARGIN_BOTTOM, MARGIN_LEFT, MARGIN_TOP};
use rust_thermo_core::application::AppResult;
use rust_thermo_core::graphics::sidebar::{ICON_COUNT, ICON_DISPLAY_SIZE, ICON_SPACING};

const SIDEBAR_SHADER: &str = include_str!("shaders/sidebar.wgsl");

/// Embedded PNG icon data (compile-time).
pub(crate) const ICON_WALL: &[u8] = include_bytes!("icons/wall.png");
pub(crate) const ICON_VALVE: &[u8] = include_bytes!("icons/valve.png");
pub(crate) const ICON_THERMO: &[u8] = include_bytes!("icons/thermo.png");
pub(crate) const ICON_SOURCE_OPEN: &[u8] = include_bytes!("icons/sourceOpen.png");

/// Padding around the icon group inside the background rectangle.
pub(crate) const RECT_PADDING: f32 = 8.0;
/// Corner radius for the background rounded rectangle.
const CORNER_RADIUS: f32 = 12.0;
/// Border width for the background rounded rectangle outline.
const BORDER_WIDTH: f32 = 2.0;
/// 6 vertices per quad: 1 background + 1 highlight + ICON_COUNT icon quads.
pub(crate) const MAX_VERTICES: usize = (ICON_COUNT + 2) * 6;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct SidebarVertex {
    pub(crate) position: [f32; 2],
    pub(crate) uv: [f32; 2],
    /// 0.0 = background rounded rect, 1.0 = icon texture, 2.0 = highlight.
    pub(crate) mode: f32,
}

/// Uniform data for the rounded rectangle background.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct RoundedRectParams {
    pub(crate) size_px: [f32; 2],
    pub(crate) corner_radius: f32,
    pub(crate) border_width: f32,
}

/// Sidebar icon rendering pipeline.
///
/// Packs all icons into a single RGBA texture atlas (vertically stacked)
/// and draws one textured quad per icon.
///
/// Since icons are static, vertex data is only rebuilt when the window
/// size changes (i.e. on resize). No per-frame work is done otherwise.
pub struct SidebarPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    rect_uniform_buffer: wgpu::Buffer,
    vertex_count: u32,
    /// How many icons are in the atlas (used for UV calculation).
    atlas_icon_count: u32,
    /// Cached window size to skip redundant vertex rebuilds.
    last_window_size: (u32, u32),
    /// Cached selected icon to detect selection changes.
    last_selected_icon: Option<usize>,
}

/// Decode a PNG from memory into an RGBA8 buffer, resized to `size×size`.
pub(crate) fn decode_icon(png_bytes: &[u8], size: u32) -> Vec<u8> {
    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .expect("Failed to decode embedded PNG icon");
    let resized = img.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
    resized.to_rgba8().into_raw()
}

impl SidebarPipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        // ── Build texture atlas ────────────────────────────────────
        // All icons are decoded to ATLAS_CELL × ATLAS_CELL RGBA and stacked vertically.
        const ATLAS_CELL: u32 = 64; // texture resolution per icon
        let icons: [&[u8]; ICON_COUNT] = [ICON_WALL, ICON_VALVE, ICON_THERMO, ICON_SOURCE_OPEN];

        let atlas_width = ATLAS_CELL;
        let atlas_height = ATLAS_CELL * ICON_COUNT as u32;
        let mut atlas_data = Vec::with_capacity((atlas_width * atlas_height * 4) as usize);

        for icon_bytes in &icons {
            let decoded = decode_icon(icon_bytes, ATLAS_CELL);
            atlas_data.extend_from_slice(&decoded);
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Sidebar Icon Atlas"),
            size: wgpu::Extent3d {
                width: atlas_width,
                height: atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
            &atlas_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_width * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: atlas_width,
                height: atlas_height,
                depth_or_array_layers: 1,
            },
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Sidebar Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Bind group ─────────────────────────────────────────────
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Sidebar Bind Group Layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let rect_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Sidebar Rect Uniform"),
            contents: bytemuck::bytes_of(&RoundedRectParams {
                size_px: [0.0, 0.0],
                corner_radius: CORNER_RADIUS,
                border_width: BORDER_WIDTH,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sidebar Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: rect_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        // ── Render pipeline ────────────────────────────────────────
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sidebar Shader"),
            source: wgpu::ShaderSource::Wgsl(SIDEBAR_SHADER.into()),
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SidebarVertex>() as u64,
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
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        };

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sidebar Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sidebar Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[vertex_layout],
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
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // ── Vertex buffer (updated each frame in prepare) ──────────
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Sidebar Vertex Buffer"),
            contents: bytemuck::cast_slice(
                &[SidebarVertex {
                    position: [0.0; 2],
                    uv: [0.0; 2],
                    mode: 0.0,
                }; MAX_VERTICES],
            ),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            pipeline,
            bind_group,
            vertex_buffer,
            rect_uniform_buffer,
            vertex_count: 0,
            atlas_icon_count: ICON_COUNT as u32,
            last_window_size: (0, 0),
            last_selected_icon: None,
        }
    }

    /// Build vertex data and rect params for all sidebar geometry.
    fn build_geometry(
        &self,
        win_w: f32,
        win_h: f32,
        selected: Option<usize>,
    ) -> (Vec<SidebarVertex>, RoundedRectParams) {
        build_sidebar_geometry(self.atlas_icon_count, win_w, win_h, selected)
    }
}

/// Build sidebar geometry: a background rounded-rect quad followed by icon quads.
///
/// Returns the vertex list and the rounded-rect uniform params.
/// Icons are centered horizontally in the left margin, stacked vertically
/// starting below the top margin. Icons that would exceed the bottom margin
/// are omitted. The background rectangle encloses all visible icons with
/// [`RECT_PADDING`] on each side.
pub(crate) fn build_sidebar_geometry(
    icon_count: u32,
    win_w: f32,
    win_h: f32,
    selected: Option<usize>,
) -> (Vec<SidebarVertex>, RoundedRectParams) {
    let mut verts = Vec::with_capacity(MAX_VERTICES);

    let to_ndc = |px: f32, py: f32| -> [f32; 2] {
        let x = (px / win_w) * 2.0 - 1.0;
        let y = 1.0 - (py / win_h) * 2.0;
        [x, y]
    };

    let pad_x = (MARGIN_LEFT - ICON_DISPLAY_SIZE) * 0.5;
    let first_icon_top = MARGIN_TOP + ICON_SPACING;

    // Count how many icons fit.
    let visible_count =
        rust_thermo_core::graphics::sidebar::visible_icon_count(win_h, MARGIN_TOP, MARGIN_BOTTOM)
            as u32;
    // Clamp to the number of icons actually in the atlas.
    let visible_count = visible_count.min(icon_count);

    let mut rect_params = RoundedRectParams {
        size_px: [0.0, 0.0],
        corner_radius: CORNER_RADIUS,
        border_width: BORDER_WIDTH,
    };

    if visible_count == 0 {
        return (verts, rect_params);
    }

    // ── Background rounded rectangle ───────────────────────────────
    let rect_left = pad_x - RECT_PADDING;
    let rect_right = pad_x + ICON_DISPLAY_SIZE + RECT_PADDING;
    let rect_top = first_icon_top - RECT_PADDING;
    let last_icon_bottom = first_icon_top
        + (visible_count - 1) as f32 * (ICON_DISPLAY_SIZE + ICON_SPACING)
        + ICON_DISPLAY_SIZE;
    let rect_bottom = last_icon_bottom + RECT_PADDING;

    let rect_w = rect_right - rect_left;
    let rect_h = rect_bottom - rect_top;
    rect_params.size_px = [rect_w, rect_h];

    let bg_tl = to_ndc(rect_left, rect_top);
    let bg_tr = to_ndc(rect_right, rect_top);
    let bg_bl = to_ndc(rect_left, rect_bottom);
    let bg_br = to_ndc(rect_right, rect_bottom);

    // Two triangles for the background quad (mode = 0.0).
    verts.push(SidebarVertex {
        position: bg_tl,
        uv: [0.0, 0.0],
        mode: 0.0,
    });
    verts.push(SidebarVertex {
        position: bg_bl,
        uv: [0.0, 1.0],
        mode: 0.0,
    });
    verts.push(SidebarVertex {
        position: bg_tr,
        uv: [1.0, 0.0],
        mode: 0.0,
    });
    verts.push(SidebarVertex {
        position: bg_tr,
        uv: [1.0, 0.0],
        mode: 0.0,
    });
    verts.push(SidebarVertex {
        position: bg_bl,
        uv: [0.0, 1.0],
        mode: 0.0,
    });
    verts.push(SidebarVertex {
        position: bg_br,
        uv: [1.0, 1.0],
        mode: 0.0,
    });

    // ── Highlight quad behind selected icon ──────────────────────────
    if let Some(sel) = selected {
        let sel = sel as u32;
        if sel < visible_count {
            let icon_top = first_icon_top + sel as f32 * (ICON_DISPLAY_SIZE + ICON_SPACING);
            let icon_bottom = icon_top + ICON_DISPLAY_SIZE;
            let hl_tl = to_ndc(pad_x, icon_top);
            let hl_tr = to_ndc(pad_x + ICON_DISPLAY_SIZE, icon_top);
            let hl_bl = to_ndc(pad_x, icon_bottom);
            let hl_br = to_ndc(pad_x + ICON_DISPLAY_SIZE, icon_bottom);

            for &(pos, uv) in &[
                (hl_tl, [0.0, 0.0]),
                (hl_bl, [0.0, 1.0]),
                (hl_tr, [1.0, 0.0]),
                (hl_tr, [1.0, 0.0]),
                (hl_bl, [0.0, 1.0]),
                (hl_br, [1.0, 1.0]),
            ] {
                verts.push(SidebarVertex {
                    position: pos,
                    uv,
                    mode: 2.0,
                });
            }
        }
    }

    // ── Icon quads ─────────────────────────────────────────────────
    for i in 0..visible_count {
        let icon_top = first_icon_top + i as f32 * (ICON_DISPLAY_SIZE + ICON_SPACING);
        let icon_left = pad_x;
        let icon_right = icon_left + ICON_DISPLAY_SIZE;
        let icon_bottom = icon_top + ICON_DISPLAY_SIZE;

        let uv_top = i as f32 / icon_count as f32;
        let uv_bottom = (i + 1) as f32 / icon_count as f32;

        let tl = to_ndc(icon_left, icon_top);
        let tr = to_ndc(icon_right, icon_top);
        let bl = to_ndc(icon_left, icon_bottom);
        let br = to_ndc(icon_right, icon_bottom);

        verts.push(SidebarVertex {
            position: tl,
            uv: [0.0, uv_top],
            mode: 1.0,
        });
        verts.push(SidebarVertex {
            position: bl,
            uv: [0.0, uv_bottom],
            mode: 1.0,
        });
        verts.push(SidebarVertex {
            position: tr,
            uv: [1.0, uv_top],
            mode: 1.0,
        });
        verts.push(SidebarVertex {
            position: tr,
            uv: [1.0, uv_top],
            mode: 1.0,
        });
        verts.push(SidebarVertex {
            position: bl,
            uv: [0.0, uv_bottom],
            mode: 1.0,
        });
        verts.push(SidebarVertex {
            position: br,
            uv: [1.0, uv_bottom],
            mode: 1.0,
        });
    }

    (verts, rect_params)
}

impl RenderPipeline for SidebarPipeline {
    fn name(&self) -> &'static str {
        "sidebar"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::FullWindow
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        let selected = ctx.selected_sidebar_icon;
        if ctx.window_size == self.last_window_size && selected == self.last_selected_icon {
            return Ok(());
        }
        self.last_window_size = ctx.window_size;
        self.last_selected_icon = selected;

        let (w, h) = ctx.window_size;
        let (verts, rect_params) = self.build_geometry(w as f32, h as f32, selected);
        self.vertex_count = verts.len() as u32;
        if !verts.is_empty() {
            ctx.queue
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            ctx.queue.write_buffer(
                &self.rect_uniform_buffer,
                0,
                bytemuck::bytes_of(&rect_params),
            );
        }
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        if self.vertex_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}
