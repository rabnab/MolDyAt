// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::wgpu::render::pipelines::thermostat_label_pipeline::ThermostatLabelPipeline;
use thermo_physics::objects2d::{SimThermostat, Vec2D};

fn identity_view_proj() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

#[test]
fn finite_values_are_formatted_with_prefix() {
    assert_eq!(
        ThermostatLabelPipeline::build_label_text('T', 0.5),
        "T:0.50"
    );
}

#[test]
fn non_finite_values_are_rendered_as_dashes() {
    assert_eq!(
        ThermostatLabelPipeline::build_label_text('L', f32::NAN),
        "L:--"
    );
}

#[test]
fn one_thermostat_emits_two_label_quads_per_character() {
    let thermostat = SimThermostat::new(7, Vec2D::new(0.0, 0.0));
    let local =
        ThermostatLabelPipeline::build_local_label(&thermostat, thermostat.current_local_temp);
    let target = ThermostatLabelPipeline::build_label_text('T', thermostat.target_temperature);
    // Every character is a quad of the shared ASCII atlas, a space too
    // (#303), so the label background spans the whole text.
    let glyphs = local.chars().chain(target.chars());
    let expected_vertices = glyphs.count() * 6;

    let actual_vertices = ThermostatLabelPipeline::vertex_count_for_thermostats(
        &[thermostat],
        identity_view_proj(),
        0.02,
        0.03,
    );

    assert_eq!(actual_vertices, expected_vertices);
}

#[test]
fn empty_thermostat_list_emits_no_vertices() {
    let actual_vertices = ThermostatLabelPipeline::vertex_count_for_thermostats(
        &[],
        identity_view_proj(),
        0.02,
        0.03,
    );

    assert_eq!(actual_vertices, 0);
}

#[test]
fn deactivated_thermostat_label_shows_live_value_and_inactive_mark() {
    let mut thermostat = SimThermostat::new(1, Vec2D::new(0.0, 0.0));
    assert!(!thermostat.is_active());

    thermostat.current_local_temp = 0.31;
    assert_eq!(
        ThermostatLabelPipeline::build_local_label(&thermostat, 0.31),
        "L:0.31 OFF"
    );
    // The reading follows the physics while the thermostat stays off.
    assert_eq!(
        ThermostatLabelPipeline::build_local_label(&thermostat, 0.45),
        "L:0.45 OFF"
    );

    thermostat.state.toggle();
    assert_eq!(
        ThermostatLabelPipeline::build_local_label(&thermostat, 0.31),
        "L:0.31"
    );
}
