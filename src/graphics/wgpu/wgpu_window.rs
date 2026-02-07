//! WGPU window implementation
//!
//! Creates a winit window with WGPU rendering context and runs the event loop.
//! Renders a background grid and balls using instanced quads.
//! Integrates with the physics thread via PhysicsCoordinator to receive simulation snapshots.

use super::wgpu_context::WgpuContext;
use super::wgpu_input::execute_input_action;
use super::wgpu_pipelines::{WgpuPipelines, compute_view_proj};
use crate::application::MainContent;
use crate::application::renderer_snapshot::{CellGridSpec, RendererSnapshot};
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::event_loop::UnifiedRenderState;
use crate::graphics::window::{MARGIN_PX_X, MARGIN_PX_Y};
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::BallFactory;
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

        // Build UnifiedRenderState (spawns physics thread internally)
        let initial_world_bounds = Rect::new(-18.0, 12.0, 36.0, -24.0);
        let window_size = (ctx.size().0 as f32, ctx.size().1 as f32);

        println!("\n========================================");
        println!("WGPU THREADED RENDERER ARCHITECTURE");
        println!("========================================");
        println!("Main thread ID: {:?}", std::thread::current().id());
        println!("Spawning physics thread...");
        let state = UnifiedRenderState::new(
            application,
            initial_world_bounds,
            window_size,
            (MARGIN_PX_X, MARGIN_PX_Y),
        );
        println!("✓ Physics thread spawned successfully");
        println!("  - Main thread: Rendering (WGPU)");
        println!("  - Physics thread: Simulation + snapshot publishing");
        println!("========================================\n");

        // Run the event loop
        Self::run_event_loop(event_loop, window, ctx, pipelines, state);
    }

    /// Main event loop - handles window events and renders frames.
    /// Uses `UnifiedRenderState` for all shared state, eliminating scattered locals.
    fn run_event_loop(
        event_loop: EventLoop<()>,
        window: Arc<Window>,
        mut ctx: WgpuContext,
        mut pipelines: WgpuPipelines,
        mut state: UnifiedRenderState,
    ) {
        event_loop.run(move |event, window_target, control_flow| {
            *control_flow = ControlFlow::Poll;

            // Acquire the latest snapshot on frame-boundary events so
            // window-event handlers always see up-to-date data.
            let is_frame_boundary =
                matches!(event, Event::MainEventsCleared | Event::RedrawRequested(_));
            if is_frame_boundary {
                state.begin_frame();
            }

            // Use cached snapshot for current grid_spec
            let current_grid_spec = state
                .last_snapshot
                .as_ref()
                .map(|s| s.grid_spec.clone())
                .unwrap_or_else(default_grid_spec);

            match event {
                Event::WindowEvent { event, window_id } => {
                    // Check if this event is for the energy window
                    if state.energy_manager.is_energy_window_event(window_id) {
                        state.energy_manager.handle_event(event);
                        return;
                    }

                    // Main window events
                    handle_window_event(
                        event,
                        &mut ctx,
                        &mut state,
                        &mut pipelines,
                        &current_grid_spec,
                        window_target,
                        control_flow,
                    );
                }
                Event::MainEventsCleared => {
                    // Request a redraw after processing events
                    window.request_redraw();
                }
                Event::RedrawRequested(window_id) => {
                    // Check if redraw is for energy window
                    if state.energy_manager.is_energy_window_event(window_id) {
                        state.energy_manager.handle_redraw();
                        return;
                    }

                    // Render main window
                    if window_id != window.id() {
                        return;
                    }

                    if state.should_render() {
                        // Start metrics timer just before issuing draw calls.
                        state.begin_render();

                        // Render from snapshot if available
                        if let Some(snapshot) = &state.last_snapshot {
                            log_frame_periodically(snapshot);

                            // Compute view-projection matrix
                            let pan = state.viewport.pan_offset();
                            let zoom = state.viewport.zoom();
                            let view_proj =
                                compute_view_proj(&snapshot.grid_spec, pan.x, pan.y, zoom);

                            // Update all pipelines from snapshot
                            let ctrl_pressed = state.input.is_key_pressed(VirtualKeyCode::LControl)
                                || state.input.is_key_pressed(VirtualKeyCode::RControl);
                            let cursor_wp = (state.cursor_world_pos.x, state.cursor_world_pos.y);
                            let ball_count = snapshot.balls.len() as u32;
                            pipelines.update_from_snapshot(
                                &ctx,
                                snapshot,
                                view_proj,
                                cursor_wp,
                                BallFactory::radius_for_size_idx(state.current_ball_size_idx),
                                ctrl_pressed,
                            );

                            // Record metrics: ball count + one draw call per
                            // wgpu pipeline stage (frame, grid, lines, balls,
                            // crosshair, text = 6 stages).  Exact vertex
                            // counts are not tracked at this level.
                            state.metrics.record_balls_rendered(ball_count);
                            for _ in 0..6 {
                                state.metrics.record_draw_call(0);
                            }

                            // Render frame
                            if let Err(e) = render_frame(&ctx, &mut pipelines) {
                                eprintln!("Render error: {:?}", e);
                                if e == wgpu::SurfaceError::OutOfMemory {
                                    *control_flow = ControlFlow::Exit;
                                }
                            }
                        }

                        // Finalise metrics and mark frame as displayed so
                        // should_render() won't re-trigger for the same data.
                        state.end_frame();
                    }

                    // Always update energy data so the energy window stays
                    // responsive even when rendering is paused (F2).
                    state.update_energy_from_snapshot();
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
fn handle_window_event(
    event: WindowEvent,
    ctx: &mut WgpuContext,
    state: &mut UnifiedRenderState,
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
            state.handle_resize_with_world_rect(size.width as f32, size.height as f32, world_rect);
        }
        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
            ctx.resize(new_inner_size.width, new_inner_size.height);
            let world_rect = ViewportTransform::compute_world_bounds_from_window_size(
                new_inner_size.width,
                new_inner_size.height,
                (MARGIN_PX_X, MARGIN_PX_Y),
            );
            state.handle_resize_with_world_rect(
                new_inner_size.width as f32,
                new_inner_size.height as f32,
                world_rect,
            );
        }
        WindowEvent::MouseInput { .. }
        | WindowEvent::CursorMoved { .. }
        | WindowEvent::MouseWheel { .. } => {
            state.view_dirty |= state.handle_mouse_event(&event);
        }
        WindowEvent::KeyboardInput {
            input:
                KeyboardInput {
                    virtual_keycode: Some(key),
                    state: key_state,
                    ..
                },
            ..
        } => {
            // Use InputHandler to process keyboard input
            let action = match key_state {
                ElementState::Pressed => state.input.handle_key_down(key, false),
                ElementState::Released => state.input.handle_key_up(key),
            };

            if let Some(action) = action {
                state.view_dirty |= execute_input_action(
                    &action,
                    state,
                    &mut pipelines.ball,
                    &ctx.queue,
                    window_target,
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
