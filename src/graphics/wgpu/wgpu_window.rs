//! WGPU window implementation
//!
//! Creates a winit window with WGPU rendering context and runs the event loop.
//! Implements a basic clear-screen render to validate the setup.

use super::wgpu_context::WgpuContext;
use std::path::PathBuf;
use std::sync::Arc;
use winit::{
    dpi::PhysicalSize,
    event::{Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

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
    /// the main event loop with a basic clear-screen render.
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

        // Run the event loop
        self.run_event_loop(event_loop, window, ctx);
    }

    /// Main event loop - handles window events and renders frames.
    fn run_event_loop(&self, event_loop: EventLoop<()>, window: Arc<Window>, mut ctx: WgpuContext) {
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
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                virtual_keycode: Some(keycode),
                                ..
                            },
                        ..
                    } => {
                        if keycode == VirtualKeyCode::Escape {
                            println!("Escape pressed, exiting");
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                    _ => {}
                },
                Event::MainEventsCleared => {
                    // Request a redraw after processing events
                    window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    // Render a frame
                    if let Err(e) = Self::render_frame(&ctx) {
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

    /// Render a single frame - clears the screen to Cornflower Blue.
    ///
    /// This is a minimal render pass to validate the WGPU setup.
    /// The Cornflower Blue color is a traditional "hello world" for graphics.
    fn render_frame(ctx: &WgpuContext) -> Result<(), wgpu::SurfaceError> {
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

        // Begin render pass - clear to Cornflower Blue
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Cornflower Blue: RGB(100, 149, 237) normalized
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.392,
                            g: 0.584,
                            b: 0.929,
                            a: 1.0,
                        }),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });
            // Render pass ends when _render_pass is dropped
        }

        // Submit commands and present
        ctx.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
