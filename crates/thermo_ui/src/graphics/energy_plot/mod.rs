// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod energy_data;
pub mod energy_window;
pub mod plot_layout;
pub mod plot_selection;
pub mod status_text;
pub mod time_view;

#[cfg(test)]
mod tests;

pub use energy_data::{EnergyData, SharedEnergyData};
pub use energy_window::{
    DEFAULT_HEIGHT, DEFAULT_WIDTH, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, PlotWindowRequest,
    SplitScreenLayout, WindowGeometry, split_screen_layout, window_title,
};
pub use plot_layout::{LegendEntry, PanelArea, PlotLabel, PlotRect, PlotScene, PlotSegment};
pub use plot_selection::{MAX_PLOTS, PRESETS, PlotPanel, PlotSelection};
pub use time_view::{TimeHistory, TimeView};
