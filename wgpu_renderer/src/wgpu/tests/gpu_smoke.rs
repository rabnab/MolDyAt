// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Builds every pipeline on a real GPU device and renders populated frames
//! offscreen, failing on any wgpu validation error (shader ↔ vertex layout ↔
//! bind group mismatches that WGSL validation alone cannot see).
//!
//! Skipped (passes with a note) when no GPU adapter is available, e.g. in CI.

use std::sync::{Arc, Mutex};

use thermo_physics::application::thermo_field::ThermoGrid;
use thermo_physics::objects2d::wall_chain::ObjectRef;
use thermo_physics::objects2d::{
    OperatorState, Rect, SimLineSegment, SimSourceSphere, SimThermostat, Vec2D,
};
use thermo_runtime::application::renderer_snapshot::{
    BallRenderData, CellGridSpec, DebugSnapshot, NeighborLines, RendererSnapshot, ThermoGridSpec,
    ValveSpan, WallSnapshot,
};
use thermo_runtime::application::scene_layers::SceneLayers;
use thermo_ui::graphics::renderer::event_loop::FramePreviews;
use thermo_ui::graphics::renderer::state_layers::{ScalarField, ThermoDisplayMode};

use crate::wgpu::MARGINS;
use crate::wgpu::render::frame::encode_frame;
use crate::wgpu::render::gpu::geometry::build_view_proj_matrix;
use crate::wgpu::render::render_graph::PrepareContext;
use crate::wgpu::render::wgpu_pipelines::WgpuPipelines;

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
        species: Default::default(),
        position: Vec2D::new(x, y),
        velocity: Vec2D::new(0.0, 0.0),
        mass: 1.0,
        color: [0.8, 0.3, 0.2, 1.0],
        contact_radius: 0.5,
        display_radius: 0.5,
        equilibrium_radius: 0.56,
        cutoff_radius: 2.0,
    }
}

/// A snapshot that makes every pipeline draw something.
fn populated_snapshot() -> RendererSnapshot {
    let mut snapshot = RendererSnapshot::new();
    snapshot.frame.world_rect = Rect::new(0.0, 0.0, WORLD, WORLD);
    snapshot.fields.cell_grid = CellGridSpec {
        rows: 4,
        cols: 4,
        cell_width: WORLD / 4.0,
        cell_height: WORLD / 4.0,
        origin_x: 0.0,
        origin_y: WORLD,
    };
    snapshot.balls.data = vec![ball(0, 10.0, 10.0), ball(1, 12.0, 10.0)];
    snapshot.debug = Some(DebugSnapshot {
        neighbor_lines: (0..2)
            .map(|ball| NeighborLines {
                ball,
                main_only: false,
                neighbors: vec![(1 - ball, 1.0)],
            })
            .collect(),
    });

    let (rows, cols) = (5, 7);
    snapshot.fields.thermo_grid = Some(ThermoGridSpec {
        rows,
        cols,
        cell_width: WORLD / cols as f32,
        cell_height: WORLD / rows as f32,
        origin_x: 0.0,
        origin_y: 0.0,
    });
    snapshot.fields.cells.temperature = ThermoGrid::filled(rows, cols, 1.5);
    snapshot.fields.cells.pressure = ThermoGrid::filled(rows, cols, f32::NAN);
    snapshot.fields.cells.density = ThermoGrid::filled(rows, cols, 0.3);
    snapshot.fields.cells.set_flow_velocity_grids(
        ThermoGrid::filled(rows, cols, 0.5),
        ThermoGrid::filled(rows, cols, -0.2),
    );

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
    layers: SceneLayers,
) {
    let ctx = PrepareContext {
        device,
        queue,
        frame: &snapshot.frame,
        balls: &snapshot.balls,
        operators: &snapshot.operators,
        fields: &snapshot.fields,
        debug: snapshot.debug.as_ref(),
        view_proj: build_view_proj_matrix(WORLD, WORLD, WORLD * 0.5, WORLD * 0.5, 1.0),
        cursor_world_pos: (20.0, 20.0),
        current_ball_radius: 0.5,
        show_ball_outline,
        window_size: SIZE,
        margin: MARGINS,
        inset: MARGINS,
        thermo_display,
        operator_previews: previews(),
        layers,
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
    // The main window's frame recording, into an offscreen target (#372).
    let commands = encode_frame(device, pipelines, &view, SIZE, MARGINS, layers);
    queue.submit(std::iter::once(commands));
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
        (ThermoDisplayMode::Scalar(ScalarField::Temperature), true),
        // All-NaN field: heatmap skipped, value labels empty.
        (ThermoDisplayMode::Scalar(ScalarField::Pressure), false),
        (ThermoDisplayMode::Scalar(ScalarField::Density), false),
        (ThermoDisplayMode::Scalar(ScalarField::FlowSpeed), false),
        (ThermoDisplayMode::FlowArrows, false),
        // Back to temperature: the heatmap texture is reused.
        (ThermoDisplayMode::Scalar(ScalarField::Temperature), false),
    ];
    for (mode, outline) in modes {
        render_frame(
            &device,
            &queue,
            &mut pipelines,
            &snapshot,
            mode,
            outline,
            SceneLayers::ALL,
        );
    }
    pipelines.cycle_ring_display_mode();
    // Rendering off (F2), then on again: the light frame skips the ball and
    // thermo pipelines, the next full frame prepares them afresh.
    let light = SceneLayers::ALL.difference(SceneLayers::HEAVY);
    for layers in [light, SceneLayers::ALL] {
        render_frame(
            &device,
            &queue,
            &mut pipelines,
            &snapshot,
            ThermoDisplayMode::Scalar(ScalarField::Temperature),
            false,
            layers,
        );
    }
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

/// The energy plot pipeline on a real device: a scene with curves, a dot,
/// axes and labels drawn offscreen, in the format the plot surface prefers
/// and in an sRGB one.
#[test]
fn plot_pipeline_builds_and_renders_without_validation_errors() {
    use thermo_ui::graphics::energy_plot::{EnergyData, PlotScene, PlotSelection, TimeView};

    use crate::wgpu::render::pipelines::plot_pipeline::PlotPipeline;

    let Some((device, queue)) = request_device() else {
        eprintln!("no GPU adapter available; skipping GPU smoke test");
        return;
    };
    let errors: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = errors.clone();
    device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
        sink.lock().unwrap().push(e.to_string());
    }));

    let mut data = EnergyData::new();
    data.insert_series("E_kin", Some("eps*"), [1.0, 3.0, 2.0, f32::NAN, 4.0]);
    data.insert_series("E_pot", Some("eps*"), [-1.0, -2.0, -1.5, -1.0, -3.0]);
    data.insert_series("wall_momentum", None, [0.0, 0.5, 1.0, 1.5, 2.0]);
    // Two stacked plots (#339).
    let selection = PlotSelection::from_panels(&[&["E_kin", "E_pot"], &["wall_momentum"]]);
    data.sample_times.extend([0.0, 0.1, 0.2, 0.3, 0.4]);
    data.status_text = Some("fps 60\n#Balls: 2".to_string());
    let empty = PlotScene::build(
        &EnergyData::new(),
        &selection,
        SIZE,
        1.0,
        "Esc: close",
        &TimeView::default(),
    );
    let full = PlotScene::build(
        &data,
        &selection,
        SIZE,
        1.0,
        "Esc: close",
        &TimeView::default(),
    );
    assert!(full.plot_area.is_some() && !full.segments.is_empty());

    for format in [wgpu::TextureFormat::Bgra8Unorm, FORMAT] {
        let mut pipeline = PlotPipeline::new(&device, &queue, format);
        for scene in [&full, &empty, &full] {
            pipeline.prepare(&device, &queue, scene);
            let target = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("plot smoke target"),
                size: wgpu::Extent3d {
                    width: SIZE.0,
                    height: SIZE.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("plot smoke pass"),
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
                pipeline.render(&mut pass);
            }
            queue.submit(std::iter::once(encoder.finish()));
        }
    }
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
