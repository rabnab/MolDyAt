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
//! The x axis is the simulation time of the samples, not their step. It
//! shows the range of a [`TimeView`]: by default [`TIME_SPAN`] time units,
//! so the time step changes only how fast the curves scroll, never the
//! scale. Until the history spans that much, the curves start at the left
//! edge; then the newest sample stays at the right edge. The scroll wheel
//! and a middle-button drag change the range (#367). A history shorter than
//! the span (the registry keeps a fixed number of samples) leaves the left
//! part empty.

use std::borrow::Cow;

use super::energy_data::EnergyData;
use super::plot_selection::{PlotPanel, PlotSelection};
use super::time_view::{TimeHistory, TimeView};

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
    /// The area of all plots together (from the top of the first to the
    /// bottom of the last; they share their left and right edges), when the
    /// window is large enough for one. The time axis maps onto its width.
    pub plot_area: Option<PlotRect>,
    /// The area inside the axes of each plot, top to bottom.
    pub panels: Vec<PanelArea>,
    /// Where each series is named in a plot's legend, for clicks.
    pub legend: Vec<LegendEntry>,
    /// The time range `(t0, t1)` across the plot area, when there is one.
    pub time_window: Option<(f64, f64)>,
    /// What there was to show, when there were samples.
    pub history: Option<TimeHistory>,
    pub segments: Vec<PlotSegment>,
    pub labels: Vec<PlotLabel>,
}

/// Colours of the series other than the energies, by their place in the
/// list of series on offer.
const PALETTE: [Rgba; 6] = [
    [0.95, 0.60, 0.20, 1.0],
    [0.90, 0.85, 0.30, 1.0],
    [0.85, 0.45, 0.85, 1.0],
    [0.30, 0.85, 0.85, 1.0],
    [0.95, 0.55, 0.65, 1.0],
    [0.75, 0.75, 0.75, 1.0],
];

/// A hidden series in the legend.
pub const COLOR_HIDDEN: Rgba = [0.40, 0.40, 0.40, 1.0];

/// The colour of series `name`: fixed for the energies, else from
/// [`PALETTE`] by its place among the series of `data`, so it stays the same
/// in every plot.
pub fn series_color(data: &EnergyData, name: &str) -> Rgba {
    match name {
        "E_kin" => COLOR_KINETIC,
        "E_pot" => COLOR_POTENTIAL,
        "E_tot" => COLOR_TOTAL,
        _ => {
            let others = data
                .series_names()
                .into_iter()
                .filter(|n| !matches!(*n, "E_kin" | "E_pot" | "E_tot"))
                .position(|n| n == name)
                .unwrap_or(0);
            PALETTE[others % PALETTE.len()]
        }
    }
}

/// The area inside the axes of plot `index` of the selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelArea {
    pub index: usize,
    pub area: PlotRect,
}

/// Series `series` of plot `panel`, named in its legend at `rect`.
#[derive(Debug, Clone, PartialEq)]
pub struct LegendEntry {
    pub panel: usize,
    pub series: String,
    pub rect: PlotRect,
}

/// One curve of a plot.
struct Series {
    name: String,
    color: Rgba,
    hidden: bool,
    values: Vec<f32>,
}

/// The series of `panel` in legend order, with their values from `data`
/// (none for a series `data` lacks).
fn panel_series(data: &EnergyData, panel: &PlotPanel) -> Vec<Series> {
    panel
        .series()
        .iter()
        .map(|name| Series {
            name: name.clone(),
            color: series_color(data, name),
            hidden: panel.is_hidden(name),
            values: data
                .series(name)
                .map(|values| values.iter().copied().collect())
                .unwrap_or_default(),
        })
        .collect()
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

/// Most points of a curve drawn per pixel column of the plot (its lowest
/// and its highest) when the curve has more points than the plot has
/// columns (#366).
const POINTS_PER_COLUMN: usize = 2;

/// `curve` with at most [`POINTS_PER_COLUMN`] points per pixel column of
/// `window` (`columns` across), so the draw cost stays the same however many
/// samples the window holds: each column keeps its lowest and its highest
/// point, in time order, so spikes stay visible. A curve with no more
/// points inside the window than that is returned as it is. The points
/// right before and after the window are kept for the cut at its edges, and
/// a missing point still breaks the curve.
fn downsample(
    curve: &[Option<(f64, f64)>],
    window: TimeWindow,
    columns: usize,
) -> Cow<'_, [Option<(f64, f64)>]> {
    let inside = |p: &Option<(f64, f64)>| p.is_some_and(|(t, _)| window.contains(t));
    let count = curve.iter().filter(|p| inside(p)).count();
    if columns == 0 || count <= POINTS_PER_COLUMN * columns {
        return Cow::Borrowed(curve);
    }
    let (Some(first), Some(last)) = (
        curve.iter().position(inside),
        curve.iter().rposition(inside),
    ) else {
        return Cow::Borrowed(curve);
    };
    let span = window.t1 - window.t0;
    let column_of = |t: f64| {
        (((t - window.t0) / span * columns as f64).floor().max(0.0) as usize).min(columns - 1)
    };

    /// The lowest and highest point of one column, with their indices.
    struct Bucket {
        column: usize,
        low: (usize, (f64, f64)),
        high: (usize, (f64, f64)),
    }
    fn flush(bucket: &mut Option<Bucket>, out: &mut Vec<Option<(f64, f64)>>) {
        if let Some(b) = bucket.take() {
            let (a, z) = if b.low.0 <= b.high.0 {
                (b.low, b.high)
            } else {
                (b.high, b.low)
            };
            out.push(Some(a.1));
            if z.0 != a.0 {
                out.push(Some(z.1));
            }
        }
    }

    let mut out = Vec::with_capacity(POINTS_PER_COLUMN * columns + 2);
    if first > 0 {
        out.push(curve[first - 1]);
    }
    let mut bucket: Option<Bucket> = None;
    for (i, point) in curve.iter().enumerate().take(last + 1).skip(first) {
        let Some(p) = *point else {
            flush(&mut bucket, &mut out);
            out.push(None);
            continue;
        };
        let column = column_of(p.0);
        match &mut bucket {
            Some(b) if b.column == column => {
                if p.1 < b.low.1.1 {
                    b.low = (i, p);
                }
                if p.1 > b.high.1.1 {
                    b.high = (i, p);
                }
            }
            _ => {
                flush(&mut bucket, &mut out);
                bucket = Some(Bucket {
                    column,
                    low: (i, p),
                    high: (i, p),
                });
            }
        }
    }
    flush(&mut bucket, &mut out);
    if let Some(&after) = curve.get(last + 1) {
        out.push(after);
    }
    Cow::Owned(out)
}

/// The time range the plot shows (the [`TimeView`]'s window).
#[derive(Debug, Clone, Copy, PartialEq)]
struct TimeWindow {
    t0: f64,
    t1: f64,
}

impl TimeWindow {
    fn contains(&self, t: f64) -> bool {
        (self.t0..=self.t1).contains(&t)
    }
}

/// The time history of `curves`; `None` without a single finite point.
fn history_of(curves: &[Curve]) -> Option<TimeHistory> {
    let mut times = curves.iter().flatten().flatten().map(|&(t, _)| t);
    let first = times.next()?;
    let (oldest, newest) = times.fold((first, first), |(lo, hi), t| (lo.min(t), hi.max(t)));
    Some(TimeHistory {
        oldest,
        newest,
        samples: curves.iter().map(Vec::len).max().unwrap_or(0),
    })
}

/// The value at time `t` on the straight line from `p` to `q`.
fn interpolate(p: (f64, f64), q: (f64, f64), t: f64) -> f64 {
    p.1 + (q.1 - p.1) * (t - p.0) / (q.0 - p.0)
}

/// Where each curve crosses an edge of `window`: the value cut there from
/// the segment between the last point before and the first point after it.
fn edge_values(curves: &[Curve], window: TimeWindow) -> impl Iterator<Item = f64> + '_ {
    curves
        .iter()
        .flat_map(move |curve| {
            curve.windows(2).flat_map(move |pair| {
                let (Some(p), Some(q)) = (pair[0], pair[1]) else {
                    return [None, None];
                };
                let cut = |edge: f64| (p.0 < edge && q.0 > edge).then(|| interpolate(p, q, edge));
                [cut(window.t0), cut(window.t1)]
            })
        })
        .flatten()
}

/// The y range shown: the values of the points inside `window` and where
/// the curves are cut at its edges, so everything drawn fits, with 5% room
/// above and below. `None` when nothing of the curves is inside.
fn value_range(curves: &[Curve], window: TimeWindow) -> Option<(f64, f64)> {
    let inside = curves
        .iter()
        .flatten()
        .flatten()
        .filter(|&&(t, _)| window.contains(t))
        .map(|&(_, v)| v);
    let mut values = inside.chain(edge_values(curves, window));
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

/// The longest leading run of `hints` (separated by two spaces) that `fits`;
/// empty when not even the first one does.
fn fitting_hints(hints: &str, fits: impl Fn(&str) -> bool) -> String {
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
    /// The time range to show.
    view: &'a TimeView,
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
    /// The plots of `selection` with their series from `data`, for a window
    /// of `size` pixels at `scale_factor`: title and `key_hints` at the top,
    /// then the plots stacked on a shared time axis (the range of `view`;
    /// time labels under the last plot), each with its legend line above it
    /// and its own y axis scaled to what it shows, then the info line and
    /// the status text at the bottom. Without samples a waiting message
    /// stands in for the plots.
    pub fn build(
        data: &EnergyData,
        selection: &PlotSelection,
        size: (u32, u32),
        scale_factor: f64,
        key_hints: &str,
        view: &TimeView,
    ) -> Self {
        let scale = text_scale(scale_factor);
        let glyph = GLYPH_CELL * scale;
        let mut layout = Layout {
            scene: PlotScene {
                size,
                background: BACKGROUND,
                plot_area: None,
                panels: Vec::new(),
                legend: Vec::new(),
                time_window: None,
                history: None,
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
            view,
        };
        layout.build(selection);
        layout.scene
    }
}

/// One plot while it is laid out: its series, their points, its value
/// range and y ticks.
struct PanelPlan {
    index: usize,
    series: Vec<Series>,
    curves: Vec<Curve>,
    range: (f64, f64),
    ticks: (f64, Vec<f64>),
    labels: Vec<String>,
}

impl Layout<'_> {
    fn build(&mut self, selection: &PlotSelection) {
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
                let curves = self.curves(&series);
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
        let shown: Vec<Curve> = plans
            .iter()
            .flat_map(|plan| {
                plan.series
                    .iter()
                    .zip(&plan.curves)
                    .filter(|(s, _)| !s.hidden)
                    .map(|(_, curve)| curve.clone())
            })
            .collect();
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
            let shown: Vec<Curve> = plan
                .series
                .iter()
                .zip(&plan.curves)
                .filter(|(s, _)| !s.hidden)
                .map(|(_, curve)| curve.clone())
                .collect();
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

    /// Grid, y ticks, zero line, axes and curves of one plot in `area`; time
    /// labels only with `time_labels` (the last plot).
    fn panel(&mut self, plan: &PanelPlan, window: TimeWindow, area: PlotRect, time_labels: bool) {
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
