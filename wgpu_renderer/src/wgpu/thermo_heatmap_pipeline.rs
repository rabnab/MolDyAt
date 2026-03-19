//! Thermo heatmap overlay pipeline for WGPU.
//!
//! Renders per-cell coloured quads representing temperature or pressure
//! fields.  Colours are computed on the CPU via [`rainbow_colormap`] and
//! uploaded to a storage buffer each frame.  The pipeline reads
//! [`ThermoDisplayMode`] from the [`PrepareContext`] and skips all work
//! when the mode is `Off` or there is no data.

use bytemuck::{Pod, Zeroable};
use rust_thermo_core::application::renderer_snapshot::ThermoGridSpec;
use rust_thermo_core::application::thermo_field::ThermoGrid;
use rust_thermo_core::graphics::renderer::interpolation::rainbow_colormap;
use rust_thermo_core::graphics::renderer::state_layers::ThermoDisplayMode;
use wgpu::util::DeviceExt;

use super::render_graph::{PrepareContext, RenderPipeline, ViewportMode};
use rust_thermo_core::application::AppResult;

// ── GPU types ──────────────────────────────────────────────────────

/// Uniform buffer data — must match `thermo_heatmap.wgsl` layout.
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
struct HeatmapUniforms {
    view_proj: [[f32; 4]; 4],
    grid_origin: [f32; 2],
    grid_cell_size: [f32; 2],
    grid_dims: [u32; 2],
    _padding: [u32; 2],
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
            grid_cell_size: [1.0, 1.0],
            grid_dims: [0, 0],
            _padding: [0, 0],
        }
    }
}

// ── Pipeline ───────────────────────────────────────────────────────

/// Thermo heatmap overlay pipeline.
pub struct ThermoHeatmapPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    storage_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    uniforms: HeatmapUniforms,
    max_cells: usize,
    cell_count: u32,
}

/// Initial storage buffer capacity (in cells).
const INITIAL_MAX_CELLS: usize = 256;

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

        let storage_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Thermo Heatmap Storage Buffer"),
            contents: &vec![0u8; INITIAL_MAX_CELLS * std::mem::size_of::<[f32; 4]>()],
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Thermo Heatmap Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group =
            Self::create_bind_group(device, &bind_group_layout, &uniform_buffer, &storage_buffer);

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
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            uniform_buffer,
            storage_buffer,
            bind_group_layout,
            bind_group,
            uniforms,
            max_cells: INITIAL_MAX_CELLS,
            cell_count: 0,
        }
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform_buffer: &wgpu::Buffer,
        storage_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Thermo Heatmap Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: storage_buffer.as_entire_binding(),
                },
            ],
        })
    }

    /// Upload cell colours from ThermoGrid data.
    ///
    /// Computes v_min/v_max from finite values, maps through
    /// `rainbow_colormap`, and writes to the storage buffer.
    /// Returns `false` if rendering should be skipped (no data,
    /// empty grid, or uniform field).
    fn upload_colors(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        grid: &ThermoGrid,
        spec: &ThermoGridSpec,
        view_proj: [[f32; 4]; 4],
    ) -> bool {
        let (rows, cols) = grid.shape();
        let total_cells = rows * cols;
        if total_cells == 0 {
            self.cell_count = 0;
            return false;
        }

        let data = grid.as_slice();

        // Compute v_min / v_max from finite values.
        let mut v_min = f32::INFINITY;
        let mut v_max = f32::NEG_INFINITY;
        for &v in data {
            if v.is_finite() {
                v_min = v_min.min(v);
                v_max = v_max.max(v);
            }
        }

        // Skip if no finite values or field is effectively uniform.
        let range_threshold = 1e-6 * v_max.abs().max(1.0);
        if !v_min.is_finite() || !v_max.is_finite() || (v_max - v_min) < range_threshold {
            log::info!(
                "upload_colors: skipping — v_min={v_min:.4}, v_max={v_max:.4} (all-NaN or uniform)"
            );
            self.cell_count = 0;
            return false;
        }

        let range = v_max - v_min;

        // Build RGBA colour buffer.
        let colors: Vec<[f32; 4]> = data
            .iter()
            .map(|&v| {
                if v.is_finite() {
                    let t = (v - v_min) / range;
                    rainbow_colormap(t)
                } else {
                    [0.0, 0.0, 0.0, 0.0] // transparent for NaN cells
                }
            })
            .collect();

        // Grow storage buffer if needed.
        if total_cells > self.max_cells {
            let new_max = total_cells * 2;
            self.storage_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Thermo Heatmap Storage Buffer"),
                contents: bytemuck::cast_slice(&colors),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
            self.max_cells = new_max;
            self.bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                &self.uniform_buffer,
                &self.storage_buffer,
            );
        } else {
            queue.write_buffer(&self.storage_buffer, 0, bytemuck::cast_slice(&colors));
        }

        // Update uniforms.
        self.uniforms.view_proj = view_proj;
        self.uniforms.grid_origin = [spec.origin_x, spec.origin_y];
        self.uniforms.grid_cell_size = [spec.cell_width, spec.cell_height];
        self.uniforms.grid_dims = [cols as u32, rows as u32];

        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[self.uniforms]),
        );

        self.cell_count = total_cells as u32;
        log::info!(
            "upload_colors: uploading {total_cells} cells, v_min={v_min:.4}, v_max={v_max:.4}"
        );
        true
    }
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
        // Skip entirely when overlay is off.
        if ctx.thermo_display == ThermoDisplayMode::Off {
            self.cell_count = 0;
            return Ok(());
        }

        // Need a thermo grid spec.
        let Some(spec) = &ctx.snapshot.thermo_grid_spec else {
            log::debug!("ThermoHeatmapPipeline: no thermo_grid_spec, skipping");
            self.cell_count = 0;
            return Ok(());
        };

        let grid = match ctx.thermo_display {
            ThermoDisplayMode::Temperature => &ctx.snapshot.cell_temperature,
            ThermoDisplayMode::Pressure => &ctx.snapshot.cell_pressure,
            ThermoDisplayMode::Off => unreachable!(),
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

        self.upload_colors(ctx.device, ctx.queue, grid, spec, ctx.view_proj);
        Ok(())
    }

    fn render<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        if self.cell_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..6, 0..self.cell_count);
    }
}
