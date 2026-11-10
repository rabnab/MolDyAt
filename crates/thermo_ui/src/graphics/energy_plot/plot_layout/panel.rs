// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! One plot inside its area: grid, ticks, axes and curves as segments.

use super::curve::{Curve, Series, TimeWindow, downsample, interpolate};
use super::ticks::{
    MIN_X_TICK_CELLS, X_TICK_INTERVALS, XAxis, format_tick, is_multiple, multiples_in,
};
use super::{COLOR_AXIS, COLOR_GRID, COLOR_TEXT, COLOR_ZERO, Layout, PlotRect, Rgba, TICK_CELLS};

/// One plot while it is laid out: its series, their points, its value
/// range and y ticks.
pub(super) struct PanelPlan {
    pub(super) index: usize,
    pub(super) series: Vec<Series>,
    pub(super) curves: Vec<Curve>,
    pub(super) range: (f64, f64),
    pub(super) ticks: (f64, Vec<f64>),
    pub(super) labels: Vec<String>,
}

impl PanelPlan {
    /// The curves of the series that are not hidden.
    pub(super) fn shown_curves(&self) -> impl Iterator<Item = Curve> + '_ {
        self.series
            .iter()
            .zip(&self.curves)
            .filter(|(s, _)| !s.hidden)
            .map(|(_, curve)| curve.clone())
    }
}

impl Layout<'_> {
    /// Grid, y ticks, zero line, axes and curves of one plot in `area`; time
    /// labels only with `time_labels` (the last plot).
    pub(super) fn panel(
        &mut self,
        plan: &PanelPlan,
        window: TimeWindow,
        area: PlotRect,
        time_labels: bool,
    ) {
        let (glyph, thin) = (self.glyph, self.thin);
        let tick = glyph * TICK_CELLS;
        let (lo, hi) = plan.range;
        let y_of = |v: f64| area.bottom - ((v - lo) / (hi - lo)) as f32 * area.height();

        // Horizontal grid lines with their labels.
        for (&v, label) in plan.ticks.1.iter().zip(&plan.labels) {
            let y = self.snap(y_of(v));
            self.segment([area.left, y], [area.right, y], thin, COLOR_GRID);
            self.segment([area.left - tick, y], [area.left, y], thin, COLOR_AXIS);
            let x = area.left - tick - glyph * 0.25 - self.text_width(label);
            self.label(label.clone(), [x, y - glyph * 0.5], COLOR_TEXT);
        }
        if lo < 0.0 && hi > 0.0 {
            let y = self.snap(y_of(0.0));
            self.segment([area.left, y], [area.right, y], thin, COLOR_ZERO);
        }

        let x_axis = XAxis::new(window, area);
        self.x_ticks(&x_axis, area, time_labels);

        // Axes over the grid, curves over the axes.
        let (left, bottom) = (self.snap(area.left), self.snap(area.bottom));
        self.segment([left, area.top], [left, bottom], thin, COLOR_AXIS);
        self.segment([left, bottom], [area.right, bottom], thin, COLOR_AXIS);
        let to_px = |(t, v): (f64, f64)| [x_axis.x_of(t), y_of(v)];
        let columns = area.width().ceil() as usize;
        for (s, curve) in plan.series.iter().zip(&plan.curves) {
            if s.hidden {
                continue;
            }
            let curve = downsample(curve, window, columns);
            self.curve_segments(&curve, window, &to_px, thin * 2.0, s.color);
        }
    }

    /// Tick marks and time labels below the x axis: ticks every 0.5 time
    /// units when they are far enough apart (else every 1, 2, 5, …), labels
    /// on the smallest multiple of that at which they do not overlap.
    fn x_ticks(&mut self, x_axis: &XAxis, area: PlotRect, labels: bool) {
        let (glyph, thin) = (self.glyph, self.thin);
        let tick = glyph * TICK_CELLS;
        let scale = x_axis.scale();
        let last = X_TICK_INTERVALS[X_TICK_INTERVALS.len() - 1];
        let interval = X_TICK_INTERVALS
            .into_iter()
            .find(|&i| i as f32 * scale >= glyph * MIN_X_TICK_CELLS)
            .unwrap_or(last);
        let ticks = multiples_in(interval, x_axis.window);
        let widest = ticks
            .iter()
            .map(|&t| self.text_width(&format_tick(t, interval)))
            .fold(0.0, f32::max);
        let label_interval = X_TICK_INTERVALS
            .into_iter()
            .filter(|&i| i >= interval && is_multiple(i, interval))
            .find(|&i| i as f32 * scale >= widest + glyph)
            .unwrap_or(last);
        for t in ticks {
            let x = self.snap(x_axis.x_of(t));
            self.segment([x, area.bottom], [x, area.bottom + tick], thin, COLOR_AXIS);
            if !labels || !is_multiple(t, label_interval) {
                continue;
            }
            let label = format_tick(t, label_interval);
            // Centred under its tick, but moved in at the window edges.
            let label_w = self.text_width(&label);
            let max_x = (self.scene.size.0 as f32 - self.pad - label_w).max(self.pad);
            let label_x = (x - label_w * 0.5).clamp(self.pad, max_x);
            self.label(
                label,
                [label_x, area.bottom + tick + glyph * 0.25],
                COLOR_TEXT,
            );
        }
    }

    /// Connect consecutive points of `curve` (window pixels by `to_px`),
    /// cut at the edges of `window`: a segment that crosses an edge ends
    /// there, one outside is left out. A missing point breaks the curve,
    /// and a lone point inside the window is drawn as a dot.
    fn curve_segments(
        &mut self,
        curve: &[Option<(f64, f64)>],
        window: TimeWindow,
        to_px: &impl Fn((f64, f64)) -> [f32; 2],
        width: f32,
        color: Rgba,
    ) {
        let TimeWindow { t0, t1 } = window;
        for (i, point) in curve.iter().enumerate() {
            let Some(p) = *point else { continue };
            let prev = i.checked_sub(1).and_then(|j| curve[j]);
            match curve.get(i + 1).copied().flatten() {
                // Not left of t0 nor right of t1; a segment that only
                // touches an edge would be cut to nothing.
                Some(q) if (q.0 > t0 || p.0 >= t0) && (p.0 < t1 || q.0 <= t1) => {
                    // Cut at the edges; `value_range` includes these values.
                    let from = if p.0 < t0 && q.0 > p.0 {
                        (t0, interpolate(p, q, t0))
                    } else {
                        p
                    };
                    let to = if q.0 > t1 && q.0 > p.0 {
                        (t1, interpolate(p, q, t1))
                    } else {
                        q
                    };
                    self.segment(to_px(from), to_px(to), width, color);
                }
                None if prev.is_none() && window.contains(p.0) => {
                    self.segment(to_px(p), to_px(p), width, color);
                }
                _ => {}
            }
        }
    }
}
