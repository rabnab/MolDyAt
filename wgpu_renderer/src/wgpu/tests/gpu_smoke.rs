//! Builds every pipeline on a real GPU device and renders populated frames
//! offscreen, failing on any wgpu validation error (shader ↔ vertex layout ↔
//! bind group mismatches that WGSL validation alone cannot see).
//!
//! Skipped (passes with a note) when no GPU adapter is available, e.g. in CI.

use std::sync::{Arc, Mutex};

use rust_thermo_core::application::renderer_snapshot::{
    BallRenderData, CellGridSpec, RendererSnapshot, ThermoGridSpec, ValveSpan, WallSnapshot,
};
use rust_thermo_core::application::thermo_field::ThermoGrid;
use rust_thermo_core::graphics::renderer::event_loop::{FrameDetail, FramePreviews};
use rust_thermo_core::graphics::renderer::state_layers::ThermoDisplayMode;
use rust_thermo_core::objects2d::wall_chain::ObjectRef;
use rust_thermo_core::objects2d::{
    OperatorState, Rect, SimLineSegment, SimSourceSphere, SimThermostat, Vec2D,
};

use crate::wgpu::MARGINS;
use crate::wgpu::gpu::geometry::build_view_proj_matrix;
use crate::wgpu::render_graph::PrepareContext;
use crate::wgpu::wgpu_pipelines::WgpuPipelines;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const SIZE: (u32, u32) = (800, 600);
const WORLD: f32 = 40.0;

fn request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

fn ball(id: usize, x: f32, y: f32) -> BallRenderData {
    BallRenderData {
        id,
        size_idx: 0,
        position: Vec2D::new(x, y),
        velocity: Vec2D::new(0.0, 0.0),
        mass: 1.0,
        color: [0.8, 0.3, 0.2, 1.0],
        radius: 0.5,
        show_neighbor_lines: true,
        show_main_neighbors_only: false,
        neighbors: vec![(1 - id, 1.0)],
        update_counter: 0,
        tail: Vec::new(),
    }
}

/// A snapshot that makes every pipeline draw something.
fn populated_snapshot() -> RendererSnapshot {
    let mut snapshot = RendererSnapshot::new();
    snapshot.cell_grid_bounds = Rect::new(0.0, 0.0, WORLD, WORLD);
    snapshot.grid_spec = CellGridSpec {
        rows: 4,
        cols: 4,
        cell_width: WORLD / 4.0,
        cell_height: WORLD / 4.0,
        origin_x: 0.0,
        origin_y: WORLD,
    };
    snapshot.balls = vec![ball(0, 10.0, 10.0), ball(1, 12.0, 10.0)];

    let (rows, cols) = (5, 7);
    snapshot.thermo_grid_spec = Some(ThermoGridSpec {
        rows,
        cols,
        cell_width: WORLD / cols as f32,
        cell_height: WORLD / rows as f32,
        origin_x: 0.0,
        origin_y: 0.0,
    });
    snapshot.cell_temperature = ThermoGrid::filled(rows, cols, 1.5);
    snapshot.cell_pressure = ThermoGrid::filled(rows, cols, f32::NAN);

    let ops = &mut snapshot.operators;
    let (a, b) = (Vec2D::new(5.0, 30.0), Vec2D::new(35.0, 30.0));
    ops.line_segments.push(SimLineSegment::new(0, a, b));
    ops.walls.push(WallSnapshot {
        chain_id: 0,
        start: a,
        end: b,
        piece_ids: vec![0],
        valve_ids: vec![0],
    });
    ops.valve_spans.push(ValveSpan {
        valve_id: 0,
        p0: Vec2D::new(15.0, 30.0),
        p1: Vec2D::new(20.0, 30.0),
        thickness: 1.0,
        is_closed: true,
    });
    ops.thermostats
        .push(SimThermostat::new(0, Vec2D::new(20.0, 20.0)));
    let mut sphere = SimSourceSphere::new(0, Vec2D::new(30.0, 10.0));
    sphere.state = OperatorState::On;
    ops.source_spheres.push(sphere);
    snapshot
}

fn previews() -> FramePreviews {
    FramePreviews {
        line_segment: Some(([2.0, 2.0], [8.0, 8.0], 1.0)),
        snap_indicators: vec![[5.0, 30.0]],
        selected_sidebar_icon: Some(1),
        hovered: Some(ObjectRef::Wall(0)),
        ..FramePreviews::default()
    }
}

fn render_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipelines: &mut WgpuPipelines,
    snapshot: &RendererSnapshot,
    thermo_display: ThermoDisplayMode,
    show_ball_outline: bool,
    detail: FrameDetail,
) {
    let ctx = PrepareContext {
        device,
        queue,
        snapshot,
        view_proj: build_view_proj_matrix(WORLD, WORLD, WORLD * 0.5, WORLD * 0.5, 1.0),
        cursor_world_pos: (20.0, 20.0),
        current_ball_radius: 0.5,
        show_ball_outline,
        window_size: SIZE,
        margin: MARGINS,
        inset: MARGINS,
        thermo_display,
        operator_previews: previews(),
        detail,
    };
    pipelines.prepare(&ctx).expect("prepare");

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("smoke target"),
        size: wgpu::Extent3d {
            width: SIZE.0,
            height: SIZE.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("smoke pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        pipelines.render(&mut pass, SIZE, MARGINS, detail);
    }
    queue.submit(std::iter::once(encoder.finish()));
}

#[test]
fn all_pipelines_build_and_render_without_validation_errors() {
    let Some((device, queue)) = request_device() else {
        eprintln!("no GPU adapter available; skipping GPU smoke test");
        return;
    };

    let errors: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = errors.clone();
    device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
        sink.lock().unwrap().push(e.to_string());
    }));

    let mut pipelines = WgpuPipelines::new(&device, &queue, FORMAT);
    let snapshot = populated_snapshot();
    let modes = [
        (ThermoDisplayMode::Off, true),
        (ThermoDisplayMode::Grid, false),
        (ThermoDisplayMode::Temperature, true),
        // All-NaN field: heatmap skipped, value labels empty.
        (ThermoDisplayMode::Pressure, false),
        // Back to temperature: the heatmap texture is reused.
        (ThermoDisplayMode::Temperature, false),
    ];
    for (mode, outline) in modes {
        render_frame(
            &device,
            &queue,
            &mut pipelines,
            &snapshot,
            mode,
            outline,
            FrameDetail::Full,
        );
    }
    pipelines.cycle_ring_display_mode();
    render_frame(
        &device,
        &queue,
        &mut pipelines,
        &snapshot,
        ThermoDisplayMode::Off,
        false,
        FrameDetail::Full,
    );

    // Rendering off (F2): a light frame does no heavy work, a full frame
    // right after restores it.
    let before = pipelines.stats();
    render_frame(
        &device,
        &queue,
        &mut pipelines,
        &snapshot,
        ThermoDisplayMode::Temperature,
        true,
        FrameDetail::Light,
    );
    let light = pipelines.stats();
    assert_eq!(light.heavy_prepared, before.heavy_prepared);
    assert_eq!(light.heavy_drawn, before.heavy_drawn);
    assert!(light.light_prepared > before.light_prepared);
    render_frame(
        &device,
        &queue,
        &mut pipelines,
        &snapshot,
        ThermoDisplayMode::Temperature,
        true,
        FrameDetail::Full,
    );
    assert!(pipelines.stats().heavy_prepared > light.heavy_prepared);
    assert!(pipelines.stats().heavy_drawn > light.heavy_drawn);
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");

    let errors = errors.lock().unwrap();
    assert!(
        errors.is_empty(),
        "wgpu validation errors:\n{}",
        errors.join("\n")
    );
}
