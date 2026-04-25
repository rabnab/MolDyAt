use super::super::thermostat_label_pipeline::ThermostatLabelPipeline;
use rust_thermo_core::objects2d::{SimThermostat, Vec2D};

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
    let local = ThermostatLabelPipeline::build_label_text('L', thermostat.current_local_temp);
    let target = ThermostatLabelPipeline::build_label_text('T', thermostat.target_temperature);
    let expected_vertices = (local.len() + target.len()) * 6;

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
