// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Renderer Snapshot Module
//!
//! This module provides a framework-independent snapshot of the simulation state
//! for rendering, in sections (#341): [`FrameInfo`] (step, time, energies),
//! [`BallBuffer`] (balls and species), [`OperatorSnapshot`], [`FieldSnapshot`]
//! (cell grid and thermo fields) and an optional [`DebugSnapshot`] (neighbour
//! lines of verbose balls).
//!
//! # Design: Array-of-Structures
//!
//! The balls are one `Vec<BallRenderData>` rather than separate vectors per
//! field: lengths cannot disagree, and rendering reads every field of a ball
//! together. `BallRenderData` holds no heap data, so a reused snapshot
//! refills the vector without allocating.

use crate::application::log_util::SpeciesCounts;
use crate::application::sim_stats::StatusNumbers;
use crate::objects2d::ball_impl::{DEFAULT_SPECIES, SpeciesId, SpeciesSet, SpeciesTable};
use crate::objects2d::line_segment::valve_half_width;
use crate::objects2d::{Rect, SimLineSegment, SimSourceSphere, SimThermostat, Vec2D};
use std::sync::Arc;
use std::time::Instant;

use crate::application::scene_layers::SceneLayers;
use crate::application::thermo_field::CellFields;

/// A valve resolved to world space (derived from its wall chain).
///
/// Consumed by the valve renderer and by valve hit-testing, so neither has
/// to know how walls are split into chains.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValveSpan {
    pub valve_id: usize,
    /// World-space start of the valve centre line.
    pub p0: Vec2D,
    /// World-space end of the valve centre line.
    pub p1: Vec2D,
    pub thickness: f32,
    /// Whether the valve currently blocks passage.
    pub is_closed: bool,
}

impl ValveSpan {
    /// Midpoint of the valve centre line.
    pub fn center(&self) -> Vec2D {
        Vec2D::new(0.5 * (self.p0.x + self.p1.x), 0.5 * (self.p0.y + self.p1.y))
    }

    /// Half-width of the rectangle the valve is drawn and hit-tested as.
    pub fn half_width(&self, world_extent: f32) -> f32 {
        valve_half_width(self.thickness, world_extent)
    }

    /// True if `pos` lies inside the valve's rectangle (oriented along it).
    pub fn contains(&self, pos: Vec2D, world_extent: f32) -> bool {
        let (a, b) = (self.p0, self.p1);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            return false;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (px, py) = (pos.x - a.x, pos.y - a.y);
        let along = px * ux + py * uy;
        let across = -px * uy + py * ux;
        (0.0..=len).contains(&along) && across.abs() <= self.half_width(world_extent)
    }
}

/// One user-drawn wall as the renderer and tools see it: its own two
/// endpoints (the only draggable points) and the pieces it is made of.
#[derive(Debug, Clone, PartialEq)]
pub struct WallSnapshot {
    pub chain_id: usize,
    pub start: Vec2D,
    pub end: Vec2D,
    /// Ids of this wall's pieces in `OperatorSnapshot::line_segments`.
    pub piece_ids: Vec<usize>,
    /// Ids of the valves cut into this wall (see `valve_spans`).
    pub valve_ids: Vec<usize>,
}

/// All operator data needed for rendering, bundled in one struct.
///
/// New operator types add a field here instead of expanding
/// `RendererSnapshot` directly.  Population is driven by
/// `create_snapshot()` which iterates the operator collections.
#[derive(Clone, Debug, Default)]
pub struct OperatorSnapshot {
    /// Wall pieces (a wall split by valves is several pieces).
    pub line_segments: Vec<SimLineSegment>,
    /// Every wall with its two draggable endpoints. Internal piece
    /// boundaries next to valves are deliberately not endpoints.
    pub walls: Vec<WallSnapshot>,
    /// Valves resolved to world space.
    pub valve_spans: Vec<ValveSpan>,
    /// Source spheres that emit balls into the simulation.
    pub source_spheres: Vec<SimSourceSphere>,
    /// Thermostats that control local spherical regions.
    pub thermostats: Vec<SimThermostat>,
}

/// Distance threshold (world units) used by [`OperatorSnapshot::is_near_any_operator`]
/// to suppress ball placement near operators.
const OPERATOR_PROXIMITY_THRESHOLD: f32 = 1.5;

impl OperatorSnapshot {
    /// The wall that piece `line_id` belongs to.
    pub fn wall_of_piece(&self, line_id: usize) -> Option<&WallSnapshot> {
        self.walls.iter().find(|w| w.piece_ids.contains(&line_id))
    }

    /// The wall that valve `valve_id` is cut into.
    pub fn wall_of_valve(&self, valve_id: usize) -> Option<&WallSnapshot> {
        self.walls.iter().find(|w| w.valve_ids.contains(&valve_id))
    }

    /// The valve whose rectangle contains `pos`; if several overlap, the one
    /// whose centre is closest.
    pub fn valve_at(&self, pos: Vec2D, world_extent: f32) -> Option<usize> {
        self.valve_spans
            .iter()
            .filter(|v| v.contains(pos, world_extent))
            .min_by(|a, b| {
                pos.distance_squared(&a.center())
                    .total_cmp(&pos.distance_squared(&b.center()))
            })
            .map(|v| v.valve_id)
    }

    /// Returns `true` when `pos` is close to any placed operator.
    ///
    /// Checked operators: line segments, valve midpoints, and source sphere
    /// centres.  The threshold is `OPERATOR_PROXIMITY_THRESHOLD` world
    /// units so that a Ctrl-release ball placement does not interfere with
    /// operator interaction zones.
    pub fn is_near_any_operator(&self, pos: Vec2D) -> bool {
        let threshold_sq = OPERATOR_PROXIMITY_THRESHOLD * OPERATOR_PROXIMITY_THRESHOLD;

        // Line segments — closest-point distance.
        for seg in &self.line_segments {
            let closest = seg.closest_point(&pos);
            if pos.distance_squared(&closest) <= threshold_sq {
                return true;
            }
        }

        // Valves — midpoint of the valve span.
        for valve in &self.valve_spans {
            if pos.distance_squared(&valve.center()) <= threshold_sq {
                return true;
            }
        }

        // Source spheres — centre distance.
        for sphere in &self.source_spheres {
            if pos.distance_squared(&sphere.center) <= threshold_sq {
                return true;
            }
        }

        // Thermostats — centre distance.
        for thermostat in &self.thermostats {
            if pos.distance_squared(&thermostat.position) <= threshold_sq {
                return true;
            }
        }

        false
    }
}

/// Lightweight specification for cell grid dimensions and layout.
/// Used by renderer to draw cell background without cloning the full CellGrid.
#[derive(Debug, Clone)]
pub struct CellGridSpec {
    /// Number of rows in the grid
    pub rows: usize,
    /// Number of columns in the grid
    pub cols: usize,
    /// Width of each cell in world coordinates
    pub cell_width: f32,
    /// Height of each cell in world coordinates
    pub cell_height: f32,
    /// X-coordinate of the top-left corner of the grid in world coordinates
    pub origin_x: f32,
    /// Y-coordinate of the top-left corner of the grid in world coordinates
    pub origin_y: f32,
}

/// Specification for the thermo field grid layout.
///
/// Separate from `CellGridSpec`: the thermo field has a grid of its own,
/// sized from the world with cells of about `THERMO_CELL_SIZE` (#290), while
/// the physics cells follow the largest species present.
#[derive(Debug, Clone)]
pub struct ThermoGridSpec {
    /// Number of rows in the visualization grid
    pub rows: usize,
    /// Number of columns in the visualization grid
    pub cols: usize,
    /// Width of each cell in world coordinates
    pub cell_width: f32,
    /// Height of each cell in world coordinates
    pub cell_height: f32,
    /// World x of the grid's minimum-x corner (min of rect x-extent)
    pub origin_x: f32,
    /// World y of the grid's minimum-y corner (min of rect y-extent)
    pub origin_y: f32,
}

/// Rendering data for a single ball: what the ball pipeline, hit-testing,
/// headless output and export read. Debug data (neighbour lines) of verbose
/// balls is in [`DebugSnapshot`].
#[derive(Clone, Debug)]
pub struct BallRenderData {
    /// Ball ID for debugging and tracking
    pub id: usize,

    /// Particle type: index into the species table
    pub species: SpeciesId,

    /// Ball position in 2D space
    pub position: Vec2D,

    /// Ball velocity in 2D space
    pub velocity: Vec2D,

    /// Ball mass for export and physics inspection
    pub mass: f32,

    /// Ball color as RGBA [r, g, b, a] in range [0.0, 1.0]
    pub color: [f32; 4],

    /// `σ / 2` of the species: two balls touch (zero pair potential) when
    /// their centres are the sum of their contact radii apart
    pub contact_radius: f32,

    /// Radius the renderer draws (must be positive and finite)
    pub display_radius: f32,

    /// Half the equilibrium distance of a like pair, `2^(1/6)·σ / 2`: the
    /// rings of two balls touch at their pair's potential minimum
    pub equilibrium_radius: f32,

    /// Half the cutoff of a like pair: the rings of two balls touch where
    /// their pair stops interacting
    pub cutoff_radius: f32,
}

/// Step, time and the global numbers of the world: every snapshot has
/// them, whatever its layers (headless output and the metrics store read
/// them).
#[derive(Clone, Debug)]
pub struct FrameInfo {
    /// Simulation step counter (for display)
    pub simulation_step: u64,

    /// Which world this snapshot shows: changes whenever the world is
    /// replaced (a load), whose step count then starts again.
    pub world_generation: u64,

    /// Scene layers this snapshot holds. Without `BALLS` it carries no ball
    /// render data, without `THERMO` no thermo grids, without `READOUTS` no
    /// fresh thermostat readings (rendering off, F2).
    pub layers: SceneLayers,

    /// Simulation time
    pub time: f32,

    /// Kinetic energy of the system
    pub kinetic_energy: f32,

    /// Potential energy of the system
    pub potential_energy: f32,

    /// Magnitude of the momentum given to walls since the start of the run.
    pub wall_momentum: f32,

    /// Ball count per species index (see `SpeciesTable`), whatever the
    /// layers: the metrics store and the status lines read it.
    pub species_counts: SpeciesCounts,

    /// Current gravity preset as a multiple of Earth gravity (0, 1, or 2).
    pub gravity_force_multiple: u8,

    /// Actual physics thread step rate (steps per second)
    /// This is the true simulation performance, independent of rendering
    pub physics_fps: f32,

    /// The world rectangle (the cell grid's bounds).
    pub world_rect: Rect,

    /// The numbers of the status lines, `None` while ball stats are not
    /// shown (`B`). The UI formats them into text (#341).
    pub status: Option<StatusNumbers>,

    /// Timestamp for debugging
    pub created_at: Instant,
}

impl Default for FrameInfo {
    fn default() -> Self {
        Self {
            simulation_step: 0,
            world_generation: 0,
            layers: SceneLayers::ALL,
            time: 0.0,
            kinetic_energy: 0.0,
            potential_energy: 0.0,
            wall_momentum: 0.0,
            species_counts: [0; SpeciesSet::CAPACITY],
            gravity_force_multiple: 0,
            physics_fps: 0.0,
            world_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            status: None,
            created_at: Instant::now(),
        }
    }
}

impl FrameInfo {
    /// Returns the total energy (kinetic + potential)
    pub fn total_energy(&self) -> f32 {
        self.kinetic_energy + self.potential_energy
    }
}

/// The balls (the `BALLS` layer) and the species they are drawn with.
///
/// Derefs to the ball slice, so `snapshot.balls.len()`, `.iter()` and
/// indexing read the balls directly.
#[derive(Clone, Debug)]
pub struct BallBuffer {
    /// Render data of every ball, in the world's ball order (empty without
    /// the `BALLS` layer).
    pub data: Vec<BallRenderData>,

    /// The world's species table: names, colours and drawing radii of
    /// `BallRenderData::species` and `cursor_species`.
    pub species: Arc<SpeciesTable>,

    /// Species of new balls, for the cursor preview
    pub cursor_species: SpeciesId,

    /// Equilibrium and cutoff ring radii of `cursor_species` (see
    /// [`BallRenderData::equilibrium_radius`]), for the cursor preview
    pub cursor_ring_radii: [f32; 2],
}

impl Default for BallBuffer {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            species: SpeciesTable::builtin_shared(),
            cursor_species: DEFAULT_SPECIES,
            cursor_ring_radii: DEFAULT_RING_RADII,
        }
    }
}

impl std::ops::Deref for BallBuffer {
    type Target = [BallRenderData];

    fn deref(&self) -> &[BallRenderData] {
        &self.data
    }
}

impl<'a> IntoIterator for &'a BallBuffer {
    type Item = &'a BallRenderData;
    type IntoIter = std::slice::Iter<'a, BallRenderData>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}

/// Grids over the world: the physics cell grid (drawn as the grid overlay)
/// and the thermo fields (the `THERMO` layer).
#[derive(Clone, Debug)]
pub struct FieldSnapshot {
    /// Grid specification for drawing cell backgrounds
    pub cell_grid: CellGridSpec,

    /// Per-cell fields of the thermo grid, averaged over the rolling
    /// window: temperature, pressure, density and flow velocity (#342).
    /// Empty (0×0) until at least one sample is available, and without the
    /// `THERMO` layer.
    pub cells: CellFields,

    /// Grid layout for mapping thermo cell indices to world coordinates.
    /// `None` until at least one step of data is available.
    pub thermo_grid: Option<ThermoGridSpec>,
}

impl Default for FieldSnapshot {
    fn default() -> Self {
        Self {
            cell_grid: CellGridSpec {
                rows: 0,
                cols: 0,
                cell_width: 0.0,
                cell_height: 0.0,
                origin_x: 0.0,
                origin_y: 0.0,
            },
            cells: CellFields::empty(),
            thermo_grid: None,
        }
    }
}

/// Neighbour lines of one verbose ball (`V` / verbose mode).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NeighborLines {
    /// Index of the ball in [`BallBuffer::data`].
    pub ball: usize,
    /// Draw only the main neighbours (the strongest forces).
    pub main_only: bool,
    /// Indices of this ball's neighbours in [`BallBuffer::data`] with their
    /// force magnitudes, from the last physics step.
    pub neighbors: Vec<(usize, f32)>,
}

/// Debug data, only for verbose balls: absent (`None`) in a snapshot
/// without any, so a normal snapshot carries none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DebugSnapshot {
    /// Neighbour lines of every ball that shows them.
    pub neighbor_lines: Vec<NeighborLines>,
}

/// A snapshot of simulation state for rendering, in sections (#341): each
/// pipeline reads only its section, and a new visual adds a section or
/// derived data instead of a top-level field.
///
/// The physics thread refills the allocations of an earlier snapshot the
/// renderer has released (see `SnapshotPublisher`).
#[derive(Clone, Default)]
pub struct RendererSnapshot {
    /// Step, time, energies and the other global numbers.
    pub frame: FrameInfo,

    /// The balls and their species.
    pub balls: BallBuffer,

    /// All operator data (line segments, valves, future types).
    pub operators: OperatorSnapshot,

    /// The cell grid and the thermo fields.
    pub fields: FieldSnapshot,

    /// Neighbour lines of verbose balls, if any.
    pub debug: Option<DebugSnapshot>,
}

impl RendererSnapshot {
    /// Create new empty snapshot with default values.
    /// Sets timestamp to current instant for debugging snapshot age.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create snapshot with pre-allocated ball capacity to avoid reallocation.
    ///
    /// # Arguments
    /// * `capacity` - Number of balls to pre-allocate space for
    pub fn with_capacity(capacity: usize) -> Self {
        let mut snapshot = Self::default();
        snapshot.balls.data.reserve_exact(capacity);
        snapshot
    }

    /// Returns the number of balls in this snapshot
    pub fn len(&self) -> usize {
        self.balls.len()
    }

    /// Returns true if the snapshot contains no balls
    pub fn is_empty(&self) -> bool {
        self.balls.is_empty()
    }

    /// Returns the total energy (kinetic + potential)
    pub fn total_energy(&self) -> f32 {
        self.frame.total_energy()
    }
}

/// Colour of a ball whose species is not in the species table (white).
pub const UNKNOWN_SPECIES_COLOUR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Ring radii of `N2` with the default cutoff (`2^(1/6) / 2`, `4 / 2`), until
/// the first snapshot of a world arrives.
const DEFAULT_RING_RADII: [f32; 2] = [0.561_231, 2.0];

/// Apply special coloring for verbose or braking balls
/// Returns Some(color) if special coloring should override size class color
pub fn get_special_ball_color(is_verbose: bool, is_braking: bool) -> Option<[f32; 4]> {
    if is_verbose {
        Some([0.0, 0.0, 1.0, 1.0]) // Blue for verbose balls
    } else if is_braking {
        Some([1.0, 1.0, 0.0, 1.0]) // Yellow for braking balls
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects2d::ball_impl::SpeciesTable;

    #[test]
    fn test_snapshot_new() {
        let snapshot = RendererSnapshot::new();
        assert_eq!(snapshot.len(), 0);
        assert!(snapshot.is_empty());
        assert_eq!(snapshot.frame.gravity_force_multiple, 0);
        assert!(snapshot.debug.is_none());
    }

    #[test]
    fn test_snapshot_with_capacity() {
        let snapshot = RendererSnapshot::with_capacity(100);
        assert_eq!(snapshot.len(), 0);
        assert_eq!(snapshot.balls.data.capacity(), 100);
    }

    #[test]
    fn species_colours_are_valid_rgba() {
        let table = SpeciesTable::builtin();
        for color in (0..table.len())
            .filter_map(|i| table.id(i))
            .map(|id| table.species(id).colour)
            .chain([UNKNOWN_SPECIES_COLOUR])
        {
            assert!(color.iter().all(|c| (0.0..=1.0).contains(c)), "{color:?}");
        }
    }

    #[test]
    fn test_get_special_ball_color() {
        assert_eq!(
            get_special_ball_color(true, false),
            Some([0.0, 0.0, 1.0, 1.0])
        ); // Verbose -> Blue
        assert_eq!(
            get_special_ball_color(false, true),
            Some([1.0, 1.0, 0.0, 1.0])
        ); // Braking -> Yellow
        assert_eq!(
            get_special_ball_color(true, true),
            Some([0.0, 0.0, 1.0, 1.0])
        ); // Verbose takes priority
        assert_eq!(get_special_ball_color(false, false), None); // No special color
    }
}
