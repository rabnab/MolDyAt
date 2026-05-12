use super::energy_data::{DisplayMode, SharedEnergyData};
use super::energy_renderer::EnergyPlotRenderer;
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::{Window, WindowId};

const DEFAULT_WIDTH: u32 = 1024; // Reduced by 20% from 1280
const DEFAULT_HEIGHT: u32 = 576; // Reduced by 20% from 720
const MIN_WINDOW_WIDTH: u32 = 320;
const MIN_WINDOW_HEIGHT: u32 = 240;

// Text rendering constants
const CHAR_WIDTH_PIX: u32 = 13; // pixel width per character (realistic value)
const CHAR_HEIGHT_PIX: u32 = 19; // pixel height per character (realistic value)
const MARGIN_LEFT: u32 = 10;
const MARGIN_TOP: u32 = 8;
const MARGIN_RIGHT: u32 = 10;
const MARGIN_BOTTOM: u32 = 10;

// Base line count without status text
const BASE_LINES: u32 = 41;
const STATUS_TEXT_LINES: u32 = 10;

const MIN_HEIGHT_WITHOUT_STATUS: u32 = MARGIN_TOP + BASE_LINES * CHAR_HEIGHT_PIX;
const MIN_HEIGHT_WITH_STATUS: u32 = MARGIN_TOP + (BASE_LINES + STATUS_TEXT_LINES) * CHAR_HEIGHT_PIX;

#[derive(Debug, Clone, Copy)]
struct TextMetrics {
    char_width: u32,
    char_height: u32,
    margin_left: u32,
    margin_top: u32,
    margin_right: u32,
    margin_bottom: u32,
}

fn text_metrics(scale_factor: f64) -> TextMetrics {
    let ui_scale = scale_factor.max(1.0) as f32;
    let scaled = |value: u32| -> u32 { ((value as f32) * ui_scale).round().max(1.0) as u32 };

    TextMetrics {
        char_width: scaled(CHAR_WIDTH_PIX),
        char_height: scaled(CHAR_HEIGHT_PIX),
        margin_left: scaled(MARGIN_LEFT),
        margin_top: scaled(MARGIN_TOP),
        margin_right: scaled(MARGIN_RIGHT),
        margin_bottom: scaled(MARGIN_BOTTOM),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowGeometry {
    pub position: PhysicalPosition<i32>,
    pub size: PhysicalSize<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitScreenLayout {
    pub simulation: WindowGeometry,
    pub energy: WindowGeometry,
}

pub fn split_screen_layout_for_monitor(monitor: &MonitorHandle) -> SplitScreenLayout {
    split_screen_layout(monitor.position(), monitor.size())
}

fn split_screen_layout(
    monitor_position: PhysicalPosition<i32>,
    monitor_size: PhysicalSize<u32>,
) -> SplitScreenLayout {
    let simulation_width = (monitor_size.width / 2).max(1);
    let energy_width = (monitor_size.width - simulation_width).max(1);
    let window_height = monitor_size.height.max(1);

    SplitScreenLayout {
        simulation: WindowGeometry {
            position: monitor_position,
            size: PhysicalSize::new(simulation_width, window_height),
        },
        energy: WindowGeometry {
            position: PhysicalPosition::new(
                monitor_position.x + simulation_width as i32,
                monitor_position.y,
            ),
            size: PhysicalSize::new(energy_width, window_height),
        },
    }
}

/// Energy plot window - a separate OS window for displaying energy plots
pub struct EnergyPlotWindow {
    pub window: Option<Rc<Window>>,
    context: Option<Context<Rc<Window>>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    energy_data: SharedEnergyData,
    display_mode: DisplayMode,
    has_status_text: bool,
    /// Version of `EnergyData` that the current pixel buffer was rendered from.
    /// Used to skip re-rasterising when the data and display settings haven't
    /// changed since the last frame.
    cached_data_version: Option<u64>,
    /// Pixel dimensions `(width, height)` of the currently cached buffer, so
    /// we invalidate the cache on resize.
    cached_size: Option<(u32, u32)>,
    /// Cached display mode so we invalidate the cache when it changes.
    cached_display_mode: Option<DisplayMode>,
}

impl EnergyPlotWindow {
    /// Create new energy plot window (not yet shown).
    ///
    /// # Arguments
    /// * `energy_data` - Shared data synchronized with main simulation
    /// * `display_mode` - Initial display mode for which energy traces to show
    pub fn new(energy_data: SharedEnergyData, display_mode: DisplayMode) -> Self {
        Self {
            window: None,
            context: None,
            surface: None,
            energy_data,
            display_mode,
            has_status_text: false,
            cached_data_version: None,
            cached_size: None,
            cached_display_mode: None,
        }
    }

    /// Create OS window and initialize softbuffer surface for pixel rendering.
    /// Window height adapts based on whether status text is present.
    ///
    /// # Arguments
    /// * `window_target` - Event loop window target from winit
    pub fn create(
        &mut self,
        window_target: &ActiveEventLoop,
        geometry: Option<WindowGeometry>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let title = format!("Energy Plot 🐾 — {}", self.display_mode.as_str());

        // Determine initial height based on whether status text is available
        let initial_height = {
            let data = self.energy_data.lock().unwrap();
            if data.status_text.is_some() {
                MIN_HEIGHT_WITH_STATUS
            } else {
                MIN_HEIGHT_WITHOUT_STATUS
            }
        };

        let initial_size = geometry
            .map(|geometry| geometry.size)
            .unwrap_or(PhysicalSize::new(DEFAULT_WIDTH, initial_height));

        let mut attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(initial_size)
            .with_resizable(true)
            .with_min_inner_size(LogicalSize::new(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT));
        if let Some(geometry) = geometry {
            attrs = attrs.with_position(geometry.position);
        }
        let window = Rc::new(window_target.create_window(attrs)?);

        // Create softbuffer context and surface
        // SAFETY: We own the window and it's valid for the lifetime of the context
        let context = Context::new(window.clone())?;
        // SAFETY: We own the context and window, and they're valid for the lifetime of the surface
        let surface = Surface::new(&context, window.clone())?;

        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
        Ok(())
    }

    pub fn is_open(&self) -> bool {
        self.window.is_some()
    }

    pub fn close(&mut self) {
        self.window = None;
        self.context = None;
        self.surface = None;
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

    pub fn get_display_mode(&self) -> DisplayMode {
        self.display_mode
    }

    pub fn set_display_mode(&mut self, mode: DisplayMode) {
        self.display_mode = mode;
        self.cached_data_version = None; // invalidate cache on mode change
        // Update window title
        if let Some(window) = &self.window {
            window.set_title(&format!("Energy Plot 🐾 — {}", mode.as_str()));
        }
    }

    pub fn cycle_display_mode(&mut self) {
        self.display_mode = self.display_mode.next();
        self.cached_data_version = None; // invalidate cache on mode change
        // Update window title
        if let Some(window) = &self.window {
            window.set_title(&format!("Energy Plot 🐾 — {}", self.display_mode.as_str()));
        }
    }

    pub fn render(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let (Some(window), Some(surface)) = (&self.window, &mut self.surface) {
            let size = window.inner_size();
            let width = size.width;
            let height = size.height;
            let metrics = text_metrics(window.scale_factor());

            if width == 0 || height == 0 {
                return Ok(());
            }

            // Read data version, status-text presence, and display mode before
            // deciding whether we can reuse the cached frame.
            let (current_version, current_has_status) = {
                let data = self.energy_data.lock().unwrap();
                (data.version, data.status_text.is_some())
            };

            let current_size = (width, height);

            // If data version, window size, and display mode are all unchanged,
            // the pixel buffer already reflects the current state — skip
            // re-rasterising.
            let cache_valid = self.cached_data_version == Some(current_version)
                && self.cached_size == Some(current_size)
                && self.cached_display_mode == Some(self.display_mode);

            if cache_valid {
                return Ok(());
            }

            // Track status text presence, but keep the current geometry stable
            // so the window can stay docked to a screen half on small displays.
            self.has_status_text = current_has_status;

            surface.resize(
                NonZeroU32::new(width).unwrap(),
                NonZeroU32::new(height).unwrap(),
            )?;

            let mut buffer = surface.buffer_mut()?;

            // Clear to dark background (softer gray for better contrast)
            let bg_color = 0xFF141416; // Dark gray (less harsh than pure black)
            for pixel in buffer.iter_mut() {
                *pixel = bg_color;
            }

            // Compute character grid dimensions from pixel size and character metrics
            // Subtract all margins (top, bottom, left, right) for actual plot area

            let available_width = width.saturating_sub(metrics.margin_left + metrics.margin_right);
            let available_height =
                height.saturating_sub(metrics.margin_top + metrics.margin_bottom);

            // Calculate grid size to fill window, no hard clamp
            let cols = (available_width / metrics.char_width) as usize;
            let rows = (available_height / metrics.char_height) as usize;

            // Get status text and render to colored string using layout-aware renderer
            let (text, color_map) = {
                let data = self.energy_data.lock().unwrap();
                let status = data.status_text.clone();
                drop(data);

                let renderer = EnergyPlotRenderer::new(self.energy_data.clone());
                renderer.render_to_colored_string_with_layout(
                    status.as_deref(),
                    self.display_mode,
                    cols.max(20),
                    rows.max(10),
                )
            };

            // Render text to pixels using colors
            Self::render_colored_text_to_buffer(
                &mut buffer,
                width as usize,
                height as usize,
                &text,
                &color_map,
                metrics,
            );

            buffer.present()?;

            // Update cache metadata so the next redraw can skip re-rasterising
            // when nothing has changed.
            self.cached_data_version = Some(current_version);
            self.cached_size = Some(current_size);
            self.cached_display_mode = Some(self.display_mode);
        }
        Ok(())
    }

    /// Colored bitmap font renderer - renders text with per-character colors
    fn render_colored_text_to_buffer(
        buffer: &mut [u32],
        width: usize,
        height: usize,
        text: &str,
        color_map: &[Vec<u32>],
        metrics: TextMetrics,
    ) {
        let left_margin = metrics.margin_left as usize;
        let top_margin = metrics.margin_top as usize;
        let char_width = metrics.char_width as usize;
        let char_height = metrics.char_height as usize;

        let mut y = top_margin;
        for (line_idx, line) in text.lines().enumerate() {
            if y + char_height >= height {
                break;
            }

            let mut x = left_margin;
            for (char_idx, ch) in line.chars().enumerate() {
                if x + char_width >= width {
                    break;
                }

                // Get color for this character from color map
                let color = if line_idx < color_map.len() && char_idx < color_map[line_idx].len() {
                    color_map[line_idx][char_idx]
                } else {
                    0xFFCCCCCC // Default text color
                };

                // Render character using bitmap font
                Self::draw_char_scaled(buffer, width, x, y, ch, color, metrics);

                x += char_width;
            }

            y += char_height;
        }
    }

    /// Simple bitmap font renderer - renders text as pixels
    fn render_text_to_buffer(buffer: &mut [u32], width: usize, height: usize, text: &str) {
        const TEXT_COLOR: u32 = 0xFFCCCCCC;
        let metrics = text_metrics(1.0);
        let left_margin = metrics.margin_left as usize;
        let top_margin = metrics.margin_top as usize;
        let char_width = metrics.char_width as usize;
        let char_height = metrics.char_height as usize;

        let mut y = top_margin; // top margin
        for line in text.lines() {
            if y + char_height >= height {
                break;
            }

            let mut x = left_margin;
            for ch in line.chars() {
                if x + char_width >= width {
                    break;
                }

                // Render actual character using bitmap font
                Self::draw_char_scaled(buffer, width, x, y, ch, TEXT_COLOR, metrics);

                x += char_width;
            }

            y += char_height;
        }
    }

    /// Draw a character using a simple 8x8 bitmap font scaled to ~1.6x (13x19)
    fn draw_char_scaled(
        buffer: &mut [u32],
        width: usize,
        x: usize,
        y: usize,
        ch: char,
        color: u32,
        metrics: TextMetrics,
    ) {
        let glyph = Self::get_glyph(ch);

        // Target size is 13x19, source is 8x8
        let target_width = metrics.char_width as usize;
        let target_height = metrics.char_height as usize;
        const SOURCE_SIZE: usize = 8;

        for ty in 0..target_height {
            // Optimization: Calculate source Y once per row
            let sy = (ty * SOURCE_SIZE) / target_height;
            if sy >= SOURCE_SIZE {
                continue;
            }
            for tx in 0..target_width {
                // Map target pixel to source pixel
                let sx = (tx * SOURCE_SIZE) / target_width;

                if sy < SOURCE_SIZE && sx < SOURCE_SIZE {
                    let row = glyph[sy];
                    if row & (1 << (7 - sx)) != 0 {
                        let px = x + tx;
                        let py = y + ty;
                        let idx = py * width + px;
                        if idx < buffer.len() {
                            buffer[idx] = color;
                        }
                    }
                }
            }
        }
    }

    /// Get 8x8 bitmap for a character
    fn get_glyph(ch: char) -> &'static [u8; 8] {
        match ch {
            ' ' => &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            '!' => &[0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00],
            '"' => &[0x6C, 0x6C, 0x6C, 0x00, 0x00, 0x00, 0x00, 0x00],
            '#' => &[0x36, 0x36, 0x7F, 0x36, 0x7F, 0x36, 0x36, 0x00],
            '$' => &[0x0C, 0x3F, 0x68, 0x3E, 0x0B, 0x7E, 0x18, 0x00],
            '%' => &[0x60, 0x66, 0x0C, 0x18, 0x30, 0x66, 0x06, 0x00],
            '&' => &[0x38, 0x6C, 0x6C, 0x38, 0x6D, 0x66, 0x3B, 0x00],
            '\'' => &[0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00],
            '(' => &[0x0C, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0C, 0x00],
            ')' => &[0x30, 0x18, 0x0C, 0x0C, 0x0C, 0x18, 0x30, 0x00],
            '*' => &[0x00, 0x18, 0x7E, 0x3C, 0x7E, 0x18, 0x00, 0x00],
            '+' => &[0x00, 0x18, 0x18, 0x7E, 0x18, 0x18, 0x00, 0x00],
            ',' => &[0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30],
            '-' => &[0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
            '.' => &[0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
            '/' => &[0x00, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x00, 0x00],
            '0' => &[0x3C, 0x66, 0x6E, 0x7E, 0x76, 0x66, 0x3C, 0x00],
            '1' => &[0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
            '2' => &[0x3C, 0x66, 0x06, 0x0C, 0x18, 0x30, 0x7E, 0x00],
            '3' => &[0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
            '4' => &[0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00],
            '5' => &[0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
            '6' => &[0x1C, 0x30, 0x60, 0x7C, 0x66, 0x66, 0x3C, 0x00],
            '7' => &[0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
            '8' => &[0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
            '9' => &[0x3C, 0x66, 0x66, 0x3E, 0x06, 0x0C, 0x38, 0x00],
            ':' => &[0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00, 0x00],
            ';' => &[0x00, 0x18, 0x18, 0x00, 0x18, 0x18, 0x30, 0x00],
            '<' => &[0x0C, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0C, 0x00],
            '=' => &[0x00, 0x00, 0x7E, 0x00, 0x7E, 0x00, 0x00, 0x00],
            '>' => &[0x30, 0x18, 0x0C, 0x06, 0x0C, 0x18, 0x30, 0x00],
            '?' => &[0x3C, 0x66, 0x0C, 0x18, 0x18, 0x00, 0x18, 0x00],
            '@' => &[0x3C, 0x66, 0x6E, 0x6A, 0x6E, 0x60, 0x3C, 0x00],
            'A' => &[0x3C, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
            'B' => &[0x7C, 0x66, 0x66, 0x7C, 0x66, 0x66, 0x7C, 0x00],
            'C' => &[0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
            'D' => &[0x78, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0x78, 0x00],
            'E' => &[0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x7E, 0x00],
            'F' => &[0x7E, 0x60, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x00],
            'G' => &[0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3C, 0x00],
            'H' => &[0x66, 0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
            'I' => &[0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
            'J' => &[0x3E, 0x0C, 0x0C, 0x0C, 0x0C, 0x6C, 0x38, 0x00],
            'K' => &[0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00],
            'L' => &[0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00],
            'M' => &[0x63, 0x77, 0x7F, 0x6B, 0x6B, 0x63, 0x63, 0x00],
            'N' => &[0x66, 0x66, 0x76, 0x7E, 0x6E, 0x66, 0x66, 0x00],
            'O' => &[0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
            'P' => &[0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
            'Q' => &[0x3C, 0x66, 0x66, 0x66, 0x6A, 0x6C, 0x36, 0x00],
            'R' => &[0x7C, 0x66, 0x66, 0x7C, 0x6C, 0x66, 0x66, 0x00],
            'S' => &[0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00],
            'T' => &[0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
            'U' => &[0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
            'V' => &[0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
            'W' => &[0x63, 0x63, 0x6B, 0x6B, 0x7F, 0x77, 0x63, 0x00],
            'X' => &[0x66, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x66, 0x00],
            'Y' => &[0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00],
            'Z' => &[0x7E, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x7E, 0x00],
            '[' => &[0x3C, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3C, 0x00],
            '\\' => &[0x00, 0x60, 0x30, 0x18, 0x0C, 0x06, 0x00, 0x00],
            ']' => &[0x3C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3C, 0x00],
            '^' => &[0x18, 0x3C, 0x66, 0x00, 0x00, 0x00, 0x00, 0x00],
            '_' => &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF],
            '`' => &[0x30, 0x18, 0x0C, 0x00, 0x00, 0x00, 0x00, 0x00],
            'a' => &[0x00, 0x00, 0x3C, 0x06, 0x3E, 0x66, 0x3E, 0x00],
            'b' => &[0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00],
            'c' => &[0x00, 0x00, 0x3C, 0x66, 0x60, 0x66, 0x3C, 0x00],
            'd' => &[0x06, 0x06, 0x3E, 0x66, 0x66, 0x66, 0x3E, 0x00],
            'e' => &[0x00, 0x00, 0x3C, 0x66, 0x7E, 0x60, 0x3C, 0x00],
            'f' => &[0x1C, 0x36, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x00],
            'g' => &[0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x3C],
            'h' => &[0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
            'i' => &[0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00],
            'j' => &[0x0C, 0x00, 0x1C, 0x0C, 0x0C, 0x0C, 0x6C, 0x38],
            'k' => &[0x60, 0x60, 0x66, 0x6C, 0x78, 0x6C, 0x66, 0x00],
            'l' => &[0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
            'm' => &[0x00, 0x00, 0x66, 0x7F, 0x6B, 0x6B, 0x63, 0x00],
            'n' => &[0x00, 0x00, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
            'o' => &[0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x3C, 0x00],
            'p' => &[0x00, 0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60],
            'q' => &[0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x07],
            'r' => &[0x00, 0x00, 0x6C, 0x76, 0x60, 0x60, 0x60, 0x00],
            's' => &[0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00],
            't' => &[0x30, 0x30, 0x7C, 0x30, 0x30, 0x36, 0x1C, 0x00],
            'u' => &[0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3E, 0x00],
            'v' => &[0x00, 0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
            'w' => &[0x00, 0x00, 0x63, 0x6B, 0x6B, 0x7F, 0x36, 0x00],
            'x' => &[0x00, 0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00],
            'y' => &[0x00, 0x00, 0x66, 0x66, 0x66, 0x3E, 0x06, 0x3C],
            'z' => &[0x00, 0x00, 0x7E, 0x0C, 0x18, 0x30, 0x7E, 0x00],
            '{' => &[0x0E, 0x18, 0x18, 0x70, 0x18, 0x18, 0x0E, 0x00],
            '|' => &[0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
            '}' => &[0x70, 0x18, 0x18, 0x0E, 0x18, 0x18, 0x70, 0x00],
            '~' => &[0x31, 0x6B, 0x46, 0x00, 0x00, 0x00, 0x00, 0x00],
            '─' => &[0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0x00],
            '│' => &[0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18],
            '└' => &[0x00, 0x00, 0x00, 0xFF, 0x18, 0x18, 0x18, 0x18],
            '═' => &[0x00, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0x00, 0x00],
            '🐾' => &[0x6C, 0xFE, 0xFE, 0x7C, 0x38, 0x10, 0x00, 0x00], // Simplified paw print
            _ => &[0x3C, 0x42, 0x99, 0xA1, 0xA1, 0x99, 0x42, 0x3C],    // Unknown char (face)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_height_constants_calculation() {
        // Compute expected values from constants, not hardcoded numbers
        let expected_without_status = MARGIN_TOP + BASE_LINES * CHAR_HEIGHT_PIX;
        let expected_with_status = MARGIN_TOP + (BASE_LINES + STATUS_TEXT_LINES) * CHAR_HEIGHT_PIX;

        assert_eq!(MIN_HEIGHT_WITHOUT_STATUS, expected_without_status);
        assert_eq!(MIN_HEIGHT_WITH_STATUS, expected_with_status);

        assert_eq!(
            MIN_HEIGHT_WITH_STATUS - MIN_HEIGHT_WITHOUT_STATUS,
            STATUS_TEXT_LINES * CHAR_HEIGHT_PIX
        );
    }

    #[test]
    fn test_energy_plot_window_new() {
        use crate::graphics::energy_plot::EnergyData;
        use std::sync::{Arc, Mutex};

        let energy_data = Arc::new(Mutex::new(EnergyData::new()));
        let window = EnergyPlotWindow::new(energy_data, DisplayMode::All);

        assert!(!window.is_open());
        assert!(!window.has_status_text);
        assert_eq!(window.get_display_mode(), DisplayMode::All);
    }

    #[test]
    fn test_split_screen_layout_divides_monitor_into_halves() {
        let layout =
            split_screen_layout(PhysicalPosition::new(100, 50), PhysicalSize::new(1600, 900));

        assert_eq!(layout.simulation.position, PhysicalPosition::new(100, 50));
        assert_eq!(layout.simulation.size, PhysicalSize::new(800, 900));
        assert_eq!(layout.energy.position, PhysicalPosition::new(900, 50));
        assert_eq!(layout.energy.size, PhysicalSize::new(800, 900));
    }

    #[test]
    fn test_split_screen_layout_preserves_full_monitor_width_for_odd_sizes() {
        let layout = split_screen_layout(PhysicalPosition::new(0, 0), PhysicalSize::new(1365, 768));

        assert_eq!(
            layout.simulation.size.width + layout.energy.size.width,
            1365
        );
        assert_eq!(layout.simulation.size.height, 768);
        assert_eq!(layout.energy.size.height, 768);
    }

    #[test]
    fn test_split_screen_layout_stays_within_small_monitor_bounds() {
        let layout =
            split_screen_layout(PhysicalPosition::new(25, 40), PhysicalSize::new(500, 200));

        assert_eq!(layout.simulation.size.width + layout.energy.size.width, 500);
        assert_eq!(layout.simulation.size.height, 200);
        assert_eq!(layout.energy.size.height, 200);
        assert_eq!(
            layout.energy.position.x,
            25 + layout.simulation.size.width as i32
        );
    }
}
