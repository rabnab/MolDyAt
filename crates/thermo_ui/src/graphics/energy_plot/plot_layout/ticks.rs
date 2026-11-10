// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Axis arithmetic: round tick values and their labels, the map from time to
//! pixel column, and the key hints that fit a line. Pure functions.

use super::PlotRect;
use super::curve::TimeWindow;

/// Distances between x ticks, in time units; the smallest whose ticks are at
/// least [`MIN_X_TICK_CELLS`] apart is used, and labels go on the smallest
/// multiple of it they fit at.
pub(super) const X_TICK_INTERVALS: [f64; 10] =
    [0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0];

/// Smallest distance between two x ticks, in glyph cells.
pub(super) const MIN_X_TICK_CELLS: f32 = 2.0;

/// Ticks at round values (1, 2 or 5 × 10ⁿ apart) inside `[lo, hi]`, at most
/// about `max_ticks` of them, and their spacing. A range without width gives
/// one tick at `lo`. The count is bounded even where the spacing is lost in
/// the precision of `lo` (huge values).
pub fn nice_ticks(lo: f64, hi: f64, max_ticks: usize) -> (f64, Vec<f64>) {
    if hi.partial_cmp(&lo) != Some(std::cmp::Ordering::Greater)
        || !lo.is_finite()
        || !hi.is_finite()
    {
        return (0.0, vec![lo]);
    }
    let raw = (hi - lo) / max_ticks.max(1) as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let nice = match raw / magnitude {
        n if n <= 1.0 => 1.0,
        n if n <= 2.0 => 2.0,
        n if n <= 5.0 => 5.0,
        _ => 10.0,
    };
    let step = nice * magnitude;
    let first = (lo / step).ceil();
    let ticks = (0..)
        .map(|k| (first + k as f64) * step)
        .take_while(|&v| v <= hi + step * 1e-9)
        .take(max_ticks.max(1) * 2 + 2)
        .collect();
    (step, ticks)
}

/// The label of tick `value` on an axis with ticks `step` apart: as many
/// decimals as the step needs, scientific notation for very large or small
/// steps.
pub fn format_tick(value: f64, step: f64) -> String {
    if step > 0.0 && value.abs() < step * 1e-6 {
        return "0".to_string();
    }
    if step >= 1e5 || (step > 0.0 && step < 1e-3) {
        return format!("{value:.2e}");
    }
    let decimals = if step > 0.0 {
        (-step.log10().floor()).max(0.0) as usize
    } else {
        2
    };
    format!("{value:.decimals$}")
}

/// Maps the times of a [`TimeWindow`] onto the width of the plot area.
pub(super) struct XAxis {
    pub(super) window: TimeWindow,
    left: f32,
    width: f32,
}

impl XAxis {
    pub(super) fn new(window: TimeWindow, area: PlotRect) -> Self {
        Self {
            window,
            left: area.left,
            width: area.width(),
        }
    }

    /// Window pixels per time unit.
    pub(super) fn scale(&self) -> f32 {
        self.width / (self.window.t1 - self.window.t0) as f32
    }

    /// The pixel column of time `t`.
    pub(super) fn x_of(&self, t: f64) -> f32 {
        self.left + (t - self.window.t0) as f32 * self.scale()
    }
}

/// Whether `v` is a whole multiple of `step`.
pub(super) fn is_multiple(v: f64, step: f64) -> bool {
    let r = v / step;
    (r - r.round()).abs() < 1e-6
}

/// The multiples of `step` inside `window`, at most a thousand.
pub(super) fn multiples_in(step: f64, window: TimeWindow) -> Vec<f64> {
    let first = (window.t0 / step - 1e-9).ceil();
    (0..1000)
        .map(|k| (first + f64::from(k)) * step)
        .take_while(|&t| t <= window.t1 + step * 1e-9)
        .collect()
}

/// The longest leading run of `hints` (separated by two spaces) that `fits`;
/// empty when not even the first one does.
pub(super) fn fitting_hints(hints: &str, fits: impl Fn(&str) -> bool) -> String {
    let mut shown = String::new();
    for hint in hints.split("  ").filter(|h| !h.is_empty()) {
        let longer = if shown.is_empty() {
            hint.to_string()
        } else {
            format!("{shown}  {hint}")
        };
        if !fits(&longer) {
            break;
        }
        shown = longer;
    }
    shown
}
