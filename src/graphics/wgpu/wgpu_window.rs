//! WGPU window implementation
//!
//! Creates a winit window with WGPU rendering context and runs the event loop.
//! Renders a background grid and balls using instanced quads.
//! Integrates with the physics thread via PhysicsCoordinator to receive simulation snapshots.

use super::ball_pipeline::BallPipeline;
use super::crosshair_pipeline::CrosshairPipeline;
use super::frame_pipeline::FramePipeline;
use super::grid_pipeline::{GridPipeline, build_view_proj_matrix};
use super::text_pipeline::TextPipeline;
use super::wgpu_context::WgpuContext;
use crate::application::MainContent;
use crate::application::physics_thread::PhysicsCommand;
use crate::application::renderer_snapshot::{CellGridSpec, RendererSnapshot};
use crate::graphics::renderer::event_loop::PhysicsCoordinator;
use crate::graphics::renderer::{InputAction, InputHandler};
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

/// Grid shader source code (embedded at compile time)
const GRID_SHADER: &str = include_str!("shaders/grid.wgsl");

/// Ball shader source code (embedded at compile time)
const BALL_SHADER: &str = include_str!("shaders/ball.wgsl");

/// Crosshair shader source code (embedded at compile time)
const CROSSHAIR_SHADER: &str = include_str!("shaders/crosshair.wgsl");

/// Ball sizes for cursor placement (matching BallFactory)
const BALL_SIZES: [f32; 5] = [0.15, 0.30, 0.60, 1.20, 2.40];

/// WGPU-based window for rendering the molecular dynamics simulation.
///
/// This struct manages the winit window and WGPU context lifecycle.
pub struct WgpuWindow {
    resource_dir: PathBuf,
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
        println!("  Resource dir: {:?}", self.resource_dir);
        println!("  Region: {:?}", self.region);

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

        // Initialize grid pipeline
        let grid_pipeline = GridPipeline::new(&ctx.device, ctx.format, GRID_SHADER);
        println!("✓ Grid pipeline initialized");
        // Initialize frame pipeline
        let frame_pipeline =
            FramePipeline::new(&ctx.device, ctx.format, include_str!("shaders/frame.wgsl"));

        // Initialize ball pipeline
        let ball_pipeline = BallPipeline::new(&ctx.device, ctx.format, BALL_SHADER);
        println!("✓ Ball pipeline initialized");

        // Initialize crosshair pipeline
        let crosshair_pipeline = CrosshairPipeline::new(&ctx.device, ctx.format, CROSSHAIR_SHADER);
        println!("✓ Crosshair pipeline initialized");

        // Initialize text pipeline for cursor overlay
        let text_pipeline = TextPipeline::new(&ctx.device, ctx.format);
        println!("✓ Text pipeline initialized");

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
        self.run_event_loop(
            event_loop,
            window,
            ctx,
            grid_pipeline,
            ball_pipeline,
            crosshair_pipeline,
            frame_pipeline,
            text_pipeline,
            physics,
        );
    }

    /// Main event loop - handles window events and renders frames.
    /// Receives simulation data from PhysicsCoordinator via snapshots.
    #[allow(clippy::too_many_arguments)]
    fn run_event_loop(
        &self,
        event_loop: EventLoop<()>,
        window: Arc<Window>,
        mut ctx: WgpuContext,
        mut grid_pipeline: GridPipeline,
        mut ball_pipeline: BallPipeline,
        mut crosshair_pipeline: CrosshairPipeline,
        mut frame_pipeline: FramePipeline,
        mut text_pipeline: TextPipeline,
        physics: PhysicsCoordinator,
    ) {
        // Centralized input handler for consistent behavior with ggez
        let mut input_handler = InputHandler::new();

        // Viewport state for pan/zoom
        let mut pan_offset = (0.0_f32, 0.0_f32);
        let mut zoom = 1.0_f32;

        // Mouse state for drag panning and zoom centering
        let mut is_dragging = false;
        let mut last_mouse_pos: Option<(f64, f64)> = None;
        let mut cursor_world_pos = (0.0_f32, 0.0_f32);

        // Current ball size index (updated from input handler)
        let mut current_ball_size_idx: usize = 2; // Default to size index 2 (0.60)

        // Cache the last snapshot for rendering when no new data available
        let mut last_snapshot: Option<Arc<RendererSnapshot>> = None;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            // Get latest snapshot from physics thread (if available)
            if let Some(snapshot) = physics.get_latest_snapshot() {
                last_snapshot = Some(snapshot);
            }

            // Use cached snapshot for current grid_spec
            let current_grid_spec = last_snapshot
                .as_ref()
                .map(|s| s.grid_spec.clone())
                .unwrap_or_else(|| {
                    // Fallback grid spec if no snapshot yet
                    CellGridSpec {
                        rows: 8,
                        cols: 12,
                        cell_width: 3.0,
                        cell_height: 3.0,
                        origin_x: -18.0,
                        origin_y: 12.0,
                    }
                });

            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        println!("Window close requested");
                        *control_flow = ControlFlow::Exit;
                    }
                    WindowEvent::Resized(size) => {
                        println!("Window resized to {}x{}", size.width, size.height);
                        ctx.resize(size.width, size.height);

                        // Send resize command to physics thread to rebuild CellGrid
                        // This ensures grid dimensions match the new window size
                        let world_rect = Self::compute_world_rect_for_size(size.width, size.height);
                        physics.send_command(PhysicsCommand::Resize { world_rect });
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        ctx.resize(new_inner_size.width, new_inner_size.height);
                        let world_rect = Self::compute_world_rect_for_size(
                            new_inner_size.width,
                            new_inner_size.height,
                        );
                        physics.send_command(PhysicsCommand::Resize { world_rect });
                    }
                    // Mouse button events - middle button for panning, right for ball size
                    WindowEvent::MouseInput { state, button, .. } => {
                        if button == MouseButton::Middle {
                            is_dragging = state == ElementState::Pressed;
                            if !is_dragging {
                                last_mouse_pos = None;
                            }
                        } else if button == MouseButton::Right && state == ElementState::Pressed {
                            // Cycle through ball sizes
                            current_ball_size_idx = (current_ball_size_idx + 1) % BALL_SIZES.len();
                            physics.send_command(PhysicsCommand::NextBallSize);
                            println!(
                                "Ball size: {} (index {})",
                                BALL_SIZES[current_ball_size_idx], current_ball_size_idx
                            );
                        }
                    }
                    // Mouse movement for panning when dragging
                    WindowEvent::CursorMoved { position, .. } => {
                        // Get current world dimensions from grid spec (from CellGrid via snapshot)
                        let world_width =
                            current_grid_spec.cols as f32 * current_grid_spec.cell_width;
                        let world_height =
                            current_grid_spec.rows as f32 * current_grid_spec.cell_height;

                        // Convert screen position to world position for zoom centering
                        let (win_width, win_height) = ctx.size();
                        let viewport_width = (win_width as f32 - 2.0 * MARGIN_PX_X).max(1.0);
                        let viewport_height = (win_height as f32 - 2.0 * MARGIN_PX_Y).max(1.0);
                        let world_visible_width = world_width / zoom;
                        let world_visible_height = world_height / zoom;

                        // Screen position relative to viewport (accounting for margin)
                        let vp_x = (position.x as f32 - MARGIN_PX_X) / viewport_width;
                        let vp_y = (position.y as f32 - MARGIN_PX_Y) / viewport_height;

                        // Convert to world coordinates (centered at pan_offset)
                        cursor_world_pos.0 = pan_offset.0 + (vp_x - 0.5) * world_visible_width;
                        cursor_world_pos.1 = pan_offset.1 - (vp_y - 0.5) * world_visible_height; // Y flipped

                        // Update cursor position in input handler
                        input_handler.set_cursor_pos(
                            Vec2D::new(cursor_world_pos.0, cursor_world_pos.1),
                            [position.x as f32, position.y as f32],
                        );

                        if is_dragging {
                            if let Some((last_x, last_y)) = last_mouse_pos {
                                // Calculate delta in screen pixels
                                let dx = position.x - last_x;
                                let dy = position.y - last_y;

                                // Convert pixel delta to world delta
                                let world_dx = (dx as f32 / viewport_width) * world_visible_width;
                                let world_dy = (dy as f32 / viewport_height) * world_visible_height;

                                // Pan in opposite direction of drag (natural scrolling)
                                pan_offset.0 -= world_dx;
                                pan_offset.1 += world_dy; // Y is flipped in screen coords

                                // Clamp pan to keep view within simulation bounds
                                // Max pan is half the world size (can pan to edge but not beyond)
                                let max_pan_x = world_width / 2.0 * (1.0 - 1.0 / zoom).max(0.0);
                                let max_pan_y = world_height / 2.0 * (1.0 - 1.0 / zoom).max(0.0);
                                pan_offset.0 = pan_offset.0.clamp(-max_pan_x, max_pan_x);
                                pan_offset.1 = pan_offset.1.clamp(-max_pan_y, max_pan_y);
                            }
                            last_mouse_pos = Some((position.x, position.y));
                        }
                    }
                    // Mouse wheel for zooming centered on cursor
                    WindowEvent::MouseWheel { delta, .. } => {
                        let scroll_amount = match delta {
                            MouseScrollDelta::LineDelta(_, y) => y,
                            MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 50.0,
                        };

                        // Get current world dimensions from grid spec (from CellGrid via snapshot)
                        let world_width =
                            current_grid_spec.cols as f32 * current_grid_spec.cell_width;
                        let world_height =
                            current_grid_spec.rows as f32 * current_grid_spec.cell_height;

                        // Calculate zoom factor
                        let zoom_factor = 1.1_f32.powf(scroll_amount);
                        let new_zoom = (zoom * zoom_factor).clamp(0.1, 10.0);

                        // Zoom centered on cursor position:
                        // The world point under the cursor should stay under the cursor
                        // pan_new = cursor_world - (cursor_world - pan_old) * (zoom_old / zoom_new)
                        // Simplified: pan_new = pan_old + (cursor_world - pan_old) * (1 - zoom_old / zoom_new)
                        let scale_ratio = 1.0 - zoom / new_zoom;
                        pan_offset.0 += (cursor_world_pos.0 - pan_offset.0) * scale_ratio;
                        pan_offset.1 += (cursor_world_pos.1 - pan_offset.1) * scale_ratio;

                        zoom = new_zoom;

                        // Clamp pan to keep view within simulation bounds
                        let max_pan_x = world_width / 2.0 * (1.0 - 1.0 / zoom).max(0.0);
                        let max_pan_y = world_height / 2.0 * (1.0 - 1.0 / zoom).max(0.0);
                        pan_offset.0 = pan_offset.0.clamp(-max_pan_x, max_pan_x);
                        pan_offset.1 = pan_offset.1.clamp(-max_pan_y, max_pan_y);
                    }
                    // Centralized keyboard input handling via InputHandler
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
                            Self::execute_input_action(
                                &action,
                                &physics,
                                &mut pan_offset,
                                &mut zoom,
                                &mut current_ball_size_idx,
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
                            Self::execute_input_action(
                                &action,
                                &physics,
                                &mut pan_offset,
                                &mut zoom,
                                &mut current_ball_size_idx,
                            );
                        }
                    }
                    _ => {}
                },
                Event::MainEventsCleared => {
                    // Request a redraw after processing events
                    window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    // Render from snapshot if available
                    if let Some(snapshot) = &last_snapshot {
                        // Get grid spec from CellGrid (via snapshot)
                        let grid_spec = &snapshot.grid_spec;

                        // Debug: Log grid spec from snapshot periodically
                        static mut FRAME_COUNT: u64 = 0;
                        static mut PREV_BALL_COUNT: usize = 0;
                        unsafe {
                            FRAME_COUNT += 1;
                            let ball_count_changed = PREV_BALL_COUNT != snapshot.balls.len();
                            if FRAME_COUNT % 600 == 1 || ball_count_changed {
                                // Log every ~10 seconds at 60fps, or when ball count changes
                                println!(
                                    "Rendering: {}x{} grid, {} balls",
                                    grid_spec.cols,
                                    grid_spec.rows,
                                    snapshot.balls.len()
                                );
                                PREV_BALL_COUNT = snapshot.balls.len();
                            }
                        }

                        // Get world dimensions from grid spec
                        let world_width = grid_spec.cols as f32 * grid_spec.cell_width;
                        let world_height = grid_spec.rows as f32 * grid_spec.cell_height;

                        // Update grid uniforms with current viewport
                        let view_proj = build_view_proj_matrix(
                            world_width,
                            world_height,
                            pan_offset.0,
                            pan_offset.1,
                            zoom,
                        );
                        grid_pipeline.update_uniforms(&ctx.queue, grid_spec, view_proj);

                        // Physics constants from snapshot for ball rendering outlines
                        let eq_edge_distance = snapshot.equilibrium_edge_distance;
                        let cutoff_distance = snapshot.cutoff_distance;

                        // Update ball uniforms and upload ball data from snapshot
                        let (win_width, win_height) = ctx.size();
                        ball_pipeline.update_uniforms(
                            &ctx.queue,
                            view_proj,
                            [win_width as f32, win_height as f32],
                            eq_edge_distance,
                            cutoff_distance,
                        );
                        ball_pipeline.upload_balls(&ctx.device, &ctx.queue, &snapshot.balls);

                        // Update crosshair uniforms
                        let current_ball_radius = BALL_SIZES[current_ball_size_idx];
                        let ctrl_pressed = input_handler.is_key_pressed(VirtualKeyCode::LControl)
                            || input_handler.is_key_pressed(VirtualKeyCode::RControl);
                        crosshair_pipeline.update_uniforms(
                            &ctx.queue,
                            view_proj,
                            [cursor_world_pos.0, cursor_world_pos.1],
                            current_ball_radius,
                            ctrl_pressed,
                            eq_edge_distance,
                            cutoff_distance,
                        );

                        // Update cursor position text overlay (bottom-right margin)
                        let margin = (MARGIN_PX_X, MARGIN_PX_Y);
                        let cursor_text =
                            format!("({:.2}, {:.2})", cursor_world_pos.0, cursor_world_pos.1);
                        text_pipeline.update_text(
                            &ctx.device,
                            &ctx.queue,
                            &cursor_text,
                            (win_width, win_height),
                            margin,
                        );

                        // Render a frame with margin inset
                        if let Err(e) = Self::render_frame(
                            &ctx,
                            &grid_pipeline,
                            &ball_pipeline,
                            &crosshair_pipeline,
                            &mut frame_pipeline,
                            &text_pipeline,
                            margin,
                        ) {
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

    /// Render a single frame with grid background, balls, and crosshair.
    ///
    /// The grid and balls are rendered in an inset viewport with margins around them.
    fn render_frame(
        ctx: &WgpuContext,
        grid_pipeline: &GridPipeline,
        ball_pipeline: &BallPipeline,
        crosshair_pipeline: &CrosshairPipeline,
        frame_pipeline: &mut FramePipeline,
        text_pipeline: &TextPipeline,
        margin: (f32, f32),
    ) -> Result<(), wgpu::SurfaceError> {
        // Get the next frame's texture
        let output = ctx.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Create command encoder
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Clear whole surface to black (margin/background)
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

            // Draw frame border in full-window viewport
            let (width, height) = ctx.size();
            frame_pipeline.update_vertices(&ctx.queue, width as f32, height as f32, margin);
            render_pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
            frame_pipeline.render(&mut render_pass);

            // Set viewport with margins to create inset simulation pane
            let viewport_x = margin.0;
            let viewport_y = margin.1;
            let viewport_width = (width as f32 - 2.0 * margin.0).max(1.0);
            let viewport_height = (height as f32 - 2.0 * margin.1).max(1.0);
            render_pass.set_viewport(
                viewport_x,
                viewport_y,
                viewport_width,
                viewport_height,
                0.0,
                1.0,
            );

            // Draw grid - single instanced draw call
            let cell_count = grid_pipeline.cell_count();
            grid_pipeline.render(&mut render_pass, cell_count);

            // Draw balls - single instanced draw call
            ball_pipeline.render(&mut render_pass);

            // Draw crosshair (always) and ball outline (when ctrl pressed)
            crosshair_pipeline.render(&mut render_pass);

            // Draw cursor position text in margin (screen space)
            render_pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
            text_pipeline.render(&mut render_pass);
        }

        // Submit commands and present
        ctx.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    /// Compute world rectangle for a given window size.
    ///
    /// Uses the same LEN_TO_PX conversion as ggez (0.1 = 1 world unit per 10 pixels).
    /// The world rect is centered at origin with appropriate dimensions.
    fn compute_world_rect_for_size(width: u32, height: u32) -> Rect {
        const LEN_TO_PX: f32 = 0.1; // 1 world unit = 10 pixels
        let region_width = (width as f32 - 2.0 * MARGIN_PX_X).max(1.0);
        let region_height = (height as f32 - 2.0 * MARGIN_PX_Y).max(1.0);
        let world_width = region_width * LEN_TO_PX;
        let world_height = region_height * LEN_TO_PX;

        // Rect format: (x, y, w, h) where (x, y) is top-left corner
        // World is centered at origin, so top-left is (-w/2, h/2)
        // Height is negative because y increases downward in world coords
        Rect::new(
            -world_width / 2.0,
            world_height / 2.0,
            world_width,
            -world_height,
        )
    }

    /// Execute input actions from the centralized InputHandler.
    /// Maps InputAction enum to physics commands and view updates.
    #[allow(clippy::too_many_arguments)]
    fn execute_input_action(
        action: &InputAction,
        physics: &PhysicsCoordinator,
        pan_offset: &mut (f32, f32),
        zoom: &mut f32,
        ball_size_idx: &mut usize,
    ) {
        match action {
            // Simulation control
            InputAction::TogglePause => {
                physics.send_command(PhysicsCommand::TogglePause);
                println!("Toggle pause");
            }
            InputAction::ToggleStepwise => {
                physics.send_command(PhysicsCommand::ToggleStepwise);
                println!("Toggle stepwise mode");
            }
            InputAction::RandomizeSpeeds => {
                physics.send_command(PhysicsCommand::RandomizeSpeeds);
                println!("Randomizing ball speeds");
            }
            InputAction::DoubleTimeStep => {
                physics.send_command(PhysicsCommand::DoubleTimeStep);
                println!("Double time step");
            }
            InputAction::HalveTimeStep => {
                physics.send_command(PhysicsCommand::HalveTimeStep);
                println!("Halve time step");
            }
            InputAction::IncreaseWaitTime => {
                physics.send_command(PhysicsCommand::IncreaseWaitTime);
                println!("Increase wait time (slower)");
            }
            InputAction::DecreaseWaitTime => {
                physics.send_command(PhysicsCommand::DecreaseWaitTime);
                println!("Decrease wait time (faster)");
            }

            // Ball manipulation
            InputAction::AddBall { position } => {
                physics.send_command(PhysicsCommand::AddBall(*position));
                println!("Adding ball at ({:.2}, {:.2})", position.x, position.y);
            }
            InputAction::AddRandomBalls { count } => {
                physics.send_command(PhysicsCommand::AddRandomBalls(*count));
                println!("Adding {} random balls", count);
            }
            InputAction::ClearBalls => {
                physics.send_command(PhysicsCommand::ClearBalls);
                println!("Clearing all balls");
            }
            InputAction::StopAllBalls => {
                physics.send_command(PhysicsCommand::StopAllBalls);
                println!("Stopping all balls");
            }
            InputAction::NextBallSize => {
                *ball_size_idx = (*ball_size_idx + 1) % BALL_SIZES.len();
                physics.send_command(PhysicsCommand::NextBallSize);
                println!(
                    "Ball size: {} (index {})",
                    BALL_SIZES[*ball_size_idx], ball_size_idx
                );
            }

            // View control
            InputAction::ResetView => {
                *pan_offset = (0.0, 0.0);
                *zoom = 1.0;
                println!("View reset");
            }

            // Actions not implemented for WGPU (ggez-only UI features)
            InputAction::ToggleRendering
            | InputAction::CyclePlotContent
            | InputAction::ToggleEnergyWindow
            | InputAction::PrintHelp
            | InputAction::ToggleRealData
            | InputAction::ToggleTimingDisplay
            | InputAction::ToggleBallStats
            | InputAction::StartVerbosityInput => {
                // TODO(WGPU): Add UI for these features (ggez has these in UI, WGPU doesn't yet)
                println!("Action not yet implemented in WGPU: {:?}", action);
            }

            // Actions that are context-dependent or handled elsewhere
            InputAction::EnableCursor { .. }
            | InputAction::DisableCursor
            | InputAction::UpdateCursor { .. }
            | InputAction::EnableRandomBallSpeed
            | InputAction::RemoveBallAt { .. }
            | InputAction::ZoomIn { .. }
            | InputAction::ZoomOut { .. }
            | InputAction::PanStart { .. }
            | InputAction::PanUpdate { .. }
            | InputAction::PanEnd => {
                // These are handled separately in the event loop or not needed for WGPU
            }
        }
    }
}
