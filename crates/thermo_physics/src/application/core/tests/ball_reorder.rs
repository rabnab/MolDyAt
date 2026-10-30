// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Sorting the balls by cell (`NeighbourConfig::reorder_interval`, P4, #345).

use super::drift_harness::{
    DriftHarnessConfig, create_dense_harness_with_seed, populate_grid_balls, run_drift_harness,
};
use super::*;
use crate::application::MainContent;

const BALLS: usize = 400;
const STEPS: usize = 60;

fn gas(reorder_interval: u32) -> MainContent {
    let mut main_content = create_dense_harness_with_seed(7);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    main_content.world_mut().reorder_interval = reorder_interval;
    populate_grid_balls(&mut main_content, BALLS, 1.5);
    main_content
}

/// Flat cell of every ball, in ball order.
fn cells_in_ball_order(main_content: &MainContent) -> Vec<(usize, usize)> {
    let world = main_content.world();
    (0..world.balls.len())
        .map(|i| world.cell_grid.cell_of_ball(i).expect("ball in the grid"))
        .collect()
}

#[test]
fn test_reorder_sorts_balls_by_cell_and_keeps_every_ball() {
    let mut main_content = gas(0);
    for _ in 0..3 {
        main_content.simulation_step().unwrap();
    }
    let mut before: Vec<(i64, i64)> = main_content
        .world()
        .balls
        .iter()
        .map(|b| {
            (
                b.get_position().x.to_bits() as i64,
                b.get_position().y.to_bits() as i64,
            )
        })
        .collect();
    let mut pairs_before = neighbour_pairs(&main_content);
    main_content.world_mut().reorder_balls_by_cell();

    let cells = cells_in_ball_order(&main_content);
    assert!(
        cells.windows(2).all(|w| w[0] <= w[1]),
        "balls are not in cell order"
    );
    let mut after: Vec<(i64, i64)> = main_content
        .world()
        .balls
        .iter()
        .map(|b| {
            (
                b.get_position().x.to_bits() as i64,
                b.get_position().y.to_bits() as i64,
            )
        })
        .collect();
    before.sort_unstable();
    after.sort_unstable();
    assert_eq!(before, after, "reordering lost or changed balls");
    let mut pairs_after = neighbour_pairs(&main_content);
    pairs_before.sort_unstable();
    pairs_after.sort_unstable();
    assert!(!pairs_before.is_empty());
    assert_eq!(
        pairs_before, pairs_after,
        "the lists must keep their partners under the new indices"
    );
}

/// Every listed pair as the bit patterns of the two positions.
fn neighbour_pairs(main_content: &MainContent) -> Vec<(u32, u32, u32, u32)> {
    let balls = &main_content.world().balls;
    balls
        .iter()
        .flat_map(|b| {
            b.cold.hood.get_neighbour_indices().iter().map(move |&j| {
                let (p, q) = (b.get_position(), balls[j].get_position());
                (p.x.to_bits(), p.y.to_bits(), q.x.to_bits(), q.y.to_bits())
            })
        })
        .collect()
}

#[test]
fn test_reordered_run_keeps_neighbour_lists_and_energy() {
    let cfg = DriftHarnessConfig {
        steps: STEPS,
        sample_every: 10,
        dt: DT_GOAL,
    };
    let mut plain = gas(0);
    let plain_metrics = run_drift_harness(&mut plain, cfg);
    let mut sorted = gas(7);
    let sorted_metrics = run_drift_harness(&mut sorted, cfg);

    assert_eq!(sorted_metrics.max_missed_neighbor_pairs(), 0);
    assert_eq!(sorted_metrics.max_asymmetric_neighbor_pairs(), 0);
    assert_eq!(
        sorted_metrics.final_sample.ball_count,
        plain_metrics.final_sample.ball_count
    );
    // Only the order of f32 sums differs, so the energies agree closely.
    let (a, b) = (
        plain_metrics.final_sample.total_energy,
        sorted_metrics.final_sample.total_energy,
    );
    assert!(
        (a - b).abs() <= 1e-4 * a.abs().max(1.0),
        "total energy {a} without and {b} with reordering"
    );
}
