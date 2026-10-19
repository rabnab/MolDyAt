// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Conversion between the live world and the world file.
//!
//! Restoring builds a fresh [`SimulationWorld`] through its normal
//! constructor, so every run-time field (statistics, caches, the step counter,
//! scratch buffers) starts at its default without a hand-kept reset list.
//! Operator id counters continue after the restored ids (the collections'
//! `From<Vec<_>>`). Each file type converts to and from its simulation type
//! here, side by side, so adding a field means touching one place.

use super::format::{BallData, SourceSphereData, ThermostatData, WORLD_FILE_VERSION, WorldFile};
use crate::application::core::SimulationWorld;
use crate::application::{AppResult, MainContent};
use crate::objects2d::ball_impl::{Ball2D, SpeciesId, SpeciesTable};
use crate::objects2d::operator::{OperatorState, SimSourceSphere, SimThermostat};
use std::sync::Arc;

impl WorldFile {
    /// The world of `mc` in file form, with the config it runs with.
    pub(crate) fn capture(mc: &MainContent) -> Self {
        let world = mc.world();
        let config = mc.config();
        let species = &*world.species;
        Self {
            version: WORLD_FILE_VERSION,
            world_rect: world.world_rect,
            time_step: config.integration.time_step,
            gravity: world.physics.constants.gravity,
            config: Some(config),
            balls: world
                .balls
                .iter()
                .map(|ball| BallData::capture(ball, species))
                .collect(),
            line_segments: world.line_segments.segments().to_vec(),
            valves: world.valve_segments.valves().to_vec(),
            wall_chains: world.wall_chains.chains().cloned().collect(),
            source_spheres: world
                .source_spheres
                .spheres()
                .iter()
                .map(|sphere| SourceSphereData::capture(sphere, species))
                .collect(),
            thermostats: world
                .thermostats
                .thermostats()
                .iter()
                .map(ThermostatData::from)
                .collect(),
        }
    }
}

impl TryFrom<&WorldFile> for SimulationWorld {
    type Error = crate::application::AppError;

    /// A fresh world holding the file's contents (validated first), with the
    /// physics of its config. Neighbour lists are not built yet; that needs
    /// the run settings (see `MainContent::load_world`).
    fn try_from(file: &WorldFile) -> AppResult<Self> {
        file.validate()?;
        let mut world = SimulationWorld::with_config(file.world_rect, &file.sim_config())?;
        world.physics.constants.gravity = file.gravity;
        let species = Arc::clone(&world.species);
        world.balls = file
            .balls
            .iter()
            .map(|data| data.restore(&species))
            .collect();
        world.rebuild_cell_grid();
        world.line_segments = file.line_segments.clone().into();
        world.valve_segments = file.valves.clone().into();
        world.wall_chains = file.wall_chains.clone().into();
        world.source_spheres = file
            .source_spheres
            .iter()
            .map(|data| data.restore(&species))
            .collect::<Vec<_>>()
            .into();
        world.thermostats = file
            .thermostats
            .iter()
            .map(SimThermostat::from)
            .collect::<Vec<_>>()
            .into();
        Ok(world)
    }
}

impl BallData {
    /// `ball` in file form; `species` is the world's table.
    fn capture(ball: &Ball2D, species: &SpeciesTable) -> Self {
        Self {
            id: ball.get_id(),
            species: species.species(ball.species()).name.clone(),
            pos: *ball.get_position(),
            v: ball.v,
            pre_update_pos: ball.pre_update_pos,
            accel: ball.accel,
        }
    }

    /// The ball; its mass and radius come from `species`, the world's table.
    fn restore(&self, species: &SpeciesTable) -> Ball2D {
        let id = species_by_name(species, &self.species);
        let mut ball = Ball2D::of_species(self.id, self.pos, id, species.species(id));
        ball.v = self.v;
        ball.pre_update_pos = self.pre_update_pos;
        ball.accel = self.accel;
        ball
    }
}

/// The species of `table` called `name`. `WorldFile::validate` has checked
/// that it exists.
fn species_by_name(table: &SpeciesTable, name: &str) -> SpeciesId {
    table
        .by_name(name)
        .expect("validate() checks species names against the species table")
}

fn operator_state(active: bool) -> OperatorState {
    if active {
        OperatorState::On
    } else {
        OperatorState::Off
    }
}

impl SourceSphereData {
    /// `sphere` in file form; `species` is the world's table.
    fn capture(sphere: &SimSourceSphere, species: &SpeciesTable) -> Self {
        Self {
            id: sphere.id,
            center: sphere.center,
            active: sphere.state.is_on(),
            emit_period: sphere.emit_period,
            ball_species: species.species(sphere.ball_species).name.clone(),
            emission_speed: sphere.emission_speed,
        }
    }

    /// The sphere; its species is looked up in `species`, the world's table.
    fn restore(&self, species: &SpeciesTable) -> SimSourceSphere {
        let mut sphere = SimSourceSphere::new(self.id, self.center);
        sphere.state = operator_state(self.active);
        sphere.emit_period = self.emit_period;
        sphere.ball_species = species_by_name(species, &self.ball_species);
        sphere.emission_speed = self.emission_speed;
        sphere
    }
}

impl From<&SimThermostat> for ThermostatData {
    fn from(thermostat: &SimThermostat) -> Self {
        Self {
            id: thermostat.id,
            position: thermostat.position,
            active: thermostat.state.is_on(),
            target_temperature: thermostat.target_temperature,
            radius: thermostat.radius,
            tau: thermostat.tau,
            chain: thermostat.chain.clone(),
        }
    }
}

impl From<&ThermostatData> for SimThermostat {
    fn from(data: &ThermostatData) -> Self {
        let mut thermostat = SimThermostat::new(data.id, data.position);
        thermostat.state = operator_state(data.active);
        thermostat.target_temperature = data.target_temperature;
        thermostat.radius = data.radius;
        thermostat.tau = data.tau;
        thermostat.chain = data.chain.clone();
        thermostat
    }
}
