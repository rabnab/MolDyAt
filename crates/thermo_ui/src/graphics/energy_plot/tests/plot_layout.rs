// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::energy_plot::plot_layout::*;
use crate::graphics::energy_plot::{DisplayMode, EnergyData};

const SIZE: (u32, u32) = (1000, 700);

fn data(kin: &[f32], pot: &[f32], steps: &[u64]) -> EnergyData {
    let mut data = EnergyData::new();
    data.v_kin.extend(kin.iter().copied());
    data.v_pot.extend(pot.iter().copied());
    data.steps.extend(steps.iter().copied());
    data.time = 1.5;
    data
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
    let scene = PlotScene::build(&EnergyData::new(), DisplayMode::All, SIZE, 1.0);

    assert!(has_label(&scene, "Energy plot - all"));
    assert!(has_label(&scene, "Waiting for simulation data..."));
    assert!(scene.plot_area.is_none());
    assert!(scene.segments.is_empty());
}

#[test]
fn test_all_mode_draws_three_curves_with_legend() {
    let scene = PlotScene::build(
        &data(&[10.0, 20.0, 30.0], &[5.0, 10.0, 15.0], &[0, 10, 20]),
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
    let scene = PlotScene::build(
        &data(&[1.0, 2.0], &[3.0, 4.0], &[0, 1]),
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
    let scene = PlotScene::build(
        &data(&[-4.0, 7.0, 2.0, 9.0], &[0.0; 4], &[100, 200, 300, 400]),
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
    let crossing = PlotScene::build(
        &data(&[-5.0, 5.0], &[0.0; 2], &[0, 1]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );
    let positive = PlotScene::build(
        &data(&[5.0, 6.0], &[0.0; 2], &[0, 1]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );

    assert_eq!(segments_of(&crossing, COLOR_ZERO).len(), 1);
    assert!(segments_of(&positive, COLOR_ZERO).is_empty());
}

#[test]
fn test_x_tick_labels_are_steps() {
    let scene = PlotScene::build(
        &data(&[1.0; 5], &[0.0; 5], &[1000, 2000, 3000, 4000, 5000]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );

    assert!(has_label(&scene, "1000"));
    assert!(has_label(&scene, "5000"));
}

#[test]
fn test_non_finite_values_break_the_curve() {
    let scene = PlotScene::build(
        &data(&[1.0, 2.0, f32::NAN, 3.0, 4.0], &[0.0; 5], &[0, 1, 2, 3, 4]),
        DisplayMode::Kin,
        SIZE,
        1.0,
    );

    // 0-1 and 3-4; nothing across the gap.
    assert_eq!(segments_of(&scene, COLOR_KINETIC).len(), 2);
}

#[test]
fn test_single_sample_is_a_dot() {
    let scene = PlotScene::build(&data(&[3.0], &[1.0], &[7]), DisplayMode::Kin, SIZE, 1.0);
    let curve = segments_of(&scene, COLOR_KINETIC);

    assert_eq!(curve.len(), 1);
    assert_eq!(curve[0].from, curve[0].to);
}

#[test]
fn test_status_text_is_shown_below_the_plot() {
    let mut d = data(&[1.0, 2.0], &[0.0; 2], &[0, 1]);
    d.status_text = Some("fps 60\n#Balls: 12".to_string());
    let scene = PlotScene::build(&d, DisplayMode::Kin, SIZE, 1.0);
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
    let d = data(&[1.0, 2.0], &[0.0; 2], &[0, 1]);
    let wide = PlotScene::build(&d, DisplayMode::Kin, SIZE, 1.0);
    let narrow = PlotScene::build(&d, DisplayMode::Kin, (400, 300), 1.0);

    let hints = wide
        .labels
        .iter()
        .find(|l| l.text == KEY_HINTS)
        .expect("key hints");
    assert!(hints.origin[0] + hints.width() <= SIZE.0 as f32);
    assert!(!has_label(&narrow, KEY_HINTS));
}

#[test]
fn test_labels_stay_inside_a_default_sized_window() {
    let mut d = data(&[10.0, 20.0, 30.0], &[5.0, 10.0, 15.0], &[0, 10, 20]);
    d.time = 123.45;
    let scene = PlotScene::build(&d, DisplayMode::All, (1024, 576), 1.0);

    for label in &scene.labels {
        assert!(label.origin[0] >= 0.0, "{label:?}");
        assert!(label.origin[0] + label.width() <= 1024.0, "{label:?}");
    }
}

#[test]
fn test_tiny_window_has_no_plot_area() {
    let scene = PlotScene::build(
        &data(&[1.0, 2.0], &[0.0; 2], &[0, 1]),
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
