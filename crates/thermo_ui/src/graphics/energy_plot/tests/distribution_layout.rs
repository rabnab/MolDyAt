// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The distributions view of the plot window (#343).

use crate::graphics::energy_plot::plot_layout::COLOR_KINETIC;
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
    let scene = PlotScene::build_distributions(Some(&data()), (800, 600), 1.0, "Esc: close");
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
    let scene = PlotScene::build_distributions(Some(&data()), (800, 600), 1.0, "");
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
    let scene = PlotScene::build_distributions(Some(&data()), (800, 600), 1.0, "");
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
    let scene = PlotScene::build_distributions(None, (800, 600), 1.0, "");
    assert!(scene.labels.iter().any(|l| l.text.starts_with("Waiting")));
    assert!(scene.segments.is_empty());
}
