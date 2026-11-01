// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The distributions view of the plot window (#343): the speed histogram
//! with the 2D Maxwell–Boltzmann counts at the current temperature, and
//! g(r) per species pair, as a [`PlotScene`] of segments and labels in
//! window pixels (see `plot_layout.rs` for the conventions).

use super::plot_layout::{
    BACKGROUND, COLOR_AXIS, COLOR_HIDDEN, COLOR_KINETIC, COLOR_TEXT, COLOR_TITLE, COLOR_TOTAL,
    COLOR_ZERO, GLYPH_CELL, LegendEntry, PlotLabel, PlotRect, PlotScene, PlotSegment, Rgba,
    format_tick, nice_ticks, text_scale,
};
use thermo_runtime::distributions::Distributions;

/// Colours of the g(r) curves, by pair.
const PAIR_COLORS: [Rgba; 6] = [
    [0.30, 0.60, 0.90, 1.0],
    [0.95, 0.60, 0.20, 1.0],
    [0.30, 0.80, 0.30, 1.0],
    [0.85, 0.45, 0.85, 1.0],
    [0.90, 0.85, 0.30, 1.0],
    [0.30, 0.85, 0.85, 1.0],
];

/// Window title of the distributions view.
pub const DISTRIBUTIONS_TITLE: &str = "Plot - Distributions";

/// The distributions of the latest snapshot, with the species names of
/// every g(r) pair.
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionData {
    pub distributions: Distributions,
    /// `"N2-Ar"` per pair of `distributions.radial.pairs`.
    pub pair_names: Vec<String>,
}

/// Builds the scene: `layout` places text and lines in window pixels.
struct Layout<'a> {
    scene: PlotScene,
    /// g(r) pairs not drawn, by name (`"N2-Ar"`); their legend entry is grey.
    hidden_pairs: &'a [String],
    scale: f32,
    glyph: f32,
    line: f32,
    pad: f32,
    thin: f32,
}

impl Layout<'_> {
    fn label(&mut self, text: impl Into<String>, origin: [f32; 2], color: Rgba) {
        self.scene.labels.push(PlotLabel {
            text: text.into(),
            origin: origin.map(f32::round),
            scale: self.scale,
            color,
        });
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

    /// Axes, ticks and labels of a panel in `area` showing `x` and `y`
    /// ranges; returns the maps from values to pixels.
    fn axes(
        &mut self,
        area: PlotRect,
        x: (f64, f64),
        y: (f64, f64),
    ) -> (impl Fn(f64) -> f32 + use<>, impl Fn(f64) -> f32 + use<>) {
        let to_x = move |v: f64| area.left + ((v - x.0) / (x.1 - x.0)) as f32 * area.width();
        let to_y = move |v: f64| area.bottom - ((v - y.0) / (y.1 - y.0)) as f32 * area.height();
        let thin = self.thin;
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
        let tick = self.glyph * 0.5;
        let (step, ticks) = nice_ticks(
            y.0,
            y.1,
            ((area.height() / (self.line * 1.5)) as usize).max(2),
        );
        for v in ticks {
            let py = to_y(v);
            self.segment([area.left - tick, py], [area.left, py], thin, COLOR_AXIS);
            let text = format_tick(v, step);
            let tx = area.left - tick - self.glyph * 0.5 - self.text_width(&text);
            self.label(text, [tx, py - self.glyph * 0.5], COLOR_TEXT);
        }
        let max_x_ticks = ((area.width() / (self.glyph * 6.0)) as usize).max(2);
        let (step, ticks) = nice_ticks(x.0, x.1, max_x_ticks);
        for v in ticks {
            let px = to_x(v);
            self.segment(
                [px, area.bottom],
                [px, area.bottom + tick],
                thin,
                COLOR_AXIS,
            );
            let text = format_tick(v, step);
            let tx = px - self.text_width(&text) * 0.5;
            self.label(
                text,
                [tx, area.bottom + tick + self.glyph * 0.25],
                COLOR_TEXT,
            );
        }
        (to_x, to_y)
    }
}

impl PlotScene {
    /// The distributions view for a window of `size` pixels at
    /// `scale_factor`: the title and `key_hints`, the speed histogram (bars)
    /// with its Maxwell–Boltzmann counts (line) above, g(r) per species pair
    /// below with g = 1 marked. Pairs named in `hidden_pairs` are not drawn
    /// and their legend entry is grey; every g(r) legend entry is a click
    /// target in `legend` (panel 0). Without data a waiting message.
    pub fn build_distributions(
        data: Option<&DistributionData>,
        hidden_pairs: &[String],
        size: (u32, u32),
        scale_factor: f64,
        key_hints: &str,
    ) -> Self {
        let scale = text_scale(scale_factor);
        let glyph = GLYPH_CELL * scale;
        let mut layout = Layout {
            hidden_pairs,
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
            thin: scale_factor.max(1.0).round() as f32,
        };
        layout.build(data, key_hints);
        layout.scene
    }
}

impl Layout<'_> {
    fn build(&mut self, data: Option<&DistributionData>, key_hints: &str) {
        let (w, h) = (self.scene.size.0 as f32, self.scene.size.1 as f32);
        let (pad, line, glyph) = (self.pad, self.line, self.glyph);
        self.label(DISTRIBUTIONS_TITLE, [pad, pad], COLOR_TITLE);
        let hints_x = w - pad - self.text_width(key_hints);
        if hints_x > pad + self.text_width(DISTRIBUTIONS_TITLE) + glyph * 2.0 {
            self.label(key_hints, [hints_x, pad], COLOR_TEXT);
        }
        let top = pad + line + pad * 0.5;
        // Room for y tick labels on the left and x tick labels below.
        let left = pad + glyph * 7.0;
        let right = w - pad;
        let bottom = h - pad;
        let Some(data) = data.filter(|_| right - left > glyph * 10.0 && bottom - top > line * 10.0)
        else {
            self.label("Waiting for balls ...", [pad, top + line], COLOR_TEXT);
            return;
        };
        // Two panels, each with a heading line above and x labels below.
        let panel_h = (bottom - top) / 2.0;
        let speed_area = PlotRect {
            left,
            top: top + line,
            right,
            bottom: top + panel_h - line * 1.5,
        };
        let rdf_area = PlotRect {
            left,
            top: top + panel_h + line,
            right,
            bottom: bottom - line * 1.5,
        };
        self.speed_panel(speed_area, data);
        self.rdf_panel(rdf_area, data);
    }

    fn speed_panel(&mut self, area: PlotRect, data: &DistributionData) {
        let heading = [area.left, area.top - self.line];
        let Some(speed) = &data.distributions.speed else {
            self.label("Speed: no moving balls", heading, COLOR_TEXT);
            return;
        };
        self.label(
            format!(
                "Speed  bars: balls  line: Maxwell-Boltzmann at T = {:.3}",
                speed.temperature
            ),
            heading,
            COLOR_TEXT,
        );
        let bins = speed.counts.len();
        let x = (0.0, bins as f64 * f64::from(speed.bin_width));
        let max_count = speed
            .counts
            .iter()
            .map(|&c| c as f32)
            .chain(speed.expected.iter().copied())
            .fold(1.0_f32, f32::max);
        let (to_x, to_y) = self.axes(area, x, (0.0, f64::from(max_count) * 1.05));
        let bar = (area.width() / bins as f32 * 0.8).max(self.thin);
        for (i, &count) in speed.counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let px = to_x(f64::from(speed.bin_centre(i)));
            self.segment(
                [px, to_y(0.0)],
                [px, to_y(f64::from(count))],
                bar,
                COLOR_TOTAL,
            );
        }
        let points: Vec<[f32; 2]> = speed
            .expected
            .iter()
            .enumerate()
            .map(|(i, &e)| [to_x(f64::from(speed.bin_centre(i))), to_y(f64::from(e))])
            .collect();
        let width = self.thin * 2.0;
        for pair in points.windows(2) {
            self.segment(pair[0], pair[1], width, COLOR_KINETIC);
        }
    }

    fn rdf_panel(&mut self, area: PlotRect, data: &DistributionData) {
        let radial = &data.distributions.radial;
        let bins = radial.pairs.first().map_or(0, |p| p.g.len());
        let hidden_pairs = self.hidden_pairs;
        let hidden = |k: usize| {
            data.pair_names
                .get(k)
                .is_some_and(|name| hidden_pairs.contains(name))
        };
        // The y range follows the pairs drawn, so hiding a tall curve
        // magnifies the others.
        let max_g = radial
            .pairs
            .iter()
            .enumerate()
            .filter(|(k, _)| !hidden(*k))
            .flat_map(|(_, p)| p.g.iter().copied())
            .filter(|g| g.is_finite())
            .fold(1.5_f32, f32::max);
        let x = (0.0, bins.max(1) as f64 * f64::from(radial.bin_width));
        let (to_x, to_y) = self.axes(area, x, (0.0, f64::from(max_g) * 1.05));
        // The legend in the heading line: "g(r)" and one name per pair, a
        // click target each; a hidden pair's name is grey.
        let mut lx = area.left;
        let ly = area.top - self.line;
        self.label("g(r)", [lx, ly], COLOR_TEXT);
        lx += self.text_width("g(r)  ");
        for (k, name) in data.pair_names.iter().enumerate() {
            let color = if hidden(k) {
                COLOR_HIDDEN
            } else {
                PAIR_COLORS[k % PAIR_COLORS.len()]
            };
            self.label(name.clone(), [lx, ly], color);
            let width = self.text_width(name);
            self.scene.legend.push(LegendEntry {
                panel: 0,
                series: name.clone(),
                rect: PlotRect {
                    left: lx,
                    top: ly,
                    right: lx + width,
                    bottom: ly + self.glyph,
                },
            });
            lx += width + self.glyph * 2.0;
        }
        let one = to_y(1.0);
        self.segment([area.left, one], [area.right, one], self.thin, COLOR_ZERO);
        let width = self.thin * 2.0;
        for (k, pair) in radial.pairs.iter().enumerate() {
            if hidden(k) {
                continue;
            }
            let color = PAIR_COLORS[k % PAIR_COLORS.len()];
            let points: Vec<Option<[f32; 2]>> = pair
                .g
                .iter()
                .enumerate()
                .map(|(i, &g)| {
                    g.is_finite()
                        .then(|| [to_x(f64::from(radial.bin_centre(i))), to_y(f64::from(g))])
                })
                .collect();
            for segment in points.windows(2) {
                if let [Some(a), Some(b)] = segment {
                    self.segment(*a, *b, width, color);
                }
            }
        }
    }
}
