// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::energy_data::{DisplayMode, SharedEnergyData};
use super::plot_layout::PlotScene;
use super::time_view::TimeView;
use std::sync::Arc;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::{Window, WindowId};

pub(super) const DEFAULT_WIDTH: u32 = 1024;
pub(super) const DEFAULT_HEIGHT: u32 = 576;
pub(super) const MIN_WINDOW_WIDTH: u32 = 320;
pub(super) const MIN_WINDOW_HEIGHT: u32 = 240;
/// Keep the energy window anchored to the top while leaving room at the bottom.
pub(super) const ENERGY_WINDOW_BOTTOM_GAP_PX: u32 = 100;

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

/// Energy plot window: a separate OS window that shows the energy history.
///
/// This type owns the window and what it shows (data and display mode); it
/// draws nothing itself. The backend renders [`Self::scene`] into a surface
/// of its own for [`Self::window_handle`] (the wgpu binary: a surface on the
/// main window's device).
pub struct EnergyPlotWindow {
    window: Option<Arc<Window>>,
    energy_data: SharedEnergyData,
    display_mode: DisplayMode,
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
            energy_data,
            display_mode,
        }
    }

    /// Create the OS window, at `geometry` (the energy half of the split
    /// screen) or with the default size.
    pub fn create(
        &mut self,
        window_target: &ActiveEventLoop,
        geometry: Option<WindowGeometry>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let initial_size = geometry
            .map(|geometry| geometry.size)
            .unwrap_or(PhysicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT));

        let mut attrs = Window::default_attributes()
            .with_title(window_title(self.display_mode))
            .with_inner_size(initial_size)
            .with_resizable(true)
            .with_min_inner_size(LogicalSize::new(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT));
        if let Some(geometry) = geometry {
            attrs = attrs.with_position(geometry.position);
        }
        self.window = Some(Arc::new(window_target.create_window(attrs)?));
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

    /// The OS window, for the backend to create its surface from. A backend
    /// that keeps it must let go of it once [`Self::window_id`] changes or
    /// the window closes, or the window stays on screen.
    pub fn window_handle(&self) -> Option<&Arc<Window>> {
        self.window.as_ref()
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
        if let Some(window) = &self.window {
            window.set_title(&window_title(mode));
        }
    }

    pub fn cycle_display_mode(&mut self) {
        self.set_display_mode(self.display_mode.next());
    }

    /// What the window shows now, laid out for its current size with
    /// `key_hints` on the title line and the time range of `view`; `None`
    /// while it is closed or minimised.
    pub fn scene(&self, key_hints: &str, view: &TimeView) -> Option<PlotScene> {
        let window = self.window.as_ref()?;
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return None;
        }
        let data = self.energy_data.lock().unwrap_or_else(|e| e.into_inner());
        Some(PlotScene::build(
            &data,
            self.display_mode,
            (size.width, size.height),
            window.scale_factor(),
            key_hints,
            view,
        ))
    }
}

pub(super) fn window_title(mode: DisplayMode) -> String {
    format!("Energy Plot - {}", mode.as_str())
}
