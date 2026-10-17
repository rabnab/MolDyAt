// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::DT_GOAL;
use crate::application::MainContent;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::{Rect, Vec2D};

fn make_content() -> MainContent {
    MainContent::new(Rect::new(-20.0, 20.0, 40.0, -40.0)).expect("create MainContent")
}

/// Wall from (-10, 0) to (10, 0): line id 0, chain id 0.
fn make_content_with_wall() -> MainContent {
    let mut mc = make_content();
    mc.add_line_segment(Vec2D::new(-10.0, 0.0), Vec2D::new(10.0, 0.0));
    mc
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-4,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn add_and_toggle_valve_segment() {
    let mut mc = make_content_with_wall();

    // Reversed fractions are normalised.
    mc.add_valve_segment(0, 0.8, 0.2, false);
    assert_eq!(mc.world().valve_segments.valves().len(), 1);
    let v = mc.world().valve_segments.valves()[0];
    assert!(!v.is_closed);

    let (p0, p1) = mc.world().wall_chains.valve_span(v.id).expect("valve span");
    assert_close(p0.x, -6.0);
    assert_close(p1.x, 6.0);

    mc.set_valve_closed(v.id, true);
    assert!(mc.world().valve_segments.valves()[0].is_closed);
}

#[test]
fn open_valve_allows_pass_through() {
    let mut mc = make_content_with_wall();
    // Full-line valve section, open.
    mc.add_valve_segment(0, 0.0, 1.0, false);

    let mut ball = Ball2D::new(1, Vec2D::new(0.0, 0.5), 1.0);
    ball.v = Vec2D::new(0.0, -5.0);
    mc.world_mut().balls.push(ball);

    let _ = mc.resolve_line_segment_collisions();

    // No reflection because the open valve is not an obstacle.
    assert!(mc.world().balls[0].v.y < 0.0);
}

#[test]
fn closed_valve_reflects_like_wall() {
    let mut mc = make_content_with_wall();
    // Full-line valve section, closed.
    mc.add_valve_segment(0, 0.0, 1.0, true);

    let mut ball = Ball2D::new(1, Vec2D::new(0.0, 0.5), 1.0);
    ball.v = Vec2D::new(0.0, -5.0);
    mc.world_mut().balls.push(ball);

    let _ = mc.resolve_line_segment_collisions();

    assert!(mc.world().balls[0].v.y > 0.0);
}

/// Step the balls' positions by `v * dt` and resolve wall collisions,
/// `steps` times (no inter-ball forces, so only the walls act).
fn move_and_collide(mc: &mut MainContent, steps: usize, dt: f32) {
    for _ in 0..steps {
        for ball in &mut mc.world_mut().balls {
            let next = ball.get_position().add_scaled(&ball.v, dt);
            ball.set_position_and_update_tail(next);
        }
        let _ = mc.resolve_line_segment_collisions();
    }
}

#[test]
fn ball_caught_in_a_closing_valve_leaves_without_gaining_energy() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.0, 1.0, false);

    let mut ball = Ball2D::new(1, Vec2D::new(0.0, 0.1), 1.0);
    ball.v = Vec2D::new(0.3, -5.0);
    mc.world_mut().balls.push(ball);

    let id = mc.world().valve_segments.valves()[0].id;
    mc.set_valve_closed(id, true);
    assert_eq!(*mc.world().balls[0].get_position(), Vec2D::new(0.0, 0.1));

    move_and_collide(&mut mc, 30, 0.02);

    let ball = &mc.world().balls[0];
    assert!(
        ball.get_position().y < -(ball.radius + 0.5),
        "ball should have left the valve downwards: {:?}",
        ball.get_position()
    );
    assert_eq!(
        ball.v,
        Vec2D::new(0.3, -5.0),
        "speed and direction unchanged"
    );
}

#[test]
fn ball_trapped_near_a_valve_end_is_not_deflected_by_the_next_piece() {
    let mut mc = make_content_with_wall();
    // Valve over x = -5..5; piece 0 ends at x = -5 with a rounded cap.
    mc.add_valve_segment(0, 0.25, 0.75, false);

    // Centre inside the valve, body overlapping piece 0's end cap.
    let mut ball = Ball2D::new(1, Vec2D::new(-4.8, 0.1), 1.0);
    ball.v = Vec2D::new(0.0, -5.0);
    mc.world_mut().balls.push(ball);
    let id = mc.world().valve_segments.valves()[0].id;
    mc.set_valve_closed(id, true);

    let _ = mc.resolve_line_segment_collisions();

    assert_eq!(mc.world().balls[0].v, Vec2D::new(0.0, -5.0));
    assert_eq!(*mc.world().balls[0].get_position(), Vec2D::new(-4.8, 0.1));
}

#[test]
fn ball_approaching_a_closed_valve_from_outside_still_bounces() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.25, 0.75, true);

    let mut ball = Ball2D::new(1, Vec2D::new(0.0, 3.0), 1.0);
    ball.v = Vec2D::new(0.0, -5.0);
    mc.world_mut().balls.push(ball);

    move_and_collide(&mut mc, 30, 0.02);

    let ball = &mc.world().balls[0];
    assert!(ball.get_position().y > 0.0, "ball must stay above the wall");
    assert!(
        (ball.v.norm() - 5.0).abs() < 1e-4,
        "elastic bounce keeps speed"
    );
}

#[test]
fn clear_points_clears_valves_and_chains() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.25, 0.75, true);
    assert_eq!(mc.world().valve_segments.valves().len(), 1);

    mc.clear_points();
    assert!(mc.world().valve_segments.valves().is_empty());
    assert_eq!(mc.world().wall_chains.chains().count(), 0);
}

#[test]
fn toggle_valve_flips_closed_state() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.2, 0.8, true);

    let id = mc.world().valve_segments.valves()[0].id;
    assert!(mc.world().valve_segments.valves()[0].is_closed);

    mc.toggle_valve(id);
    assert!(!mc.world().valve_segments.valves()[0].is_closed);

    mc.toggle_valve(id);
    assert!(mc.world().valve_segments.valves()[0].is_closed);
}

#[test]
fn snapshot_includes_valve_spans() {
    let mut mc = make_content_with_wall();
    // First valve: chain fractions 0.1..0.6. The remainder of the wall is
    // then piece 1, covering chain fractions 0.6..1.0.
    mc.add_valve_segment(0, 0.1, 0.6, true);
    // Second valve on piece 1: chain fractions 0.7..0.9 = local 0.25..0.75.
    mc.add_valve_segment(1, 0.25, 0.75, false);

    let snap = mc.create_snapshot();
    let spans = &snap.operators.valve_spans;
    assert_eq!(spans.len(), 2);
    assert!(spans[0].is_closed);
    assert!(!spans[1].is_closed);
    assert_close(spans[0].p0.x, -8.0);
    assert_close(spans[0].p1.x, 2.0);
    assert_close(spans[1].p0.x, 4.0);
    assert_close(spans[1].p1.x, 8.0);
}

// ── Valve thickness follows the wall ────────────────────────────────

#[test]
fn closed_valve_collides_at_wall_thickness_changed_after_creation() {
    let mut mc = make_content_with_wall();
    // Valve spans x = -5..5; piece 0 is x = -10..-5.
    mc.add_valve_segment(0, 0.25, 0.75, true);
    // Thicken piece 0 from 1.0 to 4.0 (half-thickness 2.0).
    mc.change_thickness(0, 3);

    // Above the valve centre: outside the old reach (0.5 + radius) and
    // outside the new wall body (2.0), but inside the new reach
    // (2.0 + radius), and far from the pieces' ends.
    let mut ball = Ball2D::new(1, Vec2D::new(0.0, 2.2), 1.0);
    ball.v = Vec2D::new(0.0, -5.0);
    mc.world_mut().balls.push(ball);

    let _ = mc.resolve_line_segment_collisions();

    assert!(
        mc.world().balls[0].v.y > 0.0,
        "closed valve must collide at the wall's new thickness"
    );
}

#[test]
fn valve_thickness_is_thicker_flanking_piece() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.25, 0.75, true);
    let id = mc.world().valve_segments.valves()[0].id;
    assert_close(
        mc.world().valve_thickness(id).expect("valve thickness"),
        1.0,
    );

    // Only the trailing piece (line id 1) gets thicker.
    mc.change_thickness(1, 2);
    assert_close(
        mc.world().valve_thickness(id).expect("valve thickness"),
        3.0,
    );

    let snap = mc.create_snapshot();
    assert_close(snap.operators.valve_spans[0].thickness, 3.0);
}

// ── Wall chain structure ────────────────────────────────────────────

#[test]
fn n_valves_make_n_plus_one_pieces() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.1, 0.6, true);
    mc.add_valve_segment(1, 0.25, 0.75, true);

    let chain = mc.world().wall_chains.find(0).expect("chain");
    assert_eq!(chain.joints.len(), 2);
    assert_eq!(chain.pieces.len(), 3);
    assert_eq!(mc.world().line_segments.segments().len(), 3);
}

#[test]
fn degenerate_valve_is_rejected() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.5, 0.5, true);

    assert!(mc.world().valve_segments.valves().is_empty());
    assert_eq!(mc.world().wall_chains.find(0).unwrap().pieces.len(), 1);
}

#[test]
fn valve_flush_against_one_piece_edge_is_rejected() {
    // Flush against the piece's own start only: would otherwise leave a
    // permanent zero-length leading piece with no merge/prune path.
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.0, 0.5, true);

    assert!(mc.world().valve_segments.valves().is_empty());
    let chain = mc.world().wall_chains.find(0).unwrap();
    assert_eq!(chain.pieces.len(), 1);
    assert_eq!(mc.world().line_segments.segments().len(), 1);

    // Flush against the piece's own end only: same defect, mirrored.
    mc.add_valve_segment(0, 0.5, 1.0, true);

    assert!(mc.world().valve_segments.valves().is_empty());
    let chain = mc.world().wall_chains.find(0).unwrap();
    assert_eq!(chain.pieces.len(), 1);
    assert_eq!(mc.world().line_segments.segments().len(), 1);
}

#[test]
fn valve_flush_against_both_piece_edges_is_still_accepted() {
    // The whole piece becomes the valve: no non-degenerate piece is lost,
    // so unlike the single-edge-flush case there's nothing to strand.
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.0, 1.0, true);

    assert_eq!(mc.world().valve_segments.valves().len(), 1);
}

#[test]
fn moving_chain_endpoint_stretches_valve_proportionally() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.25, 0.75, true); // chain fractions 0.25..0.75

    // Piece 1 is the trailing piece, so it owns the chain's end.
    mc.move_line_segment_endpoint(1, Vec2D::new(30.0, 0.0), false);

    let id = mc.world().valve_segments.valves()[0].id;
    let (p0, p1) = mc.world().wall_chains.valve_span(id).expect("valve span");
    assert_close(p0.x, -10.0 + 40.0 * 0.25);
    assert_close(p1.x, -10.0 + 40.0 * 0.75);

    // The pieces follow the same stretch.
    let segs = mc.world().line_segments.segments();
    assert_close(segs[0].end.x, p0.x);
    assert_close(segs[1].start.x, p1.x);
    assert_close(segs[1].end.x, 30.0);
}

#[test]
fn internal_piece_boundary_is_not_draggable() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.25, 0.75, true);

    // Piece 0's end and piece 1's start sit next to the valve.
    mc.move_line_segment_endpoint(0, Vec2D::new(0.0, 5.0), false);
    mc.move_line_segment_endpoint(1, Vec2D::new(0.0, 5.0), true);

    let chain = mc.world().wall_chains.find(0).unwrap();
    assert_close(chain.start.x, -10.0);
    assert_close(chain.end.x, 10.0);
    let segs = mc.world().line_segments.segments();
    assert_close(segs[0].end.y, 0.0);
    assert_close(segs[1].start.y, 0.0);
}

#[test]
fn snapshot_wall_endpoints_skip_valve_boundaries() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.25, 0.75, true);

    let snap = mc.create_snapshot();
    assert_eq!(snap.operators.line_segments.len(), 2);
    assert_eq!(snap.operators.walls.len(), 1);
    let wall = &snap.operators.walls[0];
    assert_close(wall.start.x, -10.0);
    assert_close(wall.end.x, 10.0);
    assert_eq!(wall.piece_ids, vec![0, 1]);
    let valve_id = snap.operators.valve_spans[0].valve_id;
    assert_eq!(wall.valve_ids, vec![valve_id]);
}

#[test]
fn open_valve_between_pieces_lets_ball_through_but_pieces_still_reflect() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.4, 0.6, false); // open gap at x in [-2, 2]

    // Ball above the gap heading down: passes through.
    let mut through = Ball2D::new(1, Vec2D::new(0.0, 0.5), 1.0);
    through.v = Vec2D::new(0.0, -5.0);
    // Ball above the left piece heading down: reflects.
    let mut blocked = Ball2D::new(2, Vec2D::new(-8.0, 0.5), 1.0);
    blocked.v = Vec2D::new(0.0, -5.0);
    mc.world_mut().balls.push(through);
    mc.world_mut().balls.push(blocked);

    let _ = mc.resolve_line_segment_collisions();

    assert!(mc.world().balls[0].v.y < 0.0);
    assert!(mc.world().balls[1].v.y > 0.0);
}

#[test]
fn rescale_keeps_chain_endpoints_in_sync_with_pieces() {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.25, 0.75, true);

    let old = Rect::new(-20.0, 20.0, 40.0, -40.0);
    let new = Rect::new(-40.0, 40.0, 80.0, -80.0);
    mc.rescale_all_operators(&old, &new);

    let chain = mc.world().wall_chains.find(0).unwrap();
    let segs = mc.world().line_segments.segments();
    assert_close(chain.start.x, segs[0].start.x);
    assert_close(chain.end.x, segs[1].end.x);
    assert_close(chain.start.x, -20.0);
    assert_close(chain.end.x, 20.0);
}

// ── Regression: balls near / through an open valve keep their speed ──

/// Fire a single ball straight down at a wall whose middle fifth is an open
/// valve (gap between x = -2 and x = 2, capsule ends included). Returns the
/// ball's final position and its speed before/after.
fn fire_ball_at_open_valve(start_x: f32) -> (Vec2D, f32, f32) {
    let mut mc = make_content_with_wall();
    mc.add_valve_segment(0, 0.4, 0.6, false);
    mc.world_mut().speed_limit_override = Some(f32::INFINITY);

    let mut ball = Ball2D::new(1, Vec2D::new(start_x, 10.0), 1.0);
    ball.v = Vec2D::new(0.0, -5.0);
    let speed_before = ball.v.norm();
    mc.world_mut().balls.push(ball);
    mc.world_mut().rebuild_cell_grid();
    mc.collect_neighbours_by_cell(DT_GOAL);

    let bound_rect = mc.world().world_region();
    for _ in 0..400 {
        mc.update_sim(DT_GOAL, &bound_rect);
    }

    let ball = &mc.world().balls[0];
    (*ball.get_position(), speed_before, ball.v.norm())
}

#[test]
fn balls_keep_speed_around_open_valve() {
    // Through the middle, hitting the rounded ends of the neighbouring
    // pieces, and hitting a solid piece.
    for start_x in [0.0, 1.2, -1.5, 1.9, -8.0] {
        let (_, before, after) = fire_ball_at_open_valve(start_x);
        assert!(
            (after - before).abs() < 1e-2,
            "start_x={start_x}: speed {before} -> {after}"
        );
    }
}

#[test]
fn ball_passes_open_valve_but_not_solid_piece() {
    let (through, _, _) = fire_ball_at_open_valve(0.0);
    assert!(through.y < 0.0, "ball should have crossed the wall line");

    let (blocked, _, _) = fire_ball_at_open_valve(-8.0);
    assert!(
        blocked.y > 0.0,
        "ball should have bounced off the solid piece"
    );
}
