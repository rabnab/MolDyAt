// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the energy window shows: the plots of the selection and, instead of
//! them, the distributions; changed by plot commands and left clicks.

use super::EnergyWindowManager;
use super::gestures::ClickTargets;
use crate::graphics::energy_plot::{
    DISTRIBUTIONS_TITLE, DistributionData, PlotSelection, PlotWindowRequest, window_title,
};
use crate::graphics::renderer::event_loop::command_registry::PlotChange;
use std::time::{Duration, Instant};
use thermo_runtime::application::renderer_snapshot::RendererSnapshot;
use thermo_runtime::application::scene_layers::SceneLayers;
use thermo_runtime::distributions::distributions;

/// Least time between two distribution computations: g(r) costs a pass over
/// the neighbours of every ball, so it follows the snapshots at about 2 Hz.
const DISTRIBUTION_INTERVAL: Duration = Duration::from_millis(500);

impl EnergyWindowManager {
    /// g(r) pairs hidden in the distributions view.
    pub fn hidden_pairs(&self) -> &[String] {
        &self.hidden_pairs
    }

    /// Whether the window shows the distributions.
    pub fn shows_distributions(&self) -> bool {
        self.show_distributions
    }

    /// Switch the window between the time series and the distributions;
    /// a closed window opens with the distributions.
    pub fn toggle_distributions(&mut self) {
        // The click targets belong to the view just left; until the next
        // frame a click must hit nothing rather than the other view's legend.
        self.targets = ClickTargets::default();
        if self.window.is_none() {
            self.show_distributions = true;
            self.open();
            return;
        }
        self.show_distributions = !self.show_distributions;
        self.requests
            .push(PlotWindowRequest::SetTitle(self.title()));
        self.redraw_now();
    }

    /// Switch g(r) between the mean and the local density in the
    /// distributions view; the view follows with the next snapshot.
    pub fn toggle_rdf_local(&mut self) {
        if self.show_distributions {
            self.rdf_local = !self.rdf_local;
        }
    }

    /// The window title for what it shows.
    pub(super) fn title(&self) -> String {
        if self.show_distributions {
            DISTRIBUTIONS_TITLE.to_string()
        } else {
            window_title(&self.selection)
        }
    }

    /// The distributions of `snapshot`, when the window shows them, the
    /// snapshot has its balls and the last ones are older than
    /// [`DISTRIBUTION_INTERVAL`]. Computed here, off the physics thread.
    pub(super) fn update_distributions(&mut self, snapshot: &RendererSnapshot) {
        let due = self.distributions_local != self.rdf_local
            || self
                .distributions
                .as_ref()
                .is_none_or(|(_, at)| at.elapsed() >= DISTRIBUTION_INTERVAL);
        if !self.show_distributions
            || self.window.is_none()
            || !due
            || !snapshot.frame.layers.contains(SceneLayers::BALLS)
        {
            return;
        }
        let distributions = distributions(snapshot, self.rdf_local);
        self.distributions_local = self.rdf_local;
        let table = &snapshot.balls.species;
        let name = |id| table.get(id).map_or("?", |s| s.name.as_str());
        let pair_names = distributions
            .radial
            .pairs
            .iter()
            .map(|p| format!("{}-{}", name(p.species.0), name(p.species.1)))
            .collect();
        self.distributions = Some((
            DistributionData {
                distributions,
                pair_names,
            },
            Instant::now(),
        ));
    }

    /// The plots shown (also while the window is closed).
    pub fn selection(&self) -> &PlotSelection {
        &self.selection
    }

    /// Change the plots as `change` says (a plot command); with the window
    /// open it redraws now. Returns whether the plots changed.
    pub fn change_plots(&mut self, change: PlotChange) -> bool {
        self.edit_selection(|selection, available| match change {
            PlotChange::CyclePreset => {
                selection.cycle_preset();
                true
            }
            PlotChange::NextSeries => selection.step_series(available, 1),
            PlotChange::PreviousSeries => selection.step_series(available, -1),
            PlotChange::AddSeries => selection.add_series(available),
            PlotChange::RemoveSeries => selection.remove_series(),
            PlotChange::AddPlot => selection.add_plot(available),
            PlotChange::RemovePlot => selection.remove_plot(),
            PlotChange::FocusNext => {
                selection.focus_next();
                selection.panels().len() > 1
            }
        })
    }

    /// A left click at `at` (window pixels) in the last frame: on a legend
    /// entry it hides or shows that series (in the distributions view that
    /// g(r) pair), in a plot it focuses the plot. Returns whether anything
    /// changed.
    pub(in super::super) fn left_clicked(&mut self, at: [f32; 2]) -> bool {
        let entry = self
            .targets
            .legend
            .iter()
            .find(|entry| entry.rect.contains(at))
            .cloned();
        if self.show_distributions {
            let Some(entry) = entry else {
                return false;
            };
            if let Some(i) = self.hidden_pairs.iter().position(|p| *p == entry.series) {
                self.hidden_pairs.remove(i);
            } else {
                self.hidden_pairs.push(entry.series);
            }
            if self.window.is_some() {
                self.redraw_now();
            }
            return true;
        }
        if let Some(entry) = entry {
            return self.edit_selection(|selection, _| {
                selection.focus(entry.panel);
                selection.toggle_hidden(entry.panel, &entry.series)
            });
        }
        let Some(panel) = self.targets.panels.iter().find(|p| p.area.contains(at)) else {
            return false;
        };
        let index = panel.index;
        self.edit_selection(|selection, _| selection.focus(index))
    }

    fn edit_selection(&mut self, change: impl FnOnce(&mut PlotSelection, &[&str]) -> bool) -> bool {
        let changed = {
            let data = self.energy_data.lock().unwrap_or_else(|e| e.into_inner());
            change(&mut self.selection, &data.series_names())
        };
        if changed && self.window.is_some() {
            self.requests
                .push(PlotWindowRequest::SetTitle(self.title()));
            self.redraw_now();
        }
        changed
    }
}
