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
//! and a middle-button drag change the range. A history shorter than
//! the span (the registry keeps a fixed number of samples) leaves the left
//! part empty.
//!
//! The modules split the plot by concern: `curve` is the model (series,
//! curves, ranges), `ticks` the axis arithmetic, `layout` the page and
//! `panel` one plot's geometry. This file holds the scene types and the
//! state the layout builds them with.

mod curve;
mod layout;
mod panel;
mod ticks;

#[cfg(test)]
mod tests;

pub use curve::series_color;
pub use ticks::{format_tick, nice_ticks};

use super::energy_data::EnergyData;
use super::plot_selection::PlotSelection;
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

/// A hidden series in the legend.
pub const COLOR_HIDDEN: Rgba = [0.40, 0.40, 0.40, 1.0];

/// Simulation time shown across the width of the plot, in the time units of
/// the axis (reduced units, see `docs/units.md`).
pub const TIME_SPAN: f64 = 10.0;

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

/// The text size for a window at `scale_factor`: two window pixels per font
/// pixel at 1×, a whole multiple of that above, so the bitmap glyphs stay
/// crisp.
pub fn text_scale(scale_factor: f64) -> f32 {
    2.0 * scale_factor.max(1.0).round() as f32
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
