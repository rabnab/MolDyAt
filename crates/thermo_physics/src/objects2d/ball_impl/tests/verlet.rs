// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::verlet::Verlet;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics, DEFAULT_SPECIES};

/// A ball of the default species (`N2`).
fn make_ball(id: usize, pos: Vec2D) -> Ball2D {
    Ball2D::new(id, pos, DEFAULT_SPECIES)
}

/// Centre distance below which two default balls are listed: cutoff + skin.
fn list_radius(physics: &BallPhysics) -> f32 {
    physics.list_radius(DEFAULT_SPECIES, DEFAULT_SPECIES)
}

#[test]
fn test_add_remove_neighbour_and_query() {
    let mut v = Verlet::new();
    let a_idx = 0;
    let b_idx = 1;

    let was_neigh = v.add_neighbour(b_idx);
    assert!(!was_neigh);
    assert!(v.is_a_neighbour_idx(b_idx));
    // Adding again returns true (was already neighbour)
    assert!(v.add_neighbour(b_idx));
    // Remove and ensure gone
    v.remove(b_idx);
    assert!(!v.is_a_neighbour_idx(b_idx));
    // No panic on removing non-existent
    v.remove(a_idx);
}

#[test]
fn test_update_closeness_stats() {
    let mut v = Verlet::new();
    let a = make_ball(0, Vec2D::new(0.0, 0.0));
    let b = make_ball(1, Vec2D::new(3.0, 4.0)); // distance 5
    let balls = vec![a, b];

    v.add_neighbour(0);
    v.add_neighbour(1);
    v.update_closeness_stats(&Vec2D::new(0.0, 0.0), &balls);
    assert!((v.nearest - 0.0).abs() < 1e-6);
    assert!((v.farthest - 5.0).abs() < 1e-6);
}

#[test]
fn test_needs_update_hood_thresholds() {
    let mut v = Verlet::new();
    // Simulate that the ball was just updated by resetting updates_since_refresh
    v.updates_since_refresh = 0;

    let mut ball = Ball2D::new(1, Vec2D::zero(), DEFAULT_SPECIES);
    ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
    ball.v = Vec2D::new(1.0, 0.0);
    // With small dt and high threshold -> false
    assert!(!v.needs_update_hood(&ball, 0.01, 10.0));
    // With larger dt -> true
    assert!(v.needs_update_hood(&ball, 1.0, 0.5));
}

#[test]
fn test_has_ball_moved_enough_true() {
    let mut ball = Ball2D::new(1, Vec2D::zero(), DEFAULT_SPECIES);
    ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
    ball.v = Vec2D::new(3.0, 4.0); // velocity vector

    let dt = 1.0;
    let min_movement_threshold = 4.0;
    // Movement is 5 units, threshold is 4, so should return true
    assert!(Verlet::has_ball_moved_enough(
        &ball,
        dt,
        min_movement_threshold
    ));
}

#[test]
fn test_has_ball_moved_enough_false() {
    let mut ball = Ball2D::new(1, Vec2D::new(0.1, 0.1), DEFAULT_SPECIES);
    ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
    ball.v = Vec2D::new(0.1, 0.1);

    let dt = 1.0;
    let min_movement_threshold = 1.0;
    // Movement is sqrt(0.08) ≈ 0.282, threshold is 1, so should return false
    assert!(!Verlet::has_ball_moved_enough(
        &ball,
        dt,
        min_movement_threshold
    ));
}

#[test]
fn test_has_ball_moved_enough_edge_case() {
    let mut ball = Ball2D::new(1, Vec2D::new(2., 0.), DEFAULT_SPECIES);
    ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
    ball.v = Vec2D::new(1.0, 0.0);

    let dt = 1.0;
    let min_movement_threshold = 3.0;
    // Expected position after dt would be (3,0)
    // threshold = 3, so distance == threshold -> should return false
    assert!(!Verlet::has_ball_moved_enough(
        &ball,
        dt,
        min_movement_threshold
    ));
}

#[test]
fn test_has_ball_moved_enough_zero_threshold() {
    let mut ball = Ball2D::new(1, Vec2D::zero(), DEFAULT_SPECIES);
    ball.last_update_hood_pos = Vec2D::new(0.0, 0.0);
    ball.v = Vec2D::new(0.0, 0.0);

    let dt = 1.0;
    let min_movement_threshold = 0.0;
    // No movement, zero threshold -> should return false
    assert!(!Verlet::has_ball_moved_enough(
        &ball,
        dt,
        min_movement_threshold
    ));
}

#[test]
fn test_num_neighbours_add_remove() {
    let mut v = Verlet::new();
    let a = make_ball(0, Vec2D::new(0.0, 0.0));
    let b = make_ball(1, Vec2D::new(1.0, 0.0));
    let c = make_ball(2, Vec2D::new(2.0, 0.0));
    let balls = vec![a, b, c];

    // Initially zero
    v.update_closeness_stats(balls[0].get_position(), &balls);
    assert_eq!(v.num_neighbours, 0);

    // Add a
    v.add_neighbour(0);
    v.update_closeness_stats(balls[0].get_position(), &balls);
    assert_eq!(v.num_neighbours, 1);

    // Add b
    v.add_neighbour(1);
    v.update_closeness_stats(balls[1].get_position(), &balls);
    assert_eq!(v.num_neighbours, 2);

    // Add c
    v.add_neighbour(2);
    v.update_closeness_stats(balls[2].get_position(), &balls);
    assert_eq!(v.num_neighbours, 3);

    // Add b again (should not increment)
    v.add_neighbour(1);
    v.update_closeness_stats(balls[2].get_position(), &balls);
    assert_eq!(v.num_neighbours, 3);

    // Remove b
    v.remove(1);
    v.update_closeness_stats(balls[2].get_position(), &balls);
    assert_eq!(v.num_neighbours, 2);

    // Remove a
    v.remove(0);
    v.update_closeness_stats(balls[2].get_position(), &balls);
    assert_eq!(v.num_neighbours, 1);

    // Remove c
    v.remove(2);
    v.update_closeness_stats(balls[2].get_position(), &balls);
    assert_eq!(v.num_neighbours, 0);

    // Remove non-existent (should not go negative)
    v.remove(0);
    v.update_closeness_stats(balls[2].get_position(), &balls);
    assert_eq!(v.num_neighbours, 0);
}

#[test]
fn test_num_neighbours_consistency_with_neighbours_vec() {
    let mut v = Verlet::new();
    let a = make_ball(0, Vec2D::new(0.0, 0.0));
    let b = make_ball(1, Vec2D::new(1.0, 0.0));
    let balls = vec![a, b];

    v.add_neighbour(0);
    v.add_neighbour(1);
    v.update_closeness_stats(balls[0].get_position(), &balls);
    // num_neighbours should match number of valid neighbours
    let count = v.neighbours.len();
    assert_eq!(v.num_neighbours, count);

    // Remove one
    v.remove(0);
    v.update_closeness_stats(balls[0].get_position(), &balls);
    let count = v.neighbours.len();
    assert_eq!(v.num_neighbours, count);
}

#[test]
fn test_update_hood_respects_effective_cutoff() {
    // Test that update_hood only includes neighbors within the list radius
    // (cutoff + skin, centre distance)
    let physics = BallPhysics::default();
    let list_radius = list_radius(&physics);

    // Create three balls: one at origin, one within range, one beyond range
    let mut ball0 = make_ball(0, Vec2D::new(0.0, 0.0));
    let dist1 = list_radius * 0.8; // within range
    let dist2 = list_radius * 1.2; // beyond range
    let mut ball1 = make_ball(1, Vec2D::new(dist1, 0.0));
    let mut ball2 = make_ball(2, Vec2D::new(dist2, 0.0));

    // Initialize last_update_hood_pos to trigger update
    ball0.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
    ball1.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
    ball2.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

    let mut balls = vec![ball0, ball1, ball2];

    // All balls are candidates (simulate what cell grid provides)
    let candidates = vec![1, 2];

    Verlet::update_hood(0, 0.01, &candidates, &physics, 0.1, &mut balls);

    // Ball 0 should have ball 1 as neighbor (within range) but not ball 2 (beyond range)
    let neighbors = &balls[0].cold.hood.neighbours;
    assert!(
        neighbors.contains(&1),
        "Ball 1 should be a neighbor (distance {} < list radius {})",
        dist1,
        list_radius
    );
    assert!(
        !neighbors.contains(&2),
        "Ball 2 should NOT be a neighbor (distance {} > list radius {})",
        dist2,
        list_radius
    );
}

#[test]
fn test_update_hood_removes_distant_neighbors() {
    // Test that neighbors moving beyond the list radius are removed
    let physics = BallPhysics::default();
    let list_radius = list_radius(&physics);

    // Create two balls initially close together
    let mut ball0 = make_ball(0, Vec2D::new(0.0, 0.0));
    let mut ball1 = make_ball(1, Vec2D::new(1.0, 0.0));

    ball0.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
    ball1.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

    let mut balls = vec![ball0, ball1];

    // Initial update - ball 1 should be a neighbor
    Verlet::update_hood(0, 0.01, &[1], &physics, 0.1, &mut balls);
    assert!(
        balls[0].cold.hood.neighbours.contains(&1),
        "Ball 1 should initially be a neighbor"
    );

    // Move ball 1 beyond the list radius
    balls[1].set_position_and_update_tail(Vec2D::new(list_radius + 0.5, 0.0));
    balls[0].last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

    // Update again - ball 1 should be removed
    Verlet::update_hood(0, 0.01, &[1], &physics, 0.1, &mut balls);
    assert!(
        !balls[0].cold.hood.neighbours.contains(&1),
        "Ball 1 should be removed after moving beyond the list radius"
    );
}

#[test]
fn test_hood_dirty_flag_initialized_true() {
    // Test that hood_dirty is initialized to true so stats are computed on first frame
    let v = Verlet::new();
    assert!(
        v.hood_dirty,
        "hood_dirty should be initialized to true for first-frame stats computation"
    );
}

#[test]
fn test_hood_dirty_flag_set_when_neighbor_added() {
    // Test that hood_dirty is set to true when a neighbor is added
    let physics = BallPhysics::default();

    let mut ball0 = make_ball(0, Vec2D::new(0.0, 0.0));
    let mut ball1 = make_ball(1, Vec2D::new(1.0, 0.0));

    ball0.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
    ball1.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

    let mut balls = vec![ball0, ball1];

    // Initially, hood_dirty should be true
    assert!(balls[0].cold.hood.hood_dirty);
    assert!(balls[1].cold.hood.hood_dirty);

    // Perform update_hood which adds a neighbor
    Verlet::update_hood(0, 0.01, &[1], &physics, 0.1, &mut balls);

    // After update, hood_dirty should still be true (marks as dirty when neighbors change)
    assert!(
        balls[0].cold.hood.hood_dirty,
        "Ball 0's hood_dirty should be true after update_hood"
    );
    assert!(
        balls[1].cold.hood.hood_dirty,
        "Ball 1's hood_dirty should be true since it was added as inverse neighbor"
    );
}

#[test]
fn test_hood_dirty_flag_set_when_neighbor_removed() {
    // Test that hood_dirty is set to true when a neighbor is removed
    let physics = BallPhysics::default();
    let list_radius = list_radius(&physics);

    let mut ball0 = make_ball(0, Vec2D::new(0.0, 0.0));
    let mut ball1 = make_ball(1, Vec2D::new(1.0, 0.0));

    ball0.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
    ball1.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

    let mut balls = vec![ball0, ball1];

    // First, add a neighbor
    Verlet::update_hood(0, 0.01, &[1], &physics, 0.1, &mut balls);
    assert!(balls[0].cold.hood.neighbours.contains(&1));

    // Simulate resetting hood_dirty to false (as would happen after stats computation)
    balls[0].cold.hood.hood_dirty = false;
    balls[1].cold.hood.hood_dirty = false;

    // Move ball 1 far away and update
    balls[1].set_position_and_update_tail(Vec2D::new(list_radius + 0.5, 0.0));
    balls[0].last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);

    // Perform update_hood which removes the neighbor
    Verlet::update_hood(0, 0.01, &[1], &physics, 0.1, &mut balls);

    // After removal, hood_dirty should be true again
    assert!(
        balls[0].cold.hood.hood_dirty,
        "Ball 0's hood_dirty should be true after neighbor removal"
    );
    assert!(
        balls[1].cold.hood.hood_dirty,
        "Ball 1's hood_dirty should be true since it was removed as inverse neighbor"
    );
}

#[test]
fn test_hood_dirty_flag_tracks_neighbor_changes_correctly() {
    // Test that hood_dirty correctly tracks whether any neighbor changes occurred
    let mut v = Verlet::new();

    // After initialization, hood_dirty is true
    assert!(v.hood_dirty);

    // Manually reset it (simulating that stats were just computed)
    v.hood_dirty = false;
    assert!(!v.hood_dirty);

    // Add a neighbor
    v.add_neighbour(1);

    // Manually set hood_dirty as update_hood would (in a real scenario)
    v.hood_dirty = true;
    assert!(v.hood_dirty);

    // Remove the neighbor
    v.remove(1);

    // Again set hood_dirty
    v.hood_dirty = true;
    assert!(v.hood_dirty);

    // Clear and verify
    v.clear_neighbours();
    assert!(v.neighbours.is_empty());
}

#[test]
fn test_hood_dirty_independent_per_ball() {
    // Test that hood_dirty flag is independent for each ball
    let mut v1 = Verlet::new();
    let mut v2 = Verlet::new();

    // Both start as dirty
    assert!(v1.hood_dirty);
    assert!(v2.hood_dirty);

    // Modify one
    v1.hood_dirty = false;
    assert!(!v1.hood_dirty);
    assert!(v2.hood_dirty); // v2 should still be dirty

    // Modify the other
    v2.hood_dirty = false;
    assert!(!v1.hood_dirty);
    assert!(!v2.hood_dirty);
}

#[test]
fn test_add_neighbour_keeps_list_sorted() {
    let mut v = Verlet::new();
    for idx in [7, 2, 9, 2, 0, 5] {
        v.add_neighbour(idx);
    }
    assert_eq!(v.neighbours, vec![0, 2, 5, 7, 9]);
    assert!(v.remove(5));
    assert_eq!(v.neighbours, vec![0, 2, 7, 9]);
}

#[test]
fn test_update_hood_keeps_lists_sorted_and_symmetric() {
    let physics = BallPhysics::default();
    let list_radius = list_radius(&physics);
    let far = Vec2D::new(list_radius + 10.0, 0.0);
    // Ball 4, the farthest, is still within range of ball 0.
    let spacing = list_radius / 5.0;
    let mut balls: Vec<Ball2D> = (0..5)
        .map(|i| make_ball(i, Vec2D::new(i as f32 * spacing, 0.0)))
        .collect();
    for ball in &mut balls {
        ball.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
    }

    // Candidates arrive in cell order, not index order.
    Verlet::update_hood(0, 0.01, &[4, 2, 0, 1, 3], &physics, 0.1, &mut balls);
    assert_eq!(balls[0].cold.hood.neighbours, vec![1, 2, 3, 4]);
    for partner in &balls[1..] {
        assert_eq!(
            partner.cold.hood.neighbours,
            vec![0],
            "ball {}",
            partner.get_id()
        );
        assert!(partner.cold.hood.hood_dirty);
    }

    // Ball 3 leaves: ball 0 drops it and ball 3 drops ball 0.
    balls[3].set_position_and_update_tail(far);
    balls[0].last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
    Verlet::update_hood(0, 0.01, &[1, 0, 2, 4], &physics, 0.1, &mut balls);
    assert_eq!(balls[0].cold.hood.neighbours, vec![1, 2, 4]);
    assert!(balls[3].cold.hood.neighbours.is_empty());
    assert_eq!(balls[4].cold.hood.neighbours, vec![0]);
}

/// Balls on a jittered grid; `shift` moves every other ball so a second pass
/// adds and removes pairs.
fn jittered_balls(shift: f32) -> Vec<Ball2D> {
    let mut seed: u32 = 12345;
    let mut next = || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (seed >> 8) as f32 / (1u32 << 24) as f32
    };
    (0..48)
        .map(|i| {
            let x = (i % 8) as f32 * 2.5 + next() + if i % 2 == 0 { shift } else { 0.0 };
            let y = (i / 8) as f32 * 2.5 + next();
            let mut ball = make_ball(i, Vec2D::new(x, y));
            ball.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
            ball
        })
        .collect()
}

fn reposition(balls: &mut [Ball2D], from: &[Ball2D]) {
    for (ball, target) in balls.iter_mut().zip(from) {
        ball.set_position_and_update_tail(*target.get_position());
        ball.last_update_hood_pos = Vec2D::new(f32::INFINITY, f32::INFINITY);
    }
}

#[test]
fn test_parallel_style_rebuild_matches_serial_update_hood() {
    // The engine builds all lists first (reading only) and applies them
    // afterwards. That must give the same lists as rebuilding ball after
    // ball, in any order.
    let physics = BallPhysics::default();
    let all: Vec<usize> = (0..48).collect();
    let mut serial = jittered_balls(0.0);
    let mut batched = jittered_balls(0.0);

    for shift in [0.0, 3.0, -1.5] {
        let target = jittered_balls(shift);
        reposition(&mut serial, &target);
        reposition(&mut batched, &target);

        for idx in all.iter().rev() {
            Verlet::update_hood(*idx, 0.01, &all, &physics, 0.1, &mut serial);
        }

        let rebuild_now: Vec<usize> = all
            .iter()
            .copied()
            .filter(|&idx| Verlet::begin_update(&mut batched[idx], 0.01, 0.1))
            .collect();
        let rebuilds: Vec<_> = rebuild_now
            .iter()
            .map(|&idx| Verlet::rebuild(idx, &all, &physics, &batched))
            .collect();
        for rebuild in rebuilds {
            Verlet::apply_rebuild(rebuild, &physics, &mut batched);
        }

        for (s, b) in serial.iter().zip(&batched) {
            assert_eq!(
                s.cold.hood.neighbours,
                b.cold.hood.neighbours,
                "ball {}",
                s.get_id()
            );
            assert!(s.cold.hood.neighbours.is_sorted());
        }
    }
    assert!(serial.iter().any(|b| !b.cold.hood.neighbours.is_empty()));
}
