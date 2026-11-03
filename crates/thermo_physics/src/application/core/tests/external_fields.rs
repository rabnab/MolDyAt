// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! External fields (#347): gravity as a `UniformField`, the harmonic trap
//! from `[[fields]]`, their energy and their config.

use super::test_config;
use crate::application::sim_config::{FieldConfig, FieldKind};
use crate::application::{MainContent, SimConfig};
use crate::objects2d::ball_impl::BallFactory;
use crate::objects2d::ball_impl::external_field::{
    ExternalField, Field, HarmonicTrap, UniformField,
};
use crate::objects2d::{Rect, Vec2D};

const STIFFNESS: f32 = 0.05;

fn trap_config(center: [f32; 2]) -> SimConfig {
    let mut config = test_config();
    config.fields = vec![FieldConfig {
        kind: FieldKind::Harmonic,
        center,
        stiffness: STIFFNESS,
    }];
    config
}

/// `-∇U / m` by central differences.
fn numeric_acceleration(field: &impl ExternalField, pos: Vec2D, mass: f32) -> Vec2D {
    let h = 1e-2;
    let d = |dx: f32, dy: f32| {
        (field.potential(pos + Vec2D::new(dx, dy), mass)
            - field.potential(pos - &Vec2D::new(dx, dy), mass))
            / (2.0 * h)
    };
    Vec2D::new(-d(h, 0.0) / mass, -d(0.0, h) / mass)
}

#[test]
fn field_acceleration_is_minus_the_gradient_of_its_potential() {
    let fields = [
        Field::Harmonic(HarmonicTrap {
            center: Vec2D::new(1.0, -2.0),
            stiffness: 0.3,
        }),
        Field::Harmonic(HarmonicTrap {
            center: Vec2D::zero(),
            stiffness: 2.0,
        }),
    ];
    for field in &fields {
        for (pos, mass) in [(Vec2D::new(3.0, 4.0), 1.0), (Vec2D::new(-2.5, 0.5), 2.5)] {
            let a = field.acceleration(pos, mass);
            let n = numeric_acceleration(field, pos, mass);
            assert!(
                (a - &n).norm() < 1e-3 * a.norm().max(1.0),
                "{field:?} at {pos}: {a:?} vs {n:?}"
            );
        }
    }
    let gravity = UniformField {
        g: 0.7,
        floor: -5.0,
    };
    let pos = Vec2D::new(1.0, 3.0);
    let n = numeric_acceleration(&gravity, pos, 2.0);
    assert!((gravity.acceleration(pos, 2.0) - &n).norm() < 1e-3);
    assert_eq!(gravity.potential(Vec2D::new(0.0, -5.0), 2.0), 0.0);
}

/// Balls oscillating in a trap (no wall in reach): the reported total
/// energy, which includes the trap energy, is conserved.
#[test]
fn trapped_gas_conserves_its_energy() {
    let rect = Rect::new(-40.0, 40.0, 80.0, -80.0);
    let mut mc = MainContent::with_config(rect, &trap_config([0.0, 0.0]), Some(17)).unwrap();
    mc.world_mut().speed_limit_override = Some(f32::INFINITY);
    let factory = BallFactory::new();
    for i in 0..40 {
        let angle = i as f32 * 2.399;
        let radius = 3.0 + (i % 5) as f32 * 2.5;
        let pos = Vec2D::new(angle.cos(), angle.sin()).scaled(radius);
        let v = Vec2D::new(-angle.sin(), angle.cos()).scaled(0.6);
        mc.world_mut()
            .add_ball_and_register(factory.create_massive_with_velocity(pos, v));
    }
    mc.collect_neighbours_by_cell(super::DT_GOAL);
    mc.simulation_step().unwrap();
    let energy = |mc: &MainContent| {
        let sim = &mc.world().sim;
        sim.get_kinetic_energy() + sim.get_potential_energy()
    };
    let e0 = energy(&mc);
    let trap0 = trap_energy(&mc);
    assert!(
        trap0 > 0.1 * e0.abs(),
        "the trap holds a share of the energy"
    );
    let mut max_drift = 0.0f32;
    for _ in 0..4_000 {
        mc.simulation_step().unwrap();
        max_drift = max_drift.max((energy(&mc) - e0).abs() / e0.abs());
    }
    // 3.2e-3: set by the pair collisions the trap packs together (the
    // single-ball test shows the trap alone keeps 1e-5).
    assert!(max_drift < 5e-3, "energy drift {max_drift:e}");
    let max_r = mc
        .world()
        .balls
        .iter()
        .map(|b| b.get_position().norm())
        .fold(0.0, f32::max);
    assert!(max_r < 30.0, "a ball left the trap: r = {max_r}");
    assert_eq!(mc.world().sim.wall_contacts, 0);
}

fn trap_energy(mc: &MainContent) -> f32 {
    mc.world()
        .balls
        .iter()
        .map(|b| 0.5 * STIFFNESS * b.get_position().norm_sq())
        .sum()
}

/// Without `[[fields]]`, gravity alone: a world built from the config with
/// an empty field list runs bit for bit as before (the drift and fixture
/// gates cover whole runs); here a new ball's initial acceleration.
#[test]
fn a_new_ball_starts_with_the_field_acceleration() {
    let rect = Rect::new(-20.0, 20.0, 40.0, -40.0);
    let mut mc = MainContent::with_config(rect, &trap_config([2.0, 0.0]), None).unwrap();
    let ball = BallFactory::new().create_massive_with_velocity(Vec2D::new(6.0, 1.0), Vec2D::zero());
    let mass = ball.mass;
    mc.world_mut().add_ball_and_register(ball);
    let a = mc.world().balls[0].accel;
    let expected = Vec2D::new(-4.0, -1.0).scaled(STIFFNESS / mass);
    assert!((a - &expected).norm() < 1e-6, "{a:?} vs {expected:?}");
}

#[test]
fn fields_are_read_from_the_config_and_reported_back() {
    let config = SimConfig::from_toml(
        "[[fields]]\nkind = \"harmonic\"\ncenter = [1.0, -2.0]\nstiffness = 0.5\n",
    )
    .unwrap();
    assert_eq!(config.fields.len(), 1);
    let mc = MainContent::with_config(Rect::new(-10.0, 10.0, 20.0, -20.0), &config, None).unwrap();
    assert_eq!(mc.config().fields, config.fields);
    assert_eq!(
        mc.world().physics.fields(),
        [Field::Harmonic(HarmonicTrap {
            center: Vec2D::new(1.0, -2.0),
            stiffness: 0.5,
        })]
    );
}

#[test]
fn invalid_fields_are_rejected() {
    for (toml, message) in [
        (
            "kind = \"harmonic\"\ncenter = [0.0, 0.0]\nstiffness = 0.0",
            "fields[0].stiffness",
        ),
        (
            "kind = \"harmonic\"\ncenter = [0.0, 0.0]\nstiffness = 500.0",
            "fields[0].stiffness",
        ),
        (
            "kind = \"harmonic\"\ncenter = [1e9, 0.0]\nstiffness = 1.0",
            "fields[0].center.x",
        ),
        (
            "kind = \"vortex\"\ncenter = [0.0, 0.0]\nstiffness = 1.0",
            "vortex",
        ),
        (
            "kind = \"harmonic\"\ncenter = [0.0, 0.0]\nstiffness = 1.0\nk = 2",
            "k",
        ),
    ] {
        let err = SimConfig::from_toml(&format!("[[fields]]\n{toml}\n")).expect_err(toml);
        let err = format!("{err:?}");
        assert!(err.contains(message), "{toml}: {err}");
    }
}

/// One ball oscillating in the trap, no collisions: velocity Verlet keeps
/// the energy of a harmonic oscillator to (ω·dt)².
#[test]
fn single_ball_in_a_trap_keeps_its_energy() {
    let rect = Rect::new(-40.0, 40.0, 80.0, -80.0);
    let mut mc = MainContent::with_config(rect, &trap_config([0.0, 0.0]), None).unwrap();
    mc.world_mut().speed_limit_override = Some(f32::INFINITY);
    let ball = BallFactory::new()
        .create_massive_with_velocity(Vec2D::new(10.0, 0.0), Vec2D::new(0.0, 1.0));
    mc.world_mut().add_ball_and_register(ball);
    mc.collect_neighbours_by_cell(super::DT_GOAL);
    mc.simulation_step().unwrap();
    let energy = |mc: &MainContent| {
        let sim = &mc.world().sim;
        sim.get_kinetic_energy() + sim.get_potential_energy()
    };
    let e0 = energy(&mc);
    let mut max_drift = 0.0f32;
    for _ in 0..10_000 {
        mc.simulation_step().unwrap();
        max_drift = max_drift.max((energy(&mc) - e0).abs() / e0);
    }
    assert!(max_drift < 1e-5, "energy drift {max_drift:e}");
}
