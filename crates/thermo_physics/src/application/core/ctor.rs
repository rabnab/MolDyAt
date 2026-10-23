// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{BallPhysics, CellGrid, MainContent, RuntimeOptions, SimStats, SimulationWorld};
use crate::application::AppResult;
use crate::application::StepConfig;
use crate::application::sim_config::{ResolvedSpecies, SimConfig};
use crate::application::thermo_field::ThermoFieldHistory;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::{PhysicsConstants, SpeciesSet};
use std::sync::Arc;

impl MainContent {
    /// An empty world with the default [`SimConfig`].
    pub fn new(world_rect: Rect) -> AppResult<Self> {
        Self::with_config(world_rect, &SimConfig::default(), None)
    }

    /// An empty world run with `config` (validated first); balls are seeded
    /// from `seed` if given.
    pub fn with_config(world_rect: Rect, config: &SimConfig, seed: Option<u64>) -> AppResult<Self> {
        let world = SimulationWorld::with_config(world_rect, config)?;
        Ok(Self {
            step_config: StepConfig::new(
                config.integration.time_step,
                seed,
                Arc::clone(world.species()),
            ),
            world,
            runtime: super::PhysicsRuntimeState {
                wait_time: 0,
                physics_fps: 0.0,
                save_store: None,
                world_generation: 0,
                snapshot_interval_us: config.display.snapshot_interval_us,
                paused_for_resize: false,
            },
            options: RuntimeOptions::default(),
        })
    }
}

impl SimulationWorld {
    /// An empty world: no balls, no operators, physics per `config`, not
    /// paused. Fails if `config` is invalid: checking it
    /// (`SimConfig::validated_species`) also builds its species table and
    /// resolves its pair overrides, in one pass.
    pub(crate) fn with_config(world_rect: Rect, config: &SimConfig) -> AppResult<Self> {
        let resolved = config.validated_species()?;
        Self::with_resolved(world_rect, config, resolved)
    }

    /// [`Self::with_config`] for a `config` that was already validated, with
    /// its species table and pair overrides resolved as `resolved` (by
    /// `SimConfig::validated_species` or `WorldFile::validate`).
    pub(crate) fn with_resolved(
        world_rect: Rect,
        config: &SimConfig,
        resolved: ResolvedSpecies,
    ) -> AppResult<Self> {
        let ResolvedSpecies {
            table,
            pair_overrides,
        } = resolved;
        let physics = BallPhysics::with_pair_overrides(
            PhysicsConstants::default(),
            table,
            config.potential.cap_settings(),
            pair_overrides,
        )
        .with_skin_factor(config.neighbours.skin_factor);
        let species_present = SpeciesSet::default();
        let cell_grid = CellGrid::new(world_rect, physics.max_list_radius(species_present))?;
        Ok(Self {
            balls: Vec::new(),
            physics,
            species_config: config.species.clone(),
            cell_grid,
            species_present,
            world_rect,
            sim: SimStats::default(),
            step_count: 0,
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
