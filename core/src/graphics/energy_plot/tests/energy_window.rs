// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::energy_plot::DisplayMode;
use crate::graphics::energy_plot::energy_window::*;
use winit::dpi::{PhysicalPosition, PhysicalSize};

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
    let layout = split_screen_layout(PhysicalPosition::new(100, 50), PhysicalSize::new(1600, 900));

    assert_eq!(layout.simulation.position, PhysicalPosition::new(100, 50));
    assert_eq!(layout.simulation.size, PhysicalSize::new(800, 900));
    assert_eq!(layout.energy.position, PhysicalPosition::new(900, 50));
    assert_eq!(layout.energy.size, PhysicalSize::new(800, 800));
}

#[test]
fn test_split_screen_layout_right_aligns_energy_window_for_odd_sizes() {
    let layout = split_screen_layout(PhysicalPosition::new(0, 0), PhysicalSize::new(1365, 768));

    assert_eq!(layout.simulation.size.height, 768);
    assert_eq!(layout.energy.size, PhysicalSize::new(683, 668));
    assert_eq!(layout.energy.position, PhysicalPosition::new(682, 0));
}

#[test]
fn test_split_screen_layout_stays_within_small_monitor_bounds() {
    let layout = split_screen_layout(PhysicalPosition::new(25, 40), PhysicalSize::new(500, 200));

    assert_eq!(layout.simulation.size.height, 200);
    assert_eq!(layout.energy.size, PhysicalSize::new(250, 100));
    assert_eq!(layout.energy.position, PhysicalPosition::new(275, 40));
}

#[test]
fn preferred_energy_window_size_caps_height_below_monitor_height() {
    let size = preferred_energy_window_size(PhysicalSize::new(900, 1080));

    assert_eq!(size.width, 900);
    assert_eq!(size.height, 980);
}

#[test]
fn preferred_energy_window_size_preserves_small_monitor_visibility() {
    let size = preferred_energy_window_size(PhysicalSize::new(280, 300));

    assert_eq!(size.width, 280);
    assert_eq!(size.height, 200);
}
