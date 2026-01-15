//! WGPU window implementation
//!
//! Creates a winit window with WGPU rendering context and runs the event loop.
//! Renders a background grid using instanced quads.

use super::grid_pipeline::{GridPipeline, build_view_proj_matrix};
use super::wgpu_context::WgpuContext;
use crate::application::renderer_snapshot::CellGridSpec;
use crate::graphics::window::{MARGIN_PX_X, MARGIN_PX_Y};
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
    /// It initializes WGPU, creates the rendering surface, and enters
    /// the main event loop with grid rendering.
    pub fn create_window(&self) {
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

        let window = Arc::new(window);

        // Initialize WGPU context
        let ctx = WgpuContext::new(Arc::clone(&window));
        println!("✓ WGPU context initialized");
        println!("  Window size: {:?}", ctx.size());

        // Initialize grid pipeline
        let grid_pipeline = GridPipeline::new(&ctx.device, ctx.format, GRID_SHADER);
        println!("✓ Grid pipeline initialized");

        // Run the event loop
        self.run_event_loop(event_loop, window, ctx, grid_pipeline);
    }

    /// Main event loop - handles window events and renders frames.
    fn run_event_loop(
        &self,
        event_loop: EventLoop<()>,
        window: Arc<Window>,
        mut ctx: WgpuContext,
        mut grid_pipeline: GridPipeline,
    ) {
        // Viewport state for pan/zoom
        let mut pan_offset = (0.0_f32, 0.0_f32);
        let mut zoom = 1.0_f32;

        // Mouse state for drag panning and zoom centering
        let mut is_dragging = false;
        let mut last_mouse_pos: Option<(f64, f64)> = None;
        let mut cursor_world_pos = (0.0_f32, 0.0_f32);

        // Demo grid specification (will come from physics thread later)
        let grid_spec = CellGridSpec {
            rows: 8,
            cols: 12,
            cell_width: 3.0,
            cell_height: 3.0,
            origin_x: -18.0,
            origin_y: 12.0,
        };

        // World dimensions (matches the grid extent)
        let world_width = grid_spec.cols as f32 * grid_spec.cell_width;
        let world_height = grid_spec.rows as f32 * grid_spec.cell_height;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        println!("Window close requested");
                        *control_flow = ControlFlow::Exit;
                    }
                    WindowEvent::Resized(size) => {
                        println!("Window resized to {}x{}", size.width, size.height);
                        ctx.resize(size.width, size.height);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        ctx.resize(new_inner_size.width, new_inner_size.height);
                    }
                    // Mouse button events - middle button for panning
                    WindowEvent::MouseInput { state, button, .. } => {
                        if button == MouseButton::Middle {
                            is_dragging = state == ElementState::Pressed;
                            if !is_dragging {
                                last_mouse_pos = None;
                            }
                        }
                    }
                    // Mouse movement for panning when dragging
                    WindowEvent::CursorMoved { position, .. } => {
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
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                virtual_keycode: Some(keycode),
                                state: ElementState::Released,
                                ..
                            },
                        ..
                    } => match keycode {
                        // Reset view (matches ggez behavior)
                        VirtualKeyCode::Key0 | VirtualKeyCode::Numpad0 => {
                            pan_offset = (0.0, 0.0);
                            zoom = 1.0;
                            println!("View reset");
                        }
                        _ => {}
                    },
                    _ => {}
                },
                Event::MainEventsCleared => {
                    // Request a redraw after processing events
                    window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    // Update grid uniforms with current viewport
                    let view_proj = build_view_proj_matrix(
                        world_width,
                        world_height,
                        pan_offset.0,
                        pan_offset.1,
                        zoom,
                    );
                    grid_pipeline.update_uniforms(&ctx.queue, &grid_spec, view_proj);

                    // Render a frame with margin inset
                    let margin = (MARGIN_PX_X, MARGIN_PX_Y);
                    if let Err(e) = Self::render_frame(&ctx, &grid_pipeline, margin) {
                        eprintln!("Render error: {:?}", e);
                        if e == wgpu::SurfaceError::OutOfMemory {
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                }
                _ => {}
            }
        });
    }

    /// Render a single frame with grid background.
    ///
    /// The grid is rendered in an inset viewport with margins around it.
    fn render_frame(
        ctx: &WgpuContext,
        grid_pipeline: &GridPipeline,
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
                        // Dark background (visible as margin)
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

            // Set viewport with margins to create inset simulation pane
            let (width, height) = ctx.size();
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
        }

        // Submit commands and present
        ctx.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
