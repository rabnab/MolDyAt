// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Species (#234): Lennard-Jones on the centre distance with mixed `σ_ij`,
//! walls at `σ_i / 2`, a cell grid sized for the species present, and
//! `display_radius` as a drawing-only value.

use super::DT_GOAL;
use crate::application::MainContent;
use crate::application::sim_config::{PairOverrideConfig, SimConfig};
use crate::objects2d::ball_impl::ball_physics::PhysicsConstants;
use crate::objects2d::ball_impl::pair_potential::{CapSettings, PairOverride};
use crate::objects2d::ball_impl::{Ball2D, BallPhysics, SpeciesId, SpeciesTable};
use crate::objects2d::{Rect, Vec2D};

fn species(name: &str) -> SpeciesId {
    SpeciesTable::builtin().by_name(name).unwrap()
}

/// A 60 x 60 box with an 8 x 8 grid of alternating `N2` and `M` balls at
/// spacing 3.5, with small deterministic velocities, using `physics`.
fn mixed_world(physics: BallPhysics) -> MainContent {
    let mut mc = MainContent::new_with_seed(Rect::new(-30.0, 30.0, 60.0, -60.0), 9).unwrap();
    mc.world_mut().physics = physics;
    mc.world_mut().speed_limit_override = Some(f32::INFINITY);
    let kinds = [species("N2"), species("M")];
    for i in 0..8 {
        for j in 0..8 {
            let pos = Vec2D::new(-12.25 + 3.5 * i as f32, -12.25 + 3.5 * j as f32);
            let mut ball = Ball2D::new(i * 8 + j, pos, kinds[(i + j) % 2]);
            ball.v = Vec2D::new(((i * 7 + j) as f32).sin(), ((i + 3 * j) as f32).cos());
            mc.world_mut().balls.push(ball);
        }
    }
    mc.world_mut().rebuild_cell_grid();
    mc.collect_neighbours_by_cell(DT_GOAL);
    mc
}

fn run(mc: &mut MainContent, steps: usize) {
    for _ in 0..steps {
        mc.simulation_step().expect("simulation step failed");
    }
}

/// Kinetic plus pair potential energy over all pairs.
fn total_energy(mc: &MainContent) -> f64 {
    let world = mc.world();
    let balls = &world.balls;
    let mut energy = 0.0f64;
    for (i, a) in balls.iter().enumerate() {
        energy += 0.5 * (a.mass * a.v.norm_sq()) as f64;
        for b in &balls[(i + 1)..] {
            let r = a.get_position().dist(b.get_position()).norm();
            energy += world.physics.potential(a.species(), b.species(), r) as f64;
        }
    }
    energy
}

#[test]
fn display_radius_does_not_change_physics() {
    let builtin = SpeciesTable::builtin();
    let enlarged = SpeciesTable::new(
        builtin
            .iter()
            .cloned()
            .map(|mut s| {
                s.display_radius *= 3.0;
                s
            })
            .collect(),
    );
    let physics = |table: &SpeciesTable| {
        BallPhysics::new(PhysicsConstants::default(), table, CapSettings::default())
    };
    let mut drawn_normal = mixed_world(physics(builtin));
    let mut drawn_large = mixed_world(physics(&enlarged));
    run(&mut drawn_normal, 200);
    run(&mut drawn_large, 200);

    let state = |mc: &MainContent| -> Vec<[u32; 4]> {
        mc.world()
            .balls
            .iter()
            .map(|b| {
                let p = b.get_position();
                [p.x, p.y, b.v.x, b.v.y].map(f32::to_bits)
            })
            .collect()
    };
    assert_eq!(state(&drawn_normal), state(&drawn_large));
}

#[test]
fn mixed_species_conserve_energy() {
    let mut mc = mixed_world(BallPhysics::default());
    let start = total_energy(&mc);
    let mut max_rel_drift = 0.0f64;
    for _ in 0..10 {
        run(&mut mc, 50);
        max_rel_drift = max_rel_drift.max((total_energy(&mc) - start).abs() / start.abs());
    }
    println!("start energy {start}, max relative drift {max_rel_drift:.3e}");
    assert!(max_rel_drift < 1e-3, "energy drift {max_rel_drift}");
}

#[test]
fn every_pair_inside_its_cutoff_is_listed() {
    let mut mc = mixed_world(BallPhysics::default());
    run(&mut mc, 300);
    let world = mc.world();
    let balls = &world.balls;
    for i in 0..balls.len() {
        for j in (i + 1)..balls.len() {
            let r = balls[i].get_position().dist(balls[j].get_position()).norm();
            if r < world.physics.cutoff(balls[i].species(), balls[j].species()) {
                assert!(
                    balls[i].hood.is_a_neighbour_idx(j) && balls[j].hood.is_a_neighbour_idx(i),
                    "pair {i}-{j} at {r} is not listed"
                );
            }
        }
    }
}

#[test]
fn cell_grid_follows_the_species_present() {
    let mut mc = MainContent::new_with_seed(Rect::new(-50.0, 50.0, 100.0, -100.0), 3).unwrap();
    let (n2, xl) = (species("N2"), species("XL"));
    let physics = BallPhysics::default();
    mc.add_candidate_to_vec(Ball2D::new(1, Vec2D::new(-6.0, 0.0), n2));
    mc.collect_neighbours_by_cell(DT_GOAL);
    assert_eq!(
        mc.world().cell_grid.list_radius(),
        physics.list_radius(n2, n2)
    );

    // An XL ball needs wider cells; the grid grows before the next rebuild,
    // and the unlike pair 12 apart (more than two N2-sized cells) is listed.
    assert!(12.0 < physics.cutoff(n2, xl));
    assert!(12.0 > 2.0 * physics.list_radius(n2, n2));
    mc.add_candidate_to_vec(Ball2D::new(2, Vec2D::new(6.0, 0.0), xl));
    mc.collect_neighbours_by_cell(DT_GOAL);
    assert_eq!(
        mc.world().cell_grid.list_radius(),
        physics.list_radius(xl, xl)
    );
    assert!(mc.world().balls[0].hood.is_a_neighbour_idx(1));

    // Once the XL ball is gone, the next full rebuild shrinks the cells again.
    mc.world_mut().balls[1].set_position_and_update_tail(Vec2D::new(f32::NAN, 0.0));
    mc.world_mut().remove_invalid_balls();
    assert_eq!(
        mc.world().cell_grid.list_radius(),
        physics.list_radius(n2, n2)
    );
}

/// An override that makes an unlike pair reach further than any like pair
/// of the species present widens the cells for it (#235).
#[test]
fn cell_grid_fits_an_overridden_pair() {
    let mut config = SimConfig::default();
    config.potential.pair_overrides.push(PairOverrideConfig {
        a: "XS".into(),
        b: "N2".into(),
        sigma: Some(3.0),
        epsilon: None,
    });
    let rect = Rect::new(-50.0, 50.0, 100.0, -100.0);
    let mut mc = MainContent::with_config(rect, &config, Some(3)).unwrap();
    let (n2, xs) = (species("N2"), species("XS"));
    let physics = mc.world().physics.clone();
    assert_eq!(physics.cutoff(n2, xs), 4.0 * 3.0);
    assert_eq!(physics.cutoff(xs, n2), 4.0 * 3.0);

    // 11.9 apart: inside the overridden cutoff, but more than two cells of
    // the widest like pair, so only cells sized for the override find it.
    let like = physics.list_radius(n2, n2).max(physics.list_radius(xs, xs));
    assert!(11.9 < physics.cutoff(n2, xs));
    assert!(11.9 > 2.0 * like);
    mc.add_candidate_to_vec(Ball2D::new(1, Vec2D::new(-5.95, 0.0), n2));
    mc.add_candidate_to_vec(Ball2D::new(2, Vec2D::new(5.95, 0.0), xs));
    mc.collect_neighbours_by_cell(DT_GOAL);
    assert_eq!(
        mc.world().cell_grid.list_radius(),
        physics.list_radius(n2, xs)
    );
    assert!(mc.world().balls[0].hood.is_a_neighbour_idx(1));
    assert!(mc.world().balls[1].hood.is_a_neighbour_idx(0));
}

#[test]
fn overridden_pairs_conserve_energy() {
    let (n2, m) = (species("N2"), species("M"));
    let overrides = vec![
        PairOverride {
            a: n2,
            b: m,
            sigma: Some(2.0),
            epsilon: Some(1.5),
        },
        PairOverride {
            a: m,
            b: m,
            sigma: None,
            epsilon: Some(0.8),
        },
    ];
    let physics = BallPhysics::with_pair_overrides(
        PhysicsConstants::default(),
        SpeciesTable::builtin(),
        CapSettings::default(),
        overrides,
    );
    let mut mc = mixed_world(physics);
    let start = total_energy(&mc);
    let mut max_rel_drift = 0.0f64;
    for _ in 0..10 {
        run(&mut mc, 50);
        max_rel_drift = max_rel_drift.max((total_energy(&mc) - start).abs() / start.abs());
    }
    println!("start energy {start}, max relative drift {max_rel_drift:.3e}");
    assert!(max_rel_drift < 1e-3, "energy drift {max_rel_drift}");
}

/// A ball thrown at the box wall is reflected at centre distance `σ / 2`.
#[test]
fn box_walls_reflect_at_half_sigma() {
    for name in ["N2", "M", "XL"] {
        let mut mc = MainContent::new_with_seed(Rect::new(-20.0, 20.0, 40.0, -40.0), 5).unwrap();
        mc.world_mut().speed_limit_override = Some(f32::INFINITY);
        let s = species(name);
        let contact = SpeciesTable::builtin().species(s).sigma / 2.0;
        assert_eq!(mc.world().physics.wall_contact_distance(s), contact);
        let mut ball = Ball2D::new(1, Vec2D::new(-10.0, 0.0), s);
        ball.v = Vec2D::new(-4.0, 0.0);
        mc.add_candidate_to_vec(ball);
        mc.collect_neighbours_by_cell(DT_GOAL);

        let mut closest = f32::INFINITY;
        for _ in 0..600 {
            run(&mut mc, 1);
            closest = closest.min(mc.world().balls[0].get_position().x + 20.0);
        }
        let ball = &mc.world().balls[0];
        assert!(ball.v.x > 0.0, "{name}: not reflected, v {:?}", ball.v);
        assert!(
            (closest - contact).abs() < 0.05,
            "{name}: closest centre distance {closest}, contact {contact}"
        );
    }
}
