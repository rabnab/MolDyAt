// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The plot's time view (#367): zoom, pan, follow mode and their limits,
//! without a window.

use crate::graphics::energy_plot::plot_layout::TIME_SPAN;
use crate::graphics::energy_plot::time_view::{TimeHistory, TimeView, ZOOM_FACTOR};

/// Samples every 0.1 from `oldest` to `newest`.
fn history(oldest: f64, newest: f64) -> TimeHistory {
    TimeHistory {
        oldest,
        newest,
        samples: ((newest - oldest) / 0.1).round() as usize + 1,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn test_default_view_follows_with_the_default_span() {
    let view = TimeView::default();
    let h = history(0.0, 50.0);

    assert!(view.is_following() && view.is_default());
    assert_eq!(view.window(&h), (50.0 - TIME_SPAN, 50.0));
    // A history shorter than the span fills the plot from the left.
    assert_eq!(view.window(&history(3.0, 5.0)), (3.0, 3.0 + TIME_SPAN));
}

#[test]
fn test_wheel_forward_shows_less_time_and_backward_more() {
    let h = history(0.0, 50.0);
    let mut view = TimeView::default();

    view.zoom(1.0, 0.5, &h);
    assert!(close(view.span(), TIME_SPAN / ZOOM_FACTOR));
    view.zoom(-2.0, 0.5, &h);
    assert!(close(view.span(), TIME_SPAN * ZOOM_FACTOR));
    // Fractional steps (trackpads) scale the factor.
    let mut smooth = TimeView::default();
    smooth.zoom(0.5, 0.5, &h);
    assert!(close(smooth.span(), TIME_SPAN / ZOOM_FACTOR.sqrt()));
}

#[test]
fn test_zoom_in_follow_mode_keeps_the_newest_sample_at_the_right_edge() {
    let h = history(0.0, 50.0);
    let mut view = TimeView::default();

    view.zoom(3.0, 0.2, &h);
    let (t0, t1) = view.window(&h);
    assert!(view.is_following());
    assert_eq!(t1, 50.0);
    assert!(close(t1 - t0, view.span()));
}

#[test]
fn test_zoom_keeps_the_time_under_the_pointer() {
    let h = history(0.0, 50.0);
    let mut view = TimeView::default();
    view.pan(-20.0, &h); // [20, 30], not following
    let anchor = 0.25;
    let (t0, t1) = view.window(&h);
    let under_pointer = t0 + anchor * (t1 - t0);

    for notches in [1.0, 2.0, -1.0] {
        view.zoom(notches, anchor, &h);
        let (t0, t1) = view.window(&h);
        assert!(
            close(t0 + anchor * (t1 - t0), under_pointer),
            "{notches}: {t0}..{t1}"
        );
    }
}

#[test]
fn test_zoom_stays_within_the_limits() {
    let h = history(0.0, 50.0);
    let mut view = TimeView::default();

    view.zoom(100.0, 0.5, &h);
    assert!(close(view.span(), h.min_span()));
    assert!(close(h.min_span(), 0.4), "four sample intervals");
    view.zoom(-100.0, 0.5, &h);
    assert!(close(view.span(), 50.0), "the whole history");
    // A short history still allows the default span.
    let mut short = TimeView::default();
    short.zoom(-5.0, 0.5, &history(0.0, 2.0));
    assert!(close(short.span(), TIME_SPAN));
}

#[test]
fn test_pan_moves_the_range_and_stops_at_the_ends() {
    let h = history(0.0, 50.0);
    let mut view = TimeView::default();

    view.pan(-15.0, &h);
    assert_eq!(view.window(&h), (25.0, 35.0));
    assert!(!view.is_following());
    view.pan(-100.0, &h);
    assert_eq!(view.window(&h), (0.0, 10.0), "stops at the oldest sample");
}

#[test]
fn test_panning_back_to_the_newest_sample_follows_again() {
    let h = history(0.0, 50.0);
    let mut view = TimeView::default();
    view.pan(-5.0, &h);
    assert!(!view.is_following());

    view.pan(100.0, &h);
    assert!(view.is_following());
    assert_eq!(view.window(&h), (40.0, 50.0));
}

#[test]
fn test_reset_returns_to_the_default() {
    let h = history(0.0, 50.0);
    let mut view = TimeView::default();
    view.zoom(2.0, 0.5, &h);
    view.pan(-10.0, &h);

    view.reset();
    assert!(view.is_default());
}

#[test]
fn test_a_short_history_cannot_be_panned() {
    let h = history(0.0, 4.0);
    let mut view = TimeView::default();

    view.pan(-1.0, &h);
    assert!(view.is_following());
    assert_eq!(view.window(&h), (0.0, TIME_SPAN));
}

#[test]
fn test_following_view_moves_with_new_samples_and_keeps_its_span() {
    let mut view = TimeView::default();
    view.zoom(1.0, 0.5, &history(0.0, 50.0));
    let span = view.span();

    let (_, t1) = view.window(&history(0.0, 60.0));
    assert_eq!(t1, 60.0);
    assert!(close(view.span(), span));
}

#[test]
fn test_panned_view_stays_put_while_samples_arrive() {
    let mut view = TimeView::default();
    view.pan(-20.0, &history(0.0, 50.0));
    let before = view.window(&history(0.0, 50.0));

    assert_eq!(view.window(&history(0.0, 60.0)), before);
    // Until the samples it shows are evicted from the history.
    assert_eq!(view.window(&history(25.0, 60.0)), (25.0, 35.0));
}
