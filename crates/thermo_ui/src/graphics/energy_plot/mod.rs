// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod energy_data;
pub mod energy_window;
pub mod plot_layout;
pub mod time_view;

#[cfg(test)]
mod tests;

pub use energy_data::{DisplayMode, EnergyData, SharedEnergyData};
pub use energy_window::{
    EnergyPlotWindow, SplitScreenLayout, WindowGeometry, split_screen_layout_for_monitor,
};
pub use plot_layout::{PlotLabel, PlotRect, PlotScene, PlotSegment};
pub use time_view::{TimeHistory, TimeView};
