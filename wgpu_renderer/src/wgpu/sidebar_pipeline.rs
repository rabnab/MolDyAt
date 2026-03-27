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

const SIDEBAR_SHADER: &str = include_str!("shaders/sidebar.wgsl");

/// Embedded PNG icon data (compile-time).
const ICON_WALL: &[u8] = include_bytes!("icons/wall.png");
const ICON_VALVE: &[u8] = include_bytes!("icons/valve.png");
const ICON_THERMO: &[u8] = include_bytes!("icons/thermo.png");
const ICON_SOURCE_OPEN: &[u8] = include_bytes!("icons/sourceOpen.png");

/// Display size for each icon in pixels (square).
const ICON_DISPLAY_SIZE: f32 = 48.0;
/// Vertical spacing between icons in pixels.
const ICON_SPACING: f32 = 12.0;

/// Number of icons in the sidebar.
const ICON_COUNT: usize = 4;
/// Padding around the icon group inside the background rectangle.
const RECT_PADDING: f32 = 8.0;
/// Corner radius for the background rounded rectangle.
const CORNER_RADIUS: f32 = 12.0;
/// Border width for the background rounded rectangle outline.
const BORDER_WIDTH: f32 = 2.0;
/// 6 vertices per quad: 1 background + ICON_COUNT icon quads.
const MAX_VERTICES: usize = (ICON_COUNT + 1) * 6;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SidebarVertex {
    position: [f32; 2],
    uv: [f32; 2],
    /// 0.0 = background rounded rect, 1.0 = icon texture.
    mode: f32,
}

/// Uniform data for the rounded rectangle background.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RoundedRectParams {
    size_px: [f32; 2],
    corner_radius: f32,
    border_width: f32,
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
}

/// Decode a PNG from memory into an RGBA8 buffer, resized to `size×size`.
fn decode_icon(png_bytes: &[u8], size: u32) -> Vec<u8> {
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
        }
    }

    /// Build vertex data and rect params for all sidebar geometry.
    fn build_geometry(&self, win_w: f32, win_h: f32) -> (Vec<SidebarVertex>, RoundedRectParams) {
        build_sidebar_geometry(self.atlas_icon_count, win_w, win_h)
    }
}

/// Build sidebar geometry: a background rounded-rect quad followed by icon quads.
///
/// Returns the vertex list and the rounded-rect uniform params.
/// Icons are centered horizontally in the left margin, stacked vertically
/// starting below the top margin. Icons that would exceed the bottom margin
/// are omitted. The background rectangle encloses all visible icons with
/// [`RECT_PADDING`] on each side.
fn build_sidebar_geometry(
    icon_count: u32,
    win_w: f32,
    win_h: f32,
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
    let mut visible_count = 0u32;
    for i in 0..icon_count {
        let icon_top = first_icon_top + i as f32 * (ICON_DISPLAY_SIZE + ICON_SPACING);
        let icon_bottom = icon_top + ICON_DISPLAY_SIZE;
        if icon_bottom > win_h - MARGIN_BOTTOM {
            break;
        }
        visible_count += 1;
    }

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
        if ctx.window_size == self.last_window_size {
            return Ok(());
        }
        self.last_window_size = ctx.window_size;

        let (w, h) = ctx.window_size;
        let (verts, rect_params) = self.build_geometry(w as f32, h as f32);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Background quad uses 6 vertices.
    const BG_VERTS: usize = 6;

    #[test]
    fn sidebar_vertex_size_matches_shader_layout() {
        // position: vec2<f32> = 8, uv: vec2<f32> = 8, mode: f32 = 4 → 20 bytes
        assert_eq!(std::mem::size_of::<SidebarVertex>(), 20);
    }

    #[test]
    fn rounded_rect_params_size() {
        // size_px: vec2<f32> = 8, corner_radius: f32 = 4, border_width: f32 = 4 → 16 bytes
        assert_eq!(std::mem::size_of::<RoundedRectParams>(), 16);
    }

    #[test]
    fn max_vertices_includes_background() {
        assert_eq!(MAX_VERTICES, (ICON_COUNT + 1) * 6);
    }

    #[test]
    fn build_geometry_produces_correct_count_for_all_icons() {
        let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0);
        // 1 bg quad + 4 icon quads = 30 vertices
        assert_eq!(verts.len(), BG_VERTS + 4 * 6);
    }

    #[test]
    fn build_geometry_single_icon() {
        let (verts, _) = build_sidebar_geometry(1, 800.0, 600.0);
        assert_eq!(verts.len(), BG_VERTS + 6);
    }

    #[test]
    fn build_geometry_zero_icons() {
        let (verts, _) = build_sidebar_geometry(0, 800.0, 600.0);
        assert!(verts.is_empty());
    }

    #[test]
    fn build_geometry_clips_when_window_too_small() {
        let min_for_one = MARGIN_TOP + ICON_SPACING + ICON_DISPLAY_SIZE + MARGIN_BOTTOM;
        let (verts, _) = build_sidebar_geometry(4, 400.0, min_for_one + 1.0);
        assert_eq!(verts.len(), BG_VERTS + 6, "only one icon should fit");
    }

    #[test]
    fn build_geometry_empty_when_window_too_short() {
        let too_short = MARGIN_TOP + ICON_SPACING + ICON_DISPLAY_SIZE + MARGIN_BOTTOM - 1.0;
        let (verts, _) = build_sidebar_geometry(4, 400.0, too_short);
        assert!(verts.is_empty());
    }

    #[test]
    fn ndc_positions_are_within_clip_space() {
        let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0);
        for v in &verts {
            assert!(
                v.position[0] >= -1.0 && v.position[0] <= 1.0,
                "x NDC out of range: {}",
                v.position[0]
            );
            assert!(
                v.position[1] >= -1.0 && v.position[1] <= 1.0,
                "y NDC out of range: {}",
                v.position[1]
            );
        }
    }

    #[test]
    fn all_geometry_is_in_left_margin() {
        let win_w = 1920.0_f32;
        let (verts, _) = build_sidebar_geometry(4, win_w, 1080.0);
        let ndc_margin_right = (MARGIN_LEFT / win_w) * 2.0 - 1.0;
        for v in &verts {
            assert!(
                v.position[0] <= ndc_margin_right + 1e-5,
                "extends beyond left margin: NDC x={}, margin NDC x={}",
                v.position[0],
                ndc_margin_right
            );
        }
    }

    #[test]
    fn uv_coordinates_span_atlas_correctly() {
        let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0);
        let icons = &verts[BG_VERTS..];
        assert!((icons[0].uv[1] - 0.0).abs() < 1e-5, "first icon uv_top");
        assert!((icons[1].uv[1] - 0.25).abs() < 1e-5, "first icon uv_bottom");
        assert!((icons[6].uv[1] - 0.25).abs() < 1e-5, "second icon uv_top");
        assert!((icons[7].uv[1] - 0.5).abs() < 1e-5, "second icon uv_bottom");
        assert!((icons[18].uv[1] - 0.75).abs() < 1e-5, "fourth icon uv_top");
        assert!(
            (icons[19].uv[1] - 1.0).abs() < 1e-5,
            "fourth icon uv_bottom"
        );
    }

    #[test]
    fn uv_x_spans_full_texture_width() {
        let (verts, _) = build_sidebar_geometry(1, 800.0, 600.0);
        let icons = &verts[BG_VERTS..];
        assert!((icons[0].uv[0] - 0.0).abs() < 1e-5, "TL uv.x");
        assert!((icons[1].uv[0] - 0.0).abs() < 1e-5, "BL uv.x");
        assert!((icons[2].uv[0] - 1.0).abs() < 1e-5, "TR uv.x");
        assert!((icons[5].uv[0] - 1.0).abs() < 1e-5, "BR uv.x");
    }

    #[test]
    fn icons_are_vertically_ordered_top_to_bottom() {
        let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0);
        let icons = &verts[BG_VERTS..];
        for i in 0..3 {
            let quad_base = i * 6;
            let next_quad_base = (i + 1) * 6;
            assert!(
                icons[next_quad_base].position[1] < icons[quad_base].position[1],
                "icon {} is not above icon {}",
                i,
                i + 1
            );
        }
    }

    #[test]
    fn decode_icon_produces_correct_buffer_size() {
        let decoded = decode_icon(ICON_WALL, 64);
        assert_eq!(decoded.len(), 64 * 64 * 4);
    }

    #[test]
    fn decode_icon_all_embedded_pngs_are_valid() {
        for (name, data) in [
            ("wall", ICON_WALL),
            ("valve", ICON_VALVE),
            ("thermo", ICON_THERMO),
            ("sourceOpen", ICON_SOURCE_OPEN),
        ] {
            let decoded = decode_icon(data, 64);
            assert_eq!(
                decoded.len(),
                64 * 64 * 4,
                "{name} icon decoding produced wrong size"
            );
        }
    }

    #[test]
    fn icons_centered_horizontally_in_margin() {
        let win_w = 1920.0_f32;
        let (verts, _) = build_sidebar_geometry(1, win_w, 1080.0);
        let icons = &verts[BG_VERTS..];
        let expected_left_px = (MARGIN_LEFT - ICON_DISPLAY_SIZE) * 0.5;
        let expected_right_px = expected_left_px + ICON_DISPLAY_SIZE;
        let expected_left_ndc = (expected_left_px / win_w) * 2.0 - 1.0;
        let expected_right_ndc = (expected_right_px / win_w) * 2.0 - 1.0;
        assert!(
            (icons[0].position[0] - expected_left_ndc).abs() < 1e-5,
            "left edge"
        );
        assert!(
            (icons[2].position[0] - expected_right_ndc).abs() < 1e-5,
            "right edge"
        );
    }

    // ── Background rectangle tests ──────────────────────────────

    #[test]
    fn background_quad_has_mode_zero() {
        let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0);
        for v in &verts[..BG_VERTS] {
            assert!((v.mode).abs() < 1e-5, "bg vertex mode should be 0.0");
        }
    }

    #[test]
    fn icon_quads_have_mode_one() {
        let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0);
        for v in &verts[BG_VERTS..] {
            assert!(
                (v.mode - 1.0).abs() < 1e-5,
                "icon vertex mode should be 1.0"
            );
        }
    }

    #[test]
    fn background_encloses_all_icons() {
        let (verts, _) = build_sidebar_geometry(4, 1920.0, 1080.0);
        let bg = &verts[..BG_VERTS];
        let icons = &verts[BG_VERTS..];

        let bg_min_x = bg
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min);
        let bg_max_x = bg
            .iter()
            .map(|v| v.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let bg_min_y = bg
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        let bg_max_y = bg
            .iter()
            .map(|v| v.position[1])
            .fold(f32::NEG_INFINITY, f32::max);

        for v in icons {
            assert!(v.position[0] >= bg_min_x - 1e-5, "icon left of bg");
            assert!(v.position[0] <= bg_max_x + 1e-5, "icon right of bg");
            assert!(v.position[1] >= bg_min_y - 1e-5, "icon below bg");
            assert!(v.position[1] <= bg_max_y + 1e-5, "icon above bg");
        }
    }

    #[test]
    fn rect_params_have_positive_dimensions() {
        let (_, params) = build_sidebar_geometry(4, 1920.0, 1080.0);
        assert!(params.size_px[0] > 0.0, "rect width");
        assert!(params.size_px[1] > 0.0, "rect height");
        assert!(params.corner_radius > 0.0, "corner radius");
        assert!(params.border_width > 0.0, "border width");
    }

    #[test]
    fn rect_params_width_matches_icon_plus_padding() {
        let (_, params) = build_sidebar_geometry(4, 1920.0, 1080.0);
        let expected_w = ICON_DISPLAY_SIZE + 2.0 * RECT_PADDING;
        assert!(
            (params.size_px[0] - expected_w).abs() < 1e-5,
            "rect width: got {}, expected {}",
            params.size_px[0],
            expected_w
        );
    }

    #[test]
    fn rect_params_zero_when_no_icons_visible() {
        let (_, params) = build_sidebar_geometry(0, 800.0, 600.0);
        assert!((params.size_px[0]).abs() < 1e-5, "rect width should be 0");
        assert!((params.size_px[1]).abs() < 1e-5, "rect height should be 0");
    }
}
