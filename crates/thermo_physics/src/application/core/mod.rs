// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod accessors;
mod balls;
mod ctor;
mod line_segments;
mod object_deletion;
mod operators;
mod placement;
mod runtime_options;
mod sim;
mod simulation_engine;
mod source_spheres;
mod step_scratch;
mod thermostats;
mod valves;
pub use valves::{ValveSpanChange, valve_span_problem};
mod verbose;

pub(crate) use runtime_options::RuntimeOptions;
pub(crate) use simulation_engine::SimulationEngine;

use crate::application::StepConfig;
use crate::application::persistence::SaveStore;
use crate::application::phase_timings::PhaseTimings;
use crate::application::sim_config::{SpeciesConfig, WorldConfig};
use crate::application::sim_stats::SimStats;
use crate::application::thermo_field::ThermoFieldHistory;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use crate::objects2d::ball_impl::simd_forces::KernelPoint;
use crate::objects2d::ball_impl::{
    Ball2D, BallPhysics, ForceResult, SpeciesId, SpeciesSet, SpeciesTable,
};
use crate::objects2d::wall_chain::WallChainCollection;
use crate::objects2d::{
    LineSegmentCollection, Rect, SourceSphereCollection, ThermostatCollection, ValveCollection,
    Vec2D, VerboseInfoMode,
};
use std::sync::Arc;
use step_scratch::StepScratch;

/// Persistent simulation domain state.
///
/// Owns the particle data, physics engine, spatial structures, and
/// operator collections (line segments, valves, source spheres,
/// thermostats). This is the single source of truth for the physical
/// state of the simulation world.
///
/// Stepping and update orchestration live in `SimulationEngine`;
/// this struct is a pure state holder.
pub struct SimulationWorld {
    /// World bounds for physics simulation
    pub(crate) world_rect: Rect,
    pub(crate) physics: BallPhysics,
    /// The `[[species]]` list the table was built from, empty for the
    /// built-in library: what `MainContent::config` and saves write back.
    pub(crate) species_config: Vec<SpeciesConfig>,
    /// The `[world]` section the world was created with (a loaded save's
    /// own). Only recorded: the box is `world_rect`, which a window resize
    /// changes.
    pub(crate) world_config: WorldConfig,

    pub(crate) sim: SimStats,
    pub(crate) balls: Vec<Ball2D>,
    /// Per-ball position and species for the force kernel, refilled every step
    /// (same indices as `balls`). Scratch data: valid only inside `update_sim`,
    /// from `update_old_positions` to `compute_forces`. Balls are removed and
    /// added between steps, so read `balls` everywhere else.
    pub(crate) kernel_points: Vec<KernelPoint>,
    /// Buffers the step reuses instead of allocating (see `StepScratch`).
    pub(crate) scratch: StepScratch,
    pub(crate) cell_grid: CellGrid,
    /// Species of the balls in the world (it may also hold species whose
    /// last ball is gone, until the next `rebuild_cell_grid`). The cell size
    /// follows the largest pair list radius among them.
    pub(crate) species_present: SpeciesSet,
    pub(crate) step_count: usize,

    /// Speed limit as a share of the shorter world side (`SimConfig`).
    pub(crate) speed_limit_factor: f32,
    /// Override for speed limit (None = use default calculation, Some(f32::INFINITY) = disable)
    pub(crate) speed_limit_override: Option<f32>,
    /// Cached force results from last physics step for reuse in rendering
    pub(crate) cached_force_results: Vec<ForceResult>,
    /// Rolling-window averager for per-cell T/P fields
    pub(crate) thermo_history: ThermoFieldHistory,
    /// Time per step phase, for headless perf runs (#221).
    pub(crate) phase_timings: PhaseTimings,
    /// Line segment barriers placed by the operator panel
    pub(crate) line_segments: LineSegmentCollection,
    /// Valve regions placed on top of line segments.
    pub(crate) valve_segments: ValveCollection,
    /// Ties `line_segments` pieces and `valve_segments` valves together into
    /// user-drawn walls that may be split by valves. See
    /// `objects2d::wall_chain` for the full model. `line_segments` and
    /// `valve_segments` remain the geometry/state owners (a piece's own
    /// start/end/thickness; a valve's own is_closed/thickness); this is the
    /// structural layer that says which pieces and valves belong to the same
    /// wall, and in what order.
    pub(crate) wall_chains: WallChainCollection,
    /// Source spheres that emit balls into the simulation.
    pub(crate) source_spheres: SourceSphereCollection,
    /// Nosé-Hoover thermostats that control local regions.
    pub(crate) thermostats: ThermostatCollection,
    /// Wall being drawn or dragged right now; collides like a real wall.
    pub(crate) ghost_wall: Option<crate::objects2d::line_segment::GhostWall>,
    /// Deliberate physics faults so drift tests can prove they catch real
    /// regressions end to end. Test builds only.
    #[cfg(test)]
    pub(crate) test_faults: TestFaults,
}

/// Mutable access for the tests of dependent crates (feature
/// `test-support`); never compiled into a normal build.
#[cfg(any(test, feature = "test-support"))]
impl SimulationWorld {
    pub fn balls_mut(&mut self) -> &mut Vec<Ball2D> {
        &mut self.balls
    }

    pub fn sim_mut(&mut self) -> &mut SimStats {
        &mut self.sim
    }

    pub fn physics_mut(&mut self) -> &mut BallPhysics {
        &mut self.physics
    }

    pub fn thermostats_mut(&mut self) -> &mut ThermostatCollection {
        &mut self.thermostats
    }

    pub fn set_step_count(&mut self, step_count: usize) {
        self.step_count = step_count;
    }
}

/// Read-only views of the world for the runtime and the UI (#309): the
/// fields stay private to `thermo_physics`, so only its own code can break
/// invariants such as the index contract.
impl SimulationWorld {
    /// World bounds of the physics.
    pub fn world_rect(&self) -> Rect {
        self.world_rect
    }

    /// Pair potentials, species and run constants.
    pub fn physics(&self) -> &BallPhysics {
        &self.physics
    }

    /// Energies, temperature, wall momentum and other step statistics.
    pub fn sim(&self) -> &SimStats {
        &self.sim
    }

    /// The balls; cell and neighbour lists index into this vector.
    pub fn balls(&self) -> &[Ball2D] {
        &self.balls
    }

    /// The physics cell grid.
    pub fn cell_grid(&self) -> &CellGrid {
        &self.cell_grid
    }

    /// Steps run so far.
    pub fn step_count(&self) -> usize {
        self.step_count
    }

    /// The last step's force results, in ball order (empty before the first step).
    pub fn cached_force_results(&self) -> &[ForceResult] {
        &self.cached_force_results
    }

    /// Rolling window of the per-cell temperature and pressure fields.
    pub fn thermo_history(&self) -> &ThermoFieldHistory {
        &self.thermo_history
    }

    /// Wall pieces.
    pub fn line_segments(&self) -> &LineSegmentCollection {
        &self.line_segments
    }

    /// Valves.
    pub fn valve_segments(&self) -> &ValveCollection {
        &self.valve_segments
    }

    /// Walls, each a chain of pieces and valves.
    pub fn wall_chains(&self) -> &WallChainCollection {
        &self.wall_chains
    }

    /// Source spheres.
    pub fn source_spheres(&self) -> &SourceSphereCollection {
        &self.source_spheres
    }

    /// Thermostats.
    pub fn thermostats(&self) -> &ThermostatCollection {
        &self.thermostats
    }
}

impl SimulationWorld {
    /// The species balls of this world can have (`SimConfig::species_table`).
    /// Owned by the physics, so table and physics cannot diverge; an `Arc`
    /// because the ball factory and every snapshot for the render thread
    /// share it.
    pub fn species(&self) -> &Arc<SpeciesTable> {
        self.physics.species()
    }

    #[inline]
    pub(crate) fn neighbor_movement_fault_scale(&self) -> f32 {
        #[cfg(test)]
        {
            self.test_faults.neighbor_movement_scale
        }
        #[cfg(not(test))]
        {
            1.0
        }
    }

    #[inline]
    pub(crate) fn wall_restitution_fault(&self) -> f32 {
        #[cfg(test)]
        {
            self.test_faults.wall_restitution
        }
        #[cfg(not(test))]
        {
            1.0
        }
    }
}

/// Physics-thread owner that pairs a [`SimulationWorld`] with runtime bookkeeping.
///
/// `MainContent` is exclusively owned by the physics thread. It keeps the
/// simulation domain in `world` and holds the remaining thread-owned
/// coordination state: `step_config`
/// (time-step and ball-factory settings) plus the dedicated
/// [`PhysicsRuntimeState`] for sleeping, snapshot publication, and
/// rolling physics-FPS measurement.
///
/// Stale frame-render coordination that previously lived here (dead
/// `start_frame`/`end_frame` methods and the `update_hood_at_frame_start`
/// scratch field they relied on, as well as the `frames_since_draw` and
/// `update_hood_cnt` tracking in [`SimStats`]) has been removed — frame
/// orchestration now lives in the renderer orchestrators and [`SimStats`]
/// owns only actively-consumed physics metrics.
pub struct MainContent {
    world: SimulationWorld,

    step_config: StepConfig,

    /// Display and stepping options (keys `D`, `S`, `F4`, `B`); not part of
    /// the world, so a load keeps them.
    options: RuntimeOptions,

    runtime: PhysicsRuntimeState,
}

/// Runtime-only state owned by the physics thread loop.
pub struct PhysicsRuntimeState {
    /// ms to wait between simulation steps
    wait_time: u64,
    /// Actual physics FPS for display (updated and read only by physics thread)
    physics_fps: f32,
    /// Where Ctrl+S / Ctrl+O save and load; `None` disables both.
    save_store: Option<SaveStore>,
    /// Bumped whenever the world is replaced (see `RendererSnapshot`).
    world_generation: u64,
    /// Least time between two published snapshots, in microseconds
    /// (`SimConfig::display`).
    snapshot_interval_us: u64,
    /// The simulation was paused because the window is being resized; it
    /// resumes when the resize ends (`unpause_after_resize`).
    paused_for_resize: bool,
}

pub(crate) struct StatsAccumulator {
    pub(crate) pot: f32,
    pub(crate) kin: f32,
    pub(crate) v_com: Vec2D,
    sum_mass: f32,
    pub(crate) max_v: f32,
    pub(crate) max_id: usize,
}

#[cfg(test)]
mod tests;
/// Test-only fault injection knobs; the defaults leave physics unchanged.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct TestFaults {
    /// Multiplies the Verlet movement threshold (> 1 makes lists go stale).
    pub(crate) neighbor_movement_scale: f32,
    /// Multiplies the reflected wall velocity (> 1 injects energy).
    pub(crate) wall_restitution: f32,
}

#[cfg(test)]
impl Default for TestFaults {
    fn default() -> Self {
        Self {
            neighbor_movement_scale: 1.0,
            wall_restitution: 1.0,
        }
    }
}
