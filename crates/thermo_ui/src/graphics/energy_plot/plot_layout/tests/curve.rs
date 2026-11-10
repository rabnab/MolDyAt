// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::energy_plot::plot_layout::curve::{
    Curve, TimeWindow, downsample, history_of, interpolate, value_range,
};

const WINDOW: TimeWindow = TimeWindow { t0: 0.0, t1: 10.0 };

#[test]
fn test_history_of_without_finite_points_is_none() {
    let curves: Vec<Curve> = vec![vec![None, None]];
    assert!(history_of(&curves).is_none());
}

#[test]
fn test_history_of_spans_all_curves_and_counts_the_longest() {
    let curves: Vec<Curve> = vec![
        vec![Some((2.0, 1.0)), Some((3.0, 1.0))],
        vec![Some((1.0, 1.0)), None, Some((4.0, 1.0))],
    ];
    let history = history_of(&curves).expect("finite points");
    assert_eq!(
        (history.oldest, history.newest, history.samples),
        (1.0, 4.0, 3)
    );
}

#[test]
fn test_interpolate_is_linear_between_the_points() {
    assert_eq!(interpolate((0.0, 0.0), (2.0, 4.0), 1.0), 2.0);
}

#[test]
fn test_value_range_includes_the_cut_at_the_window_edge() {
    // The line from (-5, 0) to (5, 10) crosses t0 = 0 at 5; the point at 5
    // is inside, so the range spans 5..10 plus 5% room.
    let curves: Vec<Curve> = vec![vec![Some((-5.0, 0.0)), Some((5.0, 10.0))]];
    let (lo, hi) = value_range(&curves, WINDOW).expect("points inside");
    assert!(
        (lo - 4.75).abs() < 1e-9 && (hi - 10.25).abs() < 1e-9,
        "{lo} {hi}"
    );
}

#[test]
fn test_value_range_is_none_when_nothing_is_inside() {
    let curves: Vec<Curve> = vec![vec![Some((20.0, 1.0)), Some((30.0, 2.0))]];
    assert!(value_range(&curves, WINDOW).is_none());
}

#[test]
fn test_downsample_keeps_a_curve_with_few_points_as_it_is() {
    let curve: Curve = (0..4).map(|i| Some((f64::from(i), 1.0))).collect();
    assert_eq!(&*downsample(&curve, WINDOW, 100), curve.as_slice());
}

#[test]
fn test_downsample_keeps_the_spike_of_a_column() {
    // 1000 points in 10 columns; one spike must survive.
    let mut curve: Curve = (0..1000)
        .map(|i| Some((f64::from(i) * 0.01, 1.0)))
        .collect();
    curve[505] = Some((5.05, 99.0));
    let thinned = downsample(&curve, WINDOW, 10);
    assert!(thinned.len() <= 2 * 10 + 2);
    assert!(thinned.iter().flatten().any(|&(_, v)| v == 99.0));
}
