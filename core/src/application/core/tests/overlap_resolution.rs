//! Overlapping balls in a crowded box must be pushed apart (issue #240).

use super::{DT_GOAL, create_main_content_for_region};
use crate::application::{GameLogic, MainContent};
use crate::objects2d::ball_impl::BallFactory;
use crate::objects2d::{Rect, Vec2D};

const SIZE_IDX: usize = 2;

/// (overlapping pairs, deep pairs with centre distance below one radius,
/// smallest centre distance)
fn overlap_stats(main_content: &MainContent) -> (usize, usize, f32) {
    let balls = &main_content.world().balls;
    let mut overlapping = 0;
    let mut deep = 0;
    let mut min_centre_dist = f32::INFINITY;
    for i in 0..balls.len() {
        for j in (i + 1)..balls.len() {
            let centre_dist = balls[i].get_position().dist(balls[j].get_position()).norm();
            min_centre_dist = min_centre_dist.min(centre_dist);
            if centre_dist < balls[i].radius + balls[j].radius {
                overlapping += 1;
            }
            if centre_dist < balls[i].radius.min(balls[j].radius) {
                deep += 1;
            }
        }
    }
    (overlapping, deep, min_centre_dist)
}

fn assert_all_finite(main_content: &MainContent) {
    for ball in &main_content.world().balls {
        let pos = ball.get_position();
        assert!(
            pos.x.is_finite() && pos.y.is_finite() && ball.v.x.is_finite() && ball.v.y.is_finite(),
            "ball {} has pos {pos:?} v {:?}",
            ball.get_id(),
            ball.v
        );
    }
}

fn run_steps(main_content: &mut MainContent, steps: usize) {
    for _ in 0..steps {
        main_content
            .simulation_step()
            .expect("simulation step failed");
    }
}

/// `cols` x `rows` balls on a grid with the given spacing, starting one
/// spacing inside the top-left corner, with small deterministic velocities.
fn place_grid(main_content: &mut MainContent, rect: &Rect, cols: usize, rows: usize, spacing: f32) {
    let factory = BallFactory::new();
    for i in 0..cols {
        for j in 0..rows {
            let pos = Vec2D::new(
                rect.x + spacing * (i as f32 + 0.6),
                rect.y - spacing * (j as f32 + 0.6),
            );
            let v = Vec2D::new(((i * 7 + j) as f32).sin(), ((i + 3 * j) as f32).cos());
            let ball = factory.create_with_size_and_velocity(pos, SIZE_IDX, v);
            main_content.world_mut().balls.push(ball);
        }
    }
}

#[test]
fn test_deep_overlaps_in_a_crowded_box_separate() {
    // A box packed more densely than the LJ spacing allows, with nearly
    // coincident pairs placed directly (no resize). With a flat force cap the
    // neighbours pinned such pairs in place.
    let (mut main_content, transform) = create_main_content_for_region(-10., 10.);
    let full = transform.world_bounds();
    let rect = Rect::new(full.x, full.y, 36.6, -36.6);
    main_content.resize(rect);
    main_content.unpause_after_resize();

    place_grid(&mut main_content, &rect, 22, 22, 1.65);
    // Nearly coincident partners for a few grid balls
    let factory = BallFactory::new();
    for k in [3usize, 50, 101, 222, 300, 417] {
        let pos = *main_content.world().balls[k].get_position() + Vec2D::new(0.02, 0.01);
        let ball = factory.create_with_size_and_velocity(pos, SIZE_IDX, Vec2D::zero());
        main_content.world_mut().balls.push(ball);
    }
    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);

    let (_, deep_start, _) = overlap_stats(&main_content);
    assert!(
        deep_start >= 6,
        "setup must start with deep overlaps, got {deep_start}"
    );

    for block in 1..=10 {
        run_steps(&mut main_content, 100);
        println!("step {} {:?}", block * 100, overlap_stats(&main_content));
    }
    assert_all_finite(&main_content);
    let (overlapping, deep, min_centre_dist) = overlap_stats(&main_content);
    println!("final: overlapping {overlapping}, deep {deep}, min centre dist {min_centre_dist}");
    assert_eq!(
        deep, 0,
        "deep overlaps remain, min centre dist {min_centre_dist}"
    );
}

#[test]
fn test_overlaps_after_shrinking_the_box_separate() {
    // The reported trigger: a gas squeezed into a box too small for it.
    let (mut main_content, transform) = create_main_content_for_region(-10., 10.);
    let full = transform.world_bounds();
    place_grid(&mut main_content, &full, 23, 22, 4.0);
    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);
    run_steps(&mut main_content, 50);

    let shrunk = Rect::new(full.x, full.y, 36.6, -36.6);
    main_content.resize(shrunk);
    main_content.unpause_after_resize();

    for block in 1..=10 {
        run_steps(&mut main_content, 100);
        println!("step {} {:?}", block * 100, overlap_stats(&main_content));
    }
    assert_all_finite(&main_content);
    let (overlapping, deep, min_centre_dist) = overlap_stats(&main_content);
    println!("final: overlapping {overlapping}, deep {deep}, min centre dist {min_centre_dist}");
    assert_eq!(
        deep, 0,
        "deep overlaps remain, min centre dist {min_centre_dist}"
    );
}

fn total_energy(main_content: &MainContent) -> f32 {
    let world = main_content.world();
    let balls = &world.balls;
    let mut energy = 0.0f32;
    for (i, ball) in balls.iter().enumerate() {
        energy += 0.5 * ball.mass * ball.v.norm_sq();
        for other in &balls[(i + 1)..] {
            let centre_dist = ball.get_position().dist(other.get_position()).norm();
            energy += world
                .physics
                .potential(centre_dist - ball.radius - other.radius);
        }
    }
    energy
}

#[test]
fn test_separating_overlaps_conserves_energy() {
    // The sloped core is conservative: the energy released by separating
    // overlaps goes into kinetic energy, nothing is lost or gained. The
    // speed limit (a deliberate brake) is switched off.
    let (mut main_content, transform) = create_main_content_for_region(-10., 10.);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    let full = transform.world_bounds();
    let rect = Rect::new(full.x, full.y, 36.6, -36.6);
    main_content.resize(rect);
    main_content.unpause_after_resize();

    place_grid(&mut main_content, &rect, 22, 22, 1.65);
    let factory = BallFactory::new();
    for k in [3usize, 50, 101, 222, 300, 417] {
        let pos = *main_content.world().balls[k].get_position() + Vec2D::new(0.3, 0.1);
        let ball = factory.create_with_size_and_velocity(pos, SIZE_IDX, Vec2D::zero());
        main_content.world_mut().balls.push(ball);
    }
    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);

    let energy_start = total_energy(&main_content);
    let mut max_rel_drift = 0.0f32;
    for block in 1..=10 {
        run_steps(&mut main_content, 50);
        let drift = (total_energy(&main_content) - energy_start).abs() / energy_start.abs();
        println!("step {} energy drift {drift:.5}", block * 50);
        max_rel_drift = max_rel_drift.max(drift);
    }
    assert_all_finite(&main_content);
    println!("start energy {energy_start}, max relative drift {max_rel_drift}");
    assert!(max_rel_drift < 0.005, "energy drift {max_rel_drift}");
}
