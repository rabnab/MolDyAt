// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::energy_data::{DisplayMode, SharedEnergyData};
use super::energy_renderer::EnergyPlotRenderer;
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::{Window, WindowId};

pub(super) const DEFAULT_WIDTH: u32 = 1024; // Reduced by 20% from 1280
pub(super) const DEFAULT_HEIGHT: u32 = 576; // Reduced by 20% from 720
pub(super) const MIN_WINDOW_WIDTH: u32 = 320;
pub(super) const MIN_WINDOW_HEIGHT: u32 = 240;
/// Keep the energy window anchored to the top while leaving room at the bottom.
pub(super) const ENERGY_WINDOW_BOTTOM_GAP_PX: u32 = 100;

// Text rendering constants
pub(super) const CHAR_WIDTH_PIX: u32 = 13; // pixel width per character (realistic value)
pub(super) const CHAR_HEIGHT_PIX: u32 = 19; // pixel height per character (realistic value)
pub(super) const MARGIN_LEFT: u32 = 10;
pub(super) const MARGIN_TOP: u32 = 8;
pub(super) const MARGIN_RIGHT: u32 = 10;
pub(super) const MARGIN_BOTTOM: u32 = 10;

// Base line count without status text
pub(super) const BASE_LINES: u32 = 41;
pub(super) const STATUS_TEXT_LINES: u32 = 10;

pub(super) const MIN_HEIGHT_WITHOUT_STATUS: u32 = MARGIN_TOP + BASE_LINES * CHAR_HEIGHT_PIX;
pub(super) const MIN_HEIGHT_WITH_STATUS: u32 =
    MARGIN_TOP + (BASE_LINES + STATUS_TEXT_LINES) * CHAR_HEIGHT_PIX;

#[derive(Debug, Clone, Copy)]
pub(super) struct TextMetrics {
    pub(super) char_width: u32,
    pub(super) char_height: u32,
    pub(super) margin_left: u32,
    pub(super) margin_top: u32,
    pub(super) margin_right: u32,
    pub(super) margin_bottom: u32,
}

pub(super) fn text_metrics(scale_factor: f64) -> TextMetrics {
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

pub(super) fn preferred_energy_window_size(available_size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    let width = DEFAULT_WIDTH.min(available_size.width.max(1));
    let height = available_size
        .height
        .saturating_sub(ENERGY_WINDOW_BOTTOM_GAP_PX)
        .max(1);

    PhysicalSize::new(width, height)
}

pub(super) fn split_screen_layout(
    monitor_position: PhysicalPosition<i32>,
    monitor_size: PhysicalSize<u32>,
) -> SplitScreenLayout {
    let simulation_width = (monitor_size.width / 2).max(1);
    let energy_size = preferred_energy_window_size(PhysicalSize::new(
        (monitor_size.width - simulation_width).max(1),
        monitor_size.height.max(1),
    ));
    let energy_x = monitor_position.x + monitor_size.width as i32 - energy_size.width as i32;

    SplitScreenLayout {
        simulation: WindowGeometry {
            position: monitor_position,
            size: PhysicalSize::new(simulation_width, monitor_size.height.max(1)),
        },
        energy: WindowGeometry {
            position: PhysicalPosition::new(energy_x, monitor_position.y),
            size: energy_size,
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
    pub(super) has_status_text: bool,
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
        // Update window title
        if let Some(window) = &self.window {
            window.set_title(&format!("Energy Plot 🐾 — {}", mode.as_str()));
        }
    }

    pub fn cycle_display_mode(&mut self) {
        self.display_mode = self.display_mode.next();
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

            // If data version, status-text presence, window size, and display
            // mode are all unchanged, the pixel buffer already reflects the
            // current state — skip re-rasterising.
            let cache_valid = self.cached_data_version == Some(current_version)
                && self.cached_size == Some(current_size)
                && self.cached_display_mode == Some(self.display_mode)
                && self.has_status_text == current_has_status;

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
}
