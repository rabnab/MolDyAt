// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::energy_plot::plot_layout::*;
use crate::graphics::energy_plot::{EnergyData, PlotSelection, TimeView};

const SIZE: (u32, u32) = (1000, 700);

const HINTS: &str = "Esc: close  TAB / P: mode  HOME: reset";

/// E_kin, E_pot and their sum E_tot, as the registry records them.
fn data(kin: &[f32], pot: &[f32], times: &[f32]) -> EnergyData {
    let mut data = EnergyData::new();
    data.insert_series("E_kin", Some("J"), kin.iter().copied());
    data.insert_series("E_pot", Some("J"), pot.iter().copied());
    data.insert_series("E_tot", Some("J"), kin.iter().zip(pot).map(|(k, p)| k + p));
    data.sample_times.extend(times.iter().copied());
    data.time = 1.5;
    data
}

/// The plots of the old display modes: one plot of E_kin, or of all three
/// energies.
#[derive(Clone, Copy)]
enum DisplayMode {
    Kin,
    All,
}

fn selection(mode: DisplayMode) -> PlotSelection {
    match mode {
        DisplayMode::Kin => PlotSelection::from_panels(&[&["E_kin"]]),
        DisplayMode::All => PlotSelection::from_panels(&[&["E_kin", "E_pot", "E_tot"]]),
    }
}

fn build(data: &EnergyData, mode: DisplayMode, size: (u32, u32), scale_factor: f64) -> PlotScene {
    PlotScene::build(
        data,
        &selection(mode),
        size,
        scale_factor,
        HINTS,
        &TimeView::default(),
    )
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

    assert!(has_label(&scene, "Plot - E_kin, E_pot, E_tot"));
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
        &selection(DisplayMode::Kin),
        SIZE,
        1.0,
        "Esc: close  X: mode",
        &TimeView::default(),
    );
    let none = PlotScene::build(
        &d,
        &selection(DisplayMode::Kin),
        SIZE,
        1.0,
        "",
        &TimeView::default(),
    );

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
    PlotScene::build(d, &selection(DisplayMode::Kin), SIZE, 1.0, HINTS, &view)
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
    // The current time is not part of a paused (frozen) plot.
    assert!(paused.labels.iter().all(|l| !l.text.contains("t = ")));
    assert!(default.labels.iter().any(|l| l.text.contains("t = ")));
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
    let scene = PlotScene::build(
        &d,
        &selection(DisplayMode::All),
        (960, 700),
        1.0,
        HINTS,
        &view,
    );

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

/// `n` samples over the default time span, flat at 1.0 but for a one-sample
/// spike to 50.0 at `spike` and a dip to -20.0 at `dip`.
fn dense_data(n: usize, spike: usize, dip: usize) -> EnergyData {
    let times: Vec<f32> = (0..n)
        .map(|i| i as f32 * TIME_SPAN as f32 / (n - 1) as f32)
        .collect();
    let kin: Vec<f32> = (0..n)
        .map(|i| match i {
            _ if i == spike => 50.0,
            _ if i == dip => -20.0,
            _ => 1.0 + (i % 7) as f32 * 0.01,
        })
        .collect();
    data(&kin, &vec![0.0; n], &times)
}

/// The y pixel of each end of the curve's segments.
fn curve_ys(scene: &PlotScene) -> Vec<f32> {
    segments_of(scene, COLOR_KINETIC)
        .iter()
        .flat_map(|s| [s.from[1], s.to[1]])
        .collect()
}

#[test]
fn test_dense_curve_is_drawn_with_at_most_two_points_per_pixel_column() {
    let n = 20_000;
    let scene = build(&dense_data(n, 7_777, 12_345), DisplayMode::Kin, SIZE, 1.0);
    let area = scene.plot_area.unwrap();
    let segments = segments_of(&scene, COLOR_KINETIC).len();
    assert!(
        segments < 2 * area.width().ceil() as usize,
        "{segments} segments for {} columns",
        area.width()
    );

    // The spike and the dip, one sample each, are the extremes drawn.
    let ys = curve_ys(&scene);
    let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let y_range = area.height();
    assert!(top < area.top + y_range * 0.1, "the spike is drawn: {top}");
    assert!(
        bottom > area.bottom - y_range * 0.1,
        "the dip is drawn: {bottom}"
    );
}

#[test]
fn test_dense_curve_keeps_the_extremes_of_the_full_curve() {
    let n = 20_000;
    let dense = build(&dense_data(n, 3_000, 15_000), DisplayMode::Kin, SIZE, 1.0);
    let ys = curve_ys(&dense);
    // The y range comes from every sample; the drawn curve reaches both its
    // ends (5% room above and below).
    let area = dense.plot_area.unwrap();
    let room = area.height() / 1.1 * 0.05;
    let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!((top - (area.top + room)).abs() < 1.0, "max drawn: {top}");
    assert!(
        (bottom - (area.bottom - room)).abs() < 1.0,
        "min drawn: {bottom}"
    );
}

#[test]
fn test_curve_with_fewer_samples_than_columns_is_drawn_as_it_is() {
    let n = 300;
    let scene = build(&dense_data(n, 100, 200), DisplayMode::Kin, SIZE, 1.0);
    assert!(scene.plot_area.unwrap().width() > 2.0 * n as f32);
    assert_eq!(segments_of(&scene, COLOR_KINETIC).len(), n - 1);
}

// ── Several plots (#339) ────────────────────────────────────────────────────

/// The energies plus wall momentum and a species count, 21 samples over
/// 10 time units.
fn mixed_data() -> EnergyData {
    let times: Vec<f32> = (0..21).map(|i| i as f32 * 0.5).collect();
    let mut d = data(&[1.0; 21], &[-3.0; 21], &times);
    d.insert_series(
        "wall_momentum",
        Some("kg·m/s"),
        (0..21).map(|i| i as f32 * 100.0),
    );
    d.insert_series("N2:count", None, [50.0; 21]);
    d
}

fn build_selection(d: &EnergyData, selection: &PlotSelection) -> PlotScene {
    PlotScene::build(d, selection, SIZE, 1.0, HINTS, &TimeView::default())
}

#[test]
fn test_plots_are_stacked_with_shared_left_and_right_edges() {
    let selection =
        PlotSelection::from_panels(&[&["E_kin", "E_pot"], &["wall_momentum"], &["N2:count"]]);
    let scene = build_selection(&mixed_data(), &selection);
    assert_eq!(scene.panels.len(), 3);
    for pair in scene.panels.windows(2) {
        let (upper, lower) = (pair[0].area, pair[1].area);
        assert_eq!((upper.left, upper.right), (lower.left, lower.right));
        assert!(upper.bottom < lower.top, "{upper:?} above {lower:?}");
    }
    assert_eq!(
        scene.panels.iter().map(|p| p.index).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    let all = scene.plot_area.unwrap();
    assert_eq!(all.top, scene.panels[0].area.top);
    assert_eq!(all.bottom, scene.panels[2].area.bottom);
    assert!(has_label(&scene, "Plot - 3 plots"));
}

#[test]
fn test_time_labels_only_under_the_last_plot() {
    let selection = PlotSelection::from_panels(&[&["E_kin"], &["wall_momentum"]]);
    let scene = build_selection(&mixed_data(), &selection);
    let upper = scene.panels[0].area;
    let lower = scene.panels[1].area;
    let time_labels: Vec<&PlotLabel> = scene
        .labels
        .iter()
        .filter(|l| l.text == "2" || l.text == "4" || l.text == "6")
        .filter(|l| l.origin[1] > upper.bottom)
        .collect();
    assert!(!time_labels.is_empty());
    assert!(
        time_labels.iter().all(|l| l.origin[1] > lower.bottom),
        "{time_labels:?}"
    );
}

#[test]
fn test_each_plot_scales_to_its_own_series() {
    let selection = PlotSelection::from_panels(&[&["E_kin", "E_pot"], &["wall_momentum"]]);
    let d = mixed_data();
    let scene = build_selection(&d, &selection);
    // The energies' plot has ticks around -3..1, the momentum plot up to 2000.
    let ticks_in = |area: PlotRect| -> Vec<String> {
        scene
            .labels
            .iter()
            .filter(|l| {
                l.origin[0] < area.left
                    && l.origin[1] + l.height() > area.top
                    && l.origin[1] < area.bottom
            })
            .map(|l| l.text.clone())
            .collect()
    };
    let energies = ticks_in(scene.panels[0].area);
    let momentum = ticks_in(scene.panels[1].area);
    assert!(
        energies.iter().any(|t| t == "-3" || t == "-2"),
        "{energies:?}"
    );
    assert!(
        momentum.iter().any(|t| t == "2000" || t == "1000"),
        "{momentum:?}"
    );
    // The curves of each plot reach from its bottom to its top (5% room).
    for (panel, color) in [(0, COLOR_POTENTIAL), (1, series_color(&d, "wall_momentum"))] {
        let area = scene.panels[panel].area;
        for s in segments_of(&scene, color) {
            for [_, y] in [s.from, s.to] {
                assert!(area.top <= y && y <= area.bottom, "{s:?} in {area:?}");
            }
        }
    }
}

#[test]
fn test_legend_entries_are_recorded_where_they_are_drawn() {
    let selection = PlotSelection::from_panels(&[&["E_kin", "E_pot"], &["N2:count"]]);
    let scene = build_selection(&mixed_data(), &selection);
    let names: Vec<(usize, &str)> = scene
        .legend
        .iter()
        .map(|e| (e.panel, e.series.as_str()))
        .collect();
    assert_eq!(names, [(0, "E_kin"), (0, "E_pot"), (1, "N2:count")]);
    for entry in &scene.legend {
        let label = scene
            .labels
            .iter()
            .find(|l| l.text == entry.series)
            .expect("legend label");
        assert_eq!(entry.rect.left, label.origin[0]);
        assert_eq!(entry.rect.right, label.origin[0] + label.width());
        let area = scene.panels[entry.panel].area;
        assert!(
            entry.rect.bottom <= area.top,
            "the legend sits above its plot"
        );
    }
}

#[test]
fn test_focus_mark_and_units_in_the_legend() {
    let mut selection =
        PlotSelection::from_panels(&[&["E_kin", "E_pot"], &["N2:count", "wall_momentum"]]);
    selection.focus(1);
    let scene = build_selection(&mixed_data(), &selection);
    let marks: Vec<&PlotLabel> = scene.labels.iter().filter(|l| l.text == "> ").collect();
    assert_eq!(marks.len(), 1);
    assert!(
        marks[0].origin[1] > scene.panels[0].area.bottom,
        "the second plot is focused"
    );
    assert!(has_label(&scene, "[J]"), "both energies are in J");
    assert!(
        !scene.labels.iter().any(|l| l.text.contains("kg·m/s")),
        "mixed units: none shown"
    );

    let single = build_selection(&mixed_data(), &PlotSelection::from_panels(&[&["E_kin"]]));
    assert!(
        !single
            .labels
            .iter()
            .any(|l| l.text == "> " || l.text == "  "),
        "no mark for one plot"
    );
}

#[test]
fn test_hidden_series_is_listed_grey_not_drawn_and_not_scaled() {
    let mut selection = PlotSelection::from_panels(&[&["E_kin", "wall_momentum"]]);
    let d = mixed_data();
    let both = build_selection(&d, &selection);
    selection.toggle_hidden(0, "wall_momentum");
    let hidden = build_selection(&d, &selection);

    let momentum = series_color(&d, "wall_momentum");
    assert!(!segments_of(&both, momentum).is_empty());
    assert!(segments_of(&hidden, momentum).is_empty());
    let label = hidden
        .labels
        .iter()
        .find(|l| l.text == "wall_momentum")
        .unwrap();
    assert_eq!(label.color, COLOR_HIDDEN);
    // Without the momentum the y range is that of E_kin (constant 1): no
    // tick near 2000 any more.
    assert!(has_label(&both, "2000") || has_label(&both, "1000"));
    assert!(!has_label(&hidden, "2000") && !has_label(&hidden, "1000"));
}

#[test]
fn test_series_colours_are_fixed_for_energies_and_distinct_otherwise() {
    let d = mixed_data();
    assert_eq!(series_color(&d, "E_kin"), COLOR_KINETIC);
    assert_eq!(series_color(&d, "E_pot"), COLOR_POTENTIAL);
    assert_eq!(series_color(&d, "E_tot"), COLOR_TOTAL);
    assert_ne!(
        series_color(&d, "wall_momentum"),
        series_color(&d, "N2:count")
    );
}

#[test]
fn test_a_series_without_data_draws_nothing_but_is_listed() {
    let selection = PlotSelection::from_panels(&[&["E_kin"], &["gone"]]);
    let scene = build_selection(&mixed_data(), &selection);
    assert_eq!(scene.panels.len(), 2);
    assert!(scene.legend.iter().any(|e| e.series == "gone"));
}

#[test]
fn test_four_plots_fit_the_default_window() {
    let selection =
        PlotSelection::from_panels(&[&["E_kin"], &["E_pot"], &["wall_momentum"], &["N2:count"]]);
    let scene = build_selection(&mixed_data(), &selection);
    assert_eq!(scene.panels.len(), 4);
    for label in &scene.labels {
        assert!(
            label.origin[1] + label.height() <= SIZE.1 as f32,
            "{label:?}"
        );
    }
}

#[test]
fn test_a_long_legend_is_cut_with_a_count_inside_the_window() {
    let mut d = mixed_data();
    let names: Vec<String> = (0..12).map(|i| format!("Species{i:02}:count")).collect();
    for name in &names {
        d.insert_series(name, None, [1.0; 21]);
    }
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let scene = build_selection(&d, &PlotSelection::from_panels(&[&refs]));
    let count = scene
        .labels
        .iter()
        .find(|l| l.text.starts_with('+'))
        .expect("a +N count");
    let shown = scene.legend.len();
    assert!(shown < names.len());
    assert_eq!(count.text, format!("+{}", names.len() - shown));
    for label in &scene.labels {
        assert!(
            label.origin[0] + label.width() <= SIZE.0 as f32,
            "{label:?}"
        );
    }
}
