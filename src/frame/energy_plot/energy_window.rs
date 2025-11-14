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

            // Clear to dark background
            let bg_color = 0xFF0A0A0A;
            for pixel in buffer.iter_mut() {
                *pixel = bg_color;
            }

            // Get status text and render ASCII to string
            let text = {
                let data = self.energy_data.lock().unwrap();
                let status = data.status_text.clone();
                drop(data);

                let renderer = EnergyPlotRenderer::new(self.energy_data.clone());
                renderer.render_to_string(status.as_deref())
            };

            // Render text to pixels using simple 8x8 bitmap font
            Self::render_text_to_buffer(&mut buffer, width as usize, height as usize, &text);

            buffer.present()?;
        }
        Ok(())
    }

    /// Simple bitmap font renderer - renders text as pixels
    fn render_text_to_buffer(buffer: &mut [u32], width: usize, height: usize, text: &str) {
        const CHAR_WIDTH: usize = 8;
        const CHAR_HEIGHT: usize = 12;
        const TEXT_COLOR: u32 = 0xFFCCCCCC;

        let mut y = 10;
        for line in text.lines() {
            if y + CHAR_HEIGHT >= height {
                break;
            }

            let mut x = 10;
            for ch in line.chars() {
                if x + CHAR_WIDTH >= width {
                    break;
                }

                // Simple character rendering - just draw filled rectangles for visible characters
                if ch != ' ' {
                    Self::draw_char(buffer, width, x, y, TEXT_COLOR);
                }

                x += CHAR_WIDTH;
            }

            y += CHAR_HEIGHT;
        }
    }

    /// Draw a simple character block
    fn draw_char(buffer: &mut [u32], width: usize, x: usize, y: usize, color: u32) {
        const CHAR_WIDTH: usize = 7;
        const CHAR_HEIGHT: usize = 11;

        for dy in 0..CHAR_HEIGHT {
            for dx in 0..CHAR_WIDTH {
                let px = x + dx;
                let py = y + dy;
                let idx = py * width + px;
                if idx < buffer.len() {
                    buffer[idx] = color;
                }
            }
        }
    }
}
