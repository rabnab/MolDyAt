// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Orientational degrees of freedom (#348): a species with a moment of
//! inertia rotates, its rotational energy is part of the kinetic energy and
//! the temperature (equipartition), thermostats act on it, and saves keep it.

use super::test_config;
use crate::application::persistence::{LoadSource, SaveStore};
use crate::application::sim_config::SpeciesConfig;
use crate::application::thermo_field::LocalKineticStats;
use crate::application::{MainContent, SimConfig};
use crate::objects2d::ball_impl::{Ball2D, SpeciesId};
use crate::objects2d::{Rect, Vec2D};
use std::f32::consts::PI;

/// `I` of the rotating species in u·Å²: 0.43 in Ar units.
const INERTIA_U_A2: f64 = 200.0;

fn species_entry(name: &str, inertia: Option<f64>) -> SpeciesConfig {
    SpeciesConfig {
        name: name.into(),
        sigma_angstrom: Some(3.405),
        epsilon_kelvin: Some(119.8),
        mass_u: Some(39.948),
        colour: vec![0.2, 0.6, 1.0],
        display_radius_angstrom: None,
        source: None,
        moment_of_inertia_u_angstrom2: inertia,
        water: None,
        sites: Vec::new(),
    }
}

/// A run with two Ar-like species: `Rot` (with orientation) and `Ball`.
fn rotating_config() -> SimConfig {
    let mut config = test_config();
    config.species = vec![
        species_entry("Rot", Some(INERTIA_U_A2)),
        species_entry("Ball", None),
    ];
    config
}

const ROT: SpeciesId = SpeciesId::new(0);
const BALL: SpeciesId = SpeciesId::new(1);

fn world(seed: u64) -> MainContent {
    let mut mc = MainContent::with_config(
        Rect::new(-30.0, 30.0, 60.0, -60.0),
        &rotating_config(),
        Some(seed),
    )
    .unwrap();
    mc.world_mut().speed_limit_override = Some(f32::INFINITY);
    mc
}

fn add(mc: &mut MainContent, pos: Vec2D, v: Vec2D, omega: f32, species: SpeciesId) {
    let table = mc.world().species().clone();
    let id = mc.world().balls.len() + 1;
    let mut ball = Ball2D::of_species(id, pos, species, table.species(species));
    ball.v = v;
    ball.omega = omega;
    mc.world_mut().add_ball_and_register(ball);
}

fn reported_energy(mc: &MainContent) -> f32 {
    let sim = &mc.world().sim;
    sim.get_kinetic_energy() + sim.get_potential_energy()
}

#[test]
fn species_inertia_is_converted_to_argon_units() {
    let mc = world(1);
    let table = mc.world().species();
    let expected = (INERTIA_U_A2 / (39.948 * 3.405 * 3.405)) as f32;
    assert_eq!(table.species(ROT).model.inertia(), Some(expected));
    assert_eq!(table.species(BALL).model.inertia(), None);
}

/// A free rotor turns at constant ω, its angle stays wrapped, and its
/// rotational energy is in the kinetic energy.
#[test]
fn free_rotor_turns_at_constant_speed() {
    let mut mc = world(2);
    let omega = 3.0;
    add(&mut mc, Vec2D::zero(), Vec2D::zero(), omega, ROT);
    let dt = mc.active_time_step();
    let steps = 1_000;
    for _ in 0..steps {
        mc.simulation_step().unwrap();
    }
    let ball = &mc.world().balls[0];
    assert_eq!(ball.omega, omega);
    let turned = omega * dt * steps as f32;
    let expected = turned - 2.0 * PI * ((turned + PI) / (2.0 * PI)).floor();
    assert!(
        (ball.theta - expected).abs() < 1e-3,
        "{} vs {expected}",
        ball.theta
    );
    assert!((-PI..PI).contains(&ball.theta));
    let e_rot = 0.5 * ball.inertia * omega * omega;
    assert!((mc.world().sim.get_kinetic_energy() - e_rot).abs() < 1e-6);
}

/// A gas of rotating and plain balls conserves its total energy, rotation
/// included.
#[test]
fn rotating_gas_conserves_its_energy() {
    let mut mc = world(3);
    for i in 0..10 {
        for j in 0..10 {
            let pos = Vec2D::new(-13.5 + 3.0 * i as f32, -13.5 + 3.0 * j as f32);
            let v = Vec2D::new(((i * 7 + j) as f32).sin(), ((i + 3 * j) as f32).cos());
            let (species, omega) = if (i + j) % 2 == 0 {
                (ROT, ((i * 5 + j) as f32).sin() * 2.0)
            } else {
                (BALL, 0.0)
            };
            add(&mut mc, pos, v, omega, species);
        }
    }
    mc.collect_neighbours_by_cell(super::DT_GOAL);
    mc.simulation_step().unwrap();
    let e0 = reported_energy(&mc);
    let mut max_drift = 0.0f32;
    for _ in 0..1_000 {
        mc.simulation_step().unwrap();
        max_drift = max_drift.max((reported_energy(&mc) - e0).abs() / e0.abs());
    }
    assert!(max_drift < 1e-3, "energy drift {max_drift:e}");
}

/// Temperature by equipartition: one more degree of freedom per rotating
/// ball, its `I ω²` in the numerator.
#[test]
fn temperature_counts_rotation() {
    let mut stats = LocalKineticStats::new();
    let table = rotating_config().species_table().unwrap();
    for (i, (v, omega, species)) in [
        (Vec2D::new(1.0, 0.0), 2.0, ROT),
        (Vec2D::new(-1.0, 0.0), 0.0, BALL),
    ]
    .into_iter()
    .enumerate()
    {
        let mut ball = Ball2D::of_species(i, Vec2D::zero(), species, table.species(species));
        ball.v = v;
        ball.omega = omega;
        stats.add_ball(&ball);
    }
    let inertia = table.species(ROT).model.inertia().unwrap();
    // Σ m |v - v_com|² = 2, Σ I ω² = 4 I; dof = 2·(2 − 1) + 1.
    let expected = (2.0 + 4.0 * inertia) / 3.0;
    assert_eq!(stats.thermal_dof(), Some(3.0));
    assert!((stats.temperature().unwrap() - expected).abs() < 1e-6);
}

/// A thermostat counts rotation as degrees of freedom and scales it with
/// the translation: the equipartition temperature, rotation included,
/// reaches the target, and the rotation of a cool gas heats up with it.
/// (Isotropic pair forces give no torque, so here rotation exchanges energy
/// only through the thermostat and keeps its own share; equipartition by
/// collisions is tested with the anisotropic models, #349 and #406.)
#[test]
fn thermostat_brings_rotation_to_the_target() {
    const TARGET: f32 = 0.8;
    let mut mc = world(4);
    let inertia = mc.world().species().species(ROT).model.inertia().unwrap();
    for i in 0..12 {
        for j in 0..12 {
            let pos = Vec2D::new(-16.5 + 3.0 * i as f32, -16.5 + 3.0 * j as f32);
            let v = Vec2D::new(((i * 7 + j) as f32).sin(), ((i + 3 * j) as f32).cos()).scaled(0.4);
            // ½ I ω² close to ½ m v_x² (m = 1) at the start.
            let omega =
                0.4 * ((i * 5 + 3 * j) as f32).sin() * std::f32::consts::SQRT_2 / inertia.sqrt();
            let species = if (i + j) % 2 == 0 { ROT } else { BALL };
            add(
                &mut mc,
                pos,
                v,
                if species == ROT { omega } else { 0.0 },
                species,
            );
        }
    }
    mc.collect_neighbours_by_cell(super::DT_GOAL);
    let rot_start: f32 = mc.world().balls.iter().map(|b| b.rotational_energy()).sum();
    mc.add_thermostat(Vec2D::zero());
    let id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(id);
    {
        let thermostat = mc.world_mut().thermostats.find_mut(id).unwrap();
        thermostat.target_temperature = TARGET;
        thermostat.radius = 40.0;
    }
    for _ in 0..3_000 {
        mc.simulation_step().unwrap();
    }
    let (mut e_rot, mut n_rot, mut e_trans, mut n) = (0.0f64, 0, 0.0f64, 0);
    let mut temperature = 0.0f64;
    for _ in 0..200 {
        for _ in 0..10 {
            mc.simulation_step().unwrap();
        }
        let mut stats = LocalKineticStats::new();
        for ball in &mc.world().balls {
            stats.add_ball(ball);
            e_trans += f64::from(0.5 * ball.mass * ball.v.norm_sq());
            n += 1;
            if ball.inertia > 0.0 {
                e_rot += f64::from(ball.rotational_energy());
                n_rot += 1;
            }
        }
        temperature += f64::from(stats.temperature().unwrap());
    }
    let rot_per_dof = e_rot / n_rot as f64;
    let trans_per_dof = e_trans / n as f64 / 2.0;
    let temperature = temperature / 200.0;
    println!(
        "rotation {rot_per_dof:.4}, translation {trans_per_dof:.4} per dof, T {temperature:.4}, target {TARGET}"
    );
    assert!(
        (temperature / f64::from(TARGET) - 1.0).abs() < 0.05,
        "temperature {temperature} vs target {TARGET}"
    );
    // The gas starts at T ≈ 0.16: the thermostat heats rotation too.
    let rot_start_per_dof = f64::from(rot_start) / (n_rot as f64 / 200.0);
    assert!(
        rot_per_dof > 2.0 * rot_start_per_dof,
        "rotation {rot_per_dof} per dof, {rot_start_per_dof} at the start"
    );
    assert!(trans_per_dof > 0.0);
}

/// Saves keep the orientation of rotating balls.
#[test]
fn saves_keep_the_orientation() {
    let mut mc = world(5);
    add(
        &mut mc,
        Vec2D::new(1.0, 2.0),
        Vec2D::new(0.5, 0.0),
        1.5,
        ROT,
    );
    add(&mut mc, Vec2D::new(-5.0, 2.0), Vec2D::zero(), 0.0, BALL);
    for _ in 0..37 {
        mc.simulation_step().unwrap();
    }
    let dir = std::env::temp_dir().join(format!("mlj_rotation_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = SaveStore::new(dir);
    mc.save_world(&store).unwrap();
    let mut loaded = world(6);
    loaded
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    let (a, b) = (&mc.world().balls[0], &loaded.world().balls[0]);
    assert_eq!((a.theta, a.omega, a.alpha), (b.theta, b.omega, b.alpha));
    assert!(a.theta != 0.0);
    assert_eq!(loaded.world().balls[1].theta, 0.0);
}

/// Balls seeded with random speeds spin too: the mean rotational energy per
/// rotating ball matches half the mean translational energy (one degree of
/// freedom against two), so a fresh gas reports its temperature right away
/// instead of starting with cold rotation; non-rotating balls stay at rest.
#[test]
fn seeded_balls_start_with_a_thermal_spin() {
    let mut mc = world(11);
    mc.seed_random_mixture(400, 2.0, &[("Rot".into(), 1), ("Ball".into(), 1)])
        .unwrap();
    let (mut e_rot, mut e_trans, mut n_rot) = (0.0f64, 0.0f64, 0);
    for ball in &mc.world().balls {
        if ball.inertia > 0.0 {
            e_rot += f64::from(0.5 * ball.inertia * ball.omega * ball.omega);
            e_trans += f64::from(0.5 * ball.mass * ball.v.norm_sq());
            n_rot += 1;
        } else {
            assert_eq!(ball.omega, 0.0);
        }
    }
    assert!(n_rot > 100, "{n_rot} rotating balls seeded");
    let ratio = e_rot / (0.5 * e_trans);
    assert!((ratio - 1.0).abs() < 0.2, "E_rot / (E_trans / 2) = {ratio}");
}
