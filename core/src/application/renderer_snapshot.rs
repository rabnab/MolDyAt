// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Renderer Snapshot Module
//!
//! ARCHITECTURAL CONSTRAINT: This module must not depend on ggez.
//! All ggez-specific code belongs in src/application/ggez/.
//!
//! This module provides a framework-independent snapshot of the simulation state
//! for rendering. It contains only positions, colors (as RGBA arrays), and neighbor
//! information, using only standard Rust types.
//!
//! # Design: Array-of-Structures
//!
//! This module uses an **array-of-structures** design (single `Vec<BallRenderData>`)
//! rather than **structure-of-arrays** (separate vectors for each field).
//!
//! Rationale:
//! - **Type safety**: Impossible to have inconsistent vector lengths.
//! - **Simplicity**: Data for each ball is grouped together logically.
//! - **Access pattern**: Rendering typically needs all fields (position, color, radius)
//!   together for each ball, so SoA cache benefits don't apply.
//! - **Immutability**: Snapshots are immutable after creation, so no need for
//!   partial field updates that might benefit from SoA.

use crate::objects2d::ball_impl::{DEFAULT_SPECIES, SpeciesId, SpeciesTable};
use crate::objects2d::line_segment::valve_half_width;
use crate::objects2d::{SimLineSegment, SimSourceSphere, SimThermostat, Vec2D};
use std::sync::Arc;
use std::time::Instant;

use crate::application::scene_layers::SceneLayers;
use crate::application::thermo_field::ThermoGrid;

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
#[derive(Clone, Default)]
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
    /// centres.  The threshold is [`OPERATOR_PROXIMITY_THRESHOLD`] world
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

/// Specification for the visualization thermo grid layout.
///
/// Separate from `CellGridSpec` because the visualization grid dimensions
/// (based on ~2-ball occupancy) differ from the simulation CellGrid.
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

/// Rendering data for a single ball.
///
/// Contains all information needed to render one ball, including visual appearance
/// and neighbor relationships for visualization.
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
    /// NOT ggez::Color to maintain framework independence
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

    /// Whether to draw neighbor lines for this ball (verbose mode)
    pub show_neighbor_lines: bool,

    /// Whether this ball shows only main neighbors (stronger forces)
    pub show_main_neighbors_only: bool,

    /// Indices of this ball's neighbors in the snapshot with force magnitudes
    /// Each entry is (neighbor_index, force_magnitude)
    /// Used for visualizing neighbor relationships with color-coded force strength
    pub neighbors: Vec<(usize, f32)>,

    /// Additional fields can be added as needed for rendering
    pub update_counter: usize,
    pub tail: Vec<Vec2D>,
}

/// A snapshot of simulation state for rendering.
///
/// Contains only the data needed for visualization, using framework-independent types.
/// Uses array-of-structures design to guarantee consistency and simplify ball data access.
#[derive(Clone)]
pub struct RendererSnapshot {
    /// All balls in the simulation with their rendering data
    pub balls: Vec<BallRenderData>,

    /// Simulation step counter (for display)
    pub simulation_step: u64,

    /// Which world this snapshot shows: changes whenever the world is
    /// replaced (a load), whose step count then starts again.
    pub world_generation: u64,

    /// Scene layers this snapshot holds. Without `BALLS` it carries no ball
    /// render data, without `THERMO` no thermo grids, without `READOUTS` no
    /// fresh thermostat readings (rendering off, F2).
    pub layers: SceneLayers,

    /// Kinetic energy of the system
    pub kinetic_energy: f32,

    /// Potential energy of the system
    pub potential_energy: f32,

    /// Simulation time
    pub time: f32,

    /// Status text for display (ball count, FPS, etc.)
    pub status_text: Option<String>,

    /// Cell grid world bounds (for rendering the grid)
    pub cell_grid_bounds: crate::objects2d::Rect,

    /// Species of new balls, for the cursor preview
    pub cursor_species: SpeciesId,

    /// The world's species table: names, colours and drawing radii of
    /// `BallRenderData::species` and `cursor_species`.
    pub species: Arc<SpeciesTable>,

    /// Equilibrium and cutoff ring radii of `cursor_species` (see
    /// [`BallRenderData::equilibrium_radius`]), for the cursor preview
    pub cursor_ring_radii: [f32; 2],

    /// Current gravity preset as a multiple of Earth gravity (0, 1, or 2).
    pub gravity_force_multiple: u8,

    /// Timestamp for debugging
    pub created_at: Instant,

    /// Grid specification for drawing cell backgrounds
    pub grid_spec: CellGridSpec,

    /// Actual physics thread step rate (steps per second)
    /// This is the true simulation performance, independent of rendering
    pub physics_fps: f32,

    /// Per-cell temperature field (row-major flat grid, NaN for underpopulated cells).
    /// Empty grid (0×0) until at least one step of data is available.
    pub cell_temperature: ThermoGrid,

    /// Per-cell pressure field (row-major flat grid, NaN for underpopulated cells).
    /// Empty grid (0×0) until at least one step of data is available.
    pub cell_pressure: ThermoGrid,

    /// Grid layout for mapping thermo cell indices to world coordinates.
    /// `None` until at least one step of data is available.
    pub thermo_grid_spec: Option<ThermoGridSpec>,

    /// All operator data (line segments, valves, future types).
    pub operators: OperatorSnapshot,
}

impl RendererSnapshot {
    /// Create new empty snapshot with default values.
    /// Sets timestamp to current instant for debugging snapshot age.
    pub fn new() -> Self {
        Self {
            balls: Vec::new(),
            simulation_step: 0,
            world_generation: 0,
            layers: SceneLayers::ALL,
            kinetic_energy: 0.0,
            potential_energy: 0.0,
            time: 0.0,
            status_text: None,
            cell_grid_bounds: crate::objects2d::Rect::new(0.0, 0.0, 0.0, 0.0),
            cursor_species: DEFAULT_SPECIES,
            species: SpeciesTable::builtin_shared(),
            cursor_ring_radii: DEFAULT_RING_RADII,
            gravity_force_multiple: 0,
            created_at: Instant::now(),
            grid_spec: CellGridSpec {
                rows: 0,
                cols: 0,
                cell_width: 0.0,
                cell_height: 0.0,
                origin_x: 0.0,
                origin_y: 0.0,
            },
            physics_fps: 0.0,
            cell_temperature: ThermoGrid::filled(0, 0, 0.0),
            cell_pressure: ThermoGrid::filled(0, 0, 0.0),
            thermo_grid_spec: None,
            operators: OperatorSnapshot::default(),
        }
    }

    /// Create snapshot with pre-allocated ball capacity to avoid reallocation.
    ///
    /// # Arguments
    /// * `capacity` - Number of balls to pre-allocate space for
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            balls: Vec::with_capacity(capacity),
            simulation_step: 0,
            world_generation: 0,
            layers: SceneLayers::ALL,
            kinetic_energy: 0.0,
            potential_energy: 0.0,
            time: 0.0,
            status_text: None,
            cell_grid_bounds: crate::objects2d::Rect::new(0.0, 0.0, 0.0, 0.0),
            cursor_species: DEFAULT_SPECIES,
            species: SpeciesTable::builtin_shared(),
            cursor_ring_radii: DEFAULT_RING_RADII,
            gravity_force_multiple: 0,
            created_at: Instant::now(),
            grid_spec: CellGridSpec {
                rows: 0,
                cols: 0,
                cell_width: 0.0,
                cell_height: 0.0,
                origin_x: 0.0,
                origin_y: 0.0,
            },
            physics_fps: 0.0,
            cell_temperature: ThermoGrid::filled(0, 0, 0.0),
            cell_pressure: ThermoGrid::filled(0, 0, 0.0),
            thermo_grid_spec: None,
            operators: OperatorSnapshot::default(),
        }
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
        self.kinetic_energy + self.potential_energy
    }
}

impl Default for RendererSnapshot {
    fn default() -> Self {
        Self::new()
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
        assert_eq!(snapshot.gravity_force_multiple, 0);
    }

    #[test]
    fn test_snapshot_with_capacity() {
        let snapshot = RendererSnapshot::with_capacity(100);
        assert_eq!(snapshot.len(), 0);
        assert_eq!(snapshot.balls.capacity(), 100);
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
