// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pair forces on the GPU: the device, buffers and WGSL force kernel. The
//! physics and the CPU kernel, which stays the reference, are in
//! `thermo_physics` (see `docs/architecture.md`).

// For `GpuForces: Send` (`PairForceBackend`), which goes deep into wgpu's
// types.
#![recursion_limit = "256"]

use bytemuck::{Pod, Zeroable};
use std::time::Duration;
use thermo_physics::objects2d::ball_impl::{
    KernelPoint, PairForceBackend, PairForces, PairParams, SPECIES_BITS,
};
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, BufferBindingType, BufferUsages, ComputePipeline, Device,
    Queue,
};

/// Invocations per workgroup of a kernel (`WORKGROUP_SIZE` in its WGSL).
pub const WORKGROUP_SIZE: u32 = 64;

/// How long [`GpuForces::run`] waits for the GPU before it gives up.
const TIMEOUT: Duration = Duration::from_secs(10);

const LAYOUT: &str = include_str!("layout.wgsl");
const ALL_PAIRS: &str = include_str!("all_pairs.wgsl");

#[derive(Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
struct Uniforms {
    balls: u32,
    species: u32,
    cell: [f32; 2],
}

/// `kernel` with `WORKGROUP_SIZE`, `SPECIES_BITS` and the buffers of `layout.wgsl` in front.
fn shader_source(kernel: &str) -> String {
    format!(
        "const WORKGROUP_SIZE: u32 = {WORKGROUP_SIZE}u;\n\
         const SPECIES_BITS: u32 = {SPECIES_BITS}u;\n{LAYOUT}\n{kernel}"
    )
}

/// The force kernel and its buffers on one device. The kernel reads the
/// points and the pair table and writes a separate result buffer.
pub struct GpuForces {
    device: Device,
    queue: Queue,
    layout: BindGroupLayout,
    kernel: ComputePipeline,
    uniforms: Uniforms,
    uniform_buffer: Buffer,
    points: Buffer,
    pairs: Buffer,
    results: Buffer,
    readback: Buffer,
    /// Built on the next run after a buffer grew.
    bind_group: Option<BindGroup>,
}

impl GpuForces {
    /// On `device`, e.g. the renderer's (its handles can be cloned).
    pub fn new(device: Device, queue: Queue) -> Self {
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pair forces"),
            entries: &[
                entry(0, BufferBindingType::Uniform),
                entry(1, BufferBindingType::Storage { read_only: true }),
                entry(2, BufferBindingType::Storage { read_only: true }),
                entry(3, BufferBindingType::Storage { read_only: false }),
            ],
        });
        // Any size works: `grow` makes room on first use.
        let buffer = |usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256,
                usage,
                mapped_at_creation: false,
            })
        };
        GpuForces {
            kernel: pipeline(&device, &layout, ALL_PAIRS),
            uniforms: Uniforms::default(),
            uniform_buffer: buffer(BufferUsages::UNIFORM | BufferUsages::COPY_DST),
            points: buffer(BufferUsages::STORAGE | BufferUsages::COPY_DST),
            pairs: buffer(BufferUsages::STORAGE | BufferUsages::COPY_DST),
            results: buffer(BufferUsages::STORAGE | BufferUsages::COPY_SRC),
            readback: buffer(BufferUsages::MAP_READ | BufferUsages::COPY_DST),
            bind_group: None,
            layout,
            device,
            queue,
        }
    }

    /// On a device of its own, without a surface (headless runs). `None`,
    /// logged, without an adapter: the forces stay on the CPU.
    pub fn headless() -> Option<Self> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let device = pollster::block_on(async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    ..Default::default()
                })
                .await
                .map_err(|e| e.to_string())?;
            adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .map_err(|e| e.to_string())
        });
        match device {
            Ok((device, queue)) => Some(Self::new(device, queue)),
            Err(e) => {
                log::info!("No GPU for the pair forces, they stay on the CPU: {e}");
                None
            }
        }
    }

    /// Upload the balls' points, in cells of `cell` size.
    fn upload_points(&mut self, points: &[KernelPoint], cell: [f32; 2]) {
        self.uniforms.balls = points.len() as u32;
        self.uniforms.cell = cell;
        let results = points.len() * size_of::<PairForces>();
        if grow(&self.device, &mut self.points, size_of_val(points))
            | grow(&self.device, &mut self.results, results)
            | grow(&self.device, &mut self.readback, results)
        {
            self.bind_group = None;
        }
        self.queue
            .write_buffer(&self.points, 0, bytemuck::cast_slice(points));
    }

    /// Run `pipeline` once per uploaded point and read its results back
    /// into `results`, one per point; blocks until the GPU is done. After
    /// an error (no pair table, too many balls, a lost or hung device) the
    /// caller drops this and computes the forces on the CPU.
    fn run(
        &mut self,
        pipeline: &ComputePipeline,
        results: &mut Vec<PairForces>,
    ) -> Result<(), String> {
        results.clear();
        let balls = self.uniforms.balls;
        if balls == 0 {
            return Ok(());
        }
        if self.uniforms.species == 0 {
            return Err("no pair table".into());
        }
        let groups = balls.div_ceil(WORKGROUP_SIZE);
        if groups > self.device.limits().max_compute_workgroups_per_dimension {
            return Err(format!("{balls} balls are more than one dispatch takes"));
        }
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniforms));
        let bind_group = self.bind_group.get_or_insert_with(|| {
            let buffers = [
                &self.uniform_buffer,
                &self.points,
                &self.pairs,
                &self.results,
            ];
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("pair forces"),
                layout: &self.layout,
                entries: &[0, 1, 2, 3].map(|i| wgpu::BindGroupEntry {
                    binding: i,
                    resource: buffers[i as usize].as_entire_binding(),
                }),
            })
        });
        let size = (balls as usize * size_of::<PairForces>()) as u64;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &*bind_group, &[]);
            pass.dispatch_workgroups(groups, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&self.results, 0, &self.readback, 0, size);
        let submission = self.queue.submit([encoder.finish()]);
        let slice = self.readback.slice(..size);
        // A failed map shows in `get_mapped_range` below.
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(TIMEOUT),
            })
            .map_err(|e| e.to_string())?;
        let view = slice.get_mapped_range().map_err(|e| e.to_string())?;
        results.extend_from_slice(bytemuck::cast_slice(&view));
        drop(view);
        self.readback.unmap();
        Ok(())
    }
}

impl PairForceBackend for GpuForces {
    fn set_pairs(&mut self, pairs: &[PairParams]) {
        let species = pairs.len().isqrt();
        assert_eq!(species * species, pairs.len(), "pair table not square");
        self.uniforms.species = species as u32;
        if grow(&self.device, &mut self.pairs, size_of_val(pairs)) {
            self.bind_group = None;
        }
        self.queue
            .write_buffer(&self.pairs, 0, bytemuck::cast_slice(pairs));
    }

    fn compute(
        &mut self,
        points: &[KernelPoint],
        cell: [f32; 2],
        results: &mut Vec<PairForces>,
    ) -> Result<(), String> {
        self.upload_points(points, cell);
        self.run(&self.kernel.clone(), results)
    }
}

/// A pipeline from WGSL `kernel`, whose entry point `main` may use the
/// bindings of `layout.wgsl` and `WORKGROUP_SIZE`.
fn pipeline(device: &Device, layout: &BindGroupLayout, kernel: &str) -> ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("pair forces"),
        source: wgpu::ShaderSource::Wgsl(shader_source(kernel).into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pair forces"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("pair forces"),
        layout: Some(&layout),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

/// Replace `buffer` by one of the next power of two size if it holds fewer
/// than `size` bytes; whether it did.
fn grow(device: &Device, buffer: &mut Buffer, size: usize) -> bool {
    let size = size as u64;
    if buffer.size() >= size {
        return false;
    }
    *buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: size.next_power_of_two(),
        usage: buffer.usage(),
        mapped_at_creation: false,
    });
    true
}

#[cfg(test)]
mod tests;
