// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod ascii_plot;
pub mod energy_data;
pub mod energy_renderer;
pub mod energy_window;

#[cfg(test)]
mod tests;

pub use energy_data::{DisplayMode, EnergyData, SharedEnergyData};
pub use energy_window::{
    EnergyPlotWindow, SplitScreenLayout, WindowGeometry, split_screen_layout_for_monitor,
};
