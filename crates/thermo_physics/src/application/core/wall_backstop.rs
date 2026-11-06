// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The position update behind soft walls (#424): a free drift plus the hard
//! backstop, a safety net, not physics. Soft walls act only through their
//! force (`wall_forces`); a ball reaches the backstop only if that force
//! failed to hold it (an energy far above `k_B·T`, a step too long for the
//! wall's stiffness, a thin wall crossed in one step). Every hit is counted
//! (`SafetyNets::backstop_hits`, #422), and the walls take its momentum,
//! except the ghost wall.
//!
//! The backstop sits at `backstop · σ` from the wall surface
//! (`SoftWalls::backstop_points`), deep in the wall's steep core. Placing a
//! ball there at the end of a step (pushing or mirroring it) would change
//! its energy by `m·a²·dt²` with that core's large `a`, so:
//! - An atom is reflected at the exact contact time on its path
//!   (`ball_impl::wall_contact`, constant acceleration): the box sides in
//!   [`Ball2D::execute_update_pos`], the line walls by the contact sweep
//!   (`sweep_wall_contact`, which also catches a wall crossed in one step).
//! - A molecule's sites turn with it, so each site is checked after the
//!   free drift: one closer to a surface than its backstop distance that
//!   moves into it gets the rigid-body impulse at the site
//!   (`Ball2D::wall_impulse`) and the overshoot is mirrored, an energy error
//!   first order in dt. A site whose step crossed a line wall's centre line
//!   (where the force pushes it on through) is first mirrored back across
//!   it.
//!
//! A ball whose line walls are still ramping in (`Ball2D::wall_ramp < 1`)
//! meets only the box: it may still be inside a wall that appeared on it.

use super::SimulationWorld;
use super::line_segments::Obstacle;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::molecule::axis;
use crate::objects2d::ball_impl::wall_law::SoftWalls;
use crate::objects2d::{Rect, Vec2D};

/// The position update of `ball` behind soft walls: its drift and the
/// backstop's contacts and impulses. Distance checks against line walls
/// are added to `checks`.
pub(crate) fn soft_position_update(
    walls: &SoftWalls,
    ball: &mut Ball2D,
    (bound, obstacles): (&Rect, &[Obstacle]),
    dt: f32,
    restitution: f32,
    checks: &mut u64,
) -> BounceInfo {
    let line_walls = ball.wall_ramp >= 1.0;
    let start = *ball.get_position();
    if walls.sites(ball.species()).is_none() {
        let [(_, distance)] = walls.backstop_points(ball.species()) else {
            unreachable!("an atom has one backstop point");
        };
        let mut info = ball.execute_update_pos(dt, bound, [*distance; 2], restitution);
        if ball.inertia > 0.0 {
            ball.advance_orientation(dt);
        }
        // Contact normals in one step must stay orthogonal (`BounceInfo`).
        if line_walls && !info.touched_box() {
            info.line_wall = SimulationWorld::sweep_wall_contact(
                obstacles,
                ball,
                *distance,
                start,
                dt,
                restitution,
                checks,
            );
        }
        return info;
    }
    let start_theta = ball.theta;
    ball.drift(dt);
    let obstacles = if line_walls { obstacles } else { &[] };
    site_backstop(
        walls,
        ball,
        (start, start_theta),
        (bound, obstacles),
        restitution,
        checks,
    )
}

/// The backstop for the sites of a molecule after its drift. `start` and
/// `start_theta` are its centre and angle before the drift.
fn site_backstop(
    walls: &SoftWalls,
    ball: &mut Ball2D,
    (start, start_theta): (Vec2D, f32),
    (bound, obstacles): (&Rect, &[Obstacle]),
    restitution: f32,
    checks: &mut u64,
) -> BounceInfo {
    let mut info = BounceInfo::default();
    let (axis_now, axis_start) = (axis(ball.theta), axis(start_theta));
    for &(offset, distance) in walls.backstop_points(ball.species()) {
        let lever = axis_now.scaled(offset);
        for side in 0..4 {
            let (z, normal) = box_side(*ball.get_position() + lever, bound, side);
            if z < distance {
                let overlap = distance - z;
                hit(ball, (normal, overlap, lever), restitution, &mut info, true);
            }
        }
        let from = start + axis_start.scaled(offset);
        for obstacle in obstacles {
            let point = *ball.get_position() + lever;
            if let Some((z, normal, mirror)) =
                obstacle.backstop_distance(&point, &from, distance, checks)
                && z < distance
            {
                if mirror != Vec2D::zero() {
                    ball.shift(mirror);
                }
                let overlap = distance - z;
                hit(
                    ball,
                    (normal, overlap, lever),
                    restitution,
                    &mut info,
                    !obstacle.is_ghost(),
                );
            }
        }
    }
    info
}

/// Signed distance of `pos` from box side `side` (left, right, bottom, top)
/// and the side's inward normal.
fn box_side(pos: Vec2D, bound: &Rect, side: usize) -> (f32, Vec2D) {
    let (x0, y0) = bound.get_tl();
    let (x1, y1) = bound.get_br();
    match side {
        0 => (pos.x - x0.min(x1), Vec2D::new(1.0, 0.0)),
        1 => (x0.max(x1) - pos.x, Vec2D::new(-1.0, 0.0)),
        2 => (pos.y - y0.min(y1), Vec2D::new(0.0, 1.0)),
        _ => (y0.max(y1) - pos.y, Vec2D::new(0.0, -1.0)),
    }
}

/// The impulse at the site `lever` from the centre, `overlap` inside the
/// backstop of a surface with normal `normal`, if it moves into it; the
/// overshoot is mirrored. `counts`: whether the wall's momentum counts (not
/// for the ghost wall).
fn hit(
    ball: &mut Ball2D,
    (normal, overlap, lever): (Vec2D, f32, Vec2D),
    restitution: f32,
    info: &mut BounceInfo,
    counts: bool,
) {
    let Some(impulse) = ball.wall_impulse(normal, lever, restitution) else {
        return;
    };
    ball.shift(normal.scaled((1.0 + restitution) * overlap));
    info.impulses += 1;
    if counts {
        info.wall_momentum = info.wall_momentum.add_scaled(&normal, -impulse);
    }
}
