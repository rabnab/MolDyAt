// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::Obstacle;
use crate::application::core::SimulationWorld;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use rayon::prelude::*;

/// Velocity gate: only reflect if ball is moving toward the segment.
const VELOCITY_GATE_EPSILON: f32 = -1e-6;

/// Result of the overlap pass (one ball, or summed over balls).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct OverlapPass {
    /// Momentum given to non-ghost walls.
    px: f32,
    py: f32,
    /// Balls pushed out of a wall.
    corrections: u64,
    /// Distance checks (see `SimStats::wall_distance_checks`).
    checks: u64,
}

impl OverlapPass {
    fn merge(a: Self, b: Self) -> Self {
        Self {
            px: a.px + b.px,
            py: a.py + b.py,
            corrections: a.corrections + b.corrections,
            checks: a.checks + b.checks,
        }
    }
}

impl SimulationWorld {
    /// Resolve ball-wall overlaps for all balls, against `obstacles`, the
    /// currently solid obstacles the caller built this step (see
    /// `collect_solid_obstacles`; the simulation step shares one list with
    /// `sweep_wall_contact`).
    ///
    /// Contacts during a step are resolved by `sweep_wall_contact`; this pass
    /// only catches what that cannot: balls that started a step inside a wall
    /// (a wall moved onto them, or a second wall at a joint or box corner).
    ///
    /// Each obstacle is a capsule: its centre line inflated by half its
    /// thickness (`thickness * THICKNESS_TO_WORLD / 2`), with rounded ends.
    /// For each ball that overlaps an obstacle (centre-line distance < wall
    /// contact distance + half thickness) and is moving toward it
    /// (`v · n < 0`), the ball is:
    /// 1. Position-corrected so it no longer overlaps.
    /// 2. Velocity-reflected: `v' = v − 2(v · n̂) n̂` (elastic, |v| preserved).
    ///
    /// A ball flagged `leaving_wall` (a wall appeared or changed on top of
    /// it) passes through the walls it overlaps, keeping its path and speed,
    /// and the flag is cleared once it touches no wall at all. With gravity,
    /// a ball caught in a horizontal valve as it closes therefore falls
    /// through it. Every other ball collides as usual, including one whose
    /// centre got inside a wall in a single step: it is pushed back out.
    ///
    /// Hits on the ghost wall (being drawn or dragged) don't count towards
    /// the returned momentum, so they don't distort the pressure readings.
    ///
    /// Unlike the old implementation, there is NO per-ball valve lookup here
    /// at all: valve state was already resolved once, when the obstacle list
    /// was built, not per-contact -- which is what kept producing bugs
    /// (mismatched t-along-the-wall points between the openness check and
    /// the actual collision geometry).
    ///
    /// Each ball only depends on its own state and the obstacles, so balls
    /// are handled in parallel, with `passes` as the reused buffer for the
    /// per-ball results (the simulation step's scratch, #228). They are
    /// collected in ball order and summed serially: a parallel reduce would
    /// add the f32 momenta in an order that depends on the thread count
    /// (#222).
    ///
    /// Returns accumulated wall momentum `(px, py)` for pressure tracking.
    pub(crate) fn resolve_wall_overlaps_with(
        &mut self,
        obstacles: &[Obstacle],
        passes: &mut Vec<OverlapPass>,
    ) -> (f32, f32) {
        if obstacles.is_empty() {
            for ball in &mut self.balls {
                ball.leaving_wall = false;
            }
            return (0.0, 0.0);
        }
        let physics = &self.physics;
        self.balls
            .par_iter_mut()
            .map(|ball| {
                let contact = physics.wall_contact_distance(ball.species());
                Self::resolve_ball_overlaps(obstacles, ball, contact)
            })
            .collect_into_vec(passes);
        let pass = passes
            .iter()
            .copied()
            .fold(OverlapPass::default(), OverlapPass::merge);
        self.sim.wall_overlap_corrections += pass.corrections;
        self.sim.wall_distance_checks += pass.checks;
        (pass.px, pass.py)
    }

    /// Overlap pass for one ball against every obstacle, in list order.
    /// `contact` is the ball's wall contact distance.
    fn resolve_ball_overlaps(
        obstacles: &[Obstacle],
        ball: &mut Ball2D,
        contact: f32,
    ) -> OverlapPass {
        let mut total_px = 0.0_f32;
        let mut total_py = 0.0_f32;
        let mut corrections = 0;
        let mut checks = 0;

        if ball.leaving_wall
            && !obstacles
                .iter()
                .any(|o| o.within_counted(ball.get_position(), o.reach(contact), &mut checks))
        {
            ball.leaving_wall = false;
        }

        for obstacle in obstacles {
            if obstacle.is_degenerate() {
                continue; // degenerate piece/valve span
            }

            // The obstacle is a capsule: the centre line inflated by half its
            // thickness. closest_point() clamps to the endpoints, so the ends
            // are rounded, matching what wall.wgsl draws.
            // Ball touches the wall surface when its centre is closer to the
            // centre line than this.
            let reach = obstacle.reach(contact);
            if !obstacle.within_counted(ball.get_position(), reach, &mut checks) {
                continue; // no collision (cheap reject, no square root)
            }
            if ball.leaving_wall {
                continue; // was here before this wall: let it leave
            }
            let closest = obstacle.closest_point(ball.get_position());
            let to_ball = closest.dist(ball.get_position());
            let center_dist = to_ball.norm();

            let normal = if center_dist > 1e-12 {
                to_ball.scaled(1.0 / center_dist)
            } else {
                // Centre exactly on the centre line: use the obstacle's
                // perpendicular, on the side opposing the velocity.
                let n = Vec2D::new(-obstacle.direction.y, obstacle.direction.x)
                    .scaled(obstacle.inv_length_sq.sqrt());
                if n.x * ball.v.x + n.y * ball.v.y > 0.0 {
                    Vec2D::new(-n.x, -n.y)
                } else {
                    n
                }
            };

            // Velocity gate: only reflect if moving toward the obstacle.
            let v_dot_n = ball.v.x * normal.x + ball.v.y * normal.y;
            if v_dot_n > VELOCITY_GATE_EPSILON {
                continue; // moving away or parallel
            }

            // Position correction: push ball out so its surface touches
            // the obstacle's surface.
            let overlap = reach - center_dist;
            let corrected_pos = ball.get_position().add_scaled(&normal, overlap);
            ball.set_position_and_update_tail(corrected_pos);
            corrections += 1;

            // Elastic reflection: v' = v − 2(v·n̂)n̂
            ball.v.x -= 2.0 * v_dot_n * normal.x;
            ball.v.y -= 2.0 * v_dot_n * normal.y;

            // Accumulate wall momentum for pressure calculation
            if !obstacle.ghost {
                total_px += 2.0 * ball.mass * v_dot_n * normal.x;
                total_py += 2.0 * ball.mass * v_dot_n * normal.y;
            }
        }

        OverlapPass {
            px: total_px,
            py: total_py,
            corrections,
            checks,
        }
    }

    /// True if a ball of wall contact distance `radius` at `pos` would overlap
    /// any of `obstacles` (from `collect_solid_obstacles`, the same list the
    /// overlap pass `resolve_wall_overlaps_with` uses, so spawn placement and
    /// collision can never disagree about what counts as a wall). Callers
    /// placing many balls build the list once and reuse it.
    pub(crate) fn overlaps_any_wall(obstacles: &[Obstacle], pos: &Vec2D, radius: f32) -> bool {
        obstacles
            .iter()
            .any(|obstacle| !obstacle.is_degenerate() && obstacle.overlaps(pos, radius))
    }

    /// [`Self::resolve_wall_overlaps_with`] against the current solid
    /// obstacles, with a fresh buffer.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn resolve_line_segment_collisions(&mut self) -> (f32, f32) {
        let obstacles = self.collect_solid_obstacles();
        self.resolve_wall_overlaps(&obstacles)
    }

    /// [`Self::resolve_wall_overlaps_with`] with a fresh buffer.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn resolve_wall_overlaps(&mut self, obstacles: &[Obstacle]) -> (f32, f32) {
        let mut passes = Vec::new();
        self.resolve_wall_overlaps_with(obstacles, &mut passes)
    }
}
