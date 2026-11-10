// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::energy_plot::PlotRect;
use crate::graphics::energy_plot::plot_layout::curve::TimeWindow;
use crate::graphics::energy_plot::plot_layout::ticks::{
    XAxis, fitting_hints, is_multiple, multiples_in,
};

#[test]
fn test_x_axis_maps_the_window_onto_the_area_width() {
    let area = PlotRect {
        left: 100.0,
        top: 0.0,
        right: 300.0,
        bottom: 50.0,
    };
    let axis = XAxis::new(TimeWindow { t0: 10.0, t1: 20.0 }, area);
    assert_eq!(axis.scale(), 20.0);
    assert_eq!(axis.x_of(10.0), 100.0);
    assert_eq!(axis.x_of(20.0), 300.0);
}

#[test]
fn test_multiples_in_lists_the_ticks_inside_the_window() {
    let window = TimeWindow { t0: 0.7, t1: 3.1 };
    assert_eq!(multiples_in(1.0, window), vec![1.0, 2.0, 3.0]);
}

#[test]
fn test_is_multiple_tolerates_rounding() {
    assert!(is_multiple(0.3, 0.1));
    assert!(!is_multiple(0.35, 0.1));
}

#[test]
fn test_fitting_hints_keeps_the_leading_hints_that_fit() {
    let hints = "Esc: close  P: preset  R: reset";
    assert_eq!(
        fitting_hints(hints, |t| t.len() <= 22),
        "Esc: close  P: preset"
    );
    assert_eq!(fitting_hints(hints, |t| t.len() <= 3), "");
}
