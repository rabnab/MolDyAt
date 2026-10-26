// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The energy plot as a scene of line segments and text labels in window
//! pixels, without a GPU: the backend only draws what [`PlotScene::build`]
//! lays out (the wgpu binary's plot pipeline), so the layout is tested here.
//!
//! Pixel coordinates have their origin at the top-left corner of the window,
//! with y pointing down. Text is drawn with the backend's 8×8 bitmap font:
//! a label covers `chars × GLYPH_CELL × scale` by `GLYPH_CELL × scale` pixels,
//! from a whole-pixel origin so the glyphs stay crisp.
//!
//! The x axis is the simulation time of the samples, not their step, and it
//! always shows [`TIME_SPAN`] time units: the time step changes only how fast
//! the curves scroll, never the scale. Until the history spans that much,
//! the curves start at the left edge; then the newest sample stays at the
//! right edge. A history shorter than the span (the registry keeps a fixed
//! number of samples) leaves the left part empty.

use super::energy_data::{DisplayMode, EnergyData};

/// Size in font pixels of one glyph of the bitmap font (8×8).
pub const GLYPH_CELL: f32 = 8.0;

/// RGBA, each 0.0–1.0.
pub type Rgba = [f32; 4];

pub const BACKGROUND: Rgba = [0.078, 0.078, 0.086, 1.0];
pub const COLOR_KINETIC: Rgba = [0.30, 0.80, 0.30, 1.0];
pub const COLOR_POTENTIAL: Rgba = [0.80, 0.30, 0.30, 1.0];
pub const COLOR_TOTAL: Rgba = [0.30, 0.60, 0.90, 1.0];
pub const COLOR_AXIS: Rgba = [0.55, 0.55, 0.55, 1.0];
pub const COLOR_GRID: Rgba = [0.30, 0.30, 0.30, 0.6];
pub const COLOR_ZERO: Rgba = [0.60, 0.60, 0.60, 0.9];
pub const COLOR_TEXT: Rgba = [0.85, 0.85, 0.85, 1.0];
pub const COLOR_TITLE: Rgba = [0.90, 0.90, 0.90, 1.0];
pub const COLOR_STATUS: Rgba = [0.66, 0.66, 0.98, 1.0];

/// Simulation time shown across the width of the plot, in the time units of
/// the axis (reduced units, see `docs/units.md`).
pub const TIME_SPAN: f64 = 10.0;

/// Distances between x ticks, in time units; the smallest whose ticks are at
/// least [`MIN_X_TICK_CELLS`] apart is used, and labels go on the smallest
/// multiple of it they fit at.
const X_TICK_INTERVALS: [f64; 10] = [0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0];

/// Smallest distance between two x ticks, in glyph cells.
const MIN_X_TICK_CELLS: f32 = 2.0;

/// Length of a tick mark, in glyph cells.
const TICK_CELLS: f32 = 0.5;

/// A straight line from `from` to `to` with round caps, `width` pixels wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlotSegment {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub width: f32,
    pub color: Rgba,
}

/// One line of text whose top-left corner is at `origin`; each font pixel is
/// `scale` window pixels wide.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotLabel {
    pub text: String,
    pub origin: [f32; 2],
    pub scale: f32,
    pub color: Rgba,
}

impl PlotLabel {
    pub fn width(&self) -> f32 {
        self.text.chars().count() as f32 * GLYPH_CELL * self.scale
    }

    pub fn height(&self) -> f32 {
        GLYPH_CELL * self.scale
    }
}

/// An axis-aligned rectangle in window pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlotRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl PlotRect {
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    pub fn contains(&self, [x, y]: [f32; 2]) -> bool {
        (self.left..=self.right).contains(&x) && (self.top..=self.bottom).contains(&y)
    }
}

/// Everything the plot window shows, for a window of `size` pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotScene {
    pub size: (u32, u32),
    pub background: Rgba,
    /// The area inside the axes, when the window is large enough for one.
    pub plot_area: Option<PlotRect>,
    pub segments: Vec<PlotSegment>,
    pub labels: Vec<PlotLabel>,
}

/// One curve of the plot.
struct Series {
    name: &'static str,
    color: Rgba,
    values: Vec<f32>,
}

/// The curves `mode` shows, in legend order.
fn visible_series(data: &EnergyData, mode: DisplayMode) -> Vec<Series> {
    let kin = || Series {
        name: "E_kin",
        color: COLOR_KINETIC,
        values: data.v_kin.iter().copied().collect(),
    };
    let pot = || Series {
        name: "E_pot",
        color: COLOR_POTENTIAL,
        values: data.v_pot.iter().copied().collect(),
    };
    let tot = || Series {
        name: "E_tot",
        color: COLOR_TOTAL,
        values: data.v_tot().into_iter().collect(),
    };
    match mode {
        DisplayMode::Kin => vec![kin()],
        DisplayMode::Pot => vec![pot()],
        DisplayMode::Total => vec![tot()],
        DisplayMode::All => vec![kin(), pot(), tot()],
    }
}

/// The text size for a window at `scale_factor`: two window pixels per font
/// pixel at 1×, a whole multiple of that above, so the bitmap glyphs stay
/// crisp.
pub fn text_scale(scale_factor: f64) -> f32 {
    2.0 * scale_factor.max(1.0).round() as f32
}

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

/// One curve as `(time, value)` points, oldest first; `None` where the time
/// or the value is not finite (which breaks the curve).
type Curve = Vec<Option<(f64, f64)>>;

/// The time range the plot shows: [`TIME_SPAN`] long, starting at the
/// oldest sample while the history is shorter than that, else ending at the
/// newest sample.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TimeWindow {
    t0: f64,
    t1: f64,
}

impl TimeWindow {
    /// The window for `curves`; `None` without a single finite point.
    fn for_curves(curves: &[Curve]) -> Option<Self> {
        let mut times = curves.iter().flatten().flatten().map(|&(t, _)| t);
        let first = times.next()?;
        let (oldest, newest) = times.fold((first, first), |(lo, hi), t| (lo.min(t), hi.max(t)));
        let t0 = if newest - oldest > TIME_SPAN {
            newest - TIME_SPAN
        } else {
            oldest
        };
        Some(Self {
            t0,
            t1: t0 + TIME_SPAN,
        })
    }

    fn contains(&self, t: f64) -> bool {
        (self.t0..=self.t1).contains(&t)
    }
}

/// The value at time `t` on the straight line from `p` to `q`.
fn interpolate(p: (f64, f64), q: (f64, f64), t: f64) -> f64 {
    p.1 + (q.1 - p.1) * (t - p.0) / (q.0 - p.0)
}

/// Where each curve crosses the left edge `t0`: the value cut there from the
/// segment between the last point before and the first point after it.
fn edge_values(curves: &[Curve], t0: f64) -> impl Iterator<Item = f64> + '_ {
    curves.iter().flat_map(move |curve| {
        curve
            .windows(2)
            .filter_map(move |pair| match (pair[0], pair[1]) {
                (Some(p), Some(q)) if p.0 < t0 && q.0 > t0 => Some(interpolate(p, q, t0)),
                _ => None,
            })
    })
}

/// The y range shown: the values of the points inside `window` and where
/// the curves are cut at its left edge, so everything drawn fits, with 5%
/// room above and below. `None` when no point is inside.
fn value_range(curves: &[Curve], window: TimeWindow) -> Option<(f64, f64)> {
    let inside = curves
        .iter()
        .flatten()
        .flatten()
        .filter(|&&(t, _)| window.contains(t))
        .map(|&(_, v)| v);
    let mut values = inside.chain(edge_values(curves, window.t0));
    let first = values.next()?;
    let (lo, hi) = values.fold((first, first), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let span = hi - lo;
    let room = if span > hi.abs().max(lo.abs()) * 1e-6 {
        span * 0.05
    } else {
        (hi.abs() * 0.05).max(1e-3)
    };
    Some((lo - room, hi + room))
}

/// Maps the times of a [`TimeWindow`] onto the width of the plot area.
struct XAxis {
    window: TimeWindow,
    left: f32,
    width: f32,
}

impl XAxis {
    fn new(window: TimeWindow, area: PlotRect) -> Self {
        Self {
            window,
            left: area.left,
            width: area.width(),
        }
    }

    /// Window pixels per time unit.
    fn scale(&self) -> f32 {
        self.width / (self.window.t1 - self.window.t0) as f32
    }

    /// The pixel column of time `t`.
    fn x_of(&self, t: f64) -> f32 {
        self.left + (t - self.window.t0) as f32 * self.scale()
    }
}

/// Whether `v` is a whole multiple of `step`.
fn is_multiple(v: f64, step: f64) -> bool {
    let r = v / step;
    (r - r.round()).abs() < 1e-6
}

/// The multiples of `step` inside `window`, at most a thousand.
fn multiples_in(step: f64, window: TimeWindow) -> Vec<f64> {
    let first = (window.t0 / step - 1e-9).ceil();
    (0..1000)
        .map(|k| (first + f64::from(k)) * step)
        .take_while(|&t| t <= window.t1 + step * 1e-9)
        .collect()
}

/// Builds the scene; keeps track of the free space between the title at the
/// top and the footer at the bottom.
struct Layout<'a> {
    scene: PlotScene,
    scale: f32,
    /// One glyph, in window pixels.
    glyph: f32,
    /// Height of a text line, in window pixels.
    line: f32,
    /// Outer padding, in window pixels.
    pad: f32,
    /// Width of thin lines (axes, ticks), in whole window pixels.
    thin: f32,
    data: &'a EnergyData,
    /// The window's keys, shown at the right end of the title line.
    key_hints: &'a str,
}

impl Layout<'_> {
    fn label(&mut self, text: impl Into<String>, origin: [f32; 2], color: Rgba) -> f32 {
        let label = PlotLabel {
            text: text.into(),
            // Whole pixels: the glyphs are sampled with the nearest filter.
            origin: origin.map(f32::round),
            scale: self.scale,
            color,
        };
        let width = label.width();
        self.scene.labels.push(label);
        width
    }

    fn segment(&mut self, from: [f32; 2], to: [f32; 2], width: f32, color: Rgba) {
        self.scene.segments.push(PlotSegment {
            from,
            to,
            width,
            color,
        });
    }

    /// `v` moved onto the centre line of a pixel for a thin line of odd
    /// width (onto a pixel edge for an even one), so it covers whole pixels.
    fn snap(&self, v: f32) -> f32 {
        if self.thin % 2.0 == 1.0 {
            v.floor() + 0.5
        } else {
            v.round()
        }
    }

    fn text_width(&self, text: &str) -> f32 {
        text.chars().count() as f32 * self.glyph
    }
}

impl PlotScene {
    /// The plot of `data` as `mode` selects it, for a window of `size`
    /// pixels at `scale_factor`: title and `key_hints` at the top, then the
    /// plot with its axes and ticks (x: simulation time, y: energy), the
    /// legend line and the status text at the bottom. Without samples a
    /// waiting message stands in for the plot.
    pub fn build(
        data: &EnergyData,
        mode: DisplayMode,
        size: (u32, u32),
        scale_factor: f64,
        key_hints: &str,
    ) -> Self {
        let scale = text_scale(scale_factor);
        let glyph = GLYPH_CELL * scale;
        let mut layout = Layout {
            scene: PlotScene {
                size,
                background: BACKGROUND,
                plot_area: None,
                segments: Vec::new(),
                labels: Vec::new(),
            },
            scale,
            glyph,
            line: glyph * 1.25,
            pad: glyph * 0.75,
            // Whole pixels, as for the text: fractional widths look blurry.
            thin: scale_factor.max(1.0).round() as f32,
            data,
            key_hints,
        };
        layout.build(mode);
        layout.scene
    }
}

impl Layout<'_> {
    fn build(&mut self, mode: DisplayMode) {
        let (w, h) = (self.scene.size.0 as f32, self.scene.size.1 as f32);
        let (pad, line) = (self.pad, self.line);

        let title_w = self.label(
            format!("Energy plot - {}", mode.as_str()),
            [pad, pad],
            COLOR_TITLE,
        );
        // The keys at the right end of the title line, if they fit.
        let keys_x = w - pad - self.text_width(self.key_hints);
        if !self.key_hints.is_empty() && keys_x >= pad + title_w + self.glyph * 2.0 {
            self.label(self.key_hints, [keys_x, pad], COLOR_TEXT);
        }
        let top = pad + line + pad * 0.5;

        // Status lines at the very bottom, the legend line above them.
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
        let series = visible_series(self.data, mode);
        self.legend(&series, [pad, bottom]);
        bottom -= pad * 0.5;

        let curves = self.curves(&series);
        let shown = TimeWindow::for_curves(&curves)
            .and_then(|window| Some((window, value_range(&curves, window)?)));
        let Some((window, (lo, hi))) = shown else {
            self.waiting_message(top, bottom, w);
            return;
        };
        self.plot(
            &series,
            &curves,
            window,
            (lo, hi),
            PlotRect {
                left: pad,
                top,
                right: w - pad,
                bottom,
            },
        );
    }

    /// Legend: each curve in its colour, then the time and the x axis.
    fn legend(&mut self, series: &[Series], origin: [f32; 2]) {
        let mut x = origin[0];
        for s in series {
            x += self.label(s.name, [x, origin[1]], s.color);
            x += self.glyph;
        }
        let rest = format!(" t = {:.2}   x: time", self.data.time);
        self.label(rest, [x, origin[1]], COLOR_TEXT);
    }

    fn waiting_message(&mut self, top: f32, bottom: f32, width: f32) {
        let lines = [
            "Waiting for simulation data...",
            "Add some balls to see the energy plot",
        ];
        let start = (top + bottom) * 0.5 - self.line;
        for (i, text) in lines.into_iter().enumerate() {
            let x = ((width - self.text_width(text)) * 0.5).max(self.pad);
            self.label(text, [x, start + i as f32 * self.line], COLOR_TEXT);
        }
    }

    /// Axes, ticks, grid, zero line and curves inside `free` (the space
    /// between title and legend): the times of `window` across, the values
    /// `[lo, hi]` up.
    fn plot(
        &mut self,
        series: &[Series],
        curves: &[Curve],
        window: TimeWindow,
        (lo, hi): (f64, f64),
        free: PlotRect,
    ) {
        let (glyph, line, thin) = (self.glyph, self.line, self.thin);
        let tick = glyph * TICK_CELLS;

        // Room for the y tick labels left of the axis and the x tick labels
        // below it; half a glyph at the top and right so the end labels fit.
        let max_y_ticks = ((free.height() - line) / (line * 2.5)).clamp(2.0, 10.0) as usize;
        let (y_step, y_ticks) = nice_ticks(lo, hi, max_y_ticks);
        let y_labels: Vec<String> = y_ticks.iter().map(|&v| format_tick(v, y_step)).collect();
        let label_w = y_labels
            .iter()
            .map(|label| self.text_width(label))
            .fold(0.0, f32::max);
        let area = PlotRect {
            left: free.left + label_w + tick + glyph * 0.5,
            top: free.top + glyph * 0.5,
            right: free.right - glyph,
            bottom: free.bottom - line - tick,
        };
        if area.width() < glyph * 4.0 || area.height() < glyph * 2.0 {
            return;
        }
        self.scene.plot_area = Some(area);

        let y_of = |v: f64| area.bottom - ((v - lo) / (hi - lo)) as f32 * area.height();

        // Horizontal grid lines with their labels.
        for (&v, label) in y_ticks.iter().zip(&y_labels) {
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
        self.x_ticks(&x_axis, area);

        // Axes over the grid, curves over the axes.
        let (left, bottom) = (self.snap(area.left), self.snap(area.bottom));
        self.segment([left, area.top], [left, bottom], thin, COLOR_AXIS);
        self.segment([left, bottom], [area.right, bottom], thin, COLOR_AXIS);
        let to_px = |(t, v): (f64, f64)| [x_axis.x_of(t), y_of(v)];
        for (s, curve) in series.iter().zip(curves) {
            self.curve_segments(curve, window.t0, &to_px, thin * 2.0, s.color);
        }
    }

    /// The `(time, value)` points of each series; a series registered later
    /// is shorter and ends at the newest sample.
    fn curves(&self, series: &[Series]) -> Vec<Curve> {
        let times = self.sample_times(series);
        series
            .iter()
            .map(|s| {
                let offset = times.len() - s.values.len();
                s.values
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| {
                        let t = times[offset + i];
                        (v.is_finite() && t.is_finite()).then_some((t, f64::from(v)))
                    })
                    .collect()
            })
            .collect()
    }

    /// The x value of each sample of the longest series: its simulation
    /// time, or its index without a time history.
    fn sample_times(&self, series: &[Series]) -> Vec<f64> {
        let n = series.iter().map(|s| s.values.len()).max().unwrap_or(0);
        let times = &self.data.sample_times;
        if times.len() >= n {
            times
                .iter()
                .skip(times.len() - n)
                .map(|&t| f64::from(t))
                .collect()
        } else {
            (0..n).map(|i| i as f64).collect()
        }
    }

    /// Tick marks and time labels below the x axis: ticks every 0.5 time
    /// units when they are far enough apart (else every 1, 2, 5, …), labels
    /// on the smallest multiple of that at which they do not overlap.
    fn x_ticks(&mut self, x_axis: &XAxis, area: PlotRect) {
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
            if !is_multiple(t, label_interval) {
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
    /// cut at `t0`, the left edge: a segment that crosses it starts there. A
    /// missing point breaks the curve, and a lone point is drawn as a dot.
    fn curve_segments(
        &mut self,
        curve: &[Option<(f64, f64)>],
        t0: f64,
        to_px: &impl Fn((f64, f64)) -> [f32; 2],
        width: f32,
        color: Rgba,
    ) {
        for (i, point) in curve.iter().enumerate() {
            let Some(p) = *point else { continue };
            let prev = i.checked_sub(1).and_then(|j| curve[j]);
            match curve.get(i + 1).copied().flatten() {
                // A segment ending exactly at the edge would be cut to nothing.
                Some(q) if q.0 > t0 || p.0 >= t0 => {
                    // Cut at the edge; `value_range` includes this value.
                    let p = if p.0 < t0 && q.0 > p.0 {
                        (t0, interpolate(p, q, t0))
                    } else {
                        p
                    };
                    self.segment(to_px(p), to_px(q), width, color);
                }
                None if prev.is_none() && p.0 >= t0 => {
                    self.segment(to_px(p), to_px(p), width, color);
                }
                _ => {}
            }
        }
    }
}
