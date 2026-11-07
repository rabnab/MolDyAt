// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Soft walls: the force and energy of every ball in the box walls
//! and the line walls, added to its pair forces in the force pass, and the
//! backstop behind them in the position update.
//!
//! - Box walls: each of the four sides acts on its own, through the signed
//!   distance of the centre from it, so the force keeps pointing inward after
//!   an overshoot; in a corner two sides add up.
//! - Line walls (pieces, closed valves, the ghost wall): of each chain only
//!   the nearest obstacle acts (nearest-feature rule); the chains add up, so
//!   a ball between two separate walls feels both. The surfaces of a chain's
//!   pieces overlap at its joints; the minimum of their distances is the
//!   distance from the union, so the energy has no bump or well at a joint.
//!   (The two ends of a gap in one chain count as one wall too.)
//! - A ball a wall was drawn or closed on top of feels the line walls
//!   scaled by its ramp (`BallCold::wall_ramp`): the force pushes it out gently instead of
//!   kicking it out of the wall's core. The ramp does work on the ball, as
//!   any wall moved onto a ball must.
//!
//! The obstacles are checked against their bounding boxes first, as in the
//! contact sweep, so the potential is evaluated only for nearby balls.

use super::line_segments::Obstacle;
use crate::objects2d::ball_impl::ball_core::BounceInfo;
use crate::objects2d::ball_impl::molecule::{Site, axis, cross, turn};
use crate::objects2d::ball_impl::pair_potential::PairPotential;
use crate::objects2d::ball_impl::wall_law::SoftWalls;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics, ForceResult};
use crate::objects2d::{Rect, Vec2D};

/// Soft wall force and energy of one ball.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WallForce {
    /// Total force of the walls on the ball.
    pub force: Vec2D,
    /// The part of `force` that books wall momentum (all but the ghost wall).
    pub counted: Vec2D,
    pub potential: f32,
    /// Torque of the wall forces on the sites of a molecule (#406).
    pub torque: f32,
}

impl WallForce {
    /// A force `normal · force` at `lever` from the ball's centre.
    fn add(&mut self, normal: Vec2D, (force, potential): (f32, f32), counts: bool, lever: Vec2D) {
        let f = normal.scaled(force);
        self.force += f;
        if counts {
            self.counted += f;
        }
        self.potential += potential;
        self.torque += cross(lever, f);
    }

    /// Add this to the ball's force result.
    pub(crate) fn add_to(&self, result: &mut ForceResult) {
        result.force += self.force;
        result.wall_force = self.counted;
        result.wall_potential = self.potential;
        result.torque += self.torque;
    }
}

/// The soft wall force on `ball` from the box `bound` and `obstacles`, at
/// each of its sites; off the centre it adds a torque.
pub(crate) fn soft_wall_force(
    walls: &SoftWalls,
    ball: &Ball2D,
    bound: &Rect,
    obstacles: &[Obstacle],
) -> WallForce {
    let centre = *ball.get_position();
    let ramp = if ball.leaving_wall {
        ball.cold.wall_ramp
    } else {
        1.0
    };
    let obstacles = if ramp > 0.0 { obstacles } else { &[] };
    let mut out = WallForce {
        force: Vec2D::zero(),
        counted: Vec2D::zero(),
        potential: 0.0,
        torque: 0.0,
    };
    let sites = walls.sites(ball.species());
    // Sites at the centre do not turn: atoms skip the `sin_cos`.
    let axis = if sites.iter().all(|(offset, _)| *offset == Vec2D::zero()) {
        Vec2D::new(1.0, 0.0)
    } else {
        axis(ball.theta)
    };
    for (offset, potential) in sites {
        let lever = turn(*offset, axis);
        point_force(
            centre + lever,
            lever,
            potential.cutoff(),
            &|z| (potential.force(z), potential.potential(z)),
            bound,
            (obstacles, ramp),
            &mut out,
        );
    }
    out
}

/// The wall force on a point at `pos` (`lever` from the ball's centre) with
/// the wall law `law` (force, potential at wall distance `z`), cut off at
/// `cutoff`, added to `out`. The line walls (`obstacles`) act scaled by
/// `ramp` (1 except while they ramp in for this ball).
fn point_force(
    pos: Vec2D,
    lever: Vec2D,
    cutoff: f32,
    law: &impl Fn(f32) -> (f32, f32),
    bound: &Rect,
    (obstacles, ramp): (&[Obstacle], f32),
    out: &mut WallForce,
) {
    let line_law = |z: f32| {
        let (force, potential) = law(z);
        (ramp * force, ramp * potential)
    };
    for (z, normal) in box_sides(pos, bound) {
        if z < cutoff {
            out.add(normal, law(z), true, lever);
        }
    }

    // Obstacles come chain by chain: keep the nearest of the current chain
    // and add it when the next chain starts.
    let mut nearest: Option<(u32, f32, Vec2D, bool)> = None;
    for obstacle in obstacles {
        let Some((z, normal)) = obstacle.surface_distance(&pos, cutoff) else {
            continue;
        };
        let candidate = (obstacle.chain(), z, normal, !obstacle.is_ghost());
        match nearest {
            Some((chain, best, _, _)) if chain == obstacle.chain() => {
                if z < best {
                    nearest = Some(candidate);
                }
            }
            Some((_, z0, n0, counts)) => {
                out.add(n0, line_law(z0), counts, lever);
                nearest = Some(candidate);
            }
            None => nearest = Some(candidate),
        }
    }
    if let Some((_, z, normal, counts)) = nearest {
        out.add(normal, line_law(z), counts, lever);
    }
}

/// Signed distances of `pos` from the four box sides, with their inward
/// normals.
fn box_sides(pos: Vec2D, bound: &Rect) -> [(f32, Vec2D); 4] {
    let (x0, y0) = bound.get_tl();
    let (x1, y1) = bound.get_br();
    [
        (pos.x - x0.min(x1), Vec2D::new(1.0, 0.0)),
        (x0.max(x1) - pos.x, Vec2D::new(-1.0, 0.0)),
        (pos.y - y0.min(y1), Vec2D::new(0.0, 1.0)),
        (y0.max(y1) - pos.y, Vec2D::new(0.0, -1.0)),
    ]
}

/// The position update of a molecule: the drift, then each site against
/// the walls at its contact distance (`WallSettings::contact_factor`; behind
/// soft walls the backstop, a counted safety net). Its sites turn with it,
/// so after the drift each gets [`Ball2D::wall_hit`], first mirrored back
/// across a line wall whose centre line it crossed. The impulse acts at the
/// end of the drift, so its energy error is first order in dt. A ball whose
/// line walls are ramping in meets only the box.
pub(crate) fn molecule_backstop(
    physics: &BallPhysics,
    ball: &mut Ball2D,
    sites: &[Site],
    (bound, obstacles): (&Rect, &[Obstacle]),
    dt: f32,
    restitution: f32,
) -> BounceInfo {
    let start = *ball.get_position();
    let obstacles = if ball.leaving_wall { &[] } else { obstacles };
    let start_axis = axis(ball.theta);
    ball.drift(dt);
    let axis = axis(ball.theta);
    let mut info = BounceInfo::default();
    let mut hit = |ball: &mut Ball2D, normal: Vec2D, overlap, lever, counts: bool| {
        if let Some(impulse) = ball.wall_hit(normal, overlap, lever, restitution) {
            info.impulses += 1;
            if counts {
                info.wall_momentum = info.wall_momentum.add_scaled(&normal, -impulse);
            }
        }
    };
    for site in sites {
        let lever = turn(site.offset, axis);
        let distance = physics.walls().contact_factor() * site.sigma;
        for side in 0..4 {
            let (z, normal) = box_sides(*ball.get_position() + lever, bound)[side];
            if z < distance {
                hit(ball, normal, distance - z, lever, true);
            }
        }
        let from = start + turn(site.offset, start_axis);
        for obstacle in obstacles {
            let point = *ball.get_position() + lever;
            if let Some((z, normal, mirror)) = obstacle.backstop_distance(&point, &from, distance)
                && z < distance
            {
                ball.shift(mirror);
                hit(ball, normal, distance - z, lever, !obstacle.is_ghost());
            }
        }
    }
    info
}
