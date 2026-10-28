// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Walls (`WallChain`s of `SimLineSegment` pieces and valve joints): editing
//! them here, the solid obstacles collision sees (`obstacles`), the contact
//! sweep during the position update (`sweep`) and the overlap fallback for
//! balls that start a step inside a wall (`overlap`).

mod obstacles;
mod overlap;
mod sweep;

pub(crate) use obstacles::Obstacle;
pub(crate) use overlap::OverlapPass;

use super::valves::ValveSpanChange;
use super::{MainContent, SimulationWorld};
use crate::objects2d::Vec2D;
use crate::objects2d::line_segment::{DEFAULT_THICKNESS, GhostWall, MAX_THICKNESS, MIN_THICKNESS};
use crate::objects2d::wall_chain::{ChainPiece, Joint};

const THICKNESS_STEP: f32 = 1.0;

impl MainContent {
    /// Add a new line segment barrier: a brand-new, unsplit (1-piece,
    /// 0-valve) wall chain.
    pub fn add_line_segment(&mut self, start: Vec2D, end: Vec2D) {
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
    pub fn move_line_segment_endpoint(
        &mut self,
        segment_id: usize,
        new_pos: Vec2D,
        move_start: bool,
    ) {
        self.world
            .move_line_segment_endpoint(segment_id, new_pos, move_start);
    }

    pub fn change_thickness(&mut self, segment_id: usize, steps: i32) {
        self.world.change_line_thickness(segment_id, steps);
    }

    pub fn set_ghost_wall(&mut self, ghost: Option<GhostWall>) {
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
    #[cfg(any(test, feature = "test-support"))]
    pub fn resolve_line_segment_collisions(&mut self) -> (f32, f32) {
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

    /// Move or resize valve `valve_id` along its wall (#203), as `change`
    /// says, in fractions of the chain and as far as the pieces on either
    /// side allow. The valve stays between its two neighbouring pieces,
    /// which give up or take the length it moves by, and neither shrinks
    /// below [`MIN_VALVE_PIECE_LENGTH`] (or below its length now, if it is
    /// already shorter). A move keeps the valve's width exactly; a resize
    /// keeps it at least [`MIN_VALVE_LENGTH`] wide (or its width now, if
    /// narrower).
    ///
    /// Balls the moved pieces or valve now overlap are flagged for the
    /// overlap fallback, as after a wall drag. Returns the span applied, or
    /// `None` for an unknown valve or a wall without length.
    pub(crate) fn set_valve_span(
        &mut self,
        valve_id: usize,
        change: ValveSpanChange,
    ) -> Option<(f32, f32)> {
        let (chain, index) = self.wall_chains.find_valve_joint_mut(valve_id)?;
        let length = chain.start.distance_squared(&chain.end).sqrt();
        if length <= f32::EPSILON || !change.is_finite() {
            return None;
        }
        let joint = chain.joints[index];
        let (s0, e0) = (joint.t_start(), joint.t_end());
        let (before, after) = (chain.pieces[index], chain.pieces[index + 1]);
        let min_piece = MIN_VALVE_PIECE_LENGTH / length;
        let lower = before.t_start + min_piece.min(s0 - before.t_start).max(0.0);
        let upper = after.t_end - min_piece.min(after.t_end - e0).max(0.0);
        let (s, e) = match change {
            ValveSpanChange::Move { t_start } => {
                let shift = (t_start - s0).clamp(lower - s0, upper - e0);
                (s0 + shift, e0 + shift)
            }
            ValveSpanChange::Resize { t_start, t_end } => {
                let min_width = (MIN_VALVE_LENGTH / length).min(e0 - s0);
                let e = t_end.clamp(lower + min_width, upper);
                let s = t_start.clamp(lower, e - min_width);
                (s, e)
            }
        };
        chain.joints[index] = Joint::Valve {
            valve_id,
            t_start: s,
            t_end: e,
        };
        chain.pieces[index].t_end = s;
        chain.pieces[index + 1].t_start = e;
        let (chain_id, piece_id) = (chain.id, before.line_id);
        self.retarget_chain(piece_id);
        self.flag_balls_overlapping_chain(chain_id);
        Some((s, e))
    }
}

/// Shortest a wall piece next to a valve gets when the valve is moved or
/// resized (world units, `σ` of `N2`).
pub(crate) const MIN_VALVE_PIECE_LENGTH: f32 = 1.0;

/// Narrowest a valve gets when resized (world units).
pub(crate) const MIN_VALVE_LENGTH: f32 = 1.0;
