// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Soft wall forces (#280): the force and energy of every ball in the box
//! walls and the line walls, added to its pair forces in the force pass.
//!
//! - Box walls: each of the four sides acts on its own, through the signed
//!   distance of the centre from it, so the force keeps pointing inward after
//!   an overshoot; in a corner two sides add up.
//! - Line walls (pieces, closed valves, the ghost wall): only the nearest
//!   obstacle acts (nearest-feature rule). The surfaces of a chain's pieces
//!   overlap at its joints; the minimum of their distances is the distance
//!   from the union, so the energy has no bump or well at a joint.
//!
//! The obstacles are checked against their bounding boxes first, as in the
//! contact sweep, so the potential is evaluated only for nearby balls.

use super::line_segments::Obstacle;
use crate::objects2d::ball_impl::molecule::{axis, cross};
use crate::objects2d::ball_impl::pair_potential::PairPotential;
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

/// The soft wall force on `ball` from the box `bound` and `obstacles`. A
/// molecule (#406) feels it at each of its sites, which adds a torque.
pub(crate) fn soft_wall_force(
    walls: &SoftWalls,
    ball: &Ball2D,
    bound: &Rect,
    obstacles: &[Obstacle],
) -> WallForce {
    let species = ball.species();
    let centre = *ball.get_position();
    let mut out = WallForce {
        force: Vec2D::zero(),
        counted: Vec2D::zero(),
        potential: 0.0,
        torque: 0.0,
    };
    match walls.sites(species) {
        None => point_force(
            centre,
            Vec2D::zero(),
            walls.cutoff(species),
            &|z| walls.force_and_potential(species, z),
            bound,
            obstacles,
            &mut out,
        ),
        Some(sites) => {
            let axis = axis(ball.theta);
            for (offset, potential) in sites {
                let lever = axis.scaled(*offset);
                point_force(
                    centre + lever,
                    lever,
                    potential.cutoff(),
                    &|z| (potential.force(z), potential.potential(z)),
                    bound,
                    obstacles,
                    &mut out,
                );
            }
        }
    }
    out
}

/// The wall force on a point at `pos` (`lever` from the ball's centre) with
/// the wall law `law` (force, potential at wall distance `z`), cut off at
/// `cutoff`, added to `out`.
fn point_force(
    pos: Vec2D,
    lever: Vec2D,
    cutoff: f32,
    law: &impl Fn(f32) -> (f32, f32),
    bound: &Rect,
    obstacles: &[Obstacle],
    out: &mut WallForce,
) {
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
            out.add(normal, law(z), true, lever);
        }
    }

    let mut nearest: Option<(f32, Vec2D, bool)> = None;
    for obstacle in obstacles {
        if let Some((z, normal)) = obstacle.surface_distance(&pos, cutoff)
            && z < cutoff
            && nearest.is_none_or(|(best, _, _)| z < best)
        {
            nearest = Some((z, normal, !obstacle.is_ghost()));
        }
    }
    if let Some((z, normal, counts)) = nearest {
        out.add(normal, law(z), counts, lever);
    }
}
