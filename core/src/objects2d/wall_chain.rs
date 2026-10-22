// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wall chains: the data model for walls that can be split into pieces by
//! valves.
//!
//! A `WallChain` is what the user thinks of as "one wall" -- two draggable
//! endpoints, plus zero or more valves cut into it. Internally it's an
//! ordered alternation of solid `ChainPiece`s (each an existing, independent
//! `SimLineSegment` -- own id, own thickness) and `Joint`s sitting between
//! consecutive pieces. A wall with no valves is a 1-piece, 0-joint chain, so
//! "add a valve" is always "split one piece, insert a joint" -- no special
//! case for the not-yet-split wall.
//!
//! Collision code never needs to know about chains or valve state at all:
//! see `collect_solid_obstacles` (in
//! `application/core/line_segments/obstacles.rs`), which flattens every
//! piece plus every CLOSED valve joint into plain capsules once, up front,
//! instead of looking up valve openness per contact (which is where the
//! earlier bugs kept coming from).

use crate::objects2d::operator::{
    OperatorCollection, OperatorIdCounter, is_degenerate, remap_point,
};
use crate::objects2d::{Rect, Vec2D};

/// One solid piece of a chain. `line_id` is the id of the actual
/// `SimLineSegment` holding its real start/end/thickness (each piece is
/// independently adjustable, e.g. via Ctrl+scroll thickness). `t_start`/
/// `t_end` are the piece's FIXED fraction span [0, 1] along the CHAIN's own
/// endpoints -- set once, at split time, and used to recompute the piece's
/// actual world-space start/end whenever the chain's own endpoints move
/// (`WallChain::retarget`), so every piece and joint stretches
/// proportionally together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChainPiece {
    pub line_id: usize,
    pub t_start: f32,
    pub t_end: f32,
}

/// What sits between two adjacent pieces of a chain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Joint {
    /// A togglable valve. `valve_id` refers to a `ValveSegment` elsewhere
    /// (holding just `is_closed` -- no geometry or thickness of its own,
    /// since the chain and its pieces own those). `t_start`/`t_end` are fixed
    /// chain fractions, same convention as `ChainPiece`.
    Valve {
        valve_id: usize,
        t_start: f32,
        t_end: f32,
    },
    /// A deleted valve's permanent hole: no collision, no rendering, but
    /// keeps its fractional span so the chain still stretches correctly
    /// around it (see `SimulationWorld::delete_object`).
    Gap { t_start: f32, t_end: f32 },
}

impl Joint {
    pub fn t_start(&self) -> f32 {
        match self {
            Joint::Valve { t_start, .. } | Joint::Gap { t_start, .. } => *t_start,
        }
    }

    pub fn t_end(&self) -> f32 {
        match self {
            Joint::Valve { t_end, .. } | Joint::Gap { t_end, .. } => *t_end,
        }
    }
}

/// A user-drawn wall, possibly split into pieces by valves. See the module
/// doc comment above for the overall design.
#[derive(Debug, Clone, PartialEq)]
pub struct WallChain {
    pub id: usize,
    /// The chain's own two endpoints. These are the ONLY draggable points;
    /// every piece boundary in between is derived from `pieces`/`joints`
    /// fractions, not directly user-editable (no endpoint marker there --
    /// the renderer/tool should only draw/hit-test endpoint markers at
    /// `start` and `end`, never at an internal piece boundary).
    pub start: Vec2D,
    pub end: Vec2D,
    /// len == joints.len() + 1
    pub pieces: Vec<ChainPiece>,
    /// joints[i] sits between pieces[i] and pieces[i + 1]
    pub joints: Vec<Joint>,
}

impl WallChain {
    /// A brand-new, unsplit wall: one piece spanning the whole chain.
    pub fn new(id: usize, start: Vec2D, end: Vec2D, line_id: usize) -> Self {
        Self {
            id,
            start,
            end,
            pieces: vec![ChainPiece {
                line_id,
                t_start: 0.0,
                t_end: 1.0,
            }],
            joints: Vec::new(),
        }
    }

    /// Point at fraction `t` (0..1) along the CHAIN's own endpoints (linear
    /// interpolation of `start`..`end`) -- used to place both piece
    /// endpoints and joint (valve/gap) endpoints in world space.
    pub fn point_at(&self, t: f32) -> Vec2D {
        Vec2D::new(
            self.start.x + (self.end.x - self.start.x) * t,
            self.start.y + (self.end.y - self.start.y) * t,
        )
    }

    /// Ids of every valve cut into this chain.
    pub fn valve_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.joints.iter().filter_map(|joint| match joint {
            Joint::Valve { valve_id, .. } => Some(*valve_id),
            Joint::Gap { .. } => None,
        })
    }

    /// True if `line_id` is one of this chain's pieces.
    pub fn contains_piece(&self, line_id: usize) -> bool {
        self.pieces.iter().any(|p| p.line_id == line_id)
    }
}

/// Owns every `WallChain`. Mirrors the add/find/find_mut/iterate shape of
/// the existing `LineSegmentCollection`/`ValveCollection` so it fits the
/// same patterns used elsewhere in `SimulationWorld`.
#[derive(Debug, Clone, Default)]
pub struct WallChainCollection {
    chains: Vec<WallChain>,
    next_id: OperatorIdCounter,
}

impl WallChainCollection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a brand-new, unsplit chain for a freshly-created wall piece.
    pub fn add(&mut self, start: Vec2D, end: Vec2D, line_id: usize) -> usize {
        let id = self.next_id.next_id();
        self.chains.push(WallChain::new(id, start, end, line_id));
        id
    }

    pub fn find(&self, id: usize) -> Option<&WallChain> {
        self.chains.iter().find(|c| c.id == id)
    }

    pub fn find_mut(&mut self, id: usize) -> Option<&mut WallChain> {
        self.chains.iter_mut().find(|c| c.id == id)
    }

    pub fn chains(&self) -> impl Iterator<Item = &WallChain> {
        self.chains.iter()
    }

    /// Remove chain `id`, returning it so the caller can drop its pieces and
    /// valves from their own collections.
    pub fn remove(&mut self, id: usize) -> Option<WallChain> {
        let index = self.chains.iter().position(|c| c.id == id)?;
        Some(self.chains.remove(index))
    }

    /// Mutable twin of `find_valve_joint`.
    pub fn find_valve_joint_mut(&mut self, valve_id: usize) -> Option<(&mut WallChain, usize)> {
        self.chains.iter_mut().find_map(|chain| {
            chain
                .joints
                .iter()
                .position(
                    |joint| matches!(joint, Joint::Valve { valve_id: id, .. } if *id == valve_id),
                )
                .map(|index| (chain, index))
        })
    }

    /// Find the chain that owns piece `line_id`, if any. Every piece should
    /// belong to exactly one chain; `None` means the id is stale or this
    /// piece predates the chain model (shouldn't happen once fully wired up).
    pub fn find_containing_piece(&self, line_id: usize) -> Option<&WallChain> {
        self.chains.iter().find(|c| c.contains_piece(line_id))
    }

    pub fn find_containing_piece_mut(&mut self, line_id: usize) -> Option<&mut WallChain> {
        self.chains.iter_mut().find(|c| c.contains_piece(line_id))
    }

    /// The chain holding valve `valve_id` and the index of its joint, so
    /// the flanking pieces are `pieces[index]` and `pieces[index + 1]`.
    /// `None` if no chain has a `Joint::Valve` with that id.
    pub fn find_valve_joint(&self, valve_id: usize) -> Option<(&WallChain, usize)> {
        self.chains.iter().find_map(|chain| {
            chain
                .joints
                .iter()
                .position(
                    |joint| matches!(joint, Joint::Valve { valve_id: id, .. } if *id == valve_id),
                )
                .map(|index| (chain, index))
        })
    }

    /// World-space endpoints of valve `valve_id`, derived from its owning
    /// chain. `None` if no chain has a `Joint::Valve` with that id.
    pub fn valve_span(&self, valve_id: usize) -> Option<(Vec2D, Vec2D)> {
        let (chain, index) = self.find_valve_joint(valve_id)?;
        let joint = &chain.joints[index];
        Some((
            chain.point_at(joint.t_start()),
            chain.point_at(joint.t_end()),
        ))
    }
}

/// A restored collection: ids are kept, new ids continue after the largest.
impl From<Vec<WallChain>> for WallChainCollection {
    fn from(chains: Vec<WallChain>) -> Self {
        Self {
            next_id: OperatorIdCounter::after(chains.iter().map(|item| item.id)),
            chains,
        }
    }
}

impl OperatorCollection for WallChainCollection {
    fn clear(&mut self) {
        self.chains.clear();
        self.next_id.reset();
    }

    /// Only the chains' own endpoints are remapped: pieces are
    /// `SimLineSegment`s (remapped by `LineSegmentCollection`) and valve
    /// geometry is fractional, so both follow automatically.
    fn rescale(&mut self, old_rect: &Rect, new_rect: &Rect) {
        if is_degenerate(old_rect) {
            return;
        }
        for chain in &mut self.chains {
            chain.start = remap_point(chain.start, old_rect, new_rect);
            chain.end = remap_point(chain.end, old_rect, new_rect);
        }
    }
}

/// A reference to any operator object in the simulation world: what a tool
/// reports as hovered (for highlighting) and what `Delete` removes. Flat and
/// `Copy` so it is cheap to store, compare and send to the physics thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectRef {
    /// A whole wall (`WallChain` id), including every piece and valve on it.
    Wall(usize),
    Valve(usize),
    SourceSphere(usize),
    Thermostat(usize),
}
