// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Energy bookkeeping for balls hitting line-segment walls.
//!
//! Gravity gives a ball a constant force towards a horizontal wall, which is
//! the case a position-projecting collision gets wrong: every contact moved
//! the ball against the force (gaining potential energy) and left its
//! acceleration from inside the wall. The same scenarios on the world box
//! floor serve as the reference. Tilted walls, joints between wall pieces and
//! the rounded wall ends are covered as well.

use super::create_main_content_for_region;
use crate::application::MainContent;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::BallFactory;

/// Top-left corner of the 100 x 100 world: it spans y = -5..95, so every
/// height stays small and f32 position rounding does not drift the energy.
const WORLD_TOP_LEFT: (f32, f32) = (-50.0, 95.0);
/// Floor used in these tests: a horizontal wall well above the box floor.
const WALL_Y: f32 = 20.0;
/// Balls touch a wall this far outside its surface (`COLLISION_EPSILON`).
const WALL_CONTACT_MARGIN: f32 = 0.01;
/// Strong enough that a resting ball is pushed into the wall every step.
const GRAVITY: f32 = 1.0;

fn mechanical_energy(main_content: &MainContent) -> f32 {
    let world = main_content.world();
    let gravity = world.physics.constants.gravity;
    world
        .balls
        .iter()
        .map(|b| 0.5 * b.mass * b.v.norm_sq() + b.mass * gravity * b.get_position().y)
        .sum()
}

/// A single ball under gravity, dropped from `height` above the floor
/// surface. Returns the scenario and the height of the floor surface for the
/// ball's centre.
fn dropped_ball(on_wall: bool, height: f32) -> (MainContent, f32) {
    let (mut main_content, _) = create_main_content_for_region(WORLD_TOP_LEFT.0, WORLD_TOP_LEFT.1);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    main_content.world_mut().physics.constants.gravity = GRAVITY;
    let factory = BallFactory::new();
    let probe = factory.create_massive_with_velocity(Vec2D::zero(), Vec2D::zero());
    let radius = probe.radius;

    let surface_y = if on_wall {
        main_content.add_line_segment(Vec2D::new(-30.0, WALL_Y), Vec2D::new(30.0, WALL_Y));
        let half_thickness = 0.5 * main_content.world().line_segments.segments()[0].thickness;
        WALL_Y + half_thickness + radius + WALL_CONTACT_MARGIN
    } else {
        let (_, y0) = main_content.world().world_region().get_tl();
        let (_, y1) = main_content.world().world_region().get_br();
        y0.min(y1) + radius
    };

    let ball = factory
        .create_massive_with_velocity(Vec2D::new(0.3, surface_y + height), Vec2D::new(0.7, 0.0));
    main_content.world_mut().add_ball_and_register(ball);
    main_content.collect_neighbours_by_cell(super::DT_GOAL);
    (main_content, surface_y)
}

/// Relative energy change after `steps` steps.
fn relative_energy_change(main_content: &mut MainContent, steps: usize) -> f32 {
    let e0 = mechanical_energy(main_content);
    let reference = (e0 - mechanical_energy_floor(main_content)).abs().max(1e-3);
    for _ in 0..steps {
        main_content.simulation_step().unwrap();
    }
    (mechanical_energy(main_content) - e0) / reference
}

/// Energy the ball would have at rest on the box floor, so the relative change
/// is taken against the energy above the floor rather than an arbitrary zero.
fn mechanical_energy_floor(main_content: &MainContent) -> f32 {
    let world = main_content.world();
    let gravity = world.physics.constants.gravity;
    let (_, y0) = world.world_region().get_tl();
    let (_, y1) = world.world_region().get_br();
    world
        .balls
        .iter()
        .map(|b| b.mass * gravity * (y0.min(y1) + b.radius))
        .sum()
}

/// Bounces under gravity over 30 time units (about 5 bounces).
const STEPS: usize = 3_000;
/// Bouncing: f32 rounding of positions and velocities. Before the fix a
/// line-segment wall gained ~8e-4 here.
const MAX_BOUNCING_DRIFT: f32 = 2.0e-4;
/// Resting: before the fix the box floor gained ~2e-2 and a line-segment wall
/// ~8e-4 (every step was a contact with a mirrored or projected overshoot).
const MAX_RESTING_DRIFT: f32 = 1.0e-5;

fn assert_energy_kept(on_wall: bool, height: f32, max_drift: f32) {
    let (mut main_content, surface_y) = dropped_ball(on_wall, height);
    let drift = relative_energy_change(&mut main_content, STEPS);
    let gap = main_content.world().balls[0].get_position().y - surface_y;
    assert!(
        drift.abs() < max_drift,
        "on_wall={on_wall} height={height}: relative energy change {drift:e} (limit {max_drift:e})"
    );
    assert!(
        gap > -1e-3,
        "on_wall={on_wall} height={height}: ball ended {gap} inside the floor"
    );
}

#[test]
fn test_ball_bouncing_on_line_segment_keeps_energy() {
    assert_energy_kept(true, 5.0, MAX_BOUNCING_DRIFT);
}

#[test]
fn test_ball_resting_on_line_segment_keeps_energy() {
    assert_energy_kept(true, 0.0, MAX_RESTING_DRIFT);
}

#[test]
fn test_ball_bouncing_on_box_floor_keeps_energy() {
    assert_energy_kept(false, 5.0, MAX_BOUNCING_DRIFT);
}

#[test]
fn test_ball_resting_on_box_floor_keeps_energy() {
    assert_energy_kept(false, 0.0, MAX_RESTING_DRIFT);
}

/// A single ball under gravity among the walls `add_walls` places.
fn ball_among_walls(
    add_walls: impl FnOnce(&mut MainContent),
    position: Vec2D,
    velocity: Vec2D,
) -> MainContent {
    let (mut main_content, _) = create_main_content_for_region(WORLD_TOP_LEFT.0, WORLD_TOP_LEFT.1);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    main_content.world_mut().physics.constants.gravity = GRAVITY;
    add_walls(&mut main_content);
    let ball = BallFactory::new().create_massive_with_velocity(position, velocity);
    main_content.world_mut().add_ball_and_register(ball);
    main_content.collect_neighbours_by_cell(super::DT_GOAL);
    main_content
}

fn assert_exact_contacts_only(main_content: &MainContent, min_contacts: u64, scenario: &str) {
    let stats = &main_content.world().sim;
    assert!(
        stats.wall_contacts >= min_contacts,
        "{scenario}: scenario invalid, only {} wall contacts",
        stats.wall_contacts
    );
    assert_eq!(
        stats.wall_overlap_corrections, 0,
        "{scenario}: overlap fallback fired"
    );
}

/// Tilted walls: the contact is resolved on a flat wall through the contact
/// point, which is exact for a straight wall at any angle. Two walls meeting
/// in a V also exercise the corner where their capsules overlap.
#[test]
fn test_ball_in_a_valley_of_tilted_walls_keeps_energy() {
    for degrees in [7.0f32, 30.0, 45.0] {
        let rise = 30.0 * degrees.to_radians().tan();
        let slope = rise / 30.0;
        let mut main_content = ball_among_walls(
            |mc| {
                mc.add_line_segment(Vec2D::new(-30.0, WALL_Y + rise), Vec2D::new(0.0, WALL_Y));
                mc.add_line_segment(Vec2D::new(0.0, WALL_Y), Vec2D::new(30.0, WALL_Y + rise));
            },
            Vec2D::new(-12.0, WALL_Y + 12.0 * slope + 4.0),
            Vec2D::new(0.5, 0.0),
        );
        let drift = relative_energy_change(&mut main_content, STEPS);
        assert!(
            drift.abs() < MAX_BOUNCING_DRIFT,
            "{degrees} degrees: relative energy change {drift:e}"
        );
        assert_exact_contacts_only(&main_content, 3, &format!("{degrees} degree valley"));
    }
}

/// Sliding along a wall split into piece - closed valve - piece: crossing a
/// joint must not kick the ball off the wall or slow it down.
#[test]
fn test_ball_sliding_across_wall_joints_is_not_deflected() {
    let mut main_content = ball_among_walls(
        |mc| {
            mc.add_line_segment(Vec2D::new(-40.0, WALL_Y), Vec2D::new(40.0, WALL_Y));
            mc.add_valve_segment(0, 0.4, 0.6, true); // joints at x = -8 and 8
        },
        Vec2D::zero(),
        Vec2D::zero(),
    );
    assert_eq!(main_content.world().valve_segments.valves().len(), 1);
    let radius = main_content.world().balls[0].radius;
    let half_thickness = 0.5 * main_content.world().line_segments.segments()[0].thickness;
    let surface_y = WALL_Y + half_thickness + radius + WALL_CONTACT_MARGIN;
    const START_X: f32 = -20.0;
    const SPEED: f32 = 3.0;
    const STEPS: usize = 1_000; // ends at x = 10, past both joints
    {
        let ball = &mut main_content.world_mut().balls[0];
        ball.set_position_and_update_tail(Vec2D::new(START_X, surface_y));
        ball.v = Vec2D::new(SPEED, 0.0);
    }
    main_content.world_mut().rebuild_cell_grid();

    let e0 = mechanical_energy(&main_content);
    let kinetic = 0.5 * main_content.world().balls[0].mass * SPEED * SPEED;
    let mut max_vertical_speed = 0.0f32;
    for _ in 0..STEPS {
        main_content.simulation_step().unwrap();
        max_vertical_speed = max_vertical_speed.max(main_content.world().balls[0].v.y.abs());
    }

    let ball = &main_content.world().balls[0];
    let end_x = START_X + SPEED * STEPS as f32 * super::DT_GOAL;
    assert!(
        (ball.get_position().x - end_x).abs() < 1e-3,
        "slowed down or stopped at a joint: x={} expected {end_x}",
        ball.get_position().x
    );
    // Resting contact under gravity leaves ~g·dt; a snag would be ~SPEED.
    assert!(
        max_vertical_speed < 0.1,
        "kicked off the wall: max |v_y| = {max_vertical_speed}"
    );
    let drift = (mechanical_energy(&main_content) - e0) / kinetic;
    assert!(
        drift.abs() < MAX_RESTING_DRIFT,
        "relative energy change {drift:e}"
    );
    assert_exact_contacts_only(&main_content, 100, "sliding across joints");
}

/// Dropping onto the rounded end of a vertical wall: head-on and off-centre,
/// where the curved surface deflects the ball sideways.
#[test]
fn test_ball_bouncing_off_a_rounded_wall_end_keeps_energy() {
    for offset in [0.0f32, 0.3, 0.8] {
        let mut main_content = ball_among_walls(
            |mc| mc.add_line_segment(Vec2D::new(0.0, 10.0), Vec2D::new(0.0, WALL_Y + 10.0)),
            Vec2D::new(offset, WALL_Y + 16.0),
            Vec2D::zero(),
        );
        let drift = relative_energy_change(&mut main_content, 1_500);
        assert!(
            drift.abs() < MAX_BOUNCING_DRIFT,
            "offset {offset}: relative energy change {drift:e}"
        );
        assert_exact_contacts_only(&main_content, 1, &format!("cap offset {offset}"));
        if offset > 0.0 {
            let x = main_content.world().balls[0].get_position().x;
            assert!(x > 1.0, "offset {offset}: not deflected sideways, x={x}");
        }
    }
}
