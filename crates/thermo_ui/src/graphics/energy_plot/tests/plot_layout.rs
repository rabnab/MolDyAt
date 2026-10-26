// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::energy_plot::plot_layout::*;
use crate::graphics::energy_plot::{DisplayMode, EnergyData, TimeView};

const SIZE: (u32, u32) = (1000, 700);

const HINTS: &str = "Esc: close  TAB / P: mode  HOME: reset";

fn data(kin: &[f32], pot: &[f32], times: &[f32]) -> EnergyData {
    let mut data = EnergyData::new();
    data.v_kin.extend(kin.iter().copied());
    data.v_pot.extend(pot.iter().copied());
    data.sample_times.extend(times.iter().copied());
    data.time = 1.5;
    data
}

fn build(data: &EnergyData, mode: DisplayMode, size: (u32, u32), scale_factor: f64) -> PlotScene {
    PlotScene::build(data, mode, size, scale_factor, HINTS, &TimeView::default())
}

fn has_label(scene: &PlotScene, text: &str) -> bool {
    scene.labels.iter().any(|label| label.text == text)
}

fn segments_of(scene: &PlotScene, color: Rgba) -> Vec<PlotSegment> {
    scene
        .segments
        .iter()
        .copied()
        .filter(|s| s.color == color)
        .collect()
}

#[test]
fn test_empty_data_shows_the_waiting_message_and_no_plot() {
    let scene = build(&EnergyData::new(), DisplayMode::All, SIZE, 1.0);

    assert!(has_label(&scene, "Energy plot - all"));
    assert!(has_label(&scene, "Waiting for simulation data..."));
    assert!(scene.plot_area.is_none());
    assert!(scene.segments.is_empty());
}

#[test]
fn test_all_mode_draws_three_curves_with_legend() {
    let scene = build(
        &data(&[10.0, 20.0, 30.0], &[5.0, 10.0, 15.0], &[0.0, 0.1, 0.2]),
        DisplayMode::All,
        SIZE,
        1.0,
    );

    for name in ["E_kin", "E_pot", "E_tot"] {
        assert!(has_label(&scene, name), "legend lacks {name}");
    }
    // Three samples: two segments per curve.
    assert_eq!(segments_of(&scene, COLOR_KINETIC).len(), 2);
    assert_eq!(segments_of(&scene, COLOR_POTENTIAL).len(), 2);
    assert_eq!(segments_of(&scene, COLOR_TOTAL).len(), 2);
    assert!(scene.labels.iter().any(|l| l.text.contains("t = 1.50")));
    assert!(!has_label(&scene, "Waiting for simulation data..."));
}

#[test]
fn test_single_mode_draws_only_its_curve() {
    let scene = build(
        &data(&[1.0, 2.0], &[3.0, 4.0], &[0.0, 0.5]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );

    assert_eq!(segments_of(&scene, COLOR_KINETIC).len(), 1);
    assert!(segments_of(&scene, COLOR_POTENTIAL).is_empty());
    assert!(segments_of(&scene, COLOR_TOTAL).is_empty());
    assert!(!has_label(&scene, "E_pot"));
}

#[test]
fn test_short_history_starts_at_the_left_edge_and_keeps_the_scale() {
    // 3 time units of history in a span of TIME_SPAN (10): the curve covers
    // the left 30% and does not stretch.
    let scene = build(
        &data(&[-4.0, 7.0, 2.0, 9.0], &[0.0; 4], &[1.0, 2.0, 3.0, 4.0]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );
    let area = scene.plot_area.expect("plot area");
    let curve = segments_of(&scene, COLOR_KINETIC);

    for s in &curve {
        assert!(
            area.contains(s.from) && area.contains(s.to),
            "{s:?} outside {area:?}"
        );
    }
    let first = curve.first().unwrap().from[0];
    let last = curve.last().unwrap().to[0];
    assert!((first - area.left).abs() < 1e-3);
    let expected = area.left + area.width() * (3.0 / TIME_SPAN as f32);
    assert!((last - expected).abs() < 1e-3, "{last} vs {expected}");
    // Larger values are higher up (smaller y).
    assert!(curve[0].to[1] < curve[0].from[1]);
}

/// Once the history is longer than the span, the newest sample stays at the
/// right edge and older samples scroll out on the left.
#[test]
fn test_long_history_scrolls_with_the_newest_sample_at_the_right_edge() {
    let times: Vec<f32> = (0..=60).map(|i| i as f32 * 0.5).collect();
    let values: Vec<f32> = times.iter().map(|t| t.sin()).collect();
    let scene = build(
        &data(&values, &vec![0.0; times.len()], &times),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );
    let area = scene.plot_area.expect("plot area");
    let curve = segments_of(&scene, COLOR_KINETIC);

    assert!((curve.last().unwrap().to[0] - area.right).abs() < 1e-3);
    // t = 20 .. 30 is shown: 20 segments, the first starting at the left edge.
    assert_eq!(curve.len(), 20);
    assert!((curve[0].from[0] - area.left).abs() < 1e-3);
    assert!(curve.iter().all(|s| s.from[0] >= area.left - 1e-3));
    assert!(has_label(&scene, "20") && has_label(&scene, "30"));
    assert!(!has_label(&scene, "19"));
}

/// A spike that just left the window must not pull the curve, cut at the
/// left edge, out of the plot area (#365 re-review).
#[test]
fn test_curve_cut_at_the_left_edge_stays_inside_the_plot_area() {
    // The window is [2, 12]; the cut at t = 2 lies between 1000 and 1.
    let scene = build(
        &data(&[1000.0, 1.0, 1.1], &[0.0; 3], &[0.0, 5.0, 12.0]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );
    let area = scene.plot_area.expect("plot area");
    let curve = segments_of(&scene, COLOR_KINETIC);

    assert_eq!(curve.len(), 2);
    for s in &curve {
        assert!(
            area.contains(s.from) && area.contains(s.to),
            "{s:?} outside {area:?}"
        );
    }
    // The cut value (600.4) is the highest point drawn.
    assert!(curve[0].from[1] < curve[1].from[1]);
}

/// The goal of the fixed span (#365): halving the time step only scrolls
/// the plot faster; an instant lands on the same pixel column.
#[test]
fn test_time_step_does_not_change_the_scale() {
    let run = |dt: f32| {
        let times: Vec<f32> = (0..=(20.0 / dt) as usize).map(|i| i as f32 * dt).collect();
        let values: Vec<f32> = times.iter().map(|t| (t * 0.3).cos()).collect();
        build(
            &data(&values, &vec![0.0; times.len()], &times),
            DisplayMode::Kin,
            SIZE,
            1.0,
        )
    };
    let (coarse, fine) = (run(0.1), run(0.05));
    let area = coarse.plot_area.unwrap();
    // t = 15 in the window [10, 20].
    let x15 = area.left + area.width() * 0.5;
    for scene in [&coarse, &fine] {
        let curve = segments_of(scene, COLOR_KINETIC);
        assert!(
            curve.iter().any(|s| (s.to[0] - x15).abs() < 1e-2),
            "no point at t = 15"
        );
    }
    let tick_labels = |scene: &PlotScene| -> Vec<(String, [f32; 2])> {
        let area = scene.plot_area.unwrap();
        scene
            .labels
            .iter()
            .filter(|l| l.origin[1] > area.bottom && l.origin[1] < area.bottom + 2.0 * l.height())
            .map(|l| (l.text.clone(), l.origin))
            .collect()
    };
    assert_eq!(tick_labels(&coarse), tick_labels(&fine));
}

/// x tick marks: vertical axis-coloured segments below the plot area.
fn x_tick_marks(scene: &PlotScene) -> Vec<f32> {
    let area = scene.plot_area.unwrap();
    scene
        .segments
        .iter()
        .filter(|s| s.color == COLOR_AXIS && s.from[0] == s.to[0] && s.from[1] >= area.bottom - 1.0)
        .filter(|s| s.to[1] > s.from[1] + 0.5 && s.to[1] - s.from[1] < 40.0)
        .map(|s| s.from[0])
        .collect()
}

#[test]
fn test_x_ticks_every_half_unit_when_there_is_room() {
    let d = data(&[1.0, 2.0], &[0.0; 2], &[0.0, 0.1]);
    let wide = build(&d, DisplayMode::Kin, SIZE, 1.0);
    let narrow = build(&d, DisplayMode::Kin, (500, 400), 1.0);

    // [0, 10]: 21 ticks at 0.5 apart, 11 at 1 apart.
    assert_eq!(x_tick_marks(&wide).len(), 21);
    assert_eq!(x_tick_marks(&narrow).len(), 11);
}

#[test]
fn test_x_tick_labels_do_not_overlap() {
    let d = data(&[1.0, 2.0], &[0.0; 2], &[100.0, 100.1]);
    for size in [SIZE, (500, 400), (1600, 900)] {
        let scene = build(&d, DisplayMode::Kin, size, 1.0);
        let area = scene.plot_area.unwrap();
        let mut labels: Vec<&PlotLabel> = scene
            .labels
            .iter()
            .filter(|l| l.origin[1] > area.bottom && l.origin[1] < area.bottom + 2.0 * l.height())
            .filter(|l| l.color == COLOR_TEXT)
            .collect();
        labels.sort_by(|a, b| a.origin[0].total_cmp(&b.origin[0]));
        assert!(labels.len() >= 2, "{size:?}");
        for pair in labels.windows(2) {
            assert!(
                pair[0].origin[0] + pair[0].width() < pair[1].origin[0],
                "{size:?}: {:?} overlaps {:?}",
                pair[0],
                pair[1]
            );
        }
    }
}

#[test]
fn test_zero_line_is_drawn_when_the_range_crosses_zero() {
    let crossing = build(
        &data(&[-5.0, 5.0], &[0.0; 2], &[0.0, 0.5]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );
    let positive = build(
        &data(&[5.0, 6.0], &[0.0; 2], &[0.0, 0.5]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );

    assert_eq!(segments_of(&crossing, COLOR_ZERO).len(), 1);
    assert!(segments_of(&positive, COLOR_ZERO).is_empty());
}

#[test]
fn test_x_tick_labels_are_times() {
    let scene = build(
        &data(&[1.0; 5], &[0.0; 5], &[0.0, 0.5, 1.0, 1.5, 2.0]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );

    // The window is [0, TIME_SPAN]; labels every whole unit.
    for label in ["0", "1", "2", "10"] {
        assert!(has_label(&scene, label), "{label}");
    }
    assert!(scene.labels.iter().any(|l| l.text.contains("x: time")));
}

/// The goal of the time axis (#365 review): runs over the same time span
/// with different time steps, so a different number of samples, put the
/// same instants at the same pixels and get the same ticks.
#[test]
fn test_same_time_span_gives_the_same_axis_whatever_the_sample_count() {
    let coarse = build(
        &data(&[1.0, 3.0, 2.0], &[0.0; 3], &[0.0, 0.5, 1.0]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );
    let fine = build(
        &data(
            &[1.0, 2.0, 3.0, 2.5, 2.0],
            &[0.0; 5],
            &[0.0, 0.25, 0.5, 0.75, 1.0],
        ),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );
    let coarse_curve = segments_of(&coarse, COLOR_KINETIC);
    let fine_curve = segments_of(&fine, COLOR_KINETIC);

    let first = |c: &[PlotSegment]| c.first().unwrap().from;
    let last = |c: &[PlotSegment]| c.last().unwrap().to;
    assert_eq!(first(&coarse_curve), first(&fine_curve));
    assert_eq!(last(&coarse_curve), last(&fine_curve));
    // t = 0.5: the end of coarse segment 0, of fine segment 1.
    assert_eq!(coarse_curve[0].to, fine_curve[1].to);

    let ticks = |scene: &PlotScene| -> Vec<(String, [f32; 2])> {
        let area = scene.plot_area.unwrap();
        scene
            .labels
            .iter()
            .filter(|l| l.origin[1] > area.bottom && l.color == COLOR_TEXT)
            .map(|l| (l.text.clone(), l.origin))
            .collect()
    };
    assert_eq!(ticks(&coarse), ticks(&fine));
}

#[test]
fn test_samples_without_a_finite_time_are_left_out() {
    let scene = build(
        &data(&[1.0, 2.0, 3.0, 4.0], &[0.0; 4], &[0.0, f32::NAN, 2.0, 3.0]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );

    // t = 0 stands alone (a dot), 2-3 stays joined; nothing touches t = NaN.
    let curve = segments_of(&scene, COLOR_KINETIC);
    assert_eq!(curve.len(), 2);
    assert_eq!(curve[0].from, curve[0].to);
    assert!(
        curve
            .iter()
            .all(|s| s.from[0].is_finite() && s.to[0].is_finite())
    );
}

#[test]
fn test_non_finite_values_break_the_curve() {
    let scene = build(
        &data(
            &[1.0, 2.0, f32::NAN, 3.0, 4.0],
            &[0.0; 5],
            &[0.0, 1.0, 2.0, 3.0, 4.0],
        ),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );

    // 0-1 and 3-4; nothing across the gap.
    assert_eq!(segments_of(&scene, COLOR_KINETIC).len(), 2);
}

#[test]
fn test_single_sample_is_a_dot() {
    let scene = build(&data(&[3.0], &[1.0], &[7.0]), DisplayMode::Kin, SIZE, 1.0);
    let curve = segments_of(&scene, COLOR_KINETIC);

    assert_eq!(curve.len(), 1);
    assert_eq!(curve[0].from, curve[0].to);
}

#[test]
fn test_status_text_is_shown_below_the_plot() {
    let mut d = data(&[1.0, 2.0], &[0.0; 2], &[0.0, 0.5]);
    d.status_text = Some("fps 60\n#Balls: 12".to_string());
    let scene = build(&d, DisplayMode::Kin, SIZE, 1.0);
    let area = scene.plot_area.expect("plot area");

    let status: Vec<_> = scene
        .labels
        .iter()
        .filter(|l| l.color == COLOR_STATUS)
        .collect();
    assert_eq!(status.len(), 2);
    assert!(status.iter().all(|l| l.origin[1] > area.bottom));
    assert!(
        status
            .iter()
            .all(|l| l.origin[1] + l.height() <= SIZE.1 as f32)
    );
}

#[test]
fn test_key_hints_sit_on_the_title_line_only_when_they_fit() {
    let d = data(&[1.0, 2.0], &[0.0; 2], &[0.0, 0.5]);
    let wide = build(&d, DisplayMode::Kin, SIZE, 1.0);
    let narrow = build(&d, DisplayMode::Kin, (400, 300), 1.0);

    let hints = wide
        .labels
        .iter()
        .find(|l| l.text == HINTS)
        .expect("key hints");
    assert!(hints.origin[0] + hints.width() <= SIZE.0 as f32);
    assert!(!has_label(&narrow, HINTS));
}

#[test]
fn test_key_hints_are_the_ones_passed_in() {
    let d = data(&[1.0, 2.0], &[0.0; 2], &[0.0, 0.5]);
    let rebound = PlotScene::build(
        &d,
        DisplayMode::Kin,
        SIZE,
        1.0,
        "Esc: close  X: mode",
        &TimeView::default(),
    );
    let none = PlotScene::build(&d, DisplayMode::Kin, SIZE, 1.0, "", &TimeView::default());

    assert!(has_label(&rebound, "Esc: close  X: mode"));
    assert!(!has_label(&rebound, HINTS));
    assert!(none.labels.iter().all(|l| !l.text.is_empty()));
}

#[test]
fn test_labels_and_thin_lines_sit_on_whole_pixels() {
    let d = data(&[-3.3, 7.7, 1.1], &[0.0; 3], &[0.0, 0.37, 1.13]);
    for scale_factor in [1.0, 1.25, 2.0] {
        let scene = build(&d, DisplayMode::Kin, (777, 555), scale_factor);
        for label in &scene.labels {
            assert_eq!(label.origin, label.origin.map(f32::round), "{label:?}");
        }
        for s in scene.segments.iter().filter(|s| s.color != COLOR_KINETIC) {
            assert_eq!(s.width, s.width.round(), "{s:?}");
            // Odd widths on pixel centres, even widths on pixel edges.
            let on_grid = |v: f32| {
                let offset = if s.width % 2.0 == 1.0 { 0.5 } else { 0.0 };
                (v - offset).fract() == 0.0
            };
            if s.from[0] == s.to[0] {
                assert!(on_grid(s.from[0]), "{s:?}");
            } else if s.from[1] == s.to[1] {
                assert!(on_grid(s.from[1]), "{s:?}");
            }
        }
    }
}

#[test]
fn test_labels_stay_inside_a_default_sized_window() {
    let mut d = data(&[10.0, 20.0, 30.0], &[5.0, 10.0, 15.0], &[0.0, 0.1, 0.2]);
    d.time = 123.45;
    let scene = build(&d, DisplayMode::All, (1024, 576), 1.0);

    for label in &scene.labels {
        assert!(label.origin[0] >= 0.0, "{label:?}");
        assert!(label.origin[0] + label.width() <= 1024.0, "{label:?}");
    }
}

#[test]
fn test_tiny_window_has_no_plot_area() {
    let scene = build(
        &data(&[1.0, 2.0], &[0.0; 2], &[0.0, 0.5]),
        DisplayMode::Kin,
        (60, 60),
        1.0,
    );

    assert!(scene.plot_area.is_none());
    assert!(segments_of(&scene, COLOR_KINETIC).is_empty());
}

#[test]
fn test_text_scale_is_a_whole_number_of_pixels() {
    assert_eq!(text_scale(1.0), 2.0);
    assert_eq!(text_scale(0.5), 2.0);
    assert_eq!(text_scale(1.25), 2.0);
    assert_eq!(text_scale(2.0), 4.0);
}

#[test]
fn test_nice_ticks_are_round_and_inside_the_range() {
    let (step, ticks) = nice_ticks(0.3, 9.7, 5);
    assert_eq!(step, 2.0);
    assert_eq!(ticks, vec![2.0, 4.0, 6.0, 8.0]);

    let (step, ticks) = nice_ticks(-12.0, 37.0, 5);
    assert_eq!(step, 10.0);
    assert_eq!(ticks, vec![-10.0, 0.0, 10.0, 20.0, 30.0]);

    let (step, ticks) = nice_ticks(0.0, 0.004, 4);
    assert!((step - 0.001).abs() < 1e-12);
    assert_eq!(ticks.len(), 5);
}

#[test]
fn test_nice_ticks_stop_where_the_spacing_is_below_the_precision() {
    // 1e17 + k * 0.2 is 1e17 for every k: without a bound the ticks would
    // never pass `hi`.
    let (_, ticks) = nice_ticks(1e17, 1e17 + 1.0, 5);
    assert!(ticks.len() <= 12, "{} ticks", ticks.len());
}

#[test]
fn test_nice_ticks_of_an_empty_range_is_one_tick() {
    assert_eq!(nice_ticks(3.0, 3.0, 5), (0.0, vec![3.0]));
}

#[test]
fn test_format_tick_uses_the_decimals_of_the_step() {
    assert_eq!(format_tick(20.0, 10.0), "20");
    assert_eq!(format_tick(2.5, 0.5), "2.5");
    assert_eq!(format_tick(0.04, 0.02), "0.04");
    assert_eq!(format_tick(1e-17, 0.5), "0");
    assert_eq!(format_tick(300000.0, 100000.0), "3.00e5");
}

/// A view `pan` time units back from the default (#367).
fn panned(pan: f64, d: &EnergyData) -> PlotScene {
    let scene = build(d, DisplayMode::Kin, SIZE, 1.0);
    let mut view = TimeView::default();
    view.pan(pan, &scene.history.expect("history"));
    PlotScene::build(d, DisplayMode::Kin, SIZE, 1.0, HINTS, &view)
}

/// Samples every 0.5 from `start`, 41 of them; a spike of 1000 at the
/// second one.
fn spiky_data(start: f32) -> EnergyData {
    let times: Vec<f32> = (0..41).map(|i| start + i as f32 * 0.5).collect();
    let mut values: Vec<f32> = times.iter().map(|t| t.sin()).collect();
    values[1] = 1000.0;
    data(&values, &vec![0.0; times.len()], &times)
}

#[test]
fn test_panned_view_draws_only_its_range_and_fits_y_to_it() {
    // History 0..20, view [5, 15]: the spike at t = 0.5 is not shown.
    let scene = panned(-5.0, &spiky_data(0.0));
    let area = scene.plot_area.expect("plot area");
    let curve = segments_of(&scene, COLOR_KINETIC);

    assert_eq!(scene.time_window, Some((5.0, 15.0)));
    assert_eq!(curve.len(), 20, "samples 5.0 ..= 15.0");
    for s in &curve {
        assert!(area.contains(s.from) && area.contains(s.to), "{s:?}");
    }
    // The y labels fit sin values, not the spike.
    let y_labels: Vec<f64> = scene
        .labels
        .iter()
        .filter(|l| l.origin[0] < area.left && l.origin[1] < area.bottom)
        .filter_map(|l| l.text.parse().ok())
        .collect();
    assert!(!y_labels.is_empty() && y_labels.iter().all(|v| v.abs() <= 2.0));
    // The x tick labels lie inside the range.
    for label in ["5", "10", "15"] {
        assert!(has_label(&scene, label), "{label}");
    }
    assert!(!has_label(&scene, "4") && !has_label(&scene, "16"));
}

#[test]
fn test_panned_view_cuts_the_curves_at_both_edges() {
    // Samples at 0.25, 0.75, …: the edges 5 and 15 fall between samples.
    let scene = panned(-5.25, &spiky_data(0.25));
    let area = scene.plot_area.expect("plot area");
    let curve = segments_of(&scene, COLOR_KINETIC);

    assert!((curve.first().unwrap().from[0] - area.left).abs() < 1e-3);
    assert!((curve.last().unwrap().to[0] - area.right).abs() < 1e-3);
    for s in &curve {
        assert!(area.contains(s.from) && area.contains(s.to), "{s:?}");
    }
}

#[test]
fn test_legend_shows_a_changed_view() {
    let d = spiky_data(0.0);
    let default = build(&d, DisplayMode::Kin, SIZE, 1.0);
    let paused = panned(-5.0, &d);

    assert!(
        default
            .labels
            .iter()
            .all(|l| !l.text.contains("paused") && !l.text.contains("zoomed"))
    );
    assert!(
        paused
            .labels
            .iter()
            .any(|l| l.text.contains("paused 5.0-15.0")),
        "{:?}",
        paused.labels
    );
}

/// With all three curves and a paused view, the legend still fits the
/// default window width (the x axis note gives way).
#[test]
fn test_legend_with_a_view_fits_a_narrow_window() {
    let d = spiky_data(0.0);
    let probe = build(&d, DisplayMode::All, (960, 700), 1.0);
    let mut view = TimeView::default();
    view.pan(-5.0, &probe.history.unwrap());
    let scene = PlotScene::build(&d, DisplayMode::All, (960, 700), 1.0, HINTS, &view);

    let legend = scene
        .labels
        .iter()
        .find(|l| l.text.contains("paused"))
        .expect("view in the legend");
    assert!(legend.origin[0] + legend.width() <= 960.0, "{legend:?}");
}

#[test]
fn test_scene_reports_its_history_and_time_window() {
    let scene = build(&spiky_data(2.0), DisplayMode::Kin, SIZE, 1.0);
    let history = scene.history.expect("history");

    assert_eq!((history.oldest, history.newest), (2.0, 22.0));
    assert_eq!(history.samples, 41);
    assert_eq!(scene.time_window, Some((12.0, 22.0)));
}
