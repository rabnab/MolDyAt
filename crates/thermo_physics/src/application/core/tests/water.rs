// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Mercedes-Benz water (#349) in the simulation: energy conservation with
//! hydrogen bonds and torques, equipartition by collisions, water–other
//! species through the Lennard-Jones part only, and the time-step limit.

use super::test_config;
use crate::application::observables::{Obs, Values};
use crate::application::thermo_field::LocalKineticStats;
use crate::application::{MainContent, SimConfig};
use crate::objects2d::ball_impl::interaction::Model;
use crate::objects2d::ball_impl::water::MbWater;
use crate::objects2d::ball_impl::{Ball2D, SpeciesId, SpeciesTable};
use crate::objects2d::{Rect, Vec2D};

/// The derived time step limit of water (#427).
fn water_dt() -> f32 {
    super::time_step_limit_of("H2O")
}

fn species(name: &str) -> SpeciesId {
    SpeciesTable::builtin().by_name(name).unwrap()
}

/// The hydrogen-bond parameters of the water species `id` of `table`.
fn mb_water(table: &SpeciesTable, id: SpeciesId) -> MbWater {
    match table.species(id).model {
        Model::MbWater(water) => water,
        _ => panic!("not water"),
    }
}

/// The test config at the water time step.
fn water_config() -> SimConfig {
    let mut config = test_config();
    config.integration.time_step = water_dt();
    config
}

fn world(seed: u64) -> MainContent {
    let mut mc = MainContent::with_config(
        Rect::new(-20.0, 20.0, 40.0, -40.0),
        &water_config(),
        Some(seed),
    )
    .unwrap();
    mc.world_mut().speed_limit_override = Some(f32::INFINITY);
    mc
}

fn add(mc: &mut MainContent, pos: Vec2D, v: Vec2D, theta: f32, omega: f32, kind: SpeciesId) {
    let table = mc.world().species().clone();
    let id = mc.world().balls.len() + 1;
    let mut ball = Ball2D::of_species(id, pos, kind, table.species(kind));
    ball.v = v;
    ball.theta = theta;
    ball.omega = omega;
    mc.world_mut().add_ball_and_register(ball);
}

/// A loose square lattice of water near the bond length, random-ish
/// orientations and small velocities: bonds form and break.
fn water_patch(mc: &mut MainContent, side: usize, speed: f32) {
    let water = species("H2O");
    let r_hb = mb_water(mc.world().species(), water).r_hb;
    let spacing = 1.05 * r_hb;
    let offset = -0.5 * spacing * (side - 1) as f32;
    for i in 0..side {
        for j in 0..side {
            let pos = Vec2D::new(offset + spacing * i as f32, offset + spacing * j as f32);
            let phase = (i * 7 + j * 13) as f32;
            let v = Vec2D::new(phase.sin(), (1.3 * phase).cos()).scaled(speed);
            add(mc, pos, v, 0.37 * phase, 0.0, water);
        }
    }
    mc.collect_neighbours_by_cell(water_dt());
}

fn reported_energy(mc: &MainContent) -> f32 {
    let sim = &mc.world().sim;
    sim.get_kinetic_energy() + sim.get_potential_energy()
}

/// Hydrogen bonds and torques conserve the total energy at the water time
/// step: it fluctuates by up to 0.4 % (about 4 times as much as at half the
/// step) and does not drift (still within that over 16 000 steps).
#[test]
fn water_conserves_its_energy() {
    let mut mc = world(1);
    water_patch(&mut mc, 8, 0.5);
    mc.simulation_step().unwrap();
    let e0 = reported_energy(&mc);
    let mut max_drift = 0.0f32;
    let mut bonded = false;
    for _ in 0..4_000 {
        mc.simulation_step().unwrap();
        max_drift = max_drift.max((reported_energy(&mc) - e0).abs() / e0.abs());
        bonded |= mc.world().balls.iter().any(|b| b.alpha != 0.0);
    }
    println!("water: E0 {e0}, max relative drift {max_drift:e}");
    assert!(bonded, "no torque acted: no hydrogen bond formed");
    assert!(max_drift < 5e-3, "energy drift {max_drift:e}");
}

/// Torques couple rotation to translation: water whose rotation starts
/// cold reaches equipartition, `⟨½ I ω²⟩ ≈ ⟨½ m v_x²⟩`, averaged over a
/// long run (a bonded network exchanges energy slowly, and single snapshots
/// of 144 molecules scatter by ±30 %).
#[test]
#[ignore = "suite:physics (slow)"]
fn torques_bring_rotation_to_equipartition() {
    let mut mc = world(2);
    water_patch(&mut mc, 12, 1.2);
    for _ in 0..40_000 {
        mc.simulation_step().unwrap();
    }
    let (mut rot, mut trans) = (0.0f64, 0.0f64);
    for _ in 0..1_600 {
        for _ in 0..50 {
            mc.simulation_step().unwrap();
        }
        let mut stats = LocalKineticStats::new();
        for ball in &mc.world().balls {
            stats.add_ball(ball);
        }
        let v_com = stats.center_of_mass_velocity().unwrap();
        for ball in &mc.world().balls {
            rot += f64::from(ball.rotational_energy());
            trans += f64::from(0.5 * ball.mass * (ball.v - &v_com).norm_sq()) / 2.0;
        }
    }
    let ratio = rot / trans;
    println!("water: rotation / translation per degree of freedom {ratio:.3}");
    assert!(
        (ratio - 1.0).abs() < 0.15,
        "rotation vs translation per dof {ratio}"
    );
}

/// Water meets Ar through the Lennard-Jones part only: no torque, and the
/// pair energy is the mixed Lennard-Jones value whatever the orientation.
#[test]
fn water_and_argon_meet_by_lennard_jones_only() {
    let (water, ar) = (species("H2O"), species("Ar"));
    let mut mc = world(3);
    let table = mc.world().species().clone();
    let r_hb = mb_water(&table, water).r_hb;
    add(&mut mc, Vec2D::zero(), Vec2D::zero(), 0.0, 0.0, water);
    add(
        &mut mc,
        Vec2D::new(r_hb, 0.0),
        Vec2D::zero(),
        std::f32::consts::PI,
        0.0,
        ar,
    );
    mc.collect_neighbours_by_cell(water_dt());
    mc.simulation_step().unwrap();
    let world = mc.world();
    let pair = world.physics.potential(
        water,
        ar,
        world.balls[0]
            .get_position()
            .dist(world.balls[1].get_position())
            .norm(),
    );
    assert!(world.cached_force_results()[0].torque == 0.0);
    assert!(
        (world.cached_force_results()[0].potential_energy - pair).abs() < 1e-5,
        "{} vs {pair}",
        world.cached_force_results()[0].potential_energy
    );
}

/// While water is present the steps run with at most the water time step;
/// the set time step is kept and used again once the water is gone (#349).
#[test]
fn water_limits_the_time_step_while_present() {
    let mut mc =
        MainContent::with_config(Rect::new(-20.0, 20.0, 40.0, -40.0), &test_config(), None)
            .unwrap();
    let set = mc.time_step();
    assert!(set > water_dt(), "test needs a coarser set step");
    assert_eq!(mc.active_time_step(), set, "no water yet");
    assert_eq!(mc.time_step_limit(), None);

    add(
        &mut mc,
        Vec2D::zero(),
        Vec2D::zero(),
        0.0,
        0.0,
        species("H2O"),
    );
    assert_eq!(mc.active_time_step(), water_dt());
    assert_eq!(mc.time_step_limit(), Some((water_dt(), species("H2O"))));
    assert_eq!(mc.time_step(), set, "the set time step is kept");
    let status = mc.status_info().expect("test config shows ball stats");
    assert_eq!(status.dt_limited_by, Some(species("H2O")));
    assert_eq!(Values::read(&mc)[Obs::Dt], f64::from(water_dt()));

    let t0 = mc.world().sim.get_time();
    mc.simulation_step().unwrap();
    let advanced = mc.world().sim.get_time() - t0;
    assert!(
        (advanced - water_dt()).abs() < 1e-6,
        "step advanced {advanced}"
    );

    assert!(!mc.world().cached_force_results().is_empty());
    mc.world_mut().clear_balls();
    assert_eq!(mc.active_time_step(), set, "back to the set step");
    assert!(mc.world().cached_force_results().is_empty());
}

/// The limit also ends when the last water ball becomes invalid and is
/// removed, and the removal leaves no force results at the shifted indices.
#[test]
fn water_limit_ends_when_the_last_water_ball_is_removed() {
    let mut mc =
        MainContent::with_config(Rect::new(-20.0, 20.0, 40.0, -40.0), &test_config(), None)
            .unwrap();
    let (water, ar) = (species("H2O"), species("Ar"));
    add(
        &mut mc,
        Vec2D::new(5.0, 0.0),
        Vec2D::zero(),
        0.0,
        0.0,
        water,
    );
    add(&mut mc, Vec2D::new(-5.0, 0.0), Vec2D::zero(), 0.0, 0.0, ar);
    mc.simulation_step().unwrap();
    assert_eq!(mc.world().cached_force_results().len(), 2);

    mc.world_mut().balls[0].set_position_and_update_tail(Vec2D::new(f32::NAN, 0.0));
    mc.world_mut().remove_invalid_balls();
    assert_eq!(mc.world().balls.len(), 1);
    assert_ne!(mc.time_step_limit().map(|(_, s)| s), Some(water));
    assert!(mc.world().cached_force_results().is_empty());
}

/// The `+` key does not raise the time step above the water limit, and `-`
/// halves the step the simulation runs with.
#[test]
fn time_step_keys_respect_the_water_limit() {
    let mut mc = world(3);
    add(
        &mut mc,
        Vec2D::zero(),
        Vec2D::zero(),
        0.0,
        0.0,
        species("H2O"),
    );
    mc.halve_time_step();
    assert_eq!(mc.active_time_step(), 0.5 * water_dt());
    mc.double_time_step();
    assert_eq!(mc.active_time_step(), water_dt());
    mc.double_time_step();
    assert_eq!(mc.time_step(), water_dt(), "+ stops at the limit");

    let mut coarse =
        MainContent::with_config(Rect::new(-20.0, 20.0, 40.0, -40.0), &test_config(), None)
            .unwrap();
    add(
        &mut coarse,
        Vec2D::zero(),
        Vec2D::zero(),
        0.0,
        0.0,
        species("H2O"),
    );
    let set = coarse.time_step();
    coarse.double_time_step();
    assert_eq!(coarse.time_step(), set, "+ above the limit changes nothing");
    coarse.halve_time_step();
    assert_eq!(coarse.time_step(), 0.5 * water_dt());
}

/// Stopping the balls (F1) also stops their rotation, so a bonded water
/// patch does not heat up again from its rotational energy.
#[test]
fn stopping_the_balls_stops_their_rotation() {
    let mut mc = world(4);
    water_patch(&mut mc, 6, 0.5);
    for ball in &mut mc.world_mut().balls {
        ball.omega = 3.0;
    }
    mc.stop_all_balls();
    assert!(
        mc.world()
            .balls
            .iter()
            .all(|b| b.v == Vec2D::zero() && b.omega == 0.0)
    );
}

/// Mean pressure on the box walls over `steps` steps: the soft wall forces
/// on the balls (normal to their wall) per wall length.
fn wall_pressure(mc: &mut MainContent, steps: usize) -> f64 {
    let rect = mc.world().world_rect;
    let perimeter = 2.0 * f64::from(rect.w.abs() + rect.h.abs());
    let mut sum = 0.0f64;
    for _ in 0..steps {
        mc.simulation_step().unwrap();
        sum += mc
            .world()
            .cached_force_results()
            .iter()
            .map(|r| f64::from(r.wall_force.x.abs() + r.wall_force.y.abs()))
            .sum::<f64>();
    }
    sum / (steps as f64 * perimeter)
}

/// MB water at density `rho` (per `r_HB²`) held by a thermostat at `t_mb`
/// (in units of `ε_HB / k_B`): its wall pressure in Ar units.
fn pressure_at_density(t_mb: f32, rho: f32) -> f64 {
    let water = species("H2O");
    let mb = mb_water(SpeciesTable::builtin(), water);
    // 144 molecules.
    let side = (144.0 / rho).sqrt() * mb.r_hb;
    let mut mc = MainContent::with_config(
        Rect::new(-0.5 * side, 0.5 * side, side, -side),
        &water_config(),
        Some(7),
    )
    .unwrap();
    mc.world_mut().speed_limit_override = Some(f32::INFINITY);
    let spacing = side / 12.0;
    for i in 0..12 {
        for j in 0..12 {
            let pos = Vec2D::new(
                -0.5 * side + spacing * (i as f32 + 0.5),
                -0.5 * side + spacing * (j as f32 + 0.5),
            );
            add(
                &mut mc,
                pos,
                Vec2D::zero(),
                0.37 * (i * 7 + j * 13) as f32,
                0.0,
                water,
            );
        }
    }
    mc.collect_neighbours_by_cell(water_dt());
    mc.add_thermostat(Vec2D::zero());
    let id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(id);
    {
        let thermostat = mc.world_mut().thermostats.find_mut(id).unwrap();
        thermostat.target_temperature = t_mb * mb.epsilon_hb;
        thermostat.radius = side;
    }
    for _ in 0..100_000 {
        mc.simulation_step().unwrap();
    }
    wall_pressure(&mut mc, 100_000)
}

/// The density anomaly of MB water: below its density maximum the pressure
/// at fixed volume falls as the temperature rises, `(∂P/∂T)_V < 0`, since
/// `(∂P/∂T)_V = α / κ_T` and the expansion coefficient `α` is negative
/// there. A plain Lennard-Jones liquid always has `(∂P/∂T)_V > 0`.
///
/// At `ρ = 0.82 / r_HB²` the pressure peaks near `T ≈ 0.135 ε_HB/k_B` and
/// dips between 0.15 and 0.17 (`docs/units.md`); single runs scatter by
/// about ±0.1, so means of two and three temperatures are compared. About
/// 7 minutes: opt-in (`MOLDYAT_SLOW_WATER=1`, owner decision), so the drift
/// suite, which runs every ignored test, stays fast.
#[test]
#[ignore = "suite:water-anomaly (opt-in, MOLDYAT_SLOW_WATER=1)"]
fn water_has_a_negative_thermal_pressure_coefficient() {
    if std::env::var_os("MOLDYAT_SLOW_WATER").is_none() {
        println!("skipped: set MOLDYAT_SLOW_WATER=1 to run the MB water anomaly test");
        return;
    }
    let mean = |ts: &[f32]| {
        ts.iter()
            .map(|&t| pressure_at_density(t, 0.82))
            .sum::<f64>()
            / ts.len() as f64
    };
    let (cold, warm) = (mean(&[0.13, 0.14]), mean(&[0.15, 0.16, 0.17]));
    println!("MB water at ρ = 0.82: P(0.13..0.14) = {cold:.4}, P(0.15..0.17) = {warm:.4}");
    assert!(
        warm < cold,
        "pressure rose from {cold} to {warm} with temperature"
    );
}
