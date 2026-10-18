// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{MainContent, SimulationWorld};
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::ball_impl::wall_contact::{WallContact, resolve_normal_position};
use crate::objects2d::line_segment::{
    DEFAULT_THICKNESS, GhostWall, MAX_THICKNESS, MIN_THICKNESS, THICKNESS_TO_WORLD,
};
use crate::objects2d::wall_chain::{ChainPiece, Joint, WallChain};
use rayon::prelude::*;

/// A plain capsule obstacle for collision purposes: either a chain piece or
/// a closed valve's span, flattened out so the hot per-ball loop never has
/// to know which one it came from or look up any valve state. This is what
/// eliminates the whole "is this contact point inside the open valve's
/// t-range" bug class: an open valve or a gap simply never produces one of
/// these, so there's nothing left to get wrong at collision time.
pub(crate) struct Obstacle {
    start: Vec2D,
    /// The wall being drawn or dragged: collides, but its hits don't count
    /// towards the wall momentum (pressure) totals.
    ghost: bool,
    // Derived once per step, so the per-ball tests stay cheap.
    /// `end - start`
    direction: Vec2D,
    /// `1 / |end - start|²`, or 0 for a degenerate (point) obstacle.
    inv_length_sq: f32,
    /// Half the wall thickness in world units.
    half_thickness: f32,
    /// Bounding box of the centre line: (min, max).
    bounds: (Vec2D, Vec2D),
}

impl Obstacle {
    fn new(start: Vec2D, end: Vec2D, thickness: f32, ghost: bool) -> Self {
        let direction = start.dist(&end);
        let length_sq = direction.norm_sq();
        Self {
            start,
            ghost,
            direction,
            inv_length_sq: if length_sq > 1e-12 {
                1.0 / length_sq
            } else {
                0.0
            },
            half_thickness: 0.5 * thickness * THICKNESS_TO_WORLD,
            bounds: (
                Vec2D::new(start.x.min(end.x), start.y.min(end.y)),
                Vec2D::new(start.x.max(end.x), start.y.max(end.y)),
            ),
        }
    }

    #[inline]
    fn is_degenerate(&self) -> bool {
        self.inv_length_sq == 0.0
    }

    /// Centre distance at which a ball of `radius` touches the obstacle.
    #[inline]
    fn reach(&self, radius: f32) -> f32 {
        radius + self.half_thickness + COLLISION_EPSILON
    }

    /// Cheap reject: false if `pos` is farther than `reach` from the centre
    /// line's bounding box (so it cannot touch the obstacle).
    #[inline]
    fn near_bounds(&self, pos: &Vec2D, reach: f32) -> bool {
        let (min, max) = &self.bounds;
        pos.x > min.x - reach
            && pos.x < max.x + reach
            && pos.y > min.y - reach
            && pos.y < max.y + reach
    }

    /// Whether a ball centre at `pos` is within `reach` of the centre line.
    fn within(&self, pos: &Vec2D, reach: f32) -> bool {
        self.within_counted(pos, reach, &mut 0)
    }

    /// `within`, adding 1 to `checks` for every distance actually computed
    /// (i.e. `pos` passed the bounding-box reject). See
    /// `SimStats::wall_distance_checks`.
    #[inline]
    fn within_counted(&self, pos: &Vec2D, reach: f32, checks: &mut u64) -> bool {
        if !self.near_bounds(pos, reach) {
            return false;
        }
        *checks += 1;
        self.distance_sq(pos) < reach * reach
    }

    /// True if a ball of `radius` at `pos` touches the obstacle.
    fn overlaps(&self, pos: &Vec2D, radius: f32) -> bool {
        self.within(pos, self.reach(radius))
    }

    /// Where along the straight path `start -> end` (as a fraction) a ball of
    /// `radius` first touches the obstacle, if it starts clear of it and ends
    /// overlapping it. Almost every ball ends the step clear of almost every
    /// obstacle, so that is tested first, and without a square root.
    #[inline]
    fn first_touch(
        &self,
        start: &Vec2D,
        end: &Vec2D,
        radius: f32,
        checks: &mut u64,
    ) -> Option<f32> {
        let reach = self.reach(radius);
        if self.is_degenerate()
            || !self.within_counted(end, reach, checks)
            || self.within_counted(start, reach, checks)
        {
            return None;
        }
        let reach_sq = reach * reach;
        let path = start.dist(end);
        let (mut clear, mut inside) = (0.0f32, 1.0f32);
        for _ in 0..TOUCH_BISECTION_STEPS {
            let mid = 0.5 * (clear + inside);
            if self.distance_sq(&start.add_scaled(&path, mid)) >= reach_sq {
                clear = mid;
            } else {
                inside = mid;
            }
        }
        Some(clear)
    }

    /// Unit normal of the surface at `touch` (a point on it), pointing away
    /// from the obstacle; `from` decides the side if `touch` is on the centre
    /// line.
    fn outward_normal(&self, touch: &Vec2D, from: &Vec2D) -> Vec2D {
        let out = self.closest_point(touch).dist(touch);
        let len = out.norm();
        if len > 1e-12 {
            return out.scaled(1.0 / len);
        }
        let n = Vec2D::new(-self.direction.y, self.direction.x).scaled(self.inv_length_sq.sqrt());
        if n.dot(&self.start.dist(from)) < 0.0 {
            n.scaled(-1.0)
        } else {
            n
        }
    }

    /// Squared distance from `pos` to the centre line.
    fn distance_sq(&self, pos: &Vec2D) -> f32 {
        self.closest_point(pos).dist(pos).norm_sq()
    }

    /// Closest point on the (finite) obstacle to `pos`. Clamps to the
    /// endpoints, giving the capsule its rounded ends -- same as
    /// `SimLineSegment::closest_point`.
    fn closest_point(&self, pos: &Vec2D) -> Vec2D {
        let rel = self.start.dist(pos);
        let t = (rel.dot(&self.direction) * self.inv_length_sq).clamp(0.0, 1.0);
        self.start.add_scaled(&self.direction, t)
    }
}

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

/// Collision tolerance: balls closer than `radius + half_thickness + EPSILON`
/// to a wall's centre line are considered colliding.
const COLLISION_EPSILON: f32 = 0.01;
/// Bisection steps for the contact point along a step's path (2^-24 of a
/// step: below f32 resolution of the path fraction).
const TOUCH_BISECTION_STEPS: usize = 24;
/// Velocity gate: only reflect if ball is moving toward the segment.
const VELOCITY_GATE_EPSILON: f32 = -1e-6;

const THICKNESS_STEP: f32 = 1.0;

impl MainContent {
    /// Add a new line segment barrier: a brand-new, unsplit (1-piece,
    /// 0-valve) wall chain.
    pub(crate) fn add_line_segment(&mut self, start: Vec2D, end: Vec2D) {
        self.world.add_line_segment(start, end);
    }

    /// Add a wall with the given thickness (clamped to the range the
    /// thickness keys allow). Used by headless runs to place walls without
    /// UI interaction. Returns the id of the wall's (single) piece.
    pub fn add_line_segment_with_thickness(
        &mut self,
        start: Vec2D,
        end: Vec2D,
        thickness: f32,
    ) -> usize {
        self.world
            .add_line_segment_with_thickness(start, end, thickness)
    }

    /// Move one of a WALL's own two endpoints (not any internal, valve-cut
    /// piece boundary -- those aren't user-draggable at all; see
    /// `SimulationWorld::move_line_segment_endpoint`).
    ///
    /// `segment_id` is a PIECE id (whatever the endpoint-drag tool hit-test
    /// found, same as before); this resolves it to the owning chain and only
    /// applies the move if that piece is the chain's first (for move_start)
    /// or last (for !move_start) piece. `move_start`: `true` → move the
    /// chain's start endpoint, `false` → move its end endpoint.
    pub(crate) fn move_line_segment_endpoint(
        &mut self,
        segment_id: usize,
        new_pos: Vec2D,
        move_start: bool,
    ) {
        self.world
            .move_line_segment_endpoint(segment_id, new_pos, move_start);
    }

    pub(crate) fn change_thickness(&mut self, segment_id: usize, steps: i32) {
        self.world.change_line_thickness(segment_id, steps);
    }

    pub(crate) fn set_ghost_wall(&mut self, ghost: Option<GhostWall>) {
        self.world.set_ghost_wall(ghost);
    }

    /// Resolve ball–line-segment collisions for all balls.
    ///
    /// Each wall is a capsule: its centre line inflated by half its thickness
    /// (`thickness * THICKNESS_TO_WORLD / 2`), with rounded ends. For each ball
    /// that overlaps a wall (centre-line distance < wall contact distance + half
    /// thickness) and is moving toward it (`v · n < 0`), the ball is:
    /// 1. Position-corrected so it no longer overlaps.
    /// 2. Velocity-reflected: `v' = v − 2(v · n̂) n̂` (elastic, |v| preserved).
    ///
    /// Returns accumulated wall momentum `(px, py)` for pressure tracking.
    #[cfg(test)]
    pub(crate) fn resolve_line_segment_collisions(&mut self) -> (f32, f32) {
        self.world.resolve_line_segment_collisions()
    }
}

impl SimulationWorld {
    /// ASSUMPTION: `LineSegmentCollection::add` returns the new piece's id.
    /// (Previously the return value was unused; the chain needs it to link
    /// the piece into a `WallChain`.)
    pub(crate) fn add_line_segment(&mut self, start: Vec2D, end: Vec2D) {
        self.add_line_segment_with_thickness(start, end, DEFAULT_THICKNESS);
    }

    pub(crate) fn add_line_segment_with_thickness(
        &mut self,
        start: Vec2D,
        end: Vec2D,
        thickness: f32,
    ) -> usize {
        let line_id = self.line_segments.add(start, end);
        if let Some(seg) = self.line_segments.find_mut(line_id) {
            seg.thickness = f32::clamp(thickness, MIN_THICKNESS, MAX_THICKNESS);
        }
        let chain_id = self.wall_chains.add(start, end, line_id);
        self.ghost_wall = None;
        self.flag_balls_overlapping_chain(chain_id);
        line_id
    }

    /// Set (or clear) the wall being drawn or dragged. It collides like a
    /// real wall; balls it lands on are flagged to leave it on their own.
    pub(crate) fn set_ghost_wall(&mut self, ghost: Option<GhostWall>) {
        self.ghost_wall = ghost;
        if let Some(g) = ghost {
            self.flag_balls_overlapping(&Obstacle::new(g.start, g.end, g.thickness, true));
        }
    }

    /// Mark every ball overlapping `obstacle` as `leaving_wall`: it was
    /// there before the wall was, so it gets out on its own path instead of
    /// being moved (which would add energy or land it on another ball).
    fn flag_balls_overlapping(&mut self, obstacle: &Obstacle) {
        for ball in &mut self.balls {
            let contact = self.physics.wall_contact_distance(ball.species());
            if obstacle.overlaps(ball.get_position(), contact) {
                ball.leaving_wall = true;
            }
        }
    }

    /// `flag_balls_overlapping` for every solid part (pieces and closed
    /// valves) of chain `chain_id`.
    fn flag_balls_overlapping_chain(&mut self, chain_id: usize) {
        let Some(chain) = self.wall_chains.find(chain_id) else {
            return;
        };
        let mut obstacles = Vec::new();
        self.chain_obstacles(chain, &mut obstacles);
        for obstacle in &obstacles {
            self.flag_balls_overlapping(obstacle);
        }
    }

    /// `flag_balls_overlapping` for valve `valve_id`'s span (called when it
    /// closes).
    pub(crate) fn flag_balls_overlapping_valve(&mut self, valve_id: usize) {
        let (Some((start, end)), Some(thickness)) = (
            self.wall_chains.valve_span(valve_id),
            self.valve_thickness(valve_id),
        ) else {
            return;
        };
        self.flag_balls_overlapping(&Obstacle::new(start, end, thickness, false));
    }

    /// Move one of a chain's own two endpoints. `segment_id` is a PIECE id;
    /// this only applies if that piece is the FIRST piece of its chain (for
    /// `move_start`) or the LAST piece (for `!move_start`) -- i.e. the
    /// piece's own `t_start == 0.0` / `t_end == 1.0`. Grabbing an internal,
    /// valve-cut piece boundary is a no-op: those points aren't meant to be
    /// user-draggable at all (no endpoint marker should be drawn there
    /// either -- see the renderer note in wall_chain.rs).
    pub(crate) fn move_line_segment_endpoint(
        &mut self,
        segment_id: usize,
        new_pos: Vec2D,
        move_start: bool,
    ) {
        let Some(chain) = self.wall_chains.find_containing_piece_mut(segment_id) else {
            return;
        };
        let Some(piece) = chain.pieces.iter().find(|p| p.line_id == segment_id) else {
            return;
        };

        let is_chain_endpoint = if move_start {
            piece.t_start <= 1e-6
        } else {
            piece.t_end >= 1.0 - 1e-6
        };
        if !is_chain_endpoint {
            return; // internal split point: not draggable
        }

        if move_start {
            chain.start = new_pos;
        } else {
            chain.end = new_pos;
        }
        let chain_id = chain.id;
        self.retarget_chain(segment_id);
        self.ghost_wall = None;
        self.flag_balls_overlapping_chain(chain_id);
    }

    pub(crate) fn change_line_thickness(&mut self, segment_id: usize, steps: i32) {
        if let Some(seg) = self.line_segments.find_mut(segment_id) {
            seg.thickness = f32::clamp(
                seg.thickness + steps as f32 * THICKNESS_STEP,
                MIN_THICKNESS,
                MAX_THICKNESS,
            );
        }
        if let Some(chain_id) = self
            .wall_chains
            .find_containing_piece(segment_id)
            .map(|c| c.id)
        {
            self.flag_balls_overlapping_chain(chain_id);
        }
    }

    /// Recompute every piece's actual world-space start/end from its owning
    /// chain's CURRENT `start`/`end` and its own fixed fractional span. Call
    /// this whenever a chain's `start` or `end` changes, so every piece (and,
    /// since valve geometry is derived on demand via `chain.point_at`, every
    /// valve too) stretches proportionally together.
    ///
    /// `any_piece_id` may be any piece belonging to the chain to retarget.
    fn retarget_chain(&mut self, any_piece_id: usize) {
        let Some(chain) = self.wall_chains.find_containing_piece(any_piece_id) else {
            return;
        };
        // Collect first (immutable borrow of wall_chains ends here), then
        // apply to line_segments (a different field -- no simultaneous
        // borrow issue, just sequenced to keep the borrow checker happy).
        let updates: Vec<(usize, Vec2D, Vec2D)> = chain
            .pieces
            .iter()
            .map(|p| {
                (
                    p.line_id,
                    chain.point_at(p.t_start),
                    chain.point_at(p.t_end),
                )
            })
            .collect();

        for (line_id, new_start, new_end) in updates {
            if let Some(seg) = self.line_segments.find_mut(line_id) {
                seg.start = new_start;
                seg.end = new_end;
            }
        }
    }

    /// Split the piece `line_id` at two LOCAL fractions [local_t_start,
    /// local_t_end] (0..1 along that piece's OWN current start/end -- the
    /// same convention `SimLineSegment::closest_t`/`point_at_t` already use,
    /// so the valve tool's existing hit-test needs no changes) into three
    /// parts: the piece before the valve, the valve itself (a new
    /// `Joint::Valve`), and the piece after.
    ///
    /// Returns the new valve's id, or `None` if `line_id` doesn't belong to
    /// any chain (shouldn't happen -- every piece belongs to exactly one) or
    /// the fractions are degenerate.
    pub(crate) fn add_valve_on_piece(
        &mut self,
        line_id: usize,
        local_t_start: f32,
        local_t_end: f32,
        is_closed: bool,
    ) -> Option<usize> {
        let (local_t_start, local_t_end) = if local_t_start <= local_t_end {
            (local_t_start, local_t_end)
        } else {
            (local_t_end, local_t_start)
        };
        if super::valves::valve_span_problem(local_t_start, local_t_end).is_some() {
            return None;
        }

        let piece_end_t = {
            let chain = self.wall_chains.find_containing_piece(line_id)?;
            let piece = chain.pieces.iter().find(|p| p.line_id == line_id)?;

            // Chain-relative fractions, converted from the piece-local ones.
            let span = piece.t_end - piece.t_start;
            let chain_t_start = piece.t_start + local_t_start * span;
            let chain_t_end = piece.t_start + local_t_end * span;
            (piece.t_start, chain_t_start, chain_t_end, piece.t_end)
        };
        let (piece_t_start, chain_t_start, chain_t_end, piece_t_end) = piece_end_t;

        let chain = self.wall_chains.find_containing_piece(line_id)?;
        let split_a = chain.point_at(chain_t_start);
        let split_b = chain.point_at(chain_t_end);
        let far_end = chain.point_at(piece_t_end);

        let old_thickness = self.line_segments.find(line_id).map(|s| s.thickness)?;

        // Shrink the original piece to [piece_t_start, chain_t_start] -- it
        // keeps its id, so anything already referencing it stays valid.
        if let Some(seg) = self.line_segments.find_mut(line_id) {
            seg.end = split_a;
        }

        // New trailing piece, same thickness as the original (each piece is
        // independently adjustable from here on -- see change_line_thickness).
        let new_line_id = self.line_segments.add(split_b, far_end);
        if let Some(seg) = self.line_segments.find_mut(new_line_id) {
            seg.thickness = old_thickness;
        }

        let valve_id = self.valve_segments.add(is_closed);

        let chain = self.wall_chains.find_containing_piece_mut(line_id)?;
        let piece_index = chain.pieces.iter().position(|p| p.line_id == line_id)?;
        chain.pieces[piece_index] = ChainPiece {
            line_id,
            t_start: piece_t_start,
            t_end: chain_t_start,
        };
        chain.pieces.insert(
            piece_index + 1,
            ChainPiece {
                line_id: new_line_id,
                t_start: chain_t_end,
                t_end: piece_t_end,
            },
        );
        chain.joints.insert(
            piece_index,
            Joint::Valve {
                valve_id,
                t_start: chain_t_start,
                t_end: chain_t_end,
            },
        );

        Some(valve_id)
    }

    /// Flatten every currently-solid obstacle -- each chain piece (always
    /// solid) plus each CLOSED valve joint's span (derived on the fly from
    /// its chain, via `WallChain::point_at`) -- into plain capsules. Open
    /// valve joints and gap joints contribute nothing.
    ///
    /// NOTE: this assumes a `self.wall_chains` collection with a
    /// `.chains() -> impl Iterator<Item = &WallChain>` accessor, mirroring
    /// `self.line_segments.segments()` / `self.valve_segments.valves()`
    /// above. Adjust the accessor name to whatever the actual container
    /// type ends up being called.
    pub(crate) fn collect_solid_obstacles(&self) -> Vec<Obstacle> {
        let mut obstacles = Vec::new();
        self.collect_solid_obstacles_into(&mut obstacles);
        obstacles
    }

    /// `collect_solid_obstacles` into `obstacles`, which is cleared first, so
    /// the step can reuse one buffer.
    pub(crate) fn collect_solid_obstacles_into(&self, obstacles: &mut Vec<Obstacle>) {
        obstacles.clear();
        for chain in self.wall_chains.chains() {
            self.chain_obstacles(chain, obstacles);
        }
        if let Some(g) = self.ghost_wall {
            obstacles.push(Obstacle::new(g.start, g.end, g.thickness, true));
        }
    }

    /// Append `chain`'s solid parts (see `collect_solid_obstacles`).
    fn chain_obstacles(&self, chain: &WallChain, obstacles: &mut Vec<Obstacle>) {
        for piece in &chain.pieces {
            let Some(seg) = self.line_segments.find(piece.line_id) else {
                continue;
            };
            obstacles.push(Obstacle::new(seg.start, seg.end, seg.thickness, false));
        }

        for (index, joint) in chain.joints.iter().enumerate() {
            if let Joint::Valve {
                valve_id,
                t_start,
                t_end,
            } = joint
                && let Some(valve) = self.valve_segments.find(*valve_id)
                && valve.is_closed
            {
                obstacles.push(Obstacle::new(
                    chain.point_at(*t_start),
                    chain.point_at(*t_end),
                    self.joint_thickness(chain, index),
                    false,
                ));
            }
            // Open Joint::Valve and Joint::Gap: no obstacle -- nothing
            // to check, nothing to get wrong.
        }
    }

    /// Thickness of the joint `joint_index` of `chain`: the thicker of the
    /// two pieces it sits between. A valve has no thickness of its own, so
    /// a closed valve always matches the wall around it, even after a piece's
    /// thickness is changed.
    pub(crate) fn joint_thickness(&self, chain: &WallChain, joint_index: usize) -> f32 {
        chain
            .pieces
            .get(joint_index..=joint_index + 1)
            .unwrap_or_default()
            .iter()
            .filter_map(|piece| self.line_segments.find(piece.line_id))
            .map(|seg| seg.thickness)
            .reduce(f32::max)
            .unwrap_or(DEFAULT_THICKNESS)
    }

    /// Thickness of valve `valve_id` (see `joint_thickness`), or `None` if no
    /// chain holds it.
    pub(crate) fn valve_thickness(&self, valve_id: usize) -> Option<f32> {
        let (chain, index) = self.wall_chains.find_valve_joint(valve_id)?;
        Some(self.joint_thickness(chain, index))
    }

    /// Resolve a ball's contact with the walls during its position update,
    /// the same way box walls are resolved (see `wall_contact`): the contact
    /// time and point come from the step's path, the ball is reflected there
    /// along the surface normal, and the velocity half of the step uses the
    /// returned contact. Forces are then computed at the resolved position.
    ///
    /// `start` is the ball's position before this step's drift; the ball's
    /// current position is the free (unreflected) end of the drift. `contact`
    /// is the ball's wall contact distance (`BallPhysics::wall_contact_distance`). A ball
    /// flagged `leaving_wall`, or one that starts the step inside a wall, is
    /// left to the overlap pass (`resolve_wall_overlaps_with`). Distance checks are added to
    /// `checks` (see `SimStats::wall_distance_checks`).
    pub(crate) fn sweep_wall_contact(
        obstacles: &[Obstacle],
        ball: &mut Ball2D,
        contact: f32,
        start: Vec2D,
        dt: f32,
        restitution: f32,
        checks: &mut u64,
    ) -> Option<WallContact> {
        if ball.leaving_wall {
            return None;
        }
        let end = *ball.get_position();
        let mut local_checks = 0;
        let mut first: Option<(f32, &Obstacle)> = None;
        for obstacle in obstacles {
            if let Some(t) = obstacle.first_touch(&start, &end, contact, &mut local_checks)
                && first.is_none_or(|(best, _)| t < best)
            {
                first = Some((t, obstacle));
            }
        }
        *checks += local_checks;
        let (touch_fraction, obstacle) = first?;

        // Flat wall through the touching point, normal to the surface there.
        let touch = start.add_scaled(&start.dist(&end), touch_fraction);
        let normal = obstacle.outward_normal(&touch, &start);
        let resolved = resolve_normal_position(
            touch.dist(&start).dot(&normal),
            ball.v.dot(&normal),
            ball.accel.dot(&normal),
            dt,
            restitution,
        )?;
        let free_distance = touch.dist(&end).dot(&normal);
        ball.set_position_and_update_tail(
            end.add_scaled(&normal, resolved.distance - free_distance),
        );

        Some(WallContact {
            normal,
            contact_fraction: resolved.contact_fraction,
            free_fraction: resolved.free_fraction,
            counts_for_momentum: !obstacle.ghost,
        })
    }

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
    #[cfg(test)]
    pub(crate) fn resolve_line_segment_collisions(&mut self) -> (f32, f32) {
        let obstacles = self.collect_solid_obstacles();
        self.resolve_wall_overlaps(&obstacles)
    }

    /// [`Self::resolve_wall_overlaps_with`] with a fresh buffer.
    #[cfg(test)]
    pub(crate) fn resolve_wall_overlaps(&mut self, obstacles: &[Obstacle]) -> (f32, f32) {
        let mut passes = Vec::new();
        self.resolve_wall_overlaps_with(obstacles, &mut passes)
    }
}
