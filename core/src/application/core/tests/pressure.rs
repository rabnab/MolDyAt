// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::drift_harness::{create_dense_harness_with_seed, populate_grid_balls};
use crate::application::MainContent;
use crate::application::thermo_field::{LocalKineticStats, virial_pressure_2d};

const EQUILIBRATION_STEPS: usize = 1_500;
const MEASURE_STEPS: usize = 3_000;
/// The wall reading sits a few percent above the bulk virial value (wall
/// area and the density layer at the wall); the missing 1/d factor this test
/// guards against was 45 % off.
const MAX_REL_DEVIATION: f64 = 0.10;

/// Mechanical pressure on the world box: normal momentum the walls absorb
/// per unit wall length and time. A ball bounced when its velocity component
/// normal to a wall flipped sign while it was touching that wall.
fn wall_impulse(before: &[(f32, f32)], main_content: &MainContent) -> f64 {
    let world = main_content.world();
    let rect = world.world_region();
    let (x0, y0) = rect.get_tl();
    let (x1, y1) = rect.get_br();
    let (min_x, max_x) = (x0.min(x1), x0.max(x1));
    let (min_y, max_y) = (y0.min(y1), y0.max(y1));
    let mut impulse = 0.0f64;
    for (ball, &(vx, vy)) in world.balls.iter().zip(before) {
        let pos = ball.get_position();
        let touching = |gap: f32| gap < ball.radius + 0.5;
        let mass = ball.mass as f64;
        if vx * ball.v.x < 0.0 && (touching(pos.x - min_x) || touching(max_x - pos.x)) {
            impulse += mass * (ball.v.x - vx).abs() as f64;
        }
        if vy * ball.v.y < 0.0 && (touching(pos.y - min_y) || touching(max_y - pos.y)) {
            impulse += mass * (ball.v.y - vy).abs() as f64;
        }
    }
    impulse
}

/// Bulk virial pressure of the whole box, using the same 2D formula as the
/// per-cell pressure grid.
fn bulk_virial_pressure(main_content: &MainContent) -> f64 {
    let world = main_content.world();
    let rect = world.world_region();
    let mut kinetic = LocalKineticStats::new();
    for ball in &world.balls {
        kinetic.add_ball(ball);
    }
    let pair_virial: f32 = world
        .cached_force_results
        .iter()
        .map(|result| 0.5 * result.virial)
        .sum();
    virial_pressure_2d(
        kinetic.count(),
        kinetic.temperature().unwrap_or(0.0),
        pair_virial,
        (rect.w * rect.h).abs(),
    ) as f64
}

/// In a uniform hot gas the virial pressure must match the pressure the walls
/// actually feel. Guards the 1/d = 1/2 factor of the 2D virial theorem.
#[test]
#[ignore = "suite:physics (slow)"]
fn test_virial_pressure_2d_matches_wall_pressure() {
    let mut main_content = create_dense_harness_with_seed(12345);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    populate_grid_balls(&mut main_content, 900, 2.0);
    let rect = main_content.world().world_region();
    let perimeter = 2.0 * (rect.w.abs() + rect.h.abs()) as f64;
    let dt = super::DT_GOAL as f64;

    for _ in 0..EQUILIBRATION_STEPS {
        main_content.simulation_step().unwrap();
    }

    let mut impulse = 0.0f64;
    let mut virial_sum = 0.0f64;
    for _ in 0..MEASURE_STEPS {
        let before: Vec<(f32, f32)> = main_content
            .world()
            .balls
            .iter()
            .map(|b| (b.v.x, b.v.y))
            .collect();
        main_content.simulation_step().unwrap();
        impulse += wall_impulse(&before, &main_content);
        virial_sum += bulk_virial_pressure(&main_content);
    }

    let p_wall = impulse / (perimeter * MEASURE_STEPS as f64 * dt);
    let p_virial = virial_sum / MEASURE_STEPS as f64;
    let deviation = (p_virial - p_wall).abs() / p_wall;
    println!("PRESSURE_CHECK p_wall={p_wall:.4} p_virial_2d={p_virial:.4} rel_dev={deviation:.3}");
    assert!(
        deviation < MAX_REL_DEVIATION,
        "virial pressure {p_virial:.4} deviates {deviation:.3} from wall pressure {p_wall:.4}"
    );
}
