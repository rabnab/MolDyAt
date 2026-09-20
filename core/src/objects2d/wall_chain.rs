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
//! see `collect_solid_obstacles` in line_segments.rs, which flattens every
//! piece plus every CLOSED valve joint into plain capsules once, up front,
//! instead of looking up valve openness per contact (which is where the
//! earlier bugs kept coming from).

use crate::objects2d::operator::{OperatorCollection, is_degenerate, remap_point};
use crate::objects2d::{Rect, Vec2D};

/// One solid piece of a chain. `line_id` is the id of the actual
/// `SimLineSegment` holding its real start/end/thickness (each piece is
/// independently adjustable, e.g. via Ctrl+scroll thickness). `t_start`/
/// `t_end` are the piece's FIXED fraction span [0, 1] along the CHAIN's own
/// endpoints -- set once, at split time, and used to recompute the piece's
/// actual world-space start/end whenever the chain's own endpoints move
/// (`WallChain::retarget`), so every piece and joint stretches
/// proportionally together.
#[derive(Debug, Clone, Copy)]
pub struct ChainPiece {
    pub line_id: usize,
    pub t_start: f32,
    pub t_end: f32,
}

/// What sits between two adjacent pieces of a chain.
#[derive(Debug, Clone, Copy)]
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
    /// around it. (Deletion itself is not yet implemented -- see
    /// `Selection::delete_selected` -- this variant is the landing spot
    /// for it.)
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
#[derive(Debug, Clone)]
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
    next_id: usize,
}

impl WallChainCollection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a brand-new, unsplit chain for a freshly-created wall piece.
    pub fn add(&mut self, start: Vec2D, end: Vec2D, line_id: usize) -> usize {
        let id = self.next_id;
        self.next_id += 1;
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

impl OperatorCollection for WallChainCollection {
    fn clear(&mut self) {
        self.chains.clear();
        self.next_id = 0;
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

// ─────────────────────────────────────────────────────────────────────────
// General object selection / highlighting
// ─────────────────────────────────────────────────────────────────────────

/// A reference to any interactive object in the simulation world. Used for
/// hover/selection highlighting now, and as the landing spot for a future
/// "delete selected" action -- deliberately flat and `Copy` so it's cheap to
/// store and compare, and deliberately NOT tied to any one object type's
/// collection, so a single `Selection` can represent "the user is pointing
/// at a wall piece" just as well as "...a valve" or "...a thermostat" without
/// each tool inventing its own hover-tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectRef {
    /// A single wall piece (`SimLineSegment` id) -- NOT a whole chain. Most
    /// interactions (thickness, endpoint drag) are already per-piece; only
    /// "drag the wall's own endpoint" and (once implemented) "delete this
    /// wall" need to resolve the owning chain first.
    WallPiece(usize),
    Valve(usize),
    SourceSphere(usize),
    Thermostat(usize),
}

/// What's currently hovered/selected. Lives once on `SimulationWorld` so
/// every tool and the renderer share a single source of truth instead of
/// each tool tracking its own ad hoc hover state, and so highlighting a
/// valve (say) doesn't require the valve tool specifically to be active.
#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    pub hovered: Option<ObjectRef>,
    pub selected: Option<ObjectRef>,
}
