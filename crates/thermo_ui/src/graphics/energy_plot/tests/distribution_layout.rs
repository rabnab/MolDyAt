// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The distributions view of the plot window (#343).

use crate::graphics::energy_plot::plot_layout::{COLOR_HIDDEN, COLOR_KINETIC};
use crate::graphics::energy_plot::{DistributionData, PlotScene};
use thermo_physics::objects2d::ball_impl::DEFAULT_SPECIES;
use thermo_runtime::distributions::{
    Distributions, PairDistribution, RadialDistribution, SpeedHistogram,
};

fn data() -> DistributionData {
    DistributionData {
        distributions: Distributions {
            speed: Some(SpeedHistogram {
                bin_width: 0.5,
                counts: vec![3, 10, 0, 2],
                expected: vec![4.0, 8.0, 2.5, 0.5],
                temperature: 1.25,
            }),
            radial: RadialDistribution {
                bin_width: 1.0,
                pairs: vec![PairDistribution {
                    species: (DEFAULT_SPECIES, DEFAULT_SPECIES),
                    reference_balls: 10,
                    g: vec![0.0, f32::NAN, 1.8, 1.0, 0.9],
                }],
            },
        },
        pair_names: vec!["N2-N2".into()],
    }
}

#[test]
fn every_line_stays_in_the_window() {
    let scene = PlotScene::build_distributions(Some(&data()), &[], (800, 600), 1.0, "Esc: close");
    for s in &scene.segments {
        for [x, y] in [s.from, s.to] {
            assert!(
                (0.0..=800.0).contains(&x) && (0.0..=600.0).contains(&y),
                "{s:?}"
            );
        }
    }
}

#[test]
fn bars_for_the_non_empty_bins_and_a_curve_for_the_expected_counts() {
    let scene = PlotScene::build_distributions(Some(&data()), &[], (800, 600), 1.0, "");
    // At scale 1 a glyph is 16 px: the plot spans from 12 + 7 glyphs to
    // 800 - 12, and a bar is 80 % of one of its 4 bins.
    let bar_width = (800.0 - 12.0 - (12.0 + 7.0 * 16.0)) / 4.0 * 0.8;
    let bars = scene
        .segments
        .iter()
        .filter(|s| (s.width - bar_width).abs() < 1e-3);
    assert_eq!(bars.count(), 3, "one bar per bin with balls");
    let curve = scene.segments.iter().filter(|s| s.color == COLOR_KINETIC);
    assert_eq!(curve.count(), 3, "the expected counts as a line");
    let heading = scene
        .labels
        .iter()
        .find(|l| l.text.starts_with("Speed"))
        .unwrap();
    assert!(heading.text.contains("T = 1.250"), "{}", heading.text);
}

#[test]
fn g_breaks_at_missing_values_and_marks_one() {
    let scene = PlotScene::build_distributions(Some(&data()), &[], (800, 600), 1.0, "");
    assert!(scene.labels.iter().any(|l| l.text == "N2-N2"));
    // 5 points with a NaN at the second: 2 drawn pieces (2->3, 3->4) plus
    // the one at 0->1 is broken, so g contributes 2 coloured segments.
    let g_color = [0.30, 0.60, 0.90, 1.0];
    let curve = scene
        .segments
        .iter()
        .filter(|s| s.color == g_color && s.from[0] != s.to[0])
        .count();
    assert_eq!(curve, 2);
}

#[test]
fn without_data_the_view_waits() {
    let scene = PlotScene::build_distributions(None, &[], (800, 600), 1.0, "");
    assert!(scene.labels.iter().any(|l| l.text.starts_with("Waiting")));
    assert!(scene.segments.is_empty());
}

/// `data()` with a second pair, `N2-Ar`, whose g(r) peaks at 6.
fn two_pairs() -> DistributionData {
    let mut data = data();
    data.distributions.radial.pairs.push(PairDistribution {
        species: (DEFAULT_SPECIES, DEFAULT_SPECIES),
        reference_balls: 10,
        g: vec![0.0, 6.0, 1.2, 1.0, 1.0],
    });
    data.pair_names.push("N2-Ar".into());
    data
}

const AR_COLOR: [f32; 4] = [0.95, 0.60, 0.20, 1.0];

fn curve_segments(scene: &PlotScene, color: [f32; 4]) -> usize {
    scene
        .segments
        .iter()
        .filter(|s| s.color == color && s.from[0] != s.to[0])
        .count()
}

#[test]
fn every_g_pair_is_a_legend_click_target() {
    let scene = PlotScene::build_distributions(Some(&two_pairs()), &[], (800, 600), 1.0, "");
    let names: Vec<&str> = scene.legend.iter().map(|e| e.series.as_str()).collect();
    assert_eq!(names, ["N2-N2", "N2-Ar"]);
    for entry in &scene.legend {
        let label = scene
            .labels
            .iter()
            .find(|l| l.text == entry.series)
            .unwrap();
        assert!(
            entry
                .rect
                .contains([label.origin[0] + 1.0, label.origin[1] + 1.0])
        );
    }
}

#[test]
fn a_hidden_pair_is_grey_in_the_legend_and_not_drawn() {
    let hidden = vec!["N2-Ar".to_string()];
    let shown = PlotScene::build_distributions(Some(&two_pairs()), &[], (800, 600), 1.0, "");
    let scene = PlotScene::build_distributions(Some(&two_pairs()), &hidden, (800, 600), 1.0, "");
    assert_eq!(curve_segments(&shown, AR_COLOR), 4);
    assert_eq!(curve_segments(&scene, AR_COLOR), 0, "hidden curve drawn");
    let label = scene.labels.iter().find(|l| l.text == "N2-Ar").unwrap();
    assert_eq!(label.color, COLOR_HIDDEN);
    // Still a click target, to show it again.
    assert!(scene.legend.iter().any(|e| e.series == "N2-Ar"));
    // The other pair is drawn as before.
    let n2_color = [0.30, 0.60, 0.90, 1.0];
    assert_eq!(curve_segments(&scene, n2_color), 2);
}

#[test]
fn the_g_axis_follows_the_pairs_drawn() {
    // The top y tick of the g(r) panel (the lower half; its y labels end
    // left of the plot area, which starts at 12 + 7 glyphs of 16 px): up to
    // ~6.3 with N2-Ar shown, ~1.9 without it.
    let top_tick = |hidden: &[String]| {
        let scene = PlotScene::build_distributions(Some(&two_pairs()), hidden, (800, 600), 1.0, "");
        scene
            .labels
            .iter()
            .filter(|l| l.origin[0] < 12.0 + 7.0 * 16.0 && l.origin[1] > 300.0)
            .filter_map(|l| l.text.parse::<f64>().ok())
            .fold(0.0, f64::max)
    };
    assert!(top_tick(&[]) >= 6.0);
    assert!(top_tick(&["N2-Ar".to_string()]) < 6.0);
}
