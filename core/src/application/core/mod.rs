pub(crate) const FPS_GOAL: u32 = 60;

use crate::application::game_logic::GameLogicData;
use crate::application::renderer_snapshot::RendererSnapshot;
use crate::application::sim_stats::SimStats;
use crate::application::thermo_field::ThermoFieldHistory;
use crate::objects2d::ball_cell::cell_grid::CellGrid;
use crate::objects2d::ball_impl::{Ball2D, BallPhysics, ForceResult};
use crate::objects2d::wall_chain::WallChainCollection;
use crate::objects2d::{
    LineSegmentCollection, Rect, SourceSphereCollection, ThermostatCollection, ValveCollection,
    Vec2D, VerboseInfoMode,
};
use arc_swap::ArcSwapOption;
use std::sync::Arc;

/// Persistent simulation domain state.
///
/// Owns the particle data, physics engine, spatial structures, and
/// operator collections (line segments, valves, source spheres,
/// thermostats). This is the single source of truth for the physical
/// state of the simulation world.
///
/// Stepping and update orchestration live in [`SimulationEngine`];
/// this struct is a pure state holder.
pub struct SimulationWorld {
    /// World bounds for physics simulation
    pub(crate) world_rect: Rect,
    pub(crate) physics: BallPhysics,

    pub(crate) sim: SimStats,
    pub(crate) balls: Vec<Ball2D>,
    pub(crate) cell_grid: CellGrid,
    pub(crate) real_data: bool,
    pub(crate) step_by_step: bool,
    pub(crate) show_timings: bool,
    pub(crate) show_ball_stats: bool,
    pub(crate) step_count: usize,

    pub(crate) was_paused_for_resize: bool,
    /// Override for speed limit (None = use default calculation, Some(f32::INFINITY) = disable)
    pub(crate) speed_limit_override: Option<f32>,
    /// Cached force results from last physics step for reuse in rendering
    pub(crate) cached_force_results: Vec<ForceResult>,
    /// Rolling-window averager for per-cell T/P fields
    pub(crate) thermo_history: ThermoFieldHistory,
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

impl SimulationWorld {
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
/// coordination state: `game_data`
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

    game_data: GameLogicData,

    runtime: PhysicsRuntimeState,
}

/// Runtime-only state owned by the physics thread loop.
pub(crate) struct PhysicsRuntimeState {
    /// ms to wait between simulation steps
    wait_time: u64,
    /// Snapshot slot for lock-free communication with renderer
    snapshot_slot: Arc<ArcSwapOption<RendererSnapshot>>,
    /// Actual physics FPS for display (updated and read only by physics thread)
    physics_fps: f32,
}

pub struct StatsAccumulator {
    pub(crate) pot: f32,
    pub(crate) kin: f32,
    pub(crate) v_com: Vec2D,
    sum_mass: f32,
    pub(crate) max_v: f32,
    pub(crate) max_id: usize,
}

mod accessors;
mod balls;
mod ctor;
mod line_segments;
mod object_deletion;
mod operators;
mod sim;
mod simulation_engine;
mod snapshot;
mod source_spheres;
mod thermostats;
mod valves;
mod verbose;

pub(crate) use simulation_engine::SimulationEngine;

#[cfg(test)]
mod tests;
