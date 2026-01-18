//! WGPU window implementation
//!
//! Creates a winit window with WGPU rendering context and runs the event loop.
//! Renders a background grid and balls using instanced quads.
//! Integrates with the physics thread via PhysicsCoordinator to receive simulation snapshots.

use super::wgpu_context::WgpuContext;
use super::wgpu_input::{BALL_SIZES, execute_input_action};
use super::wgpu_pipelines::{WgpuPipelines, compute_view_proj};
use crate::application::MainContent;
use crate::application::physics_thread::PhysicsCommand;
use crate::application::renderer_snapshot::{CellGridSpec, RendererSnapshot};
use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
use crate::graphics::renderer::InputHandler;
use crate::graphics::renderer::event_loop::{EnergyWindowManager, PhysicsCoordinator};
use crate::graphics::renderer::viewport::ViewportState;
use crate::graphics::window::{MARGIN_PX_X, MARGIN_PX_Y};
use crate::objects2d::{Rect, Vec2D};
use std::path::PathBuf;
use std::sync::Arc;
use winit::{
    dpi::PhysicalSize,
    event::{
        ElementState, Event, KeyboardInput, MouseButton, MouseScrollDelta, VirtualKeyCode,
        WindowEvent,
    },
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

        // Viewport state for pan/zoom (shared with ggez via ViewportState)
        let initial_world_bounds = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let mut viewport = ViewportState::new(initial_world_bounds);

        // Mouse state for drag panning and zoom centering
        let mut is_dragging = false;
        let mut last_mouse_pos: Option<(f64, f64)> = None;
        let mut cursor_world_pos = (0.0_f32, 0.0_f32);

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
                        &mut viewport,
                        &mut is_dragging,
                        &mut last_mouse_pos,
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
                            BALL_SIZES[current_ball_size_idx],
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

    /// Compute world rectangle for a given window size.
    ///
    /// Uses the same LEN_TO_PX conversion as ggez (0.1 = 1 world unit per 10 pixels).
    fn compute_world_rect_for_size(width: u32, height: u32) -> Rect {
        const LEN_TO_PX: f32 = 0.1; // 1 world unit = 10 pixels
        let region_width = (width as f32 - 2.0 * MARGIN_PX_X).max(1.0);
        let region_height = (height as f32 - 2.0 * MARGIN_PX_Y).max(1.0);
        let world_width = region_width * LEN_TO_PX;
        let world_height = region_height * LEN_TO_PX;

        Rect::new(
            -world_width / 2.0,
            world_height / 2.0,
            world_width,
            -world_height,
        )
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
    viewport: &mut ViewportState,
    is_dragging: &mut bool,
    last_mouse_pos: &mut Option<(f64, f64)>,
    cursor_world_pos: &mut (f32, f32),
    current_ball_size_idx: &mut usize,
    input_handler: &mut InputHandler,
    energy_manager: &mut EnergyWindowManager,
    pipelines: &mut WgpuPipelines,
    current_grid_spec: &CellGridSpec,
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

            let world_rect = WgpuWindow::compute_world_rect_for_size(size.width, size.height);
            physics.send_command(PhysicsCommand::Resize { world_rect });
            viewport.set_world_bounds(world_rect);
        }
        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
            ctx.resize(new_inner_size.width, new_inner_size.height);
            let world_rect = WgpuWindow::compute_world_rect_for_size(
                new_inner_size.width,
                new_inner_size.height,
            );
            physics.send_command(PhysicsCommand::Resize { world_rect });
            viewport.set_world_bounds(world_rect);
        }
        WindowEvent::MouseInput { state, button, .. } => {
            handle_mouse_button(
                button,
                state,
                is_dragging,
                last_mouse_pos,
                current_ball_size_idx,
                physics,
            );
        }
        WindowEvent::CursorMoved { position, .. } => {
            handle_cursor_moved(
                position,
                ctx,
                viewport,
                is_dragging,
                last_mouse_pos,
                cursor_world_pos,
                input_handler,
                current_grid_spec,
            );
        }
        WindowEvent::MouseWheel { delta, .. } => {
            handle_mouse_wheel(delta, viewport, *cursor_world_pos);
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

/// Handle mouse button press/release.
fn handle_mouse_button(
    button: MouseButton,
    state: ElementState,
    is_dragging: &mut bool,
    last_mouse_pos: &mut Option<(f64, f64)>,
    current_ball_size_idx: &mut usize,
    physics: &PhysicsCoordinator,
) {
    if button == MouseButton::Middle {
        *is_dragging = state == ElementState::Pressed;
        if !*is_dragging {
            *last_mouse_pos = None;
        }
    } else if button == MouseButton::Right && state == ElementState::Pressed {
        *current_ball_size_idx = (*current_ball_size_idx + 1) % BALL_SIZES.len();
        physics.send_command(PhysicsCommand::NextBallSize);
        println!(
            "Ball size: {} (index {})",
            BALL_SIZES[*current_ball_size_idx], current_ball_size_idx
        );
    }
}

/// Handle cursor movement for panning and coordinate tracking.
#[allow(clippy::too_many_arguments)]
fn handle_cursor_moved(
    position: winit::dpi::PhysicalPosition<f64>,
    ctx: &WgpuContext,
    viewport: &mut ViewportState,
    is_dragging: &mut bool,
    last_mouse_pos: &mut Option<(f64, f64)>,
    cursor_world_pos: &mut (f32, f32),
    input_handler: &mut InputHandler,
    current_grid_spec: &CellGridSpec,
) {
    let world_width = current_grid_spec.cols as f32 * current_grid_spec.cell_width;
    let world_height = current_grid_spec.rows as f32 * current_grid_spec.cell_height;
    let zoom = viewport.zoom();

    let (win_width, win_height) = ctx.size();
    let viewport_width = (win_width as f32 - 2.0 * MARGIN_PX_X).max(1.0);
    let viewport_height = (win_height as f32 - 2.0 * MARGIN_PX_Y).max(1.0);
    let world_visible_width = world_width / zoom;
    let world_visible_height = world_height / zoom;

    // Screen position relative to viewport (accounting for margin)
    let vp_x = (position.x as f32 - MARGIN_PX_X) / viewport_width;
    let vp_y = (position.y as f32 - MARGIN_PX_Y) / viewport_height;

    // Convert to world coordinates using viewport
    let pan_offset = viewport.pan_offset();
    cursor_world_pos.0 = pan_offset.x + (vp_x - 0.5) * world_visible_width;
    cursor_world_pos.1 = pan_offset.y - (vp_y - 0.5) * world_visible_height;

    // Update cursor position in input handler
    input_handler.set_cursor_pos(
        Vec2D::new(cursor_world_pos.0, cursor_world_pos.1),
        [position.x as f32, position.y as f32],
    );

    if *is_dragging {
        if let Some((last_x, last_y)) = *last_mouse_pos {
            let dx = position.x - last_x;
            let dy = position.y - last_y;

            let world_dx = (dx as f32 / viewport_width) * world_visible_width;
            let world_dy = (dy as f32 / viewport_height) * world_visible_height;

            viewport.adjust_pan(&Vec2D::new(-world_dx, world_dy));
        }
        *last_mouse_pos = Some((position.x, position.y));
    }
}

/// Handle mouse wheel for zooming.
fn handle_mouse_wheel(
    delta: MouseScrollDelta,
    viewport: &mut ViewportState,
    cursor_world_pos: (f32, f32),
) {
    let scroll_amount = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 50.0,
    };

    let cursor_vec = Vec2D::new(cursor_world_pos.0, cursor_world_pos.1);
    let zoom_factor = 1.1_f32.powf(scroll_amount);
    viewport.zoom_at_point(&cursor_vec, zoom_factor);
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
