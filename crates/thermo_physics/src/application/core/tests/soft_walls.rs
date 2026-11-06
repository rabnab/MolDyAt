// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Soft walls (#280): the wall law, energy conservation, the backstop,
//! continuity across joints and the wall pressure.

use super::{create_main_content_for_region, new_main_content_seeded};
use crate::application::MainContent;
use crate::application::core::wall_forces::soft_wall_force;
use crate::objects2d::ball_impl::pair_potential::CapSettings;
use crate::objects2d::ball_impl::wall_law::{SoftWalls, WALL_RAMP_TIME, WallSettings};
use crate::objects2d::ball_impl::{BallFactory, DEFAULT_SPECIES, SPECIES_AR, SpeciesTable};
use crate::objects2d::line_segment::GhostWall;
use crate::objects2d::{Rect, Vec2D};

const WORLD_TOP_LEFT: (f32, f32) = (-50.0, 95.0);

fn soft_walls(settings: &WallSettings) -> SoftWalls {
    let table = SpeciesTable::builtin();
    SoftWalls::new(
        table,
        |id| (table.species(id).sigma, table.species(id).epsilon),
        settings,
        CapSettings::default(),
    )
}

fn reported_energy(mc: &MainContent) -> f32 {
    let sim = &mc.world().sim;
    sim.get_kinetic_energy() + sim.get_potential_energy()
}

#[test]
fn wall_force_is_minus_the_derivative_of_its_potential() {
    for attractive in [false, true] {
        let walls = soft_walls(&WallSettings {
            attractive,
            ..WallSettings::default()
        });
        let cutoff = walls.cutoff(SPECIES_AR);
        let h = 1e-3;
        let mut z = 0.1;
        while z < cutoff - 2.0 * h {
            let (force, _) = walls.force_and_potential(SPECIES_AR, z);
            let (_, up) = walls.force_and_potential(SPECIES_AR, z + h);
            let (_, down) = walls.force_and_potential(SPECIES_AR, z - h);
            let numeric = -(up - down) / (2.0 * h);
            assert!(
                (force - numeric).abs() < 2e-2 * force.abs().max(1.0),
                "attractive={attractive} z={z}: F {force} vs -dU/dz {numeric}"
            );
            z += 0.01;
        }
        let (force, potential) = walls.force_and_potential(SPECIES_AR, cutoff);
        assert_eq!((force, potential), (0.0, 0.0), "zero at the cutoff");
    }
}

#[test]
fn default_wall_is_purely_repulsive_with_the_hard_contact_distance() {
    let walls = soft_walls(&WallSettings::default());
    // σ_iw = σ_Ar / 2: zero potential (before the shift) at the hard
    // contact distance, minimum and cutoff at 2^(1/6) of it.
    let cutoff = walls.cutoff(SPECIES_AR);
    assert!((cutoff - 0.5 * 1.122_462).abs() < 1e-5, "cutoff {cutoff}");
    let mut z = 0.05;
    while z < cutoff {
        let (force, potential) = walls.force_and_potential(SPECIES_AR, z);
        assert!(
            force >= 0.0 && potential >= 0.0,
            "z={z}: F {force}, U {potential}"
        );
        z += 0.005;
    }
    // Behind the surface (an overshoot) it keeps pushing outward.
    let (force, _) = walls.force_and_potential(SPECIES_AR, -0.1);
    assert!(force > 0.0);
}

/// Relative energy error after each bounce, out of contact. Velocity Verlet
/// does not return the energy exactly after one passage through the steep
/// wall (≈ 3e-3 at the tests' dt = 0.01), but the error does not add up
/// from bounce to bounce.
const MAX_BOUNCE_DRIFT: f32 = 5e-3;

/// A ball dropped under gravity onto the box floor and onto a line wall,
/// hitting it with about 1.5 ε: the reported total (kinetic, gravity and
/// wall energy) is conserved, and the backstop is never reached.
#[test]
fn ball_bouncing_on_soft_walls_keeps_its_energy() {
    for on_wall in [false, true] {
        let (mut mc, _) = create_main_content_for_region(WORLD_TOP_LEFT.0, WORLD_TOP_LEFT.1);
        mc.world_mut().speed_limit_override = Some(f32::INFINITY);
        mc.world_mut().physics.constants.gravity = 1.0;
        if on_wall {
            mc.add_line_segment(Vec2D::new(-30.0, 20.0), Vec2D::new(30.0, 20.0));
        }
        // 1.5 above the surface (the line wall is 1 thick).
        let start_y = if on_wall { 22.0 } else { -3.5 };
        // The unit species (Ar since #405), as before #405 with `N2`.
        let ball = BallFactory::new().create_with_species_and_velocity(
            Vec2D::new(0.3, start_y),
            SPECIES_AR,
            Vec2D::new(0.7, 0.0),
        );
        mc.world_mut().add_ball_and_register(ball);
        mc.collect_neighbours_by_cell(super::DT_GOAL);
        mc.simulation_step().unwrap();
        let e0 = reported_energy(&mc);
        let mut max_drift = 0.0f32;
        let mut bounces = 0;
        let mut in_contact = false;
        for _ in 0..3_000 {
            mc.simulation_step().unwrap();
            let touching = mc.world().cached_force_results[0].wall_potential != 0.0;
            if in_contact && !touching {
                bounces += 1;
                max_drift = max_drift.max((reported_energy(&mc) - e0).abs() / e0.abs());
            }
            in_contact = touching;
        }
        assert!(bounces >= 8, "on_wall={on_wall}: only {bounces} bounces");
        println!("on_wall={on_wall}: max energy drift {max_drift:e}");
        assert!(
            max_drift < MAX_BOUNCE_DRIFT,
            "on_wall={on_wall}: energy drift {max_drift:e}"
        );
        assert_eq!(
            mc.world().sim.wall_contacts,
            0,
            "on_wall={on_wall}: backstop hit"
        );
        let y = mc.world().balls[0].get_position().y;
        let floor = if on_wall { 20.0 } else { -5.0 };
        assert!(
            y > floor,
            "on_wall={on_wall}: ball at {y} is behind the wall"
        );
    }
}

/// The energy of a ball moving along a chain at a fixed distance does not
/// change across the joints between its pieces (nearest-feature rule).
#[test]
fn wall_energy_is_continuous_across_joints() {
    let (mut mc, _) = create_main_content_for_region(-10.0, 10.0);
    // A closed valve cuts the wall into two pieces and the valve span: three
    // capsules that overlap at the two joints.
    mc.add_line_segment(Vec2D::new(5.0, -50.0), Vec2D::new(85.0, -50.0));
    mc.add_valve_segment(0, 0.4, 0.6, true)
        .expect("valve on piece 0");
    let world = mc.world();
    let walls = world.physics.soft_walls().expect("soft walls by default");
    let obstacles = world.collect_solid_obstacles();
    assert!(obstacles.len() >= 3, "pieces and the closed valve");
    let half_thickness = 0.5 * world.line_segments.segments()[0].thickness;
    let factory = BallFactory::new();
    let probe = |x: f32, z: f32| {
        let ball = factory
            .create_massive_with_velocity(Vec2D::new(x, -50.0 + half_thickness + z), Vec2D::zero());
        soft_wall_force(walls, &ball, &world.world_rect, &obstacles).potential
    };
    for z in [0.35, 0.45, 0.52] {
        let reference = probe(10.0, z);
        assert!(reference > 0.0);
        let mut x = 10.0;
        while x < 80.0 {
            let u = probe(x, z);
            assert!(
                (u - reference).abs() <= 1e-4 * reference,
                "z={z} x={x}: U {u} vs {reference} (bump or well at a joint)"
            );
            x += 0.05;
        }
    }
}

fn hot_gas(soft: bool, seed: u64, speed: f32) -> MainContent {
    let mut mc = new_main_content_seeded(Rect::new(-30.0, 30.0, 60.0, -60.0), seed).unwrap();
    if !soft {
        mc.use_hard_walls();
    }
    mc.world_mut().speed_limit_override = Some(f32::INFINITY);
    mc.add_line_segment(Vec2D::new(-20.0, 0.0), Vec2D::new(20.0, 0.0));
    mc.seed_random_balls(400);
    for (i, ball) in mc.world_mut().balls.iter_mut().enumerate() {
        let phase = i as f32 * 2.399;
        ball.v = Vec2D::new(phase.cos(), phase.sin()).scaled(speed);
    }
    mc
}

/// No ball crosses a thin line wall or a box wall, in a hot gas and with
/// balls fired at the walls far faster than thermal.
#[test]
fn hot_and_fast_balls_never_cross_a_soft_wall() {
    let mut mc = hot_gas(true, 7, 4.0);
    let factory = BallFactory::new();
    for k in 0..10 {
        let x = -18.0 + 4.0 * k as f32;
        let down = factory.create_massive_with_velocity(Vec2D::new(x, 1.5), Vec2D::new(0.0, -40.0));
        let up = factory.create_massive_with_velocity(Vec2D::new(x, -1.5), Vec2D::new(0.0, 40.0));
        mc.world_mut().add_ball_and_register(down);
        mc.world_mut().add_ball_and_register(up);
    }
    mc.collect_neighbours_by_cell(super::DT_GOAL);
    let count = mc.world().balls.len();
    for step in 0..2_000 {
        let before: Vec<Vec2D> = mc.world().balls.iter().map(|b| *b.get_position()).collect();
        mc.simulation_step().unwrap();
        let world = mc.world();
        assert_eq!(world.balls.len(), count, "step {step}: a ball was removed");
        let (left, top) = world.world_rect.get_tl();
        let (right, bottom) = world.world_rect.get_br();
        for (ball, start) in world.balls.iter().zip(&before) {
            let p = ball.get_position();
            assert!(
                p.x > left && p.x < right && p.y > bottom && p.y < top,
                "step {step}: ball {} left the box: {p}",
                ball.get_id()
            );
            // Crossing y = 0 within the wall's span (not around its ends)
            // means passing through it.
            let crossed = (start.y > 0.0) != (p.y > 0.0);
            assert!(
                !(crossed && start.x.abs() < 19.5 && p.x.abs() < 19.5),
                "step {step}: ball {} went through the wall: {start} -> {p}",
                ball.get_id()
            );
        }
    }
}

/// Thermal gas: the backstop is never reached.
#[test]
fn thermal_gas_never_reaches_the_backstop() {
    let mut mc = hot_gas(true, 11, 1.0);
    for _ in 0..2_000 {
        mc.simulation_step().unwrap();
    }
    assert_eq!(mc.world().sim.wall_contacts, 0);
}

/// Momentum the left box wall gives the balls during one step: the soft
/// wall force over the step, or the velocity flips at the hard contact.
fn left_wall_impulse(mc: &MainContent, v_before: &[f32]) -> f64 {
    let world = mc.world();
    if world.physics.soft_walls().is_some() {
        let dt = f64::from(mc.active_time_step());
        return world
            .cached_force_results
            .iter()
            .map(|r| f64::from(r.wall_force.x.max(0.0)) * dt)
            .sum();
    }
    world
        .balls
        .iter()
        .zip(v_before)
        .zip(&world.scratch.update.bounce_infos)
        .filter(|(_, bounce)| bounce.box_x.is_some_and(|c| c.normal.x > 0.0))
        .map(|((b, vx), _)| f64::from(b.mass * (b.v.x - vx)))
        .sum()
}

/// The pressure on a box wall of the same gas against hard walls and soft
/// walls with `σ_w = 0` (the hard contact distance) agrees within its noise.
#[test]
#[ignore = "suite:physics (slow)"]
fn soft_wall_pressure_matches_the_hard_wall() {
    let impulse = |soft: bool| {
        let mut total = 0.0f64;
        // About 2000 hard contacts with the left wall in all: the impulse
        // has a statistical error of about 3 %.
        for seed in [3u64, 5, 9, 17, 23, 31, 42, 57] {
            let mut mc =
                new_main_content_seeded(Rect::new(-15.0, 15.0, 30.0, -30.0), seed).unwrap();
            if !soft {
                mc.use_hard_walls();
            }
            mc.world_mut().speed_limit_override = Some(f32::INFINITY);
            mc.seed_random_balls(150);
            for (i, ball) in mc.world_mut().balls.iter_mut().enumerate() {
                let phase = i as f32 * 2.399;
                ball.v = Vec2D::new(phase.cos(), phase.sin()).scaled(1.5);
            }
            for _ in 0..500 {
                mc.simulation_step().unwrap();
            }
            for _ in 0..8_000 {
                let v_before: Vec<f32> = mc.world().balls.iter().map(|b| b.v.x).collect();
                mc.simulation_step().unwrap();
                total += left_wall_impulse(&mc, &v_before);
            }
        }
        total
    };
    let (hard, soft) = (impulse(false), impulse(true));
    let ratio = soft / hard;
    assert!(
        (0.9..1.1).contains(&ratio),
        "left wall impulse soft {soft} vs hard {hard} (ratio {ratio})"
    );
}

/// Momentum balance under gravity: what the gas gains plus what the soft
/// walls book equals the impulse of gravity, so the wall momentum (and the
/// pressure read from it) comes out of the wall forces in full.
#[test]
fn soft_walls_book_the_momentum_they_give() {
    const GRAVITY: f32 = 0.2;
    let mut mc = hot_gas(true, 13, 1.0);
    mc.world_mut().physics.constants.gravity = GRAVITY;
    let gas_momentum = |mc: &MainContent| -> f64 {
        mc.world()
            .balls
            .iter()
            .map(|b| f64::from(b.mass * b.v.y))
            .sum()
    };
    for _ in 0..200 {
        mc.simulation_step().unwrap();
    }
    let mass: f64 = mc.world().balls.iter().map(|b| f64::from(b.mass)).sum();
    let (wall0, gas0) = (mc.world().sim.wall_momentum_y, gas_momentum(&mc));
    let steps = 2_000;
    for _ in 0..steps {
        mc.simulation_step().unwrap();
    }
    assert_eq!(mc.world().sim.wall_contacts, 0, "backstop reached");
    let wall = mc.world().sim.wall_momentum_y - wall0;
    let gas = gas_momentum(&mc) - gas0;
    let gravity = -mass * f64::from(GRAVITY) * f64::from(mc.active_time_step()) * steps as f64;
    assert!(
        ((wall + gas - gravity) / gravity).abs() < 0.01,
        "wall {wall} + gas {gas} vs gravity impulse {gravity}"
    );
}

/// Kinetic energy of a ball at rest that a ghost wall lands on, after the
/// ramp, and its distance from the wall's centre line. `ramp`: whether the
/// wall ramps in (as it does); `false` switches it on at full strength.
fn ghost_wall_kick(ramp: bool) -> (f32, f32) {
    let (mut mc, _) = create_main_content_for_region(-10.0, 10.0);
    mc.add_candidate_to_vec(
        BallFactory::new().create_massive_with_velocity(Vec2D::new(40.0, -50.0), Vec2D::zero()),
    );
    mc.set_ghost_wall(Some(GhostWall {
        start: Vec2D::new(20.0, -50.2),
        end: Vec2D::new(60.0, -50.2),
        thickness: 1.0,
    }));
    assert!(mc.world().balls[0].leaving_wall, "the wall landed on it");
    assert_eq!(mc.world().balls[0].cold.wall_ramp, 0.0);
    if !ramp {
        mc.world_mut().balls[0].leaving_wall = false;
    }
    let steps = (2.0 * WALL_RAMP_TIME / mc.active_time_step()).ceil() as usize;
    for _ in 0..steps {
        mc.simulation_step().unwrap();
    }
    let ball = &mc.world().balls[0];
    assert!(!ball.leaving_wall, "ramped in");
    let kinetic = 0.5 * ball.mass * ball.v.norm_sq();
    (kinetic, (ball.get_position().y + 50.2).abs())
}

/// A wall drawn on top of a ball at rest pushes it out gently (#424): the
/// line walls ramp in for that ball, so it leaves with a fraction of the
/// kick a wall switched on at full strength gives it.
#[test]
fn ghost_wall_over_a_resting_ball_ramps_in() {
    let (ramped, distance) = ghost_wall_kick(true);
    let (kicked, _) = ghost_wall_kick(false);
    assert!(distance > 0.5, "still inside the wall: {distance}");
    assert!(
        ramped < 0.25 * kicked,
        "ramped kick {ramped} vs full-strength kick {kicked}"
    );
}

/// Two separate walls closer than two wall cutoffs both push on a ball
/// between them: the nearest-feature rule holds per chain only.
#[test]
fn separate_walls_both_act_on_a_ball_between_them() {
    let (mut mc, _) = create_main_content_for_region(-10.0, 10.0);
    mc.add_line_segment(Vec2D::new(5.0, -50.0), Vec2D::new(85.0, -50.0));
    mc.add_line_segment(Vec2D::new(5.0, -51.6), Vec2D::new(85.0, -51.6));
    let world = mc.world();
    let walls = world.physics.soft_walls().expect("soft walls by default");
    let obstacles = world.collect_solid_obstacles();
    assert_ne!(obstacles[0].chain(), obstacles[1].chain());
    let ball =
        BallFactory::new().create_massive_with_velocity(Vec2D::new(40.0, -50.8), Vec2D::zero());
    let both = soft_wall_force(walls, &ball, &world.world_rect, &obstacles);
    let one = soft_wall_force(walls, &ball, &world.world_rect, &obstacles[..1]);
    assert!(one.potential > 0.0, "the ball must be within the cutoff");
    assert!(
        (both.potential - 2.0 * one.potential).abs() <= 1e-5 * one.potential,
        "U {} vs 2 × {}",
        both.potential,
        one.potential
    );
    assert!(
        both.force.norm() <= 1e-3 * one.force.norm(),
        "the forces cancel"
    );
}

/// An `ε_ii` pair override sets the species' `ε_i` for the walls too, as a
/// `σ_ii` override sets its `σ_i`.
#[test]
fn like_pair_epsilon_override_reaches_the_wall() {
    use crate::application::sim_config::PairOverrideConfig;
    let rect = Rect::new(-50.0, 50.0, 100.0, -100.0);
    let plain = MainContent::with_config(rect, &crate::test_support::test_config(), None).unwrap();
    let mut config = crate::test_support::test_config();
    let name = SpeciesTable::builtin()
        .species(DEFAULT_SPECIES)
        .name
        .clone();
    config.potential.pair_overrides.push(PairOverrideConfig {
        a: name.clone(),
        b: name,
        sigma: None,
        epsilon: Some(4.0 * SpeciesTable::builtin().species(DEFAULT_SPECIES).epsilon),
    });
    let overridden = MainContent::with_config(rect, &config, None).unwrap();
    let u = |mc: &MainContent| {
        let walls = mc.world().physics.soft_walls().expect("soft walls");
        let z = 0.9 * walls.equilibrium_distance(DEFAULT_SPECIES);
        walls.force_and_potential(DEFAULT_SPECIES, z).1
    };
    let ratio = u(&overridden) / u(&plain);
    assert!((ratio - 2.0).abs() < 1e-4, "ε_iw ratio {ratio}, want √4");
}
