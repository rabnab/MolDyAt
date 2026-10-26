// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The energy plot as a scene of line segments and text labels in window
//! pixels, without a GPU: the backend only draws what [`PlotScene::build`]
//! lays out (the wgpu binary's plot pipeline), so the layout is tested here.
//!
//! Pixel coordinates have their origin at the top-left corner of the window,
//! with y pointing down. Text is drawn with the backend's 8×8 bitmap font:
//! a label covers `chars × GLYPH_CELL × scale` by `GLYPH_CELL × scale` pixels.

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

/// The window's keys, shown at the right end of the title line.
pub const KEY_HINTS: &str = "Esc: close  Tab: mode";

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
/// one tick at `lo`.
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

/// The y range shown for `series`: their finite values with 5% room above
/// and below. `None` when no value is finite.
fn value_range(series: &[Series]) -> Option<(f64, f64)> {
    let mut values = series
        .iter()
        .flat_map(|s| &s.values)
        .filter(|v| v.is_finite())
        .map(|&v| f64::from(v));
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

/// Maps steps `x0..=x1` onto the width of the plot area.
struct XAxis {
    x0: f64,
    x1: f64,
    left: f32,
    width: f32,
}

impl XAxis {
    fn new(steps: &[f64], area: PlotRect) -> Self {
        Self {
            x0: steps.first().copied().unwrap_or(0.0),
            x1: steps.last().copied().unwrap_or(0.0),
            left: area.left,
            width: area.width(),
        }
    }

    /// The pixel column of step `x`; a single step sits at the left edge.
    fn x_of(&self, x: f64) -> f32 {
        if self.x1 > self.x0 {
            self.left + ((x - self.x0) / (self.x1 - self.x0)) as f32 * self.width
        } else {
            self.left
        }
    }
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
    /// Width of thin lines (axes, ticks), in window pixels.
    thin: f32,
    data: &'a EnergyData,
}

impl Layout<'_> {
    fn label(&mut self, text: impl Into<String>, origin: [f32; 2], color: Rgba) -> f32 {
        let label = PlotLabel {
            text: text.into(),
            origin,
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

    fn text_width(&self, text: &str) -> f32 {
        text.chars().count() as f32 * self.glyph
    }
}

impl PlotScene {
    /// The plot of `data` as `mode` selects it, for a window of `size`
    /// pixels at `scale_factor`: title at the top, then the plot with its
    /// axes and ticks (x: simulation step, y: energy), the legend line and the
    /// status text at the bottom. Without samples a waiting message stands in
    /// for the plot.
    pub fn build(
        data: &EnergyData,
        mode: DisplayMode,
        size: (u32, u32),
        scale_factor: f64,
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
            thin: scale_factor.max(1.0) as f32,
            data,
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
        let keys_x = w - pad - self.text_width(KEY_HINTS);
        if keys_x >= pad + title_w + self.glyph * 2.0 {
            self.label(KEY_HINTS, [keys_x, pad], COLOR_TEXT);
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

        let Some((lo, hi)) = value_range(&series) else {
            self.waiting_message(top, bottom, w);
            return;
        };
        self.plot(
            &series,
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
        let rest = format!(" t = {:.2}   x: step", self.data.time);
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
    /// between title and legend). The y range is `[lo, hi]`.
    fn plot(&mut self, series: &[Series], (lo, hi): (f64, f64), free: PlotRect) {
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
            let y = y_of(v);
            self.segment([area.left, y], [area.right, y], thin, COLOR_GRID);
            self.segment([area.left - tick, y], [area.left, y], thin, COLOR_AXIS);
            let x = area.left - tick - glyph * 0.25 - self.text_width(label);
            self.label(label.clone(), [x, y - glyph * 0.5], COLOR_TEXT);
        }
        if lo < 0.0 && hi > 0.0 {
            let y = y_of(0.0);
            self.segment([area.left, y], [area.right, y], thin, COLOR_ZERO);
        }

        let steps = self.sample_steps(series);
        let x_axis = XAxis::new(&steps, area);
        self.x_ticks(&x_axis, area);

        // Axes over the grid, curves over the axes.
        self.segment(
            [area.left, area.top],
            [area.left, area.bottom],
            thin,
            COLOR_AXIS,
        );
        self.segment(
            [area.left, area.bottom],
            [area.right, area.bottom],
            thin,
            COLOR_AXIS,
        );
        for s in series {
            // A series registered later is shorter: it ends at the newest step.
            let offset = steps.len() - s.values.len();
            let points: Vec<Option<[f32; 2]>> = s
                .values
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    v.is_finite()
                        .then(|| [x_axis.x_of(steps[offset + i]), y_of(f64::from(v))])
                })
                .collect();
            self.polyline(&points, thin * 2.0, s.color);
        }
    }

    /// The x value of each sample of the longest series: its step, or its
    /// index without a step history.
    fn sample_steps(&self, series: &[Series]) -> Vec<f64> {
        let n = series.iter().map(|s| s.values.len()).max().unwrap_or(0);
        let steps = &self.data.steps;
        if steps.len() >= n {
            steps
                .iter()
                .skip(steps.len() - n)
                .map(|&s| s as f64)
                .collect()
        } else {
            (0..n).map(|i| i as f64).collect()
        }
    }

    /// Tick marks and step labels below the x axis.
    fn x_ticks(&mut self, x_axis: &XAxis, area: PlotRect) {
        let (glyph, thin) = (self.glyph, self.thin);
        let tick = glyph * TICK_CELLS;
        let (x0, x1) = (x_axis.x0, x_axis.x1);
        let max_ticks = (area.width() / (glyph * 8.0)).clamp(1.0, 10.0) as usize;
        let (step, ticks) = nice_ticks(x0, x1, max_ticks);
        // Steps are whole numbers: no ticks between two steps.
        let ticks = if step < 1.0 && x1 > x0 {
            nice_ticks(x0, x1, ((x1 - x0) as usize).max(1)).1
        } else {
            ticks
        };
        for v in ticks {
            let x = x_axis.x_of(v);
            self.segment([x, area.bottom], [x, area.bottom + tick], thin, COLOR_AXIS);
            let label = format!("{v:.0}");
            let label_x = x - self.text_width(&label) * 0.5;
            self.label(
                label,
                [label_x, area.bottom + tick + glyph * 0.25],
                COLOR_TEXT,
            );
        }
    }

    /// Connect consecutive points; a missing (non-finite) value breaks the
    /// line, and a lone point is drawn as a dot.
    fn polyline(&mut self, points: &[Option<[f32; 2]>], width: f32, color: Rgba) {
        for (i, point) in points.iter().enumerate() {
            let Some(p) = *point else { continue };
            let prev = i.checked_sub(1).and_then(|j| points[j]);
            let next = points.get(i + 1).copied().flatten();
            match next {
                Some(q) => self.segment(p, q, width, color),
                None if prev.is_none() => self.segment(p, p, width, color),
                None => {}
            }
        }
    }
}
