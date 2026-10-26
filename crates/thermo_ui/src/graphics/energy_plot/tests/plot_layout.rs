// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::energy_plot::plot_layout::*;
use crate::graphics::energy_plot::{DisplayMode, EnergyData};

const SIZE: (u32, u32) = (1000, 700);

const HINTS: &str = "Esc: close  TAB / P: mode";

fn data(kin: &[f32], pot: &[f32], times: &[f32]) -> EnergyData {
    let mut data = EnergyData::new();
    data.v_kin.extend(kin.iter().copied());
    data.v_pot.extend(pot.iter().copied());
    data.sample_times.extend(times.iter().copied());
    data.time = 1.5;
    data
}

fn build(data: &EnergyData, mode: DisplayMode, size: (u32, u32), scale_factor: f64) -> PlotScene {
    PlotScene::build(data, mode, size, scale_factor, HINTS)
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
fn test_curves_stay_inside_the_plot_area_and_span_its_width() {
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
    assert!((last - area.right).abs() < 1e-3);
    // Larger values are higher up (smaller y).
    assert!(curve[0].to[1] < curve[0].from[1]);
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

    assert!(has_label(&scene, "0"));
    assert!(has_label(&scene, "0.5"));
    assert!(has_label(&scene, "2.0"));
    assert!(scene.labels.iter().any(|l| l.text.ends_with("x: time")));
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
    let rebound = PlotScene::build(&d, DisplayMode::Kin, SIZE, 1.0, "Esc: close  X: mode");
    let none = PlotScene::build(&d, DisplayMode::Kin, SIZE, 1.0, "");

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
