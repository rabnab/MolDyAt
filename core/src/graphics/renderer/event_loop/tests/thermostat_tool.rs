use crate::graphics::renderer::event_loop::OperatorTool;

#[test]
fn thermostat_tool_enum_variant_exists() {
    assert_eq!(
        OperatorTool::Thermostat as u8,
        OperatorTool::Thermostat as u8
    );
}
