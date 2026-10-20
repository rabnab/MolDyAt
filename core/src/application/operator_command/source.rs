// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Source-sphere operator commands.

use crate::application::MainContent;
use crate::application::operator_command::OperatorCommand;
use crate::objects2d::Vec2D;

/// Place a new source sphere at `center`.
#[derive(Debug)]
pub struct PlaceSourceSphere {
    pub center: Vec2D,
}

impl OperatorCommand for PlaceSourceSphere {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.add_source_sphere(self.center);
    }
}

/// Toggle (active ↔ inactive) an existing source sphere.
#[derive(Debug)]
pub struct ToggleSourceSphere {
    pub sphere_id: usize,
}

impl OperatorCommand for ToggleSourceSphere {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.toggle_source_sphere(self.sphere_id);
    }
}

/// Cycle the species a source sphere emits (the world's table, wrapping).
#[derive(Debug)]
pub struct CycleSourceSphereSpecies {
    pub sphere_id: usize,
}

impl OperatorCommand for CycleSourceSphereSpecies {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.cycle_source_sphere_species(self.sphere_id);
    }
}

/// Adjust the emission speed of a source sphere.
#[derive(Debug)]
pub struct AdjustSourceSphereSpeed {
    pub sphere_id: usize,
    pub delta: f32,
}

impl OperatorCommand for AdjustSourceSphereSpeed {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.adjust_source_sphere_speed(self.sphere_id, self.delta);
    }
}

/// Adjust the emission period (steps between emissions) of a source sphere.
#[derive(Debug)]
pub struct AdjustSourceSphereEmitPeriod {
    pub sphere_id: usize,
    pub delta_steps: i32,
}

impl OperatorCommand for AdjustSourceSphereEmitPeriod {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.adjust_source_sphere_emit_period(self.sphere_id, self.delta_steps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_source_sphere_fields_round_trip() {
        let cmd = PlaceSourceSphere {
            center: Vec2D::new(1.0, 2.0),
        };
        assert_eq!(cmd.center, Vec2D::new(1.0, 2.0));
    }

    #[test]
    fn toggle_source_sphere_fields_round_trip() {
        let cmd = ToggleSourceSphere { sphere_id: 4 };
        assert_eq!(cmd.sphere_id, 4);
    }

    #[test]
    fn cycle_source_sphere_ball_size_fields_round_trip() {
        let cmd = CycleSourceSphereSpecies { sphere_id: 5 };
        assert_eq!(cmd.sphere_id, 5);
    }

    #[test]
    fn adjust_source_sphere_speed_fields_round_trip() {
        let cmd = AdjustSourceSphereSpeed {
            sphere_id: 6,
            delta: 0.5,
        };
        assert_eq!(cmd.sphere_id, 6);
        assert!((cmd.delta - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn adjust_source_sphere_emit_period_fields_round_trip() {
        let cmd = AdjustSourceSphereEmitPeriod {
            sphere_id: 7,
            delta_steps: -2,
        };
        assert_eq!(cmd.sphere_id, 7);
        assert_eq!(cmd.delta_steps, -2);
    }
}
