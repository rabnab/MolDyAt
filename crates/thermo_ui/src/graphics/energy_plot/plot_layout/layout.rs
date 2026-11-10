// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The page: title, key hints, the plots stacked on a shared time axis with
//! their legends, the info line and the status text.

use super::curve::{Curve, TimeWindow, curves, history_of, panel_series, value_range};
use super::panel::PanelPlan;
use super::ticks::{fitting_hints, format_tick, nice_ticks};
use super::{
    COLOR_HIDDEN, COLOR_STATUS, COLOR_TEXT, COLOR_TITLE, Layout, LegendEntry, PanelArea, PlotRect,
    TICK_CELLS,
};
use crate::graphics::energy_plot::plot_selection::PlotSelection;

impl Layout<'_> {
    pub(super) fn build(&mut self, selection: &PlotSelection) {
        let (w, h) = (self.scene.size.0 as f32, self.scene.size.1 as f32);
        let (pad, line) = (self.pad, self.line);

        let title_w = self.label(
            format!("Plot - {}", selection.title()),
            [pad, pad],
            COLOR_TITLE,
        );
        // The keys at the right end of the title line: as many of the
        // hints (two spaces apart, most important first) as fit.
        let room = w - pad - (pad + title_w + self.glyph * 2.0);
        let hints = fitting_hints(self.key_hints, |text| self.text_width(text) <= room);
        if !hints.is_empty() {
            let keys_x = w - pad - self.text_width(&hints);
            self.label(hints, [keys_x, pad], COLOR_TEXT);
        }
        let top = pad + line + pad * 0.5;

        // Status lines at the very bottom, the info line above them.
        let status: Vec<String> = self
            .data
            .status_text
            .as_deref()
            .map(|text| text.lines().map(str::to_string).collect())
            .unwrap_or_default();
        let mut bottom = h - pad - status.len() as f32 * line;
        for (i, text) in status.into_iter().enumerate() {
            self.label(text, [pad, bottom + i as f32 * line], COLOR_STATUS);
        }
        if bottom < h - pad {
            bottom -= pad * 0.5;
        }
        bottom -= line;

        let mut plans: Vec<PanelPlan> = selection
            .panels()
            .iter()
            .enumerate()
            .map(|(index, panel)| {
                let series = panel_series(self.data, panel);
                let curves = curves(self.data, &series);
                PanelPlan {
                    index,
                    series,
                    curves,
                    range: (-1.0, 1.0),
                    ticks: (0.0, Vec::new()),
                    labels: Vec::new(),
                }
            })
            .collect();
        let shown: Vec<Curve> = plans.iter().flat_map(PanelPlan::shown_curves).collect();
        let history = history_of(&shown);
        self.scene.history = history;
        let window = history.map(|history| {
            let (t0, t1) = self.view.window(&history);
            TimeWindow { t0, t1 }
        });
        self.info_line(window, [pad, bottom]);
        bottom -= pad * 0.5;

        let Some(window) = window else {
            self.waiting_message(top, bottom, w);
            return;
        };
        self.plots(
            &mut plans,
            selection.focused(),
            window,
            PlotRect {
                left: pad,
                top,
                right: w - pad,
                bottom,
            },
        );
    }

    /// The line under the plots: the time (not while paused), the x axis,
    /// and the range shown when the view was zoomed or panned. The x axis
    /// note is left out when the line would not fit the window.
    fn info_line(&mut self, window: Option<TimeWindow>, origin: [f32; 2]) {
        // A paused plot shows a frozen history: the current time is not
        // part of it (its range is shown instead).
        let time = if self.view.is_following() {
            format!("t = {:.2}", self.data.time)
        } else {
            String::new()
        };
        let axis = "   x: time";
        let view = window
            .filter(|_| !self.view.is_default())
            .map(|window| {
                let state = if self.view.is_following() {
                    "zoomed"
                } else {
                    "paused"
                };
                format!("   {state} {:.1}-{:.1}", window.t0, window.t1)
            })
            .unwrap_or_default();
        let available = self.scene.size.0 as f32 - self.pad - origin[0];
        let full = format!("{time}{axis}{view}");
        let text = if self.text_width(&full) <= available {
            full
        } else {
            format!("{time}{view}")
        };
        self.label(text.trim_start().to_string(), origin, COLOR_TEXT);
    }

    /// One plot's legend line at `origin`: a focus mark when there are
    /// several plots, each series in its colour (a hidden one grey), then
    /// the unit when all its series share one. Series that do not fit the
    /// window's width are counted (`+N`) instead. Records where each series
    /// is named.
    fn legend(&mut self, plan: &PanelPlan, focused: Option<bool>, origin: [f32; 2]) {
        let mut x = origin[0];
        if let Some(focused) = focused {
            let mark = if focused { "> " } else { "  " };
            x += self.label(mark, [x, origin[1]], COLOR_TITLE);
        }
        let right = self.scene.size.0 as f32 - self.pad;
        for (k, s) in plan.series.iter().enumerate() {
            // Entries that would run past the window are counted instead.
            let rest = plan.series.len() - k;
            let more = format!("+{}", rest - 1);
            let needed = self.text_width(&s.name)
                + if rest > 1 {
                    self.glyph + self.text_width(&more)
                } else {
                    0.0
                };
            if x + needed > right {
                self.label(format!("+{rest}"), [x, origin[1]], COLOR_TEXT);
                return;
            }
            let color = if s.hidden { COLOR_HIDDEN } else { s.color };
            let width = self.label(s.name.clone(), [x, origin[1]], color);
            self.scene.legend.push(LegendEntry {
                panel: plan.index,
                series: s.name.clone(),
                rect: PlotRect {
                    left: x,
                    top: origin[1],
                    right: x + width,
                    bottom: origin[1] + self.glyph,
                },
            });
            x += width + self.glyph;
        }
        let mut units = plan.series.iter().map(|s| self.data.unit(&s.name));
        if let Some(Some(unit)) = units.next()
            && units.all(|u| u == Some(unit))
        {
            self.label(format!("[{unit}]"), [x, origin[1]], COLOR_TEXT);
        }
    }

    fn waiting_message(&mut self, top: f32, bottom: f32, width: f32) {
        let lines = [
            "Waiting for simulation data...",
            "Add some balls to see the plot",
        ];
        let start = (top + bottom) * 0.5 - self.line;
        for (i, text) in lines.into_iter().enumerate() {
            let x = ((width - self.text_width(text)) * 0.5).max(self.pad);
            self.label(text, [x, start + i as f32 * self.line], COLOR_TEXT);
        }
    }

    /// The plots of `plans` stacked inside `free` (the space between title
    /// and info line), each with its legend line above it, all over the
    /// times of `window` with the same left and right edges; the time
    /// labels go under the last one. Each plot's y axis fits what it shows.
    fn plots(
        &mut self,
        plans: &mut [PanelPlan],
        focused: usize,
        window: TimeWindow,
        free: PlotRect,
    ) {
        let (glyph, line) = (self.glyph, self.line);
        let tick = glyph * TICK_CELLS;
        let n = plans.len() as f32;
        // Room for the time labels under the last plot.
        let x_labels = line + tick;
        let panel_h = (free.height() - x_labels) / n;
        // Each plot: its legend line, then the area (half a glyph above it
        // so the top tick label fits), then a gap to the next legend.
        let area_h = panel_h - line - glyph;
        if area_h < glyph * 2.0 {
            return;
        }
        // A tick label every two lines at most: a short plot (one of
        // several) still gets a few.
        let max_y_ticks = (area_h / (line * 2.0)).clamp(2.0, 10.0) as usize;
        for plan in plans.iter_mut() {
            let shown: Vec<Curve> = plan.shown_curves().collect();
            // A range panned or zoomed where no curve is: an empty plot
            // with its axes, around zero.
            plan.range = value_range(&shown, window).unwrap_or((-1.0, 1.0));
            plan.ticks = nice_ticks(plan.range.0, plan.range.1, max_y_ticks);
            plan.labels = plan
                .ticks
                .1
                .iter()
                .map(|&v| format_tick(v, plan.ticks.0))
                .collect();
        }
        // One left edge for all plots, so their times line up.
        let label_w = plans
            .iter()
            .flat_map(|plan| &plan.labels)
            .map(|label| self.text_width(label))
            .fold(0.0, f32::max);
        let (left, right) = (free.left + label_w + tick + glyph * 0.5, free.right - glyph);
        if right - left < glyph * 4.0 {
            return;
        }
        self.scene.time_window = Some((window.t0, window.t1));
        let several = plans.len() > 1;
        let last = plans.len() - 1;
        for (k, plan) in plans.iter().enumerate() {
            let y0 = free.top + k as f32 * panel_h;
            self.legend(
                plan,
                several.then_some(plan.index == focused),
                [free.left, y0],
            );
            let area = PlotRect {
                left,
                top: y0 + line + glyph * 0.5,
                right,
                bottom: y0 + line + glyph * 0.5 + area_h,
            };
            self.scene.panels.push(PanelArea {
                index: plan.index,
                area,
            });
            self.panel(plan, window, area, k == last);
        }
        let first = self.scene.panels[0].area;
        let bottom = self.scene.panels[last].area.bottom;
        self.scene.plot_area = Some(PlotRect { bottom, ..first });
    }
}
