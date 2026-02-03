//! WGPU window implementation
//!
//! Creates a winit window with WGPU rendering context and runs the event loop.
//! Renders a background grid and balls using instanced quads.
//! Integrates with the physics thread via PhysicsCoordinator to receive simulation snapshots.

use super::wgpu_context::WgpuContext;
use super::wgpu_input::execute_input_action;
use super::wgpu_pipelines::{WgpuPipelines, compute_view_proj};
use crate::application::MainContent;
use crate::application::physics_thread::PhysicsCommand;
use crate::application::renderer_snapshot::{CellGridSpec, RendererSnapshot};
use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::event_loop::{EnergyWindowManager, PhysicsCoordinator};
use crate::graphics::renderer::{InputAction, InputHandler};
use crate::graphics::window::{MARGIN_PX_X, MARGIN_PX_Y};
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::{BallFactory, NUM_BALL_SIZES};
use std::path::PathBuf;
use std::sync::Arc;
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

/// WGPU-based window for rendering the molecular dynamics simulation.
///
/// This struct manages the winit window and WGPU context lifecycle.
pub struct WgpuWindow {
    #[allow(dead_code)]
    resource_dir: PathBuf,
    #[allow(dead_code)]
    region: (f32, f32),
}

impl WgpuWindow {
    /// Create a new WGPU window configuration.
    ///
    /// # Arguments
    /// * `resource_dir` - Path to shader and asset resources
    /// * `reg_x` - Horizontal region size (unused currently, for future layout)
    /// * `reg_y` - Vertical region size (unused currently, for future layout)
    pub fn new(resource_dir: PathBuf, reg_x: f32, reg_y: f32) -> Self {
        Self {
            resource_dir,
            region: (reg_x, reg_y),
        }
    }

    /// Create the window and run the event loop.
    ///
    /// This method does not return until the window is closed.
    /// It initializes WGPU, creates the rendering surface, spawns the physics thread,
    /// and enters the main event loop receiving data from snapshots.
    ///
    /// # Arguments
    /// * `application` - MainContent to be moved to physics thread
    pub fn create_window(&self, application: MainContent) {
        println!("Creating WGPU window...");

        // Create winit event loop and window
        let event_loop = EventLoop::new();
        let window = WindowBuilder::new()
            .with_title("Molecular Dynamics Simulation (WGPU)")
            .with_inner_size(PhysicalSize::new(1280, 720))
            .with_min_inner_size(PhysicalSize::new(400, 300))
            .build(&event_loop)
            .expect("Failed to create window");

        // Hide system cursor - we render our own crosshair
        window.set_cursor_visible(false);

        let window = Arc::new(window);

        // Initialize WGPU context
        let ctx = WgpuContext::new(Arc::clone(&window));
        println!("✓ WGPU context initialized");
        println!("  Window size: {:?}", ctx.size());

        // Initialize all rendering pipelines
        let pipelines = WgpuPipelines::new(&ctx.device, ctx.format);

        // Create shared energy data for energy plot window
        let energy_data = Arc::new(std::sync::Mutex::new(EnergyData::new()));

        // Spawn physics thread with PhysicsCoordinator (like ggez)
        println!("\n========================================");
        println!("WGPU THREADED RENDERER ARCHITECTURE");
        println!("========================================");
        println!("Main thread ID: {:?}", std::thread::current().id());
        println!("Spawning physics thread...");
        let physics = PhysicsCoordinator::new(application);
        println!("✓ Physics thread spawned successfully");
        println!("  - Main thread: Rendering (WGPU)");
        println!("  - Physics thread: Simulation + snapshot publishing");
        println!("========================================\n");

        // Run the event loop
        Self::run_event_loop(event_loop, window, ctx, pipelines, physics, energy_data);
    }

    /// Main event loop - handles window events and renders frames.
    /// Receives simulation data from PhysicsCoordinator via snapshots.
    fn run_event_loop(
        event_loop: EventLoop<()>,
        window: Arc<Window>,
        mut ctx: WgpuContext,
        mut pipelines: WgpuPipelines,
        physics: PhysicsCoordinator,
        energy_data: SharedEnergyData,
    ) {
        // Energy window manager (shared logic with ggez)
        let mut energy_manager = EnergyWindowManager::new(energy_data);

        // Centralized input handler for consistent behavior with ggez
        let mut input_handler = InputHandler::new();

        // Unified viewport transformation system (replaces ViewportState + CoordinateTransform)
        let initial_world_bounds = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let mut viewport = ViewportTransform::new(
            initial_world_bounds,
            (ctx.size().0 as f32, ctx.size().1 as f32),
            (MARGIN_PX_X, MARGIN_PX_Y),
        );

        // Cursor world position for rendering and interaction
        let mut cursor_world_pos = (0.0_f32, 0.0_f32);

        // Render toggle (shared with EventLoopState semantics)
        let mut render_drawing = true;

        // Current ball size index (updated from input handler)
        let mut current_ball_size_idx: usize = 2; // Default to size index 2 (0.60)

        // Cache the last snapshot for rendering when no new data available
        let mut last_snapshot: Option<Arc<RendererSnapshot>> = None;

        // Track render FPS (frames per second for the renderer)
        let mut render_frame_count: u64 = 0;
        let mut render_fps_timer = std::time::Instant::now();
        let mut render_fps: f32 = 0.0;

        event_loop.run(move |event, window_target, control_flow| {
            *control_flow = ControlFlow::Poll;

            // Get latest snapshot from physics thread (if available)
            if let Some(snapshot) = physics.get_latest_snapshot() {
                last_snapshot = Some(snapshot);
            }

            // Use cached snapshot for current grid_spec
            let current_grid_spec = last_snapshot
                .as_ref()
                .map(|s| s.grid_spec.clone())
                .unwrap_or_else(default_grid_spec);

            match event {
                Event::WindowEvent { event, window_id } => {
                    // Check if this event is for the energy window
                    if energy_manager.is_energy_window_event(window_id) {
                        energy_manager.handle_event(event);
                        return;
                    }

                    // Main window events
                    handle_window_event(
                        event,
                        &mut ctx,
                        &physics,
                        &mut render_drawing,
                        &mut viewport,
                        &mut cursor_world_pos,
                        &mut current_ball_size_idx,
                        &mut input_handler,
                        &mut energy_manager,
                        &mut pipelines,
                        &current_grid_spec,
                        window_target,
                        control_flow,
                    );
                }
                Event::MainEventsCleared => {
                    // Update render FPS calculation
                    render_frame_count += 1;
                    if render_fps_timer.elapsed().as_secs_f32() >= 1.0 {
                        render_fps =
                            render_frame_count as f32 / render_fps_timer.elapsed().as_secs_f32();
                        render_frame_count = 0;
                        render_fps_timer = std::time::Instant::now();
                    }

                    // Update energy data from latest snapshot using manager
                    if let Some(snapshot) = &last_snapshot {
                        energy_manager.update_from_snapshot(
                            snapshot,
                            render_fps,
                            physics.steps_per_frame(),
                        );
                    }

                    // Request a redraw after processing events
                    window.request_redraw();
                }
                Event::RedrawRequested(window_id) => {
                    // Check if redraw is for energy window
                    if energy_manager.is_energy_window_event(window_id) {
                        energy_manager.handle_redraw();
                        return;
                    }

                    // Render main window
                    if window_id != window.id() {
                        return;
                    }

                    if !render_drawing {
                        return;
                    }

                    // Render from snapshot if available
                    if let Some(snapshot) = &last_snapshot {
                        log_frame_periodically(snapshot);

                        // Compute view-projection matrix
                        let pan = viewport.pan_offset();
                        let zoom = viewport.zoom();
                        let view_proj = compute_view_proj(&snapshot.grid_spec, pan.x, pan.y, zoom);

                        // Update all pipelines from snapshot
                        let ctrl_pressed = input_handler.is_key_pressed(VirtualKeyCode::LControl)
                            || input_handler.is_key_pressed(VirtualKeyCode::RControl);
                        pipelines.update_from_snapshot(
                            &ctx,
                            snapshot,
                            view_proj,
                            cursor_world_pos,
                            BallFactory::radius_for_size_idx(current_ball_size_idx),
                            ctrl_pressed,
                        );

                        // Render frame
                        if let Err(e) = render_frame(&ctx, &mut pipelines) {
                            eprintln!("Render error: {:?}", e);
                            if e == wgpu::SurfaceError::OutOfMemory {
                                *control_flow = ControlFlow::Exit;
                            }
                        }
                    }
                }
                _ => {}
            }
        });
    }
}

/// Default grid spec when no snapshot is available yet.
fn default_grid_spec() -> CellGridSpec {
    CellGridSpec {
        rows: 8,
        cols: 12,
        cell_width: 3.0,
        cell_height: 3.0,
        origin_x: -18.0,
        origin_y: 12.0,
    }
}

/// Handle window events (resize, input, etc.)
#[allow(clippy::too_many_arguments)]
fn handle_window_event(
    event: WindowEvent,
    ctx: &mut WgpuContext,
    physics: &PhysicsCoordinator,
    render_drawing: &mut bool,
    viewport: &mut ViewportTransform,
    cursor_world_pos: &mut (f32, f32),
    current_ball_size_idx: &mut usize,
    input_handler: &mut InputHandler,
    energy_manager: &mut EnergyWindowManager,
    pipelines: &mut WgpuPipelines,
    _current_grid_spec: &CellGridSpec,
    window_target: &winit::event_loop::EventLoopWindowTarget<()>,
    control_flow: &mut ControlFlow,
) {
    match event {
        WindowEvent::CloseRequested => {
            println!("Window close requested");
            *control_flow = ControlFlow::Exit;
        }
        WindowEvent::Resized(size) => {
            println!("Window resized to {}x{}", size.width, size.height);
            ctx.resize(size.width, size.height);

            let world_rect = ViewportTransform::compute_world_bounds_from_window_size(
                size.width,
                size.height,
                (MARGIN_PX_X, MARGIN_PX_Y),
            );
            physics.send_command(PhysicsCommand::Resize { world_rect });
            viewport.set_world_bounds(world_rect);
            viewport.on_window_resize((size.width as f32, size.height as f32));
        }
        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
            ctx.resize(new_inner_size.width, new_inner_size.height);
            let world_rect = ViewportTransform::compute_world_bounds_from_window_size(
                new_inner_size.width,
                new_inner_size.height,
                (MARGIN_PX_X, MARGIN_PX_Y),
            );
            physics.send_command(PhysicsCommand::Resize { world_rect });
            viewport.set_world_bounds(world_rect);
            viewport.on_window_resize((new_inner_size.width as f32, new_inner_size.height as f32));
        }
        WindowEvent::MouseInput { state, button, .. } => {
            // Get current cursor positions for mouse click
            let world_pos = input_handler.cursor_pos();
            let screen_pos = input_handler.cursor_pos_px();
            let pan_offset = viewport.pan_offset();

            if let Some(action) =
                input_handler.handle_mouse_button(button, state, screen_pos, world_pos, pan_offset)
            {
                // Handle WGPU-specific actions (NextBallSize)
                if let InputAction::NextBallSize = action {
                    *current_ball_size_idx = (*current_ball_size_idx + 1) % NUM_BALL_SIZES;
                    let radius = BallFactory::radius_for_size_idx(*current_ball_size_idx);
                    println!("Ball size: {:.2} (index {})", radius, current_ball_size_idx);
                }

                execute_input_action(
                    &action,
                    physics,
                    render_drawing,
                    viewport,
                    current_ball_size_idx,
                    energy_manager,
                    window_target,
                    &mut pipelines.ball,
                    &ctx.queue,
                );
            }
        }
        WindowEvent::CursorMoved { position, .. } => {
            let world_pos = viewport.px_to_world(&[position.x as f32, position.y as f32]);

            // Update cursor position in InputHandler
            input_handler.set_cursor_pos(world_pos, [position.x as f32, position.y as f32]);

            *cursor_world_pos = (world_pos.x, world_pos.y);

            if let Some(InputAction::PanUpdate { delta }) =
                input_handler.handle_mouse_motion(position.x as f32, position.y as f32)
            {
                let start_offset = input_handler.pan_start_offset();
                let zoom = viewport.zoom();
                let new_pan = ViewportTransform::apply_pan_delta(delta, start_offset, zoom);
                viewport.pan(new_pan);
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let cursor_pos = input_handler.cursor_pos();

            if let Some(action) = input_handler.handle_mouse_wheel(delta, cursor_pos) {
                match &action {
                    InputAction::ZoomIn { center } => {
                        viewport.zoom_at_point(center, 1.1);
                    }
                    InputAction::ZoomOut { center } => {
                        viewport.zoom_at_point(center, 1.0 / 1.1);
                    }
                    InputAction::ResetView => {
                        viewport.reset();
                    }
                    _ => {}
                }
            }
        }
        WindowEvent::KeyboardInput {
            input:
                KeyboardInput {
                    virtual_keycode: Some(key),
                    state: ElementState::Pressed,
                    ..
                },
            ..
        } => {
            if let Some(action) = input_handler.handle_key_down(key, false) {
                execute_input_action(
                    &action,
                    physics,
                    render_drawing,
                    viewport,
                    current_ball_size_idx,
                    energy_manager,
                    window_target,
                    &mut pipelines.ball,
                    &ctx.queue,
                );
            }
        }
        WindowEvent::KeyboardInput {
            input:
                KeyboardInput {
                    virtual_keycode: Some(key),
                    state: ElementState::Released,
                    ..
                },
            ..
        } => {
            if let Some(action) = input_handler.handle_key_up(key) {
                execute_input_action(
                    &action,
                    physics,
                    render_drawing,
                    viewport,
                    current_ball_size_idx,
                    energy_manager,
                    window_target,
                    &mut pipelines.ball,
                    &ctx.queue,
                );
            }
        }
        _ => {}
    }
}

/// Log frame info periodically for debugging.
fn log_frame_periodically(snapshot: &RendererSnapshot) {
    static mut FRAME_COUNT: u64 = 0;
    static mut PREV_BALL_COUNT: usize = 0;

    unsafe {
        FRAME_COUNT += 1;
        let ball_count_changed = PREV_BALL_COUNT != snapshot.balls.len();
        if FRAME_COUNT % 600 == 1 || ball_count_changed {
            println!(
                "Rendering: {}x{} grid, {} balls",
                snapshot.grid_spec.cols,
                snapshot.grid_spec.rows,
                snapshot.balls.len()
            );
            PREV_BALL_COUNT = snapshot.balls.len();
        }
    }
}

/// Render a single frame.
fn render_frame(
    ctx: &WgpuContext,
    pipelines: &mut WgpuPipelines,
) -> Result<(), wgpu::SurfaceError> {
    let output = ctx.surface.get_current_texture()?;
    let view = output
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Main Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        let margin = (MARGIN_PX_X, MARGIN_PX_Y);
        pipelines.render(&mut render_pass, ctx, margin);
    }

    ctx.queue.submit(std::iter::once(encoder.finish()));
    output.present();

    Ok(())
}
