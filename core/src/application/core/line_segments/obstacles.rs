// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::core::SimulationWorld;
use crate::objects2d::Vec2D;
use crate::objects2d::line_segment::{DEFAULT_THICKNESS, THICKNESS_TO_WORLD};
use crate::objects2d::wall_chain::{Joint, WallChain};

/// Collision tolerance: balls closer than `radius + half_thickness + EPSILON`
/// to a wall's centre line are considered colliding.
const COLLISION_EPSILON: f32 = 0.01;
/// Bisection steps for the contact point along a step's path (2^-24 of a
/// step: below f32 resolution of the path fraction).
const TOUCH_BISECTION_STEPS: usize = 24;

/// A plain capsule obstacle for collision purposes: either a chain piece or
/// a closed valve's span, flattened out so the hot per-ball loop never has
/// to know which one it came from or look up any valve state. This is what
/// eliminates the whole "is this contact point inside the open valve's
/// t-range" bug class: an open valve or a gap simply never produces one of
/// these, so there's nothing left to get wrong at collision time.
pub(crate) struct Obstacle {
    pub(super) start: Vec2D,
    /// The wall being drawn or dragged: collides, but its hits don't count
    /// towards the wall momentum (pressure) totals.
    pub(super) ghost: bool,
    // Derived once per step, so the per-ball tests stay cheap.
    /// `end - start`
    pub(super) direction: Vec2D,
    /// `1 / |end - start|²`, or 0 for a degenerate (point) obstacle.
    pub(super) inv_length_sq: f32,
    /// Half the wall thickness in world units.
    pub(super) half_thickness: f32,
    /// Bounding box of the centre line: (min, max).
    pub(super) bounds: (Vec2D, Vec2D),
}

impl Obstacle {
    pub(super) fn new(start: Vec2D, end: Vec2D, thickness: f32, ghost: bool) -> Self {
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
    pub(super) fn is_degenerate(&self) -> bool {
        self.inv_length_sq == 0.0
    }

    /// Centre distance at which a ball of `radius` touches the obstacle.
    #[inline]
    pub(super) fn reach(&self, radius: f32) -> f32 {
        radius + self.half_thickness + COLLISION_EPSILON
    }

    /// Cheap reject: false if `pos` is farther than `reach` from the centre
    /// line's bounding box (so it cannot touch the obstacle).
    #[inline]
    pub(super) fn near_bounds(&self, pos: &Vec2D, reach: f32) -> bool {
        let (min, max) = &self.bounds;
        pos.x > min.x - reach
            && pos.x < max.x + reach
            && pos.y > min.y - reach
            && pos.y < max.y + reach
    }

    /// Whether a ball centre at `pos` is within `reach` of the centre line.
    pub(super) fn within(&self, pos: &Vec2D, reach: f32) -> bool {
        self.within_counted(pos, reach, &mut 0)
    }

    /// `within`, adding 1 to `checks` for every distance actually computed
    /// (i.e. `pos` passed the bounding-box reject). See
    /// `SimStats::wall_distance_checks`.
    #[inline]
    pub(super) fn within_counted(&self, pos: &Vec2D, reach: f32, checks: &mut u64) -> bool {
        if !self.near_bounds(pos, reach) {
            return false;
        }
        *checks += 1;
        self.distance_sq(pos) < reach * reach
    }

    /// True if a ball of `radius` at `pos` touches the obstacle.
    pub(super) fn overlaps(&self, pos: &Vec2D, radius: f32) -> bool {
        self.within(pos, self.reach(radius))
    }

    /// Where along the straight path `start -> end` (as a fraction) a ball of
    /// `radius` first touches the obstacle, if it starts clear of it and ends
    /// overlapping it. Almost every ball ends the step clear of almost every
    /// obstacle, so that is tested first, and without a square root.
    #[inline]
    pub(super) fn first_touch(
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
    pub(super) fn outward_normal(&self, touch: &Vec2D, from: &Vec2D) -> Vec2D {
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
    pub(super) fn distance_sq(&self, pos: &Vec2D) -> f32 {
        self.closest_point(pos).dist(pos).norm_sq()
    }

    /// Closest point on the (finite) obstacle to `pos`. Clamps to the
    /// endpoints, giving the capsule its rounded ends -- same as
    /// `SimLineSegment::closest_point`.
    pub(super) fn closest_point(&self, pos: &Vec2D) -> Vec2D {
        let rel = self.start.dist(pos);
        let t = (rel.dot(&self.direction) * self.inv_length_sq).clamp(0.0, 1.0);
        self.start.add_scaled(&self.direction, t)
    }
}

impl SimulationWorld {
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
    pub(super) fn chain_obstacles(&self, chain: &WallChain, obstacles: &mut Vec<Obstacle>) {
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
}
