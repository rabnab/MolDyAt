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

/// The derived time step limits of H₂ and CO₂ (#427).
fn h2_dt() -> f32 {
    super::time_step_limit_of("H2_2S")
}
fn co2_dt() -> f32 {
    super::time_step_limit_of("CO2_3S")
}

/// The step CO₂'s energy errors are measured at (#408). Its derived limit
/// (#427) is looser: the librations of bound CO₂ are a collective motion
/// no pair frequency covers (see `time_step_limit`), so these tests pin
/// the accuracy at this step instead.
const CO2_ACCURATE_DT: f32 = 0.0025;

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
    gas_of(mc, "N2_2S", 1.6, side, speed, ar_every);
}

/// [`gas`] of the molecule `name` at lattice spacing `spacing`.
fn gas_of(
    mc: &mut MainContent,
    name: &str,
    spacing: f32,
    side: usize,
    speed: f32,
    ar_every: usize,
) {
    let (n2, ar) = (species(name), species("Ar"));
    let offset = -0.5 * spacing * (side - 1) as f32;
    for i in 0..side {
        for j in 0..side {
            let k = i * side + j;
            let pos = Vec2D::new(offset + spacing * i as f32, offset + spacing * j as f32);
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

// ── H₂ (#407) ───────────────────────────────────────────────────────────

/// `world` at the H₂ time step.
fn h2_world(seed: u64, half: f32, dt: f32) -> MainContent {
    let mut config = SimConfig::default();
    config.integration.time_step = dt;
    let mut mc = MainContent::with_config(
        Rect::new(-half, half, 2.0 * half, -2.0 * half),
        &config,
        Some(seed),
    )
    .unwrap();
    mc.world_mut().speed_limit_override = Some(f32::INFINITY);
    mc
}

/// Max energy error, as a share of the kinetic energy, of an H₂ gas at
/// about 100 K over 5 time units at `dt`.
fn h2_energy_error(dt: f32) -> f32 {
    let mut mc = h2_world(6, 9.0, dt);
    let h2 = species("H2_2S");
    // Speeds of a few k_B T for the light molecules (m = 0.0505).
    let side = 9;
    for i in 0..side {
        for j in 0..side {
            let pos = Vec2D::new(-7.2 + 1.8 * i as f32, -7.2 + 1.8 * j as f32);
            let phase = (i * 7 + j * 13) as f32;
            let v = Vec2D::new(phase.sin(), (1.3 * phase).cos()).scaled(4.0);
            add(
                &mut mc,
                pos,
                v,
                0.37 * phase,
                15.0 * (0.9 * phase).sin(),
                h2,
            );
        }
    }
    mc.collect_neighbours_by_cell(dt);
    mc.simulation_step().unwrap();
    let (e0, kin0) = (reported_energy(&mc), mc.world().sim.get_kinetic_energy());
    let mut max = 0.0f32;
    for _ in 0..(5.0 / dt) as usize {
        mc.simulation_step().unwrap();
        max = max.max((reported_energy(&mc) - e0).abs() / kin0);
    }
    max
}

/// At the H₂ time step the energy holds (1e-3 of the kinetic energy) and
/// the error shrinks with dt, so it is the step, not the model.
#[test]
fn h2_gas_conserves_its_energy_at_its_time_step() {
    let coarse = h2_energy_error(h2_dt());
    let fine = h2_energy_error(0.5 * h2_dt());
    println!("H2: energy error {coarse:e} at dt, {fine:e} at dt/2");
    assert!(coarse < 1e-3, "energy error {coarse:e}");
    assert!(
        fine < 0.5 * coarse,
        "no convergence: {coarse:e} -> {fine:e}"
    );
}

/// Wall pressure (soft wall forces per wall length) of a dilute gas of
/// `h2` H₂ and `ar` Ar molecules held at `T = 1` by a thermostat.
fn dilute_pressure(h2: usize, ar: usize) -> f64 {
    let half = 20.0;
    let mut mc = h2_world(8, half, h2_dt());
    let (h2_id, ar_id) = (species("H2_2S"), species("Ar"));
    let kinds: Vec<SpeciesId> = std::iter::repeat_n(h2_id, h2)
        .chain(std::iter::repeat_n(ar_id, ar))
        .collect();
    let side = (kinds.len() as f32).sqrt().ceil() as usize;
    let spacing = 2.0 * half / side as f32;
    for (k, &kind) in kinds.iter().enumerate() {
        let (i, j) = (k % side, k / side);
        let pos = Vec2D::new(
            -half + spacing * (i as f32 + 0.5),
            -half + spacing * (j as f32 + 0.5),
        );
        // Speeds of about T = 1 for each species (the thermostat only
        // scales velocities, it cannot start a gas at rest).
        let mass = mc.world().species().species(kind).mass;
        let phase = k as f32 * 2.399;
        let v = Vec2D::new(phase.cos(), phase.sin()).scaled((2.0 / mass).sqrt());
        add(&mut mc, pos, v, 0.0, 0.0, kind);
    }
    mc.collect_neighbours_by_cell(h2_dt());
    mc.add_thermostat(Vec2D::zero());
    let id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(id);
    {
        let thermostat = mc.world_mut().thermostats.find_mut(id).unwrap();
        thermostat.target_temperature = 1.0;
        thermostat.radius = 2.0 * half;
    }
    // Shorter runs (5k + 25k steps) scatter by up to 20 %.
    for _ in 0..10_000 {
        mc.simulation_step().unwrap();
    }
    let perimeter = 8.0 * f64::from(half);
    let steps = 40_000;
    let mut sum = 0.0f64;
    for _ in 0..steps {
        mc.simulation_step().unwrap();
        sum += mc
            .world()
            .cached_force_results
            .iter()
            .map(|r| f64::from(r.wall_force.x.abs() + r.wall_force.y.abs()))
            .sum::<f64>();
    }
    sum / (steps as f64 * perimeter)
}

/// Dalton's law in a dilute H₂/Ar gas: the pressure of the mixture is the
/// sum of the partial pressures (ideal-gas smoke test, #407).
#[test]
#[ignore = "suite:physics (slow)"]
fn h2_and_argon_partial_pressures_add_up() {
    let (p_h2, p_ar, p_mix) = (
        dilute_pressure(40, 0),
        dilute_pressure(0, 40),
        dilute_pressure(40, 40),
    );
    println!(
        "H2/Ar: P_H2 {p_h2:.5}, P_Ar {p_ar:.5}, P_mix {p_mix:.5}, sum {:.5}",
        p_h2 + p_ar
    );
    let ratio = p_mix / (p_h2 + p_ar);
    assert!((ratio - 1.0).abs() < 0.1, "P_mix / (P_H2 + P_Ar) = {ratio}");
}

// ── CO₂ (#408) ──────────────────────────────────────────────────────────

/// A gas of CO₂ chains and Ar in a box at the CO₂ time step: the total
/// energy is conserved (as a share of the kinetic energy) and the error
/// falls with dt.
#[test]
fn co2_gas_conserves_its_energy() {
    let error = |dt: f32| {
        let mut config = SimConfig::default();
        config.integration.time_step = dt;
        let mut mc =
            MainContent::with_config(Rect::new(-12.0, 12.0, 24.0, -24.0), &config, Some(9))
                .unwrap();
        mc.world_mut().speed_limit_override = Some(f32::INFINITY);
        gas_of(&mut mc, "CO2_3S", 2.2, 9, 1.0, 5);
        mc.simulation_step().unwrap();
        let (e0, kin0) = (reported_energy(&mc), mc.world().sim.get_kinetic_energy());
        let mut max = 0.0f32;
        let mut turned = false;
        for _ in 0..(10.0 / dt) as usize {
            mc.simulation_step().unwrap();
            max = max.max((reported_energy(&mc) - e0).abs() / kin0);
            turned |= mc.world().balls.iter().any(|b| b.omega != 0.0);
        }
        assert!(turned, "no torque acted");
        assert_eq!(mc.world().sim.wall_contacts, 0, "backstop reached");
        max
    };
    let (coarse, fine) = (error(CO2_ACCURATE_DT), error(0.5 * CO2_ACCURATE_DT));
    println!("CO2: energy error {coarse:e} at dt, {fine:e} at dt/2");
    assert!(coarse < 3e-3, "energy error {coarse:e}");
    assert!(
        fine < 0.5 * coarse,
        "no convergence: {coarse:e} -> {fine:e}"
    );
}

/// Torques bring CO₂ to equipartition: `⟨E_rot⟩ ≈ ⟨E_trans⟩ / 2`.
#[test]
#[ignore = "suite:physics (slow)"]
fn co2_reaches_equipartition() {
    let mut mc = world(10, 16.0);
    gas_of(&mut mc, "CO2_3S", 2.4, 12, 1.0, 0);
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
    println!("CO2: ⟨E_rot⟩ / (⟨E_trans⟩ / 2) = {ratio:.3}");
    assert!((ratio - 1.0).abs() < 0.1, "equipartition ratio {ratio}");
}

/// End sites of a CO₂ molecule `ball`: their distance from the bottom wall
/// at `floor`.
fn end_site_heights(ball: &Ball2D, floor: f32) -> [f32; 2] {
    let half = SpeciesTable::builtin()
        .species(ball.species())
        .molecule
        .as_ref()
        .unwrap()
        .half_length();
    let lift = half * ball.theta.sin();
    let y = ball.get_position().y - floor;
    [y + lift, y - lift]
}

/// A CO₂ lying along a hard wall and spinning (#408): its end sites hit the
/// wall and are stopped by the rigid-body impulse at the site, which turns
/// the molecule back and moves its centre, and the kinetic energy
/// (translation and rotation) is kept. The old centre-based contact let the
/// sites rotate into the wall.
#[test]
fn spinning_molecule_bounces_off_a_hard_wall() {
    let mut mc = world(5, 10.0);
    mc.use_hard_walls();
    let co2 = species("CO2_3S");
    let floor = mc
        .world()
        .world_region()
        .get_br()
        .1
        .min(mc.world().world_region().get_tl().1);
    let site = mc.world().physics.wall_contact_distance(species("Ar"));
    add(
        &mut mc,
        Vec2D::new(0.0, floor + 0.7),
        Vec2D::zero(),
        0.0,
        3.0,
        co2,
    );
    mc.collect_neighbours_by_cell(co2_dt());
    mc.set_time_step(co2_dt()).unwrap();
    mc.simulation_step().unwrap();
    let e0 = reported_energy(&mc);
    let mut lowest = f32::INFINITY;
    let mut hit_steps = 0;
    for _ in 0..4_000 {
        let before = mc.wall_momentum().1;
        mc.simulation_step().unwrap();
        if mc.wall_momentum().1 != before {
            hit_steps += 1;
        }
        let [a, b] = end_site_heights(&mc.world().balls[0], floor);
        lowest = lowest.min(a).min(b);
        let e = reported_energy(&mc);
        assert!((e - e0).abs() <= 1e-4 * e0, "energy {e} vs {e0}");
    }
    assert!(hit_steps > 0, "no step reported a box contact");
    let (_, py) = mc.wall_momentum();
    assert!(py.abs() > 0.0, "the wall took no momentum: no contact");
    assert!(
        lowest > 0.5 * site,
        "an end site went {lowest} from the wall (contact ≈ {site})"
    );
}

/// A hot CO₂ gas between hard walls (#408): the site impulses on the walls
/// keep the total energy, and the error shrinks with dt. The impulse acts at
/// the end of the drift, not at the exact contact time, so the error is first
/// order in dt. The largest error of one run depends on which single bad
/// contact happens: steps 1 % apart give 3e-3 to 2e-2, with the end-site
/// contact before #424 as with the per-site one. So the test takes the
/// median over five steps near dt (about 7e-3 at dt, 3.6e-3 at dt/2; the
/// centre-based contact before #408 gave 1.2e-1 and 3.9e-2). Soft walls, the
/// default, keep molecules off this contact.
#[test]
fn co2_gas_keeps_its_energy_between_hard_walls() {
    let run = |dt: f32| {
        let mut mc = world(6, 5.0);
        mc.use_hard_walls();
        mc.set_time_step(dt).unwrap();
        gas_of(&mut mc, "CO2_3S", 2.2, 4, 3.0, 0);
        mc.simulation_step().unwrap();
        let (e0, kin0) = (reported_energy(&mc), mc.world().sim.get_kinetic_energy());
        let mut max = 0.0f32;
        for _ in 0..(4.0 / dt) as usize {
            mc.simulation_step().unwrap();
            max = max.max((reported_energy(&mc) - e0).abs() / kin0);
        }
        (max, mc.wall_momentum())
    };
    let median = |scale: f32| {
        let mut runs: Vec<_> = [0.96, 0.98, 1.0, 1.02, 1.04]
            .iter()
            .map(|f| run(f * scale * CO2_ACCURATE_DT))
            .collect();
        runs.sort_by(|a, b| a.0.total_cmp(&b.0));
        runs[2]
    };
    let ((coarse, (px, py)), (fine, _)) = (median(1.0), median(0.5));
    println!("CO2 hard walls: median energy error {coarse:e} at dt, {fine:e} at dt/2");
    assert!(px.abs() + py.abs() > 0.0, "no wall contact");
    assert!(coarse < 1e-2, "energy error {coarse:e}");
    assert!(
        fine < 0.6 * coarse,
        "no convergence: {coarse:e} -> {fine:e}"
    );
}
