// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Pair forces on the GPU: the device, buffers and WGSL force kernel. The
//! physics and the CPU kernel, which stays the reference, are in
//! `thermo_physics` (see `docs/architecture.md`).

// For `GpuForces: Send`, which goes deep into wgpu's types.
#![recursion_limit = "256"]

use bytemuck::{Pod, Zeroable};
use thermo_physics::objects2d::ball_impl::{
    KernelPoint, PairForceBackend, PairForces, PairParams, SPECIES_BITS,
};
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, BufferBindingType, BufferUsages, ComputePipeline, Device,
    Queue,
};

/// Invocations per workgroup of a kernel (`WORKGROUP_SIZE` in its WGSL).
pub const WORKGROUP_SIZE: u32 = 64;

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
    bind_group: BindGroup,
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
        let buffer = |size: usize, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: size as u64,
                usage,
                mapped_at_creation: false,
            })
        };
        let uniform_buffer = buffer(
            size_of::<Uniforms>(),
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        );
        let points = buffer(
            size_of::<KernelPoint>(),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let pairs = buffer(
            size_of::<PairParams>(),
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let results = buffer(
            size_of::<PairForces>(),
            BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        );
        let readback = buffer(
            size_of::<PairForces>(),
            BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        );
        let bind_group = bind_group(
            &device,
            &layout,
            [&uniform_buffer, &points, &pairs, &results],
        );
        GpuForces {
            kernel: pipeline(&device, &layout, ALL_PAIRS),
            device,
            queue,
            layout,
            uniforms: Uniforms::default(),
            uniform_buffer,
            points,
            pairs,
            results,
            readback,
            bind_group,
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
        let mut grown = grow(&self.device, &mut self.points, size_of_val(points));
        grown |= grow(&self.device, &mut self.results, results);
        grown |= grow(&self.device, &mut self.readback, results);
        if grown {
            self.rebind();
        }
        self.queue
            .write_buffer(&self.points, 0, bytemuck::cast_slice(points));
    }

    /// Run `pipeline` once per uploaded point and read its results back
    /// into `results`, one per point; blocks until the GPU is done.
    fn run(&self, pipeline: &ComputePipeline, results: &mut Vec<PairForces>) {
        results.clear();
        let balls = self.uniforms.balls;
        if balls == 0 {
            return;
        }
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniforms));
        let size = (balls as usize * size_of::<PairForces>()) as u64;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(balls.div_ceil(WORKGROUP_SIZE), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&self.results, 0, &self.readback, 0, size);
        self.queue.submit([encoder.finish()]);
        let slice = self.readback.slice(..size);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("map the force readback"));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("wait for the force kernel");
        let view = slice.get_mapped_range().expect("read the force readback");
        results.extend_from_slice(bytemuck::cast_slice(&view));
        drop(view);
        self.readback.unmap();
    }

    fn rebind(&mut self) {
        let buffers = [
            &self.uniform_buffer,
            &self.points,
            &self.pairs,
            &self.results,
        ];
        self.bind_group = bind_group(&self.device, &self.layout, buffers);
    }
}

impl PairForceBackend for GpuForces {
    fn set_pairs(&mut self, pairs: &[PairParams]) {
        let species = pairs.len().isqrt();
        assert_eq!(species * species, pairs.len(), "a square pair table");
        self.uniforms.species = species as u32;
        if grow(&self.device, &mut self.pairs, size_of_val(pairs)) {
            self.rebind();
        }
        self.queue
            .write_buffer(&self.pairs, 0, bytemuck::cast_slice(pairs));
    }

    fn compute(&mut self, points: &[KernelPoint], cell: [f32; 2], results: &mut Vec<PairForces>) {
        debug_assert!(self.uniforms.species > 0, "set_pairs comes first");
        self.upload_points(points, cell);
        self.run(&self.kernel, results);
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

/// Bindings 0 to 3: uniforms, points, pair table, results.
fn bind_group(device: &Device, layout: &BindGroupLayout, buffers: [&Buffer; 4]) -> BindGroup {
    let entries = buffers.map(|b| b.as_entire_binding());
    let entries: Vec<_> = (0..)
        .zip(entries)
        .map(|(binding, resource)| wgpu::BindGroupEntry { binding, resource })
        .collect();
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("pair forces"),
        layout,
        entries: &entries,
    })
}

#[cfg(test)]
mod tests;
