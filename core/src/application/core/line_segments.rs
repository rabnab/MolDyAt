use super::{MainContent, SimulationWorld};
use crate::objects2d::Vec2D;
// Single source of truth for thickness units, shared with the renderer.
// Add to objects2d/line_segment.rs:  pub const THICKNESS_TO_WORLD: f32 = 0.05;
use crate::objects2d::line_segment::{DEFAULT_THICKNESS, THICKNESS_TO_WORLD};
use crate::objects2d::wall_chain::{ChainPiece, Joint, WallChain};

/// A plain capsule obstacle for collision purposes: either a chain piece or
/// a closed valve's span, flattened out so the hot per-ball loop never has
/// to know which one it came from or look up any valve state. This is what
/// eliminates the whole "is this contact point inside the open valve's
/// t-range" bug class: an open valve or a gap simply never produces one of
/// these, so there's nothing left to get wrong at collision time.
struct Obstacle {
    start: Vec2D,
    end: Vec2D,
    thickness: f32,
}

impl Obstacle {
    fn length_sq(&self) -> f32 {
        let dx = self.end.x - self.start.x;
        let dy = self.end.y - self.start.y;
        dx * dx + dy * dy
    }

    /// Closest point on the (finite) obstacle to `pos`. Clamps to the
    /// endpoints, giving the capsule its rounded ends -- same as
    /// `SimLineSegment::closest_point`.
    fn closest_point(&self, pos: &Vec2D) -> Vec2D {
        let dx = self.end.x - self.start.x;
        let dy = self.end.y - self.start.y;
        let len_sq = dx * dx + dy * dy;
        let t = if len_sq > 1e-12 {
            (((pos.x - self.start.x) * dx + (pos.y - self.start.y) * dy) / len_sq).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Vec2D::new(self.start.x + dx * t, self.start.y + dy * t)
    }
}

/// Collision tolerance: balls closer than `radius + half_thickness + EPSILON`
/// to a wall's centre line are considered colliding.
const COLLISION_EPSILON: f32 = 0.01;
/// Velocity gate: only reflect if ball is moving toward the segment.
const VELOCITY_GATE_EPSILON: f32 = -1e-6;

const THICKNESS_STEP: f32 = 1.0;

impl MainContent {
    /// Add a new line segment barrier: a brand-new, unsplit (1-piece,
    /// 0-valve) wall chain.
    pub(crate) fn add_line_segment(&mut self, start: Vec2D, end: Vec2D) {
        self.world.add_line_segment(start, end);
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

    /// Resolve ball–line-segment collisions for all balls.
    ///
    /// Each wall is a capsule: its centre line inflated by half its thickness
    /// (`thickness * THICKNESS_TO_WORLD / 2`), with rounded ends. For each ball
    /// that overlaps a wall (centre-line distance < radius + half thickness) and
    /// is moving toward it (`v · n < 0`), the ball is:
    /// 1. Position-corrected so it no longer overlaps.
    /// 2. Velocity-reflected: `v' = v − 2(v · n̂) n̂` (elastic, |v| preserved).
    ///
    /// Returns accumulated wall momentum `(px, py)` for pressure tracking.
    pub(crate) fn resolve_line_segment_collisions(&mut self) -> (f32, f32) {
        self.world.resolve_line_segment_collisions()
    }
}

impl SimulationWorld {
    /// ASSUMPTION: `LineSegmentCollection::add` returns the new piece's id.
    /// (Previously the return value was unused; the chain needs it to link
    /// the piece into a `WallChain`.)
    pub(crate) fn add_line_segment(&mut self, start: Vec2D, end: Vec2D) {
        let line_id = self.line_segments.add(start, end);
        self.wall_chains.add(start, end, line_id);
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
        self.retarget_chain(segment_id);
    }

    pub(crate) fn change_line_thickness(&mut self, segment_id: usize, steps: i32) {
        if let Some(seg) = self.line_segments.find_mut(segment_id) {
            seg.thickness = f32::clamp(seg.thickness + steps as f32 * THICKNESS_STEP, 1.0, 20.0);
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
        if local_t_end - local_t_start < 1e-4 {
            return None; // degenerate valve width
        }
        // A valve flush against only ONE of the piece's own edges would split
        // it into one normal piece plus one permanent zero-length piece
        // (start == end) that nothing ever merges or prunes -- see
        // add_valve_on_piece's doc comment. The UI's `snap_t` snaps a dragged
        // endpoint to exactly 0.0/1.0 near a piece boundary, so this is
        // reachable from an ordinary drag, not just malformed input. Reject
        // it; a valve flush against BOTH edges (the whole piece becomes the
        // valve) is unaffected -- no non-degenerate piece is lost there, so
        // there's nothing to strand.
        let start_flush = local_t_start < 1e-4;
        let end_flush = local_t_end > 1.0 - 1e-4;
        if start_flush != end_flush {
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
    fn collect_solid_obstacles(&self) -> Vec<Obstacle> {
        let mut obstacles = Vec::new();

        for chain in self.wall_chains.chains() {
            for piece in &chain.pieces {
                let Some(seg) = self.line_segments.find(piece.line_id) else {
                    continue;
                };
                obstacles.push(Obstacle {
                    start: seg.start,
                    end: seg.end,
                    thickness: seg.thickness,
                });
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
                    obstacles.push(Obstacle {
                        start: chain.point_at(*t_start),
                        end: chain.point_at(*t_end),
                        thickness: self.joint_thickness(chain, index),
                    });
                }
                // Open Joint::Valve and Joint::Gap: no obstacle -- nothing
                // to check, nothing to get wrong.
            }
        }

        obstacles
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

    /// Resolve ball-wall collisions for all balls, against every currently
    /// solid obstacle (see `collect_solid_obstacles`).
    ///
    /// Each obstacle is a capsule: its centre line inflated by half its
    /// thickness (`thickness * THICKNESS_TO_WORLD / 2`), with rounded ends.
    /// For each ball that overlaps an obstacle (centre-line distance <
    /// radius + half thickness) and is moving toward it (`v · n < 0`), the
    /// ball is:
    /// 1. Position-corrected so it no longer overlaps.
    /// 2. Velocity-reflected: `v' = v − 2(v · n̂) n̂` (elastic, |v| preserved).
    ///
    /// Unlike the old implementation, there is NO per-ball valve lookup here
    /// at all: valve state was already resolved once, when the obstacle list
    /// was built, not per-contact -- which is what kept producing bugs
    /// (mismatched t-along-the-wall points between the openness check and
    /// the actual collision geometry).
    ///
    /// Returns accumulated wall momentum `(px, py)` for pressure tracking.
    pub(crate) fn resolve_line_segment_collisions(&mut self) -> (f32, f32) {
        let mut total_px = 0.0_f32;
        let mut total_py = 0.0_f32;

        let obstacles = self.collect_solid_obstacles();

        for obstacle in &obstacles {
            if obstacle.length_sq() < 1e-12 {
                continue; // degenerate piece/valve span
            }

            // The obstacle is a capsule: the centre line inflated by half its
            // thickness. closest_point() clamps to the endpoints, so the ends
            // are rounded, matching what wall.wgsl draws.
            let half_thickness = 0.5 * obstacle.thickness * THICKNESS_TO_WORLD;

            for ball in self.balls.iter_mut() {
                let closest = obstacle.closest_point(ball.get_position());
                let to_ball = closest.dist(ball.get_position());
                let center_dist = to_ball.norm();

                // Ball touches the wall surface when its centre is closer to
                // the centre line than this.
                let reach = ball.radius + half_thickness + COLLISION_EPSILON;
                if center_dist >= reach {
                    continue; // no collision
                }

                let normal = if center_dist > 1e-12 {
                    to_ball.scaled(1.0 / center_dist)
                } else {
                    // Centre exactly on the centre line (possible with a thick
                    // wall): use the obstacle's perpendicular, on the side
                    // opposing the velocity.
                    let len = obstacle.length_sq().sqrt();
                    let n = Vec2D::new(
                        -(obstacle.end.y - obstacle.start.y) / len,
                        (obstacle.end.x - obstacle.start.x) / len,
                    );
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

                // Elastic reflection: v' = v − 2(v·n̂)n̂
                ball.v.x -= 2.0 * v_dot_n * normal.x;
                ball.v.y -= 2.0 * v_dot_n * normal.y;

                // Accumulate wall momentum for pressure calculation
                total_px += 2.0 * ball.mass * v_dot_n * normal.x;
                total_py += 2.0 * ball.mass * v_dot_n * normal.y;
            }
        }

        (total_px, total_py)
    }

    /// True if a ball of `radius` at `pos` would overlap any currently-solid
    /// obstacle (same flattened list `resolve_line_segment_collisions` uses,
    /// so spawn placement and collision can never disagree about what counts
    /// as a wall).
    pub(crate) fn overlaps_any_wall(&self, pos: &Vec2D, radius: f32) -> bool {
        for obstacle in &self.collect_solid_obstacles() {
            if obstacle.length_sq() < 1e-12 {
                continue;
            }
            let half = 0.5 * obstacle.thickness * THICKNESS_TO_WORLD;
            let center_dist = obstacle.closest_point(pos).dist(pos).norm();
            if center_dist < radius + half + COLLISION_EPSILON {
                return true;
            }
        }
        false
    }
}
