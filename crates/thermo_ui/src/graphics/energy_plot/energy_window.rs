// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The plot window as `thermo_ui` sees it (#371): what to ask the backend
//! for ([`PlotWindowRequest`]), where the window goes on the split screen
//! and its title. The OS window itself (creating, placing, closing it) is
//! the backend's: `thermo_ui` uses winit for input types only.

use super::plot_selection::PlotSelection;
use winit::dpi::{PhysicalPosition, PhysicalSize};

/// Size of the plot window when no monitor is known.
pub const DEFAULT_WIDTH: u32 = 1024;
pub const DEFAULT_HEIGHT: u32 = 576;
/// Smallest size of the plot window, in logical pixels.
pub const MIN_WINDOW_WIDTH: u32 = 320;
pub const MIN_WINDOW_HEIGHT: u32 = 240;
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

/// What the backend is asked to do with the plot window. The backend owns
/// the OS window; it takes the requests after each event
/// (`UnifiedRenderState::take_plot_window_requests`) and reports back with
/// `EnergyWindowManager::plot_window_opened` (or `plot_window_failed`) and
/// the window's own events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlotWindowRequest {
    /// Create the window with `title`, at least [`MIN_WINDOW_WIDTH`] ×
    /// [`MIN_WINDOW_HEIGHT`], placed on the energy half of the main
    /// window's monitor ([`split_screen_layout`]), else
    /// [`DEFAULT_WIDTH`] × [`DEFAULT_HEIGHT`]; the simulation window keeps
    /// its size (#167).
    Open { title: String },
    /// Close the window (and drop what draws into it).
    Close,
    /// Set the window's title.
    SetTitle(String),
    /// Redraw the window (`RedrawRequested`).
    Redraw,
}

pub(super) fn preferred_energy_window_size(available_size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    let width = DEFAULT_WIDTH.min(available_size.width.max(1));
    let height = available_size
        .height
        .saturating_sub(ENERGY_WINDOW_BOTTOM_GAP_PX)
        .max(1);

    PhysicalSize::new(width, height)
}

/// The simulation window on the left half of a monitor at
/// `monitor_position` of `monitor_size`, the plot window right-aligned on
/// the other half, below its top and with room at the bottom.
pub fn split_screen_layout(
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

/// The plot window's title for `selection`.
pub fn window_title(selection: &PlotSelection) -> String {
    format!("Plot - {}", selection.title())
}
