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

use super::format::{
    BallData, ChainPieceData, JointData, OperatorData, SourceSphereData, ThermostatData,
    WORLD_FILE_VERSION, WallChainData, WorldFile,
};
use crate::application::core::SimulationWorld;
use crate::application::{AppResult, MainContent};
use crate::objects2d::ball_impl::{Ball2D, SpeciesId, SpeciesTable};
use crate::objects2d::line_segment::DEFAULT_THICKNESS;
use crate::objects2d::operator::{OperatorState, SimSourceSphere, SimThermostat};
use crate::objects2d::wall_chain::{ChainPiece, Joint, WallChain};
use crate::objects2d::{SimLineSegment, ValveSegment};
use std::sync::Arc;

impl WorldFile {
    /// The world of `mc` in file form, with the config it runs with.
    pub(crate) fn capture(mc: &MainContent) -> Self {
        let world = mc.world();
        let config = mc.config();
        let species = &**world.species();
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
            operators: capture_operators(world, species),
        }
    }
}

/// Every operator of `world`, grouped by kind: wall chains (with their
/// pieces and valves), source spheres, thermostats.
fn capture_operators(world: &SimulationWorld, species: &SpeciesTable) -> Vec<OperatorData> {
    let chains = world
        .wall_chains
        .chains()
        .map(|chain| OperatorData::WallChain(WallChainData::capture(chain, world)));
    let spheres = world
        .source_spheres
        .spheres()
        .iter()
        .map(|sphere| OperatorData::SourceSphere(SourceSphereData::capture(sphere, species)));
    let thermostats = world
        .thermostats
        .thermostats()
        .iter()
        .map(|thermostat| OperatorData::Thermostat(ThermostatData::from(thermostat)));
    chains.chain(spheres).chain(thermostats).collect()
}

impl TryFrom<&WorldFile> for SimulationWorld {
    type Error = crate::application::AppError;

    /// A fresh world holding the file's contents (validated first), with the
    /// physics of its config. Neighbour lists are not built yet; that needs
    /// the run settings (see `MainContent::load_world`).
    fn try_from(file: &WorldFile) -> AppResult<Self> {
        let resolved = file.validate()?;
        let mut world =
            SimulationWorld::with_resolved(file.world_rect, &file.sim_config(), resolved)?;
        world.physics.constants.gravity = file.gravity;
        let species = Arc::clone(world.species());
        world.balls = file
            .balls
            .iter()
            .map(|data| data.restore(&species))
            .collect();
        world.rebuild_cell_grid();
        // The chains own their pieces and valves in the file; the world keeps
        // them in collections of their own, ordered by id as they were built.
        let mut segments: Vec<SimLineSegment> = file
            .wall_chains()
            .flat_map(|chain| &chain.pieces)
            .map(SimLineSegment::from)
            .collect();
        segments.sort_by_key(|segment| segment.id);
        world.line_segments = segments.into();
        let mut valves: Vec<ValveSegment> = file
            .wall_chains()
            .flat_map(|chain| &chain.joints)
            .filter_map(|joint| match *joint {
                JointData::Valve {
                    valve_id,
                    is_closed,
                    ..
                } => Some(ValveSegment::new(valve_id, is_closed)),
                JointData::Gap { .. } => None,
            })
            .collect();
        valves.sort_by_key(|valve| valve.id);
        world.valve_segments = valves.into();
        world.wall_chains = file
            .wall_chains()
            .map(WallChain::from)
            .collect::<Vec<_>>()
            .into();
        world.source_spheres = file
            .source_spheres()
            .map(|data| data.restore(&species))
            .collect::<Vec<_>>()
            .into();
        world.thermostats = file
            .thermostats()
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

impl From<&ChainPieceData> for SimLineSegment {
    fn from(data: &ChainPieceData) -> Self {
        Self {
            id: data.id,
            start: data.start,
            end: data.end,
            thickness: data.thickness,
        }
    }
}

impl WallChainData {
    /// `chain` in file form, with its pieces' segments and its valves'
    /// state looked up in `world`.
    fn capture(chain: &WallChain, world: &SimulationWorld) -> Self {
        Self {
            id: chain.id,
            start: chain.start,
            end: chain.end,
            pieces: chain
                .pieces
                .iter()
                .map(|piece| ChainPieceData::capture(piece, chain, world))
                .collect(),
            joints: chain
                .joints
                .iter()
                .map(|joint| JointData::capture(joint, world))
                .collect(),
        }
    }
}

impl From<&WallChainData> for WallChain {
    fn from(data: &WallChainData) -> Self {
        Self {
            id: data.id,
            start: data.start,
            end: data.end,
            pieces: data.pieces.iter().map(ChainPiece::from).collect(),
            joints: data.joints.iter().map(Joint::from).collect(),
        }
    }
}

impl ChainPieceData {
    /// `piece` of `chain` with its segment's geometry. A piece and its
    /// segment are created and deleted together; should the segment still be
    /// missing, the piece is saved where the chain puts it, with the default
    /// thickness, rather than failing the save.
    fn capture(piece: &ChainPiece, chain: &WallChain, world: &SimulationWorld) -> Self {
        let (start, end, thickness) = match world.line_segments.find(piece.line_id) {
            Some(segment) => (segment.start, segment.end, segment.thickness),
            None => {
                log::error!(
                    "wall chain {} has no line segment {}; saving the piece on the chain",
                    chain.id,
                    piece.line_id
                );
                (
                    chain.point_at(piece.t_start),
                    chain.point_at(piece.t_end),
                    DEFAULT_THICKNESS,
                )
            }
        };
        Self {
            id: piece.line_id,
            start,
            end,
            thickness,
            t_start: piece.t_start,
            t_end: piece.t_end,
        }
    }
}

impl From<&ChainPieceData> for ChainPiece {
    fn from(data: &ChainPieceData) -> Self {
        Self {
            line_id: data.id,
            t_start: data.t_start,
            t_end: data.t_end,
        }
    }
}

impl JointData {
    /// `joint` with its valve's state. A valve joint and its valve are
    /// created and deleted together; should the valve still be missing, it is
    /// saved closed rather than failing the save.
    fn capture(joint: &Joint, world: &SimulationWorld) -> Self {
        match *joint {
            Joint::Valve {
                valve_id,
                t_start,
                t_end,
            } => JointData::Valve {
                valve_id,
                is_closed: world.valve_segments.find(valve_id).map_or_else(
                    || {
                        log::error!("valve {valve_id} is missing; saving it closed");
                        true
                    },
                    |valve| valve.is_closed,
                ),
                t_start,
                t_end,
            },
            Joint::Gap { t_start, t_end } => JointData::Gap { t_start, t_end },
        }
    }
}

impl From<&JointData> for Joint {
    fn from(data: &JointData) -> Self {
        match *data {
            JointData::Valve {
                valve_id,
                t_start,
                t_end,
                ..
            } => Joint::Valve {
                valve_id,
                t_start,
                t_end,
            },
            JointData::Gap { t_start, t_end } => Joint::Gap { t_start, t_end },
        }
    }
}
