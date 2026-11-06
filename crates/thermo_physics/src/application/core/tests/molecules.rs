// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Rigid linear molecules in the simulation (#406, `N2_2S`): energy with
//! rotation, walls acting on the sites, equipartition by collisions and
//! saves.

use crate::application::persistence::{LoadSource, SaveStore};
use crate::application::thermo_field::LocalKineticStats;
use crate::application::{MainContent, SimConfig};
use crate::objects2d::ball_impl::{Ball2D, SpeciesId, SpeciesTable};
use crate::objects2d::{Rect, Vec2D};

fn species(name: &str) -> SpeciesId {
    SpeciesTable::builtin().by_name(name).unwrap()
}

/// The default config (dt = 0.005).
fn world(seed: u64, half: f32) -> MainContent {
    let mut mc = MainContent::with_config(
        Rect::new(-half, half, 2.0 * half, -2.0 * half),
        &SimConfig::default(),
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

/// A lattice of `N2_2S` (with every `ar_every`-th one `Ar`, 0 for none) at
/// spacing 1.6, random-ish orientations, speeds about `speed`.
fn gas(mc: &mut MainContent, side: usize, speed: f32, ar_every: usize) {
    let (n2, ar) = (species("N2_2S"), species("Ar"));
    let offset = -0.8 * (side - 1) as f32;
    for i in 0..side {
        for j in 0..side {
            let k = i * side + j;
            let pos = Vec2D::new(offset + 1.6 * i as f32, offset + 1.6 * j as f32);
            let phase = (i * 7 + j * 13) as f32;
            let v = Vec2D::new(phase.sin(), (1.3 * phase).cos()).scaled(speed);
            let kind = if ar_every > 0 && k.is_multiple_of(ar_every) {
                ar
            } else {
                n2
            };
            add(mc, pos, v, 0.37 * phase, 0.0, kind);
        }
    }
    mc.collect_neighbours_by_cell(0.005);
}

fn reported_energy(mc: &MainContent) -> f32 {
    let sim = &mc.world().sim;
    sim.get_kinetic_energy() + sim.get_potential_energy()
}

/// A gas of dumbbells and Ar, in a box (soft walls on the sites): the
/// total energy, rotation and wall energy included, is conserved. Measured
/// against the kinetic energy (the total is small: attraction cancels most
/// of it). The error falls as dt² (1.9e-2, 3.5e-3, 9.2e-4 of the total at
/// dt = 0.005, 0.0025, 0.00125 over the same time): integration, not a
/// missing force.
#[test]
fn dumbbell_gas_conserves_its_energy() {
    let mut mc = world(1, 9.0);
    gas(&mut mc, 9, 1.2, 4);
    mc.simulation_step().unwrap();
    let e0 = reported_energy(&mc);
    let mut max_drift = 0.0f32;
    let mut turned = false;
    let kin0 = mc.world().sim.get_kinetic_energy();
    for _ in 0..4_000 {
        mc.simulation_step().unwrap();
        max_drift = max_drift.max((reported_energy(&mc) - e0).abs() / kin0);
        turned |= mc.world().balls.iter().any(|b| b.omega != 0.0);
    }
    println!("dumbbells: E0 {e0}, max relative drift {max_drift:e}");
    assert!(turned, "no torque acted");
    assert!(
        max_drift < 3e-3,
        "energy drift {max_drift:e} of the kinetic energy"
    );
    assert_eq!(mc.world().sim.wall_contacts, 0, "backstop reached");
}

/// A dumbbell hitting a wall end-first is turned by it, and no site gets
/// behind the wall.
#[test]
fn a_wall_turns_a_dumbbell() {
    let mut mc = world(2, 9.0);
    let n2 = species("N2_2S");
    // Tilted, flying left at the box wall x = -9.
    add(
        &mut mc,
        Vec2D::new(-6.0, 0.0),
        Vec2D::new(-2.0, 0.0),
        0.5,
        0.0,
        n2,
    );
    mc.collect_neighbours_by_cell(0.005);
    let molecule = mc.world().species().species(n2).molecule.clone().unwrap();
    let mut min_site_x = f32::INFINITY;
    for _ in 0..1_500 {
        mc.simulation_step().unwrap();
        let ball = &mc.world().balls[0];
        for site in &molecule.sites {
            min_site_x = min_site_x.min(ball.get_position().x + ball.theta.cos() * site.offset);
        }
    }
    let ball = &mc.world().balls[0];
    assert!(ball.v.x > 0.0, "the ball bounced");
    assert!(ball.omega != 0.0, "the wall turned it");
    assert!(
        min_site_x > -9.0,
        "a site got behind the wall: {min_site_x}"
    );
}

/// Torques bring a dumbbell gas to equipartition: `⟨E_rot⟩ ≈ ⟨E_trans⟩ / 2`
/// per molecule (1 rotational, 2 translational degrees of freedom).
#[test]
#[ignore = "suite:physics (slow)"]
fn dumbbells_reach_equipartition() {
    let mut mc = world(3, 12.0);
    gas(&mut mc, 12, 1.2, 0);
    for _ in 0..20_000 {
        mc.simulation_step().unwrap();
    }
    let (mut rot, mut trans) = (0.0f64, 0.0f64);
    for _ in 0..800 {
        for _ in 0..25 {
            mc.simulation_step().unwrap();
        }
        let mut stats = LocalKineticStats::new();
        for ball in &mc.world().balls {
            stats.add_ball(ball);
        }
        let v_com = stats.center_of_mass_velocity().unwrap();
        for ball in &mc.world().balls {
            rot += f64::from(ball.rotational_energy());
            trans += f64::from(0.5 * ball.mass * (ball.v - &v_com).norm_sq());
        }
    }
    let ratio = rot / (0.5 * trans);
    println!("dumbbells: ⟨E_rot⟩ / (⟨E_trans⟩ / 2) = {ratio:.3}");
    assert!((ratio - 1.0).abs() < 0.1, "equipartition ratio {ratio}");
}

/// Saves keep a dumbbell's orientation.
#[test]
fn saves_keep_a_dumbbell() {
    let mut mc = world(4, 9.0);
    add(
        &mut mc,
        Vec2D::new(1.0, 2.0),
        Vec2D::new(0.5, 0.0),
        0.3,
        2.0,
        species("N2_2S"),
    );
    for _ in 0..21 {
        mc.simulation_step().unwrap();
    }
    let dir = std::env::temp_dir().join(format!("mlj_molecules_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = SaveStore::new(dir);
    mc.save_world(&store).unwrap();
    let mut loaded = world(5, 9.0);
    loaded
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    let (a, b) = (&mc.world().balls[0], &loaded.world().balls[0]);
    assert_eq!(a.species(), b.species());
    assert_eq!((a.theta, a.omega, a.alpha), (b.theta, b.omega, b.alpha));
}
