// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The model of the plot: the series of a panel, their curves over time, the
//! time and value ranges they span, and the thinning of a dense curve. No
//! pixels: see `layout.rs` and `panel.rs` for the geometry.

use std::borrow::Cow;

use super::{COLOR_KINETIC, COLOR_POTENTIAL, COLOR_TOTAL, Rgba};
use crate::graphics::energy_plot::energy_data::EnergyData;
use crate::graphics::energy_plot::plot_selection::PlotPanel;
use crate::graphics::energy_plot::time_view::TimeHistory;

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

/// One curve of a plot.
pub(super) struct Series {
    pub(super) name: String,
    pub(super) color: Rgba,
    pub(super) hidden: bool,
    pub(super) values: Vec<f32>,
}

/// The series of `panel` in legend order, with their values from `data`
/// (none for a series `data` lacks).
pub(super) fn panel_series(data: &EnergyData, panel: &PlotPanel) -> Vec<Series> {
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

/// One curve as `(time, value)` points, oldest first; `None` where the time
/// or the value is not finite (which breaks the curve).
pub(super) type Curve = Vec<Option<(f64, f64)>>;

/// The `(time, value)` points of each series; a series registered later is
/// shorter and ends at the newest sample. The x value of a sample is its
/// simulation time, or its index without a time history.
pub(super) fn curves(data: &EnergyData, series: &[Series]) -> Vec<Curve> {
    let n = series.iter().map(|s| s.values.len()).max().unwrap_or(0);
    let sample_times = &data.sample_times;
    let times: Vec<f64> = if sample_times.len() >= n {
        sample_times
            .iter()
            .skip(sample_times.len() - n)
            .map(|&t| f64::from(t))
            .collect()
    } else {
        (0..n).map(|i| i as f64).collect()
    };
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

/// Most points of a curve drawn per pixel column of the plot (its lowest
/// and its highest) when the curve has more points than the plot has
/// columns.
const POINTS_PER_COLUMN: usize = 2;

/// `curve` with at most [`POINTS_PER_COLUMN`] points per pixel column of
/// `window` (`columns` across), so the draw cost stays the same however many
/// samples the window holds: each column keeps its lowest and its highest
/// point, in time order, so spikes stay visible. A curve with no more
/// points inside the window than that is returned as it is. The points
/// right before and after the window are kept for the cut at its edges, and
/// a missing point still breaks the curve.
pub(super) fn downsample(
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

/// The time range the plot shows (the `TimeView`'s window).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TimeWindow {
    pub(super) t0: f64,
    pub(super) t1: f64,
}

impl TimeWindow {
    pub(super) fn contains(&self, t: f64) -> bool {
        (self.t0..=self.t1).contains(&t)
    }
}

/// The time history of `curves`; `None` without a single finite point.
pub(super) fn history_of<'a>(curves: impl IntoIterator<Item = &'a Curve>) -> Option<TimeHistory> {
    let mut span: Option<(f64, f64)> = None;
    let mut samples = 0;
    for curve in curves {
        samples = samples.max(curve.len());
        for &(t, _) in curve.iter().flatten() {
            span = Some(span.map_or((t, t), |(lo, hi)| (lo.min(t), hi.max(t))));
        }
    }
    let (oldest, newest) = span?;
    Some(TimeHistory {
        oldest,
        newest,
        samples,
    })
}

/// The value at time `t` on the straight line from `p` to `q`.
pub(super) fn interpolate(p: (f64, f64), q: (f64, f64), t: f64) -> f64 {
    p.1 + (q.1 - p.1) * (t - p.0) / (q.0 - p.0)
}

/// Where each curve crosses an edge of `window`: the value cut there from
/// the segment between the last point before and the first point after it.
fn edge_values<'a>(
    curves: impl Iterator<Item = &'a Curve>,
    window: TimeWindow,
) -> impl Iterator<Item = f64> {
    curves
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
pub(super) fn value_range<'a>(
    curves: impl Iterator<Item = &'a Curve> + Clone,
    window: TimeWindow,
) -> Option<(f64, f64)> {
    let inside = curves
        .clone()
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
