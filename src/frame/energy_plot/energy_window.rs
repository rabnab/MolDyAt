use super::energy_data::SharedEnergyData;
use super::energy_renderer::EnergyPlotRenderer;
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::{Window, WindowBuilder, WindowId};

const DEFAULT_WIDTH: u32 = 640;
const DEFAULT_HEIGHT: u32 = 360;

/// Energy plot window - a separate OS window for displaying energy plots
pub struct EnergyPlotWindow {
    pub window: Option<Rc<Window>>,
    surface: Option<Surface>,
    energy_data: SharedEnergyData,
}

impl EnergyPlotWindow {
    pub fn new(energy_data: SharedEnergyData) -> Self {
        Self {
            window: None,
            surface: None,
            energy_data,
        }
    }

    pub fn create(
        &mut self,
        window_target: &EventLoopWindowTarget<()>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let window = Rc::new(
            WindowBuilder::new()
                .with_title("Energy Plot 🐾")
                .with_inner_size(LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT))
                .with_resizable(true)
                .with_min_inner_size(LogicalSize::new(320, 180))
                .build(window_target)?,
        );

        // Create softbuffer context and surface
        // SAFETY: We own the window and it's valid for the lifetime of the context
        let context = unsafe { Context::new(window.as_ref())? };
        // SAFETY: We own the context and window, and they're valid for the lifetime of the surface
        let surface = unsafe { Surface::new(&context, window.as_ref())? };

        self.window = Some(window);
        self.surface = Some(surface);
        Ok(())
    }

    pub fn is_open(&self) -> bool {
        self.window.is_some()
    }

    pub fn close(&mut self) {
        self.window = None;
    }

    pub fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|w| w.id())
    }

    pub fn focus(&self) {
        if let Some(window) = &self.window {
            window.focus_window();
        }
    }

    pub fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub fn handle_resize(&mut self, _width: u32, _height: u32) {
        self.request_redraw();
    }

    pub fn get_dimensions(&self) -> (u32, u32) {
        if let Some(window) = &self.window {
            let size = window.inner_size();
            (size.width, size.height)
        } else {
            (DEFAULT_WIDTH, DEFAULT_HEIGHT)
        }
    }

    pub fn get_energy_data(&self) -> &SharedEnergyData {
        &self.energy_data
    }

    pub fn render(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let (Some(window), Some(surface)) = (&self.window, &mut self.surface) {
            let size = window.inner_size();
            let width = size.width;
            let height = size.height;

            if width == 0 || height == 0 {
                return Ok(());
            }

            surface.resize(
                NonZeroU32::new(width).unwrap(),
                NonZeroU32::new(height).unwrap(),
            )?;

            let mut buffer = surface.buffer_mut()?;

            // Clear to black background
            for pixel in buffer.iter_mut() {
                *pixel = 0xFF000000; // Black (ARGB format)
            }

            // Get status text and energy data status
            let (status_text, has_data) = {
                let data = self.energy_data.lock().unwrap();
                let status = data.status_text.clone();
                let has_data = !data.v_kin.is_empty() || !data.v_pot.is_empty();
                (status, has_data)
            };

            // Render text-based energy plot to console/stdout
            let renderer = EnergyPlotRenderer::new(self.energy_data.clone());
            let text = renderer.render_to_string(status_text.as_deref());

            // Print ASCII art to stdout (console window)
            println!("\x1B[2J\x1B[1;1H{}", text); // Clear screen and print at top

            // For visual feedback in the window, show a simple colored indicator
            // Green = has data, Yellow = waiting for data
            let indicator_color = if has_data { 0xFF00FF00 } else { 0xFFFFFF00 };

            // Draw indicator in center
            let center_x = (width / 2) as usize;
            let center_y = (height / 2) as usize;
            let indicator_size = 40;

            for y in center_y.saturating_sub(indicator_size)..center_y + indicator_size {
                for x in center_x.saturating_sub(indicator_size)..center_x + indicator_size {
                    if x < width as usize && y < height as usize {
                        let idx = y * width as usize + x;
                        if idx < buffer.len() {
                            buffer[idx] = indicator_color;
                        }
                    }
                }
            }

            buffer.present()?;
        }
        Ok(())
    }
}
