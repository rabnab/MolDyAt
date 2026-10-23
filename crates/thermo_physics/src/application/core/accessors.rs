// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use crate::application::AppResult;
use crate::application::phase_timings::PhaseTimings;
use crate::application::sim_config::{
    DisplayConfig, IntegrationConfig, NeighbourConfig, PairOverrideConfig, PotentialConfig,
    SimConfig,
};
use crate::application::sim_stats::{HIGH_GRAVITY, NORMAL_GRAVITY, is_normal_gravity};
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::ball_core::gravity_acceleration;
use crate::objects2d::operator::{is_degenerate, remap_point};
use std::time::Duration;

impl MainContent {
    pub fn world(&self) -> &SimulationWorld {
        &self.world
    }

    /// Time step and ball factory of the run.
    pub fn step_config(&self) -> &StepConfig {
        &self.step_config
    }

    /// Time spent per step phase since the last [`Self::reset_phase_timings`].
    pub fn phase_timings(&self) -> &PhaseTimings {
        &self.world.phase_timings
    }

    pub fn reset_phase_timings(&mut self) {
        self.world.phase_timings.reset();
    }

    /// Momentum `(px, py)` given to walls (box and non-ghost line walls)
    /// since the start of the run; the pressure readings derive from it.
    pub fn wall_momentum(&self) -> (f32, f32) {
        (
            self.world.sim.wall_momentum_x,
            self.world.sim.wall_momentum_y,
        )
    }

    /// Drop the recorded thermo fields. Called when the heatmap is hidden
    /// and the fields are no longer recorded, so a heatmap shown again later
    /// never mixes in fields from before.
    pub fn clear_thermo_history(&mut self) {
        self.world.thermo_history.clear();
    }

    /// Mutable access to the world, inside this crate and for the tests of
    /// dependent crates (feature `test-support`).
    #[cfg(any(test, feature = "test-support"))]
    pub fn world_mut(&mut self) -> &mut SimulationWorld {
        &mut self.world
    }

    #[cfg(not(any(test, feature = "test-support")))]
    pub(crate) fn world_mut(&mut self) -> &mut SimulationWorld {
        &mut self.world
    }

    pub fn runtime(&self) -> &PhysicsRuntimeState {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut PhysicsRuntimeState {
        &mut self.runtime
    }

    /// The species new balls and new source spheres get.
    pub(crate) fn new_ball_species(&self) -> crate::objects2d::ball_impl::SpeciesId {
        self.step_config.get_ball_factory().new_ball_species()
    }

    pub(crate) fn world_and_step_config(&mut self) -> (&mut SimulationWorld, &StepConfig) {
        let Self {
            world, step_config, ..
        } = self;
        (world, step_config)
    }

    /// What a step needs: the world, the step settings and the options.
    pub(crate) fn step_parts(&mut self) -> (&mut SimulationWorld, &StepConfig, &RuntimeOptions) {
        let Self {
            world,
            step_config,
            options,
            ..
        } = self;
        (world, step_config, options)
    }

    pub fn toggle_pause(&mut self) {
        self.world.sim.toggle_pause();
    }

    pub fn toggle_gravity(&mut self) {
        let gravity = self.world.physics.constants.gravity;
        let new_gravity = if gravity == 0.0 {
            NORMAL_GRAVITY
        } else if is_normal_gravity(gravity) {
            HIGH_GRAVITY
        } else {
            0.0
        };

        let delta_gravity = new_gravity - gravity;
        self.world.physics.constants.gravity = new_gravity;

        for ball in &mut self.world.balls {
            ball.accel.y += gravity_acceleration(delta_gravity).y;
        }
    }

    pub fn toggle_real_data(&mut self) {
        self.options.real_data = !self.options.real_data;
    }

    pub fn toggle_show_ball_stats(&mut self) {
        self.options.show_ball_stats = !self.options.show_ball_stats;
    }

    pub fn toggle_stepwise(&mut self) {
        self.options.step_by_step = !self.options.step_by_step;
    }

    pub fn toggle_timing_display(&mut self) {
        self.options.show_timings = !self.options.show_timings;
    }

    /// Display and stepping options (keys `D`, `S`, `F4`, `B`).
    pub(crate) fn options(&self) -> &RuntimeOptions {
        &self.options
    }

    /// Resize simulation world and reconstruct cell grid.
    /// Pauses simulation if running, resumes after delay.
    /// Rebuilds cell grid with new dimensions and re-adds all balls.
    ///
    /// # Arguments
    /// * `world_rect` - New world boundary rectangle
    pub fn resize(&mut self, world_rect: crate::objects2d::Rect) {
        if !self.world.sim.is_paused() {
            self.pause_for_resize()
        }

        let world = &mut self.world;

        // Rescale all operator positions so they keep their relative
        // position within the world (e.g. 20%/50% stays 20%/50%).
        let old = world.world_rect;
        world.rescale_all_operators(&old, &world_rect);

        // Keep ball centers and cached position state at the same fractional
        // location in the new world rect, matching operator resize behavior.
        if !is_degenerate(&old) {
            for ball in &mut world.balls {
                let new_pos = remap_point(*ball.get_position(), &old, &world_rect);
                ball.set_position_and_update_tail(new_pos);
                ball.pre_update_pos = remap_point(ball.pre_update_pos, &old, &world_rect);
                ball.last_update_hood_pos =
                    remap_point(ball.last_update_hood_pos, &old, &world_rect);
            }
        }

        // Rebuild cell grid with new dimensions
        world.resize_cell_grid(world_rect);

        // Discard stale thermo history — accumulated grids used the old
        // world rect and would produce incorrect cell-to-world mapping.
        world.thermo_history.clear();
    }

    pub fn double_time_step(&mut self) {
        self.step_config.double_time_step();
    }

    pub fn halve_time_step(&mut self) {
        self.step_config.halve_time_step();
    }

    /// Time step used by every following simulation step.
    pub fn time_step(&self) -> f32 {
        self.step_config.get_time_step()
    }

    /// Set the time step exactly (no clamping, unlike the double/halve
    /// keys). Used by headless runs. A non-finite or non-positive `dt` is
    /// rejected with an error and the current time step is kept.
    pub fn set_time_step(&mut self, dt: f32) -> AppResult<()> {
        self.step_config.set_time_step(dt)
    }
}

impl SimulationWorld {
    /// Get world boundary rectangle for simulation area.
    pub fn world_region(&self) -> Rect {
        self.world_rect
    }

    /// Get world boundary rectangle with inset margin.
    /// Useful for preventing balls from spawning too close to walls.
    pub(crate) fn world_region_inset(&self, inner_margin: f32) -> Rect {
        let world_region = self.world_region();
        let tl = world_region.get_tl();
        Rect::new(
            tl.0 + inner_margin,
            tl.1 - inner_margin,
            world_region.w - 2.0 * inner_margin,
            world_region.h + 2.0 * inner_margin,
        )
    }
}

impl MainContent {
    /// The config the simulation runs with now: the one it was built or
    /// loaded with, with the current time step.
    pub fn config(&self) -> SimConfig {
        let physics = &self.world.physics;
        SimConfig {
            world: self.world.world_config.clone(),
            integration: IntegrationConfig {
                time_step: self.time_step(),
                speed_limit_factor: self.world.speed_limit_factor,
            },
            potential: PotentialConfig {
                pair_overrides: physics
                    .pair_overrides()
                    .iter()
                    .map(|o| PairOverrideConfig::named(o, physics.species()))
                    .collect(),
                ..PotentialConfig::from(*physics.cap_settings())
            },
            neighbours: NeighbourConfig {
                skin_factor: physics.skin_factor(),
            },
            display: DisplayConfig {
                snapshot_interval_us: self.runtime.snapshot_interval_us,
            },
            species: self.world.species_config.clone(),
        }
    }

    /// Directory for Ctrl+S / Ctrl+O saves (set once at startup).
    pub fn with_save_store(mut self, store: SaveStore) -> Self {
        self.runtime.save_store = Some(store);
        self
    }
}

impl PhysicsRuntimeState {
    pub fn save_store(&self) -> Option<&SaveStore> {
        self.save_store.as_ref()
    }

    pub fn world_generation(&self) -> u64 {
        self.world_generation
    }

    /// Mark that the world was replaced. A pending resize pause belonged to
    /// the old world, so the end of that resize must not resume the new one.
    pub(crate) fn next_world_generation(&mut self) {
        self.world_generation = self.world_generation.wrapping_add(1);
        self.paused_for_resize = false;
    }

    /// Least time between two snapshots the physics thread publishes.
    pub fn snapshot_interval(&self) -> Duration {
        Duration::from_micros(self.snapshot_interval_us)
    }

    pub fn set_snapshot_interval_us(&mut self, interval_us: u64) {
        self.snapshot_interval_us = interval_us;
    }

    pub fn wait_time(&self) -> u64 {
        self.wait_time
    }

    pub(crate) fn increase_wait_time(&mut self) {
        self.wait_time = (self.wait_time + 1).min(100);
    }

    pub(crate) fn decrease_wait_time(&mut self) {
        self.wait_time = self.wait_time.saturating_sub(1);
    }

    pub fn physics_fps(&self) -> f32 {
        self.physics_fps
    }

    pub fn set_physics_fps(&mut self, fps: f32) {
        self.physics_fps = fps;
    }
}
