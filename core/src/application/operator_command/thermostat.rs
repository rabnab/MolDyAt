//! Thermostat operator commands.

use crate::application::MainContent;
use crate::application::operator_command::OperatorCommand;
use crate::objects2d::Vec2D;

/// Place a new thermostat at `position`.
#[derive(Debug)]
pub struct PlaceThermostat {
    pub position: Vec2D,
}

impl OperatorCommand for PlaceThermostat {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.add_thermostat(self.position);
    }
}

/// Toggle (active ↔ inactive) an existing thermostat.
#[derive(Debug)]
pub struct ToggleThermostat {
    pub thermostat_id: usize,
}

impl OperatorCommand for ToggleThermostat {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.toggle_thermostat(self.thermostat_id);
    }
}

/// Move a thermostat to a new position.
#[derive(Debug)]
pub struct MoveThermostat {
    pub thermostat_id: usize,
    pub new_pos: Vec2D,
}

impl OperatorCommand for MoveThermostat {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.move_thermostat(self.thermostat_id, self.new_pos);
    }
}

/// Nudge a thermostat's target temperature.
#[derive(Debug)]
pub struct AdjustThermostatTargetTemperature {
    pub thermostat_id: usize,
    pub delta_steps: f32,
}

impl OperatorCommand for AdjustThermostatTargetTemperature {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.adjust_thermostat_target_temperature(self.thermostat_id, self.delta_steps);
    }
}

/// Nudge a thermostat's influence radius.
#[derive(Debug)]
pub struct AdjustThermostatRadius {
    pub thermostat_id: usize,
    pub delta_steps: f32,
}

impl OperatorCommand for AdjustThermostatRadius {
    fn execute(self: Box<Self>, content: &mut MainContent) {
        content.adjust_thermostat_radius(self.thermostat_id, self.delta_steps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_thermostat_fields_round_trip() {
        let cmd = PlaceThermostat {
            position: Vec2D::new(3.0, 4.0),
        };
        assert_eq!(cmd.position, Vec2D::new(3.0, 4.0));
    }

    #[test]
    fn toggle_thermostat_fields_round_trip() {
        let cmd = ToggleThermostat { thermostat_id: 8 };
        assert_eq!(cmd.thermostat_id, 8);
    }

    #[test]
    fn move_thermostat_fields_round_trip() {
        let cmd = MoveThermostat {
            thermostat_id: 9,
            new_pos: Vec2D::new(7.0, 8.0),
        };
        assert_eq!(cmd.thermostat_id, 9);
        assert_eq!(cmd.new_pos, Vec2D::new(7.0, 8.0));
    }

    #[test]
    fn adjust_thermostat_target_temperature_fields_round_trip() {
        let cmd = AdjustThermostatTargetTemperature {
            thermostat_id: 10,
            delta_steps: 1.5,
        };
        assert_eq!(cmd.thermostat_id, 10);
        assert!((cmd.delta_steps - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn adjust_thermostat_radius_fields_round_trip() {
        let cmd = AdjustThermostatRadius {
            thermostat_id: 11,
            delta_steps: -0.5,
        };
        assert_eq!(cmd.thermostat_id, 11);
        assert!((cmd.delta_steps - (-0.5)).abs() < f32::EPSILON);
    }
}
