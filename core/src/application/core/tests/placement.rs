// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `PlacementIndex` gives the same collision answer as the full scan (#245).

use super::create_main_content_for_region;
use crate::application::core::SimulationWorld;
use crate::application::core::placement::PlacementIndex;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::{Ball2D, BallFactory, NUM_BALL_SIZES};

/// Small deterministic generator, so the test needs no RNG plumbing.
struct XorShift(u64);

impl XorShift {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }

    fn size_idx(&mut self) -> usize {
        ((self.next_f32() * NUM_BALL_SIZES as f32) as usize).min(NUM_BALL_SIZES - 1)
    }
}

fn ball(factory: &BallFactory, x: f32, y: f32, size_idx: usize) -> Ball2D {
    factory.create_with_size_and_velocity(Vec2D::new(x, y), size_idx, Vec2D::zero())
}

/// Check `candidate` both ways and return the shared answer.
fn same_answer(world: &SimulationWorld, candidate: &Ball2D, margin: f32) -> bool {
    let index = PlacementIndex::new(world);
    let indexed = world.is_ball_colliding_indexed(candidate, margin, &index);
    let full = world.is_ball_colliding(candidate, margin);
    assert_eq!(
        indexed,
        full,
        "candidate at {:?} (r {}), margin {margin}",
        candidate.get_position(),
        candidate.get_radius()
    );
    full
}

#[test]
fn test_indexed_collision_check_matches_full_scan() {
    // World x -50..50, y 50..-50, mixed radii, overlaps allowed.
    let (mut main_content, _) = create_main_content_for_region(-50.0, 50.0);
    let factory = BallFactory::with_seed(7);
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let world = main_content.world_mut();
    for _ in 0..600 {
        let b = ball(
            &factory,
            rng.range(-49.0, 49.0),
            rng.range(-49.0, 49.0),
            rng.size_idx(),
        );
        world.add_ball_and_register(b);
    }

    let index = PlacementIndex::new(world);
    let equilibrium = world.physics.equilibrium_edge_distance();
    let (cell_x, _) = world.cell_grid.get_cell_size();
    // Small, the seeding margin, near a cell, and one past it (full scan).
    let margins = [0.0, equilibrium, 0.9 * cell_x, 2.0 * cell_x];
    let (mut hits, mut misses) = (0, 0);
    for _ in 0..3000 {
        // Some candidates lie outside the world (and the grid).
        let candidate = ball(
            &factory,
            rng.range(-55.0, 55.0),
            rng.range(-55.0, 55.0),
            rng.size_idx(),
        );
        for margin in margins {
            let indexed = world.is_ball_colliding_indexed(&candidate, margin, &index);
            let full = world.is_ball_colliding(&candidate, margin);
            assert_eq!(
                indexed,
                full,
                "candidate at {:?} (r {}), margin {margin}",
                candidate.get_position(),
                candidate.get_radius()
            );
            if full { hits += 1 } else { misses += 1 }
        }
    }
    // Both answers occur, so the comparison means something.
    assert!(hits > 100 && misses > 100, "hits {hits}, misses {misses}");
}

#[test]
fn test_indexed_collision_check_finds_balls_missing_from_the_grid() {
    let (mut main_content, _) = create_main_content_for_region(-50.0, 50.0);
    let factory = BallFactory::with_seed(1);
    let world = main_content.world_mut();

    // In `balls` but never registered in a cell.
    world.balls.push(ball(&factory, 0.0, 0.0, 2));
    let candidate = ball(&factory, 0.1, 0.0, 2);
    assert!(same_answer(world, &candidate, 0.0));
}

#[test]
fn test_indexed_collision_check_finds_balls_with_a_stale_cell() {
    let (mut main_content, _) = create_main_content_for_region(-50.0, 50.0);
    let factory = BallFactory::with_seed(1);
    let world = main_content.world_mut();

    // Registered at the origin, then moved far away without a grid update.
    world.add_ball_and_register(ball(&factory, 0.0, 0.0, 2));
    world.balls[0].set_position_and_update_tail(Vec2D::new(30.0, -30.0));
    assert!(same_answer(world, &ball(&factory, 30.1, -30.0, 2), 0.0));
    assert!(!same_answer(world, &ball(&factory, 0.1, 0.0, 2), 0.0));
}

#[test]
fn test_indexed_placement_rejects_a_ball_placed_earlier_in_the_batch() {
    let (mut main_content, _) = create_main_content_for_region(-50.0, 50.0);
    let factory = BallFactory::with_seed(1);
    let world = main_content.world_mut();
    let mut index = PlacementIndex::new(world);

    let first = world.add_candidate_ball_indexed(ball(&factory, 5.0, 5.0, 2), 0.0, &[], &mut index);
    assert!(first.is_ok());
    let second =
        world.add_candidate_ball_indexed(ball(&factory, 5.2, 5.0, 2), 0.0, &[], &mut index);
    assert!(second.is_err(), "overlaps the ball placed just before");
    assert_eq!(world.balls.len(), 1);
}
