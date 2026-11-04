// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Soft wall forces (#280): the force and energy of every ball in the box
//! walls and the line walls, added to its pair forces in the force pass.
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
//! - A ball flagged `leaving_wall` (a wall was drawn or closed on top of it)
//!   feels no line wall until it is out of their reach, as it passes through
//!   them in the contact pass: a force there would kick it out and add
//!   energy.
//!
//! The obstacles are checked against their bounding boxes first, as in the
//! contact sweep, so the potential is evaluated only for nearby balls.

use super::line_segments::Obstacle;
use crate::objects2d::ball_impl::wall_law::SoftWalls;
use crate::objects2d::ball_impl::{Ball2D, ForceResult};
use crate::objects2d::{Rect, Vec2D};

/// Soft wall force and energy of one ball.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WallForce {
    /// Total force of the walls on the ball.
    pub force: Vec2D,
    /// The part of `force` that books wall momentum (all but the ghost wall).
    pub counted: Vec2D,
    pub potential: f32,
}

impl WallForce {
    fn add(&mut self, normal: Vec2D, (force, potential): (f32, f32), counts: bool) {
        let f = normal.scaled(force);
        self.force += f;
        if counts {
            self.counted += f;
        }
        self.potential += potential;
    }

    /// Add this to the ball's force result.
    pub(crate) fn add_to(&self, result: &mut ForceResult) {
        result.force += self.force;
        result.wall_force = self.counted;
        result.wall_potential = self.potential;
    }
}

/// The soft wall force on `ball` from the box `bound` and `obstacles`.
pub(crate) fn soft_wall_force(
    walls: &SoftWalls,
    ball: &Ball2D,
    bound: &Rect,
    obstacles: &[Obstacle],
) -> WallForce {
    let species = ball.species();
    let cutoff = walls.cutoff(species);
    let pos = *ball.get_position();
    let mut out = WallForce {
        force: Vec2D::zero(),
        counted: Vec2D::zero(),
        potential: 0.0,
    };

    let (left, top) = bound.get_tl();
    let (right, bottom) = bound.get_br();
    let sides = [
        (pos.x - left, Vec2D::new(1.0, 0.0)),
        (right - pos.x, Vec2D::new(-1.0, 0.0)),
        (pos.y - bottom.min(top), Vec2D::new(0.0, 1.0)),
        (top.max(bottom) - pos.y, Vec2D::new(0.0, -1.0)),
    ];
    for (z, normal) in sides {
        if z < cutoff {
            out.add(normal, walls.force_and_potential(species, z), true);
        }
    }

    if ball.leaving_wall {
        return out;
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
                out.add(n0, walls.force_and_potential(species, z0), counts);
                nearest = Some(candidate);
            }
            None => nearest = Some(candidate),
        }
    }
    if let Some((_, z, normal, counts)) = nearest {
        out.add(normal, walls.force_and_potential(species, z), counts);
    }
    out
}
