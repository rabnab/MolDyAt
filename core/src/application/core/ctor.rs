// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{BallPhysics, CellGrid, MainContent, SimStats, SimulationWorld};
use crate::application::AppResult;
use crate::application::StepConfig;
use crate::application::scene_layers::SceneLayers;
use crate::application::sim_config::{ResolvedSpecies, SimConfig};
use crate::application::thermo_field::ThermoFieldHistory;
use crate::graphics::renderer::{Margins, ViewportTransform};
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::{PhysicsConstants, SpeciesSet};
use arc_swap::ArcSwapOption;
use std::sync::Arc;

impl MainContent {
    /// Compute the default startup world rectangle for a window and viewport margins.
    ///
    /// This centralizes the initial world bootstrap policy so renderer entrypoints
    /// do not need to duplicate the world-size math.
    pub fn bootstrap_world_rect(window_width: u32, window_height: u32, margins: Margins) -> Rect {
        ViewportTransform::compute_world_bounds_from_window_size(
            window_width,
            window_height,
            margins,
        )
    }

    /// An empty world with the default [`SimConfig`].
    pub fn new(world_rect: Rect) -> AppResult<Self> {
        Self::with_config(world_rect, &SimConfig::default(), None)
    }

    /// [`Self::new`], with balls seeded from `seed` for reproducible runs.
    pub fn new_with_seed(world_rect: Rect, seed: u64) -> AppResult<Self> {
        Self::with_config(world_rect, &SimConfig::default(), Some(seed))
    }

    /// An empty world run with `config` (validated first); balls are seeded
    /// from `seed` if given.
    pub fn with_config(world_rect: Rect, config: &SimConfig, seed: Option<u64>) -> AppResult<Self> {
        let world = SimulationWorld::with_config(world_rect, config)?;
        let step_config = StepConfig::new(config.integration.time_step, seed);
        step_config
            .get_ball_factory()
            .set_species_table(Arc::clone(&world.species));
        Ok(Self {
            world,
            step_config,
            runtime: super::PhysicsRuntimeState {
                snapshot_slot: Arc::new(ArcSwapOption::from(None)),
                wait_time: 0,
                physics_fps: 0.0,
                save_store: None,
                world_generation: 0,
                snapshot_layers: SceneLayers::ALL,
                snapshot_interval_us: config.display.snapshot_interval_us,
            },
        })
    }
}

impl SimulationWorld {
    /// An empty world: no balls, no operators, physics per `config`, not
    /// paused. Fails if `config` is invalid: checking it
    /// (`SimConfig::validated_species`) also builds its species table and
    /// resolves its pair overrides, in one pass.
    pub(crate) fn with_config(world_rect: Rect, config: &SimConfig) -> AppResult<Self> {
        let ResolvedSpecies {
            table,
            pair_overrides,
        } = config.validated_species()?;
        let physics = BallPhysics::with_pair_overrides(
            PhysicsConstants::default(),
            &table,
            config.potential.cap_settings(),
            pair_overrides,
        )
        .with_skin_factor(config.neighbours.skin_factor);
        let species_present = SpeciesSet::default();
        let cell_grid = CellGrid::new(world_rect, physics.max_list_radius(species_present))?;
        Ok(Self {
            balls: Vec::new(),
            physics,
            species: Arc::new(table),
            species_config: config.species.clone(),
            cell_grid,
            species_present,
            world_rect,
            sim: SimStats::default(),
            real_data: false,
            step_by_step: false,
            step_count: 0,
            show_timings: false,
            show_ball_stats: true,
            was_paused_for_resize: false,
            speed_limit_factor: config.integration.speed_limit_factor,
            speed_limit_override: None,
            kernel_points: Vec::new(),
            scratch: Default::default(),
            cached_force_results: Vec::new(),
            thermo_history: ThermoFieldHistory::new(),
            phase_timings: Default::default(),
            line_segments: Default::default(),
            valve_segments: Default::default(),
            source_spheres: Default::default(),
            thermostats: Default::default(),
            ghost_wall: None,
            wall_chains: Default::default(),
            #[cfg(test)]
            test_faults: Default::default(),
        })
    }
}
