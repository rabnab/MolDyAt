//! Thermo heatmap overlay pipeline for WGPU.
//!
//! Renders a single quad covering the simulation world area.  The scalar T/P
//! field is uploaded as an `R8Unorm` texture (one texel per cell) and
//! reconstructed in the fragment shader with a cubic B-spline filter (four
//! bilinear taps), so a coarse grid looks like a smooth surface.
//!
//! To keep the picture calm the field is smoothed before upload:
//!   1. NaN cells are filled from their neighbours,
//!   2. a small spatial blur removes cell-to-cell noise,
//!   3. an exponential moving average over time removes frame-to-frame jiggle.
//!
//! Encoding (R8Unorm): the field value normalised to `[0, 1]` maps to bytes
//! `0..=255`. There is no "no data" sentinel: NaN cells are filled first, and an
//! all-NaN field simply isn't drawn.

use rust_thermo_core::application::renderer_snapshot::ThermoGridSpec;
use rust_thermo_core::application::thermo_field::ThermoGrid;
use rust_thermo_core::graphics::renderer::interpolation::fill_nans;
use rust_thermo_core::graphics::renderer::state_layers::ThermoDisplayMode;
use std::time::Instant;
use wgpu::util::DeviceExt;

use super::super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use rust_thermo_core::application::AppResult;

// min and max values for thermo heatmap normalisation
const TEMP_MIN: f32 = 0.0;
const TEMP_MAX: f32 = 5.0; // tune to your simulation's typical range
const PRES_MIN: f32 = -4.0;
const PRES_MAX: f32 = 4.0; // tune similarly

/// Number of 3x3 spatial blur passes (weights 1-2-1 per axis) applied to the
/// field before upload. 0 = off, 1 = gentle, 2 = strong.
const SPATIAL_BLUR_PASSES: usize = 1;
/// Time constant (seconds) of the moving average applied to the displayed
/// field. Larger = calmer but slower to follow real changes.
const TEMPORAL_SMOOTHING_SECS: f32 = 0.75;

// ── GPU types ──────────────────────────────────────────────────────

/// Uniform buffer — must match `thermo_heatmap.wgsl` `Uniforms` layout.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct HeatmapUniforms {
    view_proj: [[f32; 4]; 4],
    grid_origin: [f32; 2],
    grid_size: [f32; 2],
}

impl Default for HeatmapUniforms {
    fn default() -> Self {
        Self {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            grid_origin: [0.0, 0.0],
            grid_size: [1.0, 1.0],
        }
    }
}

// ── Pipeline ───────────────────────────────────────────────────────

/// Thermo heatmap overlay pipeline.
///
/// Lifecycle per frame:
/// 1. `prepare()` uploads a new texture when the field changes or the grid resizes.
/// 2. `render()` draws a single 6-vertex quad if `has_data` is true.
pub struct ThermoHeatmapPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    uniforms: HeatmapUniforms,
    /// Width (cols) of the current texture in texels.
    tex_cols: u32,
    /// Height (rows) of the current texture in texels.
    tex_rows: u32,
    has_data: bool,
    /// Time-smoothed field currently on screen (row-major, all finite).
    smoothed: Vec<f32>,
    smoothed_dims: (usize, usize),
    /// Display mode `smoothed` belongs to (temperature vs pressure).
    smoothed_mode: Option<std::mem::Discriminant<ThermoDisplayMode>>,
    last_smooth: Instant,
}

impl ThermoHeatmapPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, shader_source: &str) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Thermo Heatmap Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let uniforms = HeatmapUniforms::default();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Thermo Heatmap Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Start with a 1×1 placeholder texture (R8Unorm, linear filter).
        let (texture, texture_view) = Self::create_texture(device, 1, 1);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Thermo Heatmap Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let bind_group_layout = Self::create_bind_group_layout(device);
        let bind_group = Self::create_bind_group(
            device,
            &bind_group_layout,
            &uniform_buffer,
            &texture_view,
            &sampler,
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Thermo Heatmap Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Thermo Heatmap Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
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

        Self {
            pipeline,
            uniform_buffer,
            texture,
            texture_view,
            sampler,
            bind_group_layout,
            bind_group,
            uniforms,
            tex_cols: 1,
            tex_rows: 1,
            has_data: false,
            smoothed: Vec::new(),
            smoothed_dims: (0, 0),
            smoothed_mode: None,
            last_smooth: Instant::now(),
        }
    }

    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Thermo Heatmap Bind Group Layout"),
            entries: &[
                // Binding 0 — uniform buffer (vertex + fragment)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Binding 1 — R8Unorm scalar field texture (fragment)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Binding 2 — linear sampler (fragment)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform_buf: &wgpu::Buffer,
        texture_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Thermo Heatmap Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// Create a new `R8Unorm` texture of the given dimensions.
    fn create_texture(
        device: &wgpu::Device,
        cols: u32,
        rows: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Thermo Heatmap Texture"),
            size: wgpu::Extent3d {
                width: cols,
                height: rows,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Exponential moving average over time, in place.
    ///
    /// Restarts from the current field when the grid shape or the display mode
    /// (temperature / pressure) changes. The weight is derived from the elapsed
    /// time, so the smoothing doesn't depend on the frame rate, and after a long
    /// pause (e.g. the overlay was switched off) the field snaps to the current
    /// values instead of sliding from stale ones.
    fn smooth_in_time(
        &mut self,
        field: &mut [f32],
        rows: usize,
        cols: usize,
        mode: ThermoDisplayMode,
    ) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_smooth).as_secs_f32();
        self.last_smooth = now;

        let mode_id = std::mem::discriminant(&mode);
        let restart = self.smoothed.len() != field.len()
            || self.smoothed_dims != (rows, cols)
            || self.smoothed_mode != Some(mode_id);
        if restart {
            self.smoothed.clear();
            self.smoothed.extend_from_slice(field);
            self.smoothed_dims = (rows, cols);
            self.smoothed_mode = Some(mode_id);
            return;
        }

        let alpha = 1.0 - (-dt / TEMPORAL_SMOOTHING_SECS).exp();
        for (shown, &target) in self.smoothed.iter_mut().zip(field.iter()) {
            *shown += (target - *shown) * alpha;
        }
        field.copy_from_slice(&self.smoothed);
    }

    /// Upload scalar field data to the GPU texture and update uniforms.
    ///
    /// Applies NaN-fill, computes the normalisation range, encodes each cell
    /// as an `R8Unorm` byte (byte 0 = NaN sentinel, 1–255 = valid value),
    /// resizes the texture if the grid changed shape, and writes the view-proj
    /// + world-origin uniforms.
    ///
    /// Returns `false` when rendering should be skipped (no finite data, or
    /// the field is uniform to within floating-point noise).
    fn upload_field(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        grid: &ThermoGrid,
        spec: &ThermoGridSpec,
        view_proj: [[f32; 4]; 4],
        mode: ThermoDisplayMode,
    ) -> bool {
        let (rows, cols) = grid.shape();
        if rows == 0 || cols == 0 {
            self.has_data = false;
            return false;
        }

        // --- NaN fill --------------------------------------------------
        let mut filled = fill_nans(grid.as_slice(), rows, cols);

        if filled.iter().all(|v| !v.is_finite()) {
            self.has_data = false;
            return false;
        }

        // --- Smoothing (everything is finite from here on) --------------
        for _ in 0..SPATIAL_BLUR_PASSES {
            filled = blur_3x3(&filled, rows, cols);
        }
        self.smooth_in_time(&mut filled, rows, cols, mode);

        // --- Normalisation range ---------------------------------------
        // let (v_min, v_max) = filled
        //     .iter()
        //     .filter(|v| v.is_finite())
        //     .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| {
        //         (lo.min(v), hi.max(v))
        //     });

        // let range_threshold = 1e-6 * v_max.abs().max(1.0);
        // if v_max - v_min < range_threshold {
        //     self.has_data = false;
        //     return false;
        // }

        // let inv_range = 1.0 / (v_max - v_min);
        let (v_min, v_max) = match mode {
            ThermoDisplayMode::Temperature => (TEMP_MIN, TEMP_MAX),
            ThermoDisplayMode::Pressure => (PRES_MIN, PRES_MAX),
            ThermoDisplayMode::Off | ThermoDisplayMode::Grid => unreachable!(),
        };
        let inv_range = 1.0 / (v_max - v_min);

        // --- Encode bytes (R8Unorm, full 0..=255 range) ----------------
        // Row 0 of ThermoGrid = world bottom → becomes texture row 0
        // (texture UV V=0 is sampled there for world bottom, matching vertex UV).
        let bytes: Vec<u8> = filled
            .iter()
            .map(|&v| {
                let t = ((v - v_min) * inv_range).clamp(0.0, 1.0);
                (t * 255.0).round() as u8
            })
            .collect();

        // --- Resize texture if needed ----------------------------------
        let need_new = self.tex_cols != cols as u32 || self.tex_rows != rows as u32;
        if need_new {
            let (tex, view) = Self::create_texture(device, cols as u32, rows as u32);
            self.texture = tex;
            self.texture_view = view;
            self.bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                &self.uniform_buffer,
                &self.texture_view,
                &self.sampler,
            );
            self.tex_cols = cols as u32;
            self.tex_rows = rows as u32;
        }

        // --- Upload texture -------------------------------------------
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(cols as u32),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: cols as u32,
                height: rows as u32,
                depth_or_array_layers: 1,
            },
        );

        // --- Uniforms --------------------------------------------------
        self.uniforms.view_proj = view_proj;
        self.uniforms.grid_origin = [spec.origin_x, spec.origin_y];
        self.uniforms.grid_size = [
            spec.cell_width * cols as f32,
            spec.cell_height * rows as f32,
        ];
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniforms]),
        );

        self.has_data = true;
        true
    }
}

// ── Field smoothing ─────────────────────────────────────────────────

/// One separable 3x3 blur pass with weights 1-2-1 per axis. Edges replicate
/// the border cell, so the field isn't pulled towards zero at the boundary.
fn blur_3x3(data: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    if rows == 0 || cols == 0 {
        return data.to_vec();
    }

    // Horizontal pass
    let mut tmp = vec![0.0_f32; data.len()];
    for r in 0..rows {
        for c in 0..cols {
            let left = data[r * cols + c.saturating_sub(1)];
            let mid = data[r * cols + c];
            let right = data[r * cols + (c + 1).min(cols - 1)];
            tmp[r * cols + c] = 0.25 * left + 0.5 * mid + 0.25 * right;
        }
    }

    // Vertical pass
    let mut out = vec![0.0_f32; data.len()];
    for r in 0..rows {
        let up = r.saturating_sub(1);
        let down = (r + 1).min(rows - 1);
        for c in 0..cols {
            out[r * cols + c] =
                0.25 * tmp[up * cols + c] + 0.5 * tmp[r * cols + c] + 0.25 * tmp[down * cols + c];
        }
    }
    out
}

// ── RenderPipeline trait implementation ─────────────────────────────

impl RenderPipeline for ThermoHeatmapPipeline {
    fn name(&self) -> &'static str {
        "thermo_heatmap"
    }

    fn viewport_mode(&self) -> ViewportMode {
        ViewportMode::Inset
    }

    fn prepare(&mut self, ctx: &PrepareContext) -> AppResult {
        if !ctx.thermo_display.is_thermo_active() {
            self.has_data = false;
            return Ok(());
        }

        let Some(spec) = &ctx.snapshot.thermo_grid_spec else {
            log::debug!("ThermoHeatmapPipeline: no thermo_grid_spec, skipping");
            self.has_data = false;
            return Ok(());
        };

        let grid = match ctx.thermo_display {
            ThermoDisplayMode::Temperature => &ctx.snapshot.cell_temperature,
            ThermoDisplayMode::Pressure => &ctx.snapshot.cell_pressure,
            ThermoDisplayMode::Off | ThermoDisplayMode::Grid => unreachable!(),
        };

        let (grid_rows, grid_cols) = grid.shape();
        log::debug!(
            "ThermoHeatmapPipeline::prepare [{:?}] — spec: {}×{} cells ({:.3}×{:.3} world/cell, origin [{:.3},{:.3}]), grid: {}×{}",
            ctx.thermo_display,
            spec.cols,
            spec.rows,
            spec.cell_width,
            spec.cell_height,
            spec.origin_x,
            spec.origin_y,
            grid_cols,
            grid_rows,
        );

        self.upload_field(
            ctx.device,
            ctx.queue,
            grid,
            spec,
            ctx.view_proj,
            ctx.thermo_display,
        );
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        if !self.has_data {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        // Single quad = 6 vertices, 1 instance.
        render_pass.draw(0..6, 0..1);
    }
}
