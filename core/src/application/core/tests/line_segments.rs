// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::MainContent;
use crate::objects2d::OperatorCollection;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::{Ball2D, SPECIES_M};

/// Helper: create a MainContent with no balls and a default world.
fn make_content() -> MainContent {
    MainContent::new(crate::objects2d::Rect::new(-20.0, 20.0, 40.0, -40.0)).unwrap()
}

#[test]
fn add_and_clear_segments() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
    mc.add_line_segment(Vec2D::new(0.0, 5.0), Vec2D::new(10.0, 5.0));
    assert_eq!(mc.world().line_segments.segments().len(), 2);
    assert_eq!(mc.world().line_segments.segments()[0].id, 0);
    assert_eq!(mc.world().line_segments.segments()[1].id, 1);

    mc.world_mut().line_segments.clear();
    assert!(mc.world().line_segments.segments().is_empty());
}

#[test]
fn move_endpoint_start() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
    mc.move_line_segment_endpoint(0, Vec2D::new(0.5, 0.0), true);
    assert!((mc.world().line_segments.segments()[0].start.x - 0.5).abs() < 1e-6);
    // End unchanged
    assert!((mc.world().line_segments.segments()[0].end.x - 10.0).abs() < 1e-6);
}

#[test]
fn move_endpoint_end() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(0.0, 0.0), Vec2D::new(10.0, 0.0));
    mc.move_line_segment_endpoint(0, Vec2D::new(8.0, 1.0), false);
    // Start unchanged
    assert!((mc.world().line_segments.segments()[0].start.x - 0.0).abs() < 1e-6);
    assert!((mc.world().line_segments.segments()[0].end.x - 8.0).abs() < 1e-6);
    assert!((mc.world().line_segments.segments()[0].end.y - 1.0).abs() < 1e-6);
}

#[test]
fn collision_reflects_ball() {
    let mut mc = make_content();
    // Horizontal segment at y=0 from x=-10 to x=10
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));

    // Ball at y=0.5, radius=1.0, moving downward
    let mut ball = Ball2D::new(1, Vec2D::new(0.0, 0.5), SPECIES_M);
    ball.v = Vec2D::new(0.0, -5.0);
    mc.world_mut().balls.push(ball);

    let (_px, py) = mc.resolve_line_segment_collisions();

    // Ball should now be moving upward
    assert!(mc.world().balls[0].v.y > 0.0, "Ball should reflect upward");
    // Speed should be preserved (elastic)
    assert!((mc.world().balls[0].v.norm() - 5.0).abs() < 0.1);
    // Some momentum should be transferred
    assert!(py.abs() > 0.0);
}

/// Wall momentum (as bits) of one overlap pass over many balls inside a
/// wall, on a dedicated Rayon pool of `threads` workers.
fn overlap_pass_momentum_bits(threads: usize) -> [u32; 2] {
    let run = || {
        let mut mc = make_content();
        mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
        for k in 0..400 {
            let x = -9.0 + 18.0 * (k as f32 / 399.0);
            let mut ball = Ball2D::new(k + 1, Vec2D::new(x, 0.3), SPECIES_M);
            // Different speeds and masses, all moving into the wall.
            ball.v = Vec2D::new((k as f32).sin(), -1.0 - (k as f32 * 0.37).cos().abs() * 7.0);
            ball.mass = 1.0 + (k % 7) as f32 * 0.31;
            mc.world_mut().balls.push(ball);
        }
        let (px, py) = mc.resolve_line_segment_collisions();
        [px.to_bits(), py.to_bits()]
    };
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("thread pool")
        .install(run)
}

#[test]
fn test_overlap_pass_wall_momentum_does_not_depend_on_thread_count() {
    let one = overlap_pass_momentum_bits(1);
    assert_ne!(f32::from_bits(one[1]), 0.0, "no momentum was transferred");
    for threads in [2, 4, 7] {
        assert_eq!(
            one,
            overlap_pass_momentum_bits(threads),
            "wall momentum bits differ between 1 and {threads} threads"
        );
    }
}

#[test]
fn no_collision_when_moving_away() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));

    // Ball overlapping but moving away
    let mut ball = Ball2D::new(1, Vec2D::new(0.0, 0.5), SPECIES_M);
    ball.v = Vec2D::new(0.0, 5.0); // moving away from segment
    mc.world_mut().balls.push(ball);

    let _ = mc.resolve_line_segment_collisions();

    // Should NOT reflect (velocity gate)
    assert!(mc.world().balls[0].v.y > 0.0);
}

// ── Rescale tests ───────────────────────────────────────────────

#[test]
fn rescale_preserves_relative_position() {
    let mut mc = make_content();
    // World is (-20, 20, 40, -40) → x: -20..20, y: 20..-20
    // Segment at (0, 0) → (10, -10) means (50%, 50%) → (75%, 75%)
    mc.add_line_segment(Vec2D::new(0.0, 0.0), Vec2D::new(10.0, -10.0));

    let old_rect = crate::objects2d::Rect::new(-20.0, 20.0, 40.0, -40.0);
    // Double horizontal size: x: -40..40
    let new_rect = crate::objects2d::Rect::new(-40.0, 20.0, 80.0, -40.0);
    mc.world_mut().line_segments.rescale(&old_rect, &new_rect);

    // 50% of new width 80 from -40 → 0.0 → new x = -40 + 0.5*80 = 0.0 (unchanged)
    assert!((mc.world().line_segments.segments()[0].start.x - 0.0).abs() < 1e-5);
    assert!((mc.world().line_segments.segments()[0].start.y - 0.0).abs() < 1e-5);
    // 75% of new width 80 from -40 → new x = -40 + 0.75*80 = 20.0
    assert!((mc.world().line_segments.segments()[0].end.x - 20.0).abs() < 1e-5);
    // y unchanged (same height)
    assert!((mc.world().line_segments.segments()[0].end.y - (-10.0)).abs() < 1e-5);
}

#[test]
fn rescale_both_axes() {
    let mut mc = make_content();
    // Midpoint of rect: (50%, 50%) → (0, 0) in (-20,20,40,-40)
    mc.add_line_segment(Vec2D::new(0.0, 0.0), Vec2D::new(0.0, 0.0));

    let old_rect = crate::objects2d::Rect::new(-20.0, 20.0, 40.0, -40.0);
    // New rect: (-10, 10, 20, -20) — half size in both axes
    let new_rect = crate::objects2d::Rect::new(-10.0, 10.0, 20.0, -20.0);
    mc.world_mut().line_segments.rescale(&old_rect, &new_rect);

    // 50% of new rect → midpoint (0, 0)
    assert!((mc.world().line_segments.segments()[0].start.x - 0.0).abs() < 1e-5);
    assert!((mc.world().line_segments.segments()[0].start.y - 0.0).abs() < 1e-5);
}

#[test]
fn rescale_corner_endpoints() {
    let mut mc = make_content();
    // Top-left corner of world (-20, 20) → (0%, 0%)
    mc.add_line_segment(Vec2D::new(-20.0, 20.0), Vec2D::new(20.0, -20.0));

    let old_rect = crate::objects2d::Rect::new(-20.0, 20.0, 40.0, -40.0);
    let new_rect = crate::objects2d::Rect::new(-40.0, 40.0, 80.0, -80.0);
    mc.world_mut().line_segments.rescale(&old_rect, &new_rect);

    // 0% → top-left of new rect
    assert!((mc.world().line_segments.segments()[0].start.x - (-40.0)).abs() < 1e-5);
    assert!((mc.world().line_segments.segments()[0].start.y - 40.0).abs() < 1e-5);
    // 100% → bottom-right of new rect
    assert!((mc.world().line_segments.segments()[0].end.x - 40.0).abs() < 1e-5);
    assert!((mc.world().line_segments.segments()[0].end.y - (-40.0)).abs() < 1e-5);
}

#[test]
fn rescale_skips_degenerate_old_rect() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(5.0, 5.0), Vec2D::new(10.0, 10.0));

    let old_rect = crate::objects2d::Rect::new(0.0, 0.0, 0.0, 0.0);
    let new_rect = crate::objects2d::Rect::new(-10.0, 10.0, 20.0, -20.0);
    mc.world_mut().line_segments.rescale(&old_rect, &new_rect);

    // Endpoints unchanged (degenerate old rect)
    assert!((mc.world().line_segments.segments()[0].start.x - 5.0).abs() < 1e-5);
    assert!((mc.world().line_segments.segments()[0].end.x - 10.0).abs() < 1e-5);
}

#[test]
fn rescale_multiple_segments() {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-20.0, 0.0), Vec2D::new(0.0, 0.0));
    mc.add_line_segment(Vec2D::new(0.0, 0.0), Vec2D::new(20.0, 0.0));

    let old_rect = crate::objects2d::Rect::new(-20.0, 20.0, 40.0, -40.0);
    let new_rect = crate::objects2d::Rect::new(-40.0, 20.0, 80.0, -40.0);
    mc.world_mut().line_segments.rescale(&old_rect, &new_rect);

    // First segment: 0% → 50%  mapped to new width
    assert!((mc.world().line_segments.segments()[0].start.x - (-40.0)).abs() < 1e-5);
    assert!((mc.world().line_segments.segments()[0].end.x - 0.0).abs() < 1e-5);
    // Second segment: 50% → 100%
    assert!((mc.world().line_segments.segments()[1].start.x - 0.0).abs() < 1e-5);
    assert!((mc.world().line_segments.segments()[1].end.x - 40.0).abs() < 1e-5);
}
