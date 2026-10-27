// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the plot window shows (#339): one to [`MAX_PLOTS`] plots stacked on
//! a shared time axis, each with one or more series of the metrics store,
//! and the focused plot that the keys change.
//!
//! The series are named as in the `MetricsRegistry` (`E_kin`,
//! `wall_momentum`, `N2:count`, …). The selection only holds names: a series
//! that is not (or no longer) in the data is listed but draws nothing. The
//! functions that pick another series take the names on offer
//! (`EnergyData::series_names`).

/// Most plots in the window.
pub const MAX_PLOTS: usize = 4;

/// The series `view.cycle_plot` steps through for the focused plot, in
/// order (the window's display modes before #339).
pub const PRESETS: [&[&str]; 4] = [
    &["E_pot"],
    &["E_kin"],
    &["E_tot"],
    &["E_kin", "E_pot", "E_tot"],
];

/// One plot: its series, in legend order, and which of them are hidden
/// (listed in the legend, not drawn, not in the autoscale).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlotPanel {
    series: Vec<String>,
    hidden: Vec<String>,
}

impl PlotPanel {
    fn new(series: &[&str]) -> Self {
        Self {
            series: series.iter().map(|s| s.to_string()).collect(),
            hidden: Vec::new(),
        }
    }

    pub fn series(&self) -> &[String] {
        &self.series
    }

    pub fn is_hidden(&self, name: &str) -> bool {
        self.hidden.iter().any(|h| h == name)
    }

    /// The series drawn: all but the hidden ones.
    pub fn shown(&self) -> impl Iterator<Item = &str> {
        self.series
            .iter()
            .map(String::as_str)
            .filter(|name| !self.is_hidden(name))
    }

    fn contains(&self, name: &str) -> bool {
        self.series.iter().any(|s| s == name)
    }

    fn set_series(&mut self, series: Vec<String>) {
        self.hidden.retain(|h| series.contains(h));
        self.series = series;
    }
}

/// The plots of the window, top to bottom, and the focused one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlotSelection {
    panels: Vec<PlotPanel>,
    focused: usize,
}

impl Default for PlotSelection {
    /// One plot with the first preset (`E_pot`), as the window always
    /// started before #339.
    fn default() -> Self {
        Self {
            panels: vec![PlotPanel::new(PRESETS[0])],
            focused: 0,
        }
    }
}

/// The entry of `available` `step` places from `current` (wrapping), that
/// `skip` does not reject; the first acceptable one when `current` is not
/// on offer. `None` when no entry is acceptable.
fn step_from<'a>(
    available: &[&'a str],
    current: Option<&str>,
    step: isize,
    skip: impl Fn(&str) -> bool,
) -> Option<&'a str> {
    let n = available.len() as isize;
    if n == 0 {
        return None;
    }
    let start = current.and_then(|c| available.iter().position(|a| *a == c));
    let (origin, step) = match start {
        Some(i) => (i as isize, step),
        None => (if step >= 0 { -1 } else { n }, step.signum()),
    };
    (1..=n)
        .map(|k| available[(origin + step.signum() * k).rem_euclid(n) as usize])
        .find(|name| !skip(name))
}

impl PlotSelection {
    /// A selection of the given plots (tests, and later a saved layout);
    /// empty plots are left out, at most [`MAX_PLOTS`] are kept, and the
    /// default is used when none is left.
    pub fn from_panels(panels: &[&[&str]]) -> Self {
        let panels: Vec<PlotPanel> = panels
            .iter()
            .filter(|series| !series.is_empty())
            .take(MAX_PLOTS)
            .map(|series| PlotPanel::new(series))
            .collect();
        if panels.is_empty() {
            Self::default()
        } else {
            Self { panels, focused: 0 }
        }
    }

    pub fn panels(&self) -> &[PlotPanel] {
        &self.panels
    }

    pub fn focused(&self) -> usize {
        self.focused
    }

    fn focused_panel(&mut self) -> &mut PlotPanel {
        &mut self.panels[self.focused]
    }

    /// A short name for the window title: the series of a single plot (up
    /// to three, else their number), or the number of plots.
    pub fn title(&self) -> String {
        match self.panels.as_slice() {
            [panel] if panel.series.len() <= 3 => panel.series.join(", "),
            [panel] => format!("{} series", panel.series.len()),
            panels => format!("{} plots", panels.len()),
        }
    }

    /// Give the focused plot the next preset ([`PRESETS`]): the one after
    /// the preset it shows, or the first.
    pub fn cycle_preset(&mut self) {
        let panel = self.focused_panel();
        let current = PRESETS.iter().position(|preset| {
            panel
                .series
                .iter()
                .map(String::as_str)
                .eq(preset.iter().copied())
        });
        let next = current.map_or(0, |i| (i + 1) % PRESETS.len());
        panel.set_series(PRESETS[next].iter().map(|s| s.to_string()).collect());
    }

    /// Replace the last series of the focused plot by the next (`step` 1)
    /// or previous (`step` -1) one of `available` that the plot does not
    /// show yet. Returns whether the selection changed.
    pub fn step_series(&mut self, available: &[&str], step: isize) -> bool {
        let panel = self.focused_panel();
        let last = panel.series.last().cloned();
        let Some(next) = step_from(available, last.as_deref(), step, |name| {
            panel.contains(name)
        }) else {
            return false;
        };
        let mut series = panel.series.clone();
        series.pop();
        series.push(next.to_string());
        panel.set_series(series);
        true
    }

    /// Add the next series of `available` after the focused plot's last one
    /// that the plot does not show yet. Returns whether one was added.
    pub fn add_series(&mut self, available: &[&str]) -> bool {
        let panel = self.focused_panel();
        let last = panel.series.last().cloned();
        let Some(next) = step_from(available, last.as_deref(), 1, |name| panel.contains(name))
        else {
            return false;
        };
        panel.series.push(next.to_string());
        true
    }

    /// Remove the last series of the focused plot; a plot keeps at least
    /// one. Returns whether one was removed.
    pub fn remove_series(&mut self) -> bool {
        let panel = self.focused_panel();
        if panel.series.len() <= 1 {
            return false;
        }
        let mut series = panel.series.clone();
        series.pop();
        panel.set_series(series);
        true
    }

    /// Add a plot below the focused one, with the first series of
    /// `available` no plot shows yet (or the first on offer), and focus it.
    /// Returns whether one was added (at most [`MAX_PLOTS`]).
    pub fn add_plot(&mut self, available: &[&str]) -> bool {
        if self.panels.len() >= MAX_PLOTS {
            return false;
        }
        let shown = |name: &str| self.panels.iter().any(|p| p.contains(name));
        let series = available
            .iter()
            .find(|name| !shown(name))
            .or(available.first())
            .copied()
            .unwrap_or(PRESETS[0][0]);
        self.panels
            .insert(self.focused + 1, PlotPanel::new(&[series]));
        self.focused += 1;
        true
    }

    /// Remove the focused plot; the window keeps at least one. The plot
    /// above it (or the new first) gets the focus. Returns whether one was
    /// removed.
    pub fn remove_plot(&mut self) -> bool {
        if self.panels.len() <= 1 {
            return false;
        }
        self.panels.remove(self.focused);
        self.focused = self.focused.saturating_sub(1);
        true
    }

    /// Focus the next plot (wrapping).
    pub fn focus_next(&mut self) {
        self.focused = (self.focused + 1) % self.panels.len();
    }

    /// Focus plot `index`, if there is one. Returns whether the focus
    /// changed.
    pub fn focus(&mut self, index: usize) -> bool {
        let changed = index < self.panels.len() && index != self.focused;
        if changed {
            self.focused = index;
        }
        changed
    }

    /// Hide series `name` of plot `index`, or show it again. Returns
    /// whether the plot has that series.
    pub fn toggle_hidden(&mut self, index: usize, name: &str) -> bool {
        let Some(panel) = self.panels.get_mut(index) else {
            return false;
        };
        if !panel.contains(name) {
            return false;
        }
        if panel.is_hidden(name) {
            panel.hidden.retain(|h| h != name);
        } else {
            panel.hidden.push(name.to_string());
        }
        true
    }
}
