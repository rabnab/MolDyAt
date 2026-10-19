// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Hover highlighting and Delete: each tool reports only objects of its own
//! kind under the cursor, and Delete removes exactly that object.

use crate::application::renderer_snapshot::{
    OperatorSnapshot, RendererSnapshot, ValveSpan, WallSnapshot,
};
use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::event_loop::command_registry::{UiAction, command_by_id};
use crate::graphics::renderer::event_loop::energy_window_manager::EnergyWindowManager;
use crate::graphics::renderer::event_loop::input_handler::{
    InputAction, InputHandler, KeyCode, OperatorTool,
};
use crate::graphics::renderer::event_loop::input_orchestrator::InputOrchestrator;
use crate::graphics::renderer::event_loop::metrics::MetricsCollector;
use crate::graphics::renderer::event_loop::physics_coordinator::PhysicsCoordinator;
use crate::graphics::renderer::event_loop::unified_render_state::{
    RenderStateData, UnifiedRenderState,
};
use crate::graphics::renderer::margins::Margins;
use crate::graphics::renderer::state_layers::RendererConfig;
use crate::objects2d::wall_chain::ObjectRef;
use crate::objects2d::{Rect, SimLineSegment, SimSourceSphere, SimThermostat, Vec2D};
use std::sync::{Arc, Mutex};

const THERMOSTAT_POS: Vec2D = Vec2D { x: -8.0, y: -8.0 };
const SPHERE_POS: Vec2D = Vec2D { x: 8.0, y: -8.0 };

/// Wall 4 along y = 0 from x = -10 to 10: piece 0 (x -10..-2), valve 3
/// (x -2..2), piece 1 (x 2..10). Plus one thermostat and one source sphere.
fn operators() -> OperatorSnapshot {
    OperatorSnapshot {
        line_segments: vec![
            SimLineSegment::new(0, Vec2D::new(-10.0, 0.0), Vec2D::new(-2.0, 0.0)),
            SimLineSegment::new(1, Vec2D::new(2.0, 0.0), Vec2D::new(10.0, 0.0)),
        ],
        walls: vec![WallSnapshot {
            chain_id: 4,
            start: Vec2D::new(-10.0, 0.0),
            end: Vec2D::new(10.0, 0.0),
            piece_ids: vec![0, 1],
            valve_ids: vec![3],
        }],
        valve_spans: vec![ValveSpan {
            valve_id: 3,
            p0: Vec2D::new(-2.0, 0.0),
            p1: Vec2D::new(2.0, 0.0),
            thickness: 1.0,
            is_closed: true,
        }],
        thermostats: vec![SimThermostat::new(5, THERMOSTAT_POS)],
        source_spheres: vec![SimSourceSphere::new(6, SPHERE_POS)],
    }
}

fn build_state(physics: PhysicsCoordinator, operators: OperatorSnapshot) -> UnifiedRenderState {
    let energy_data: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));
    let mut snapshot = RendererSnapshot::new();
    snapshot.operators = operators;
    let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
    UnifiedRenderState {
        input: InputHandler::new(),
        physics,
        render: RenderStateData {
            viewport: ViewportTransform::new(
                world,
                (1280.0, 720.0),
                Margins::symmetric(50.0, 50.0),
            ),
            energy_manager: EnergyWindowManager::new(energy_data),
            config: RendererConfig::default(),
            view_dirty: false,
            last_snapshot: Some(Arc::new(snapshot)),
            cursor_world_pos: Vec2D::zero(),
            current_species: crate::objects2d::ball_impl::DEFAULT_SPECIES,
            metrics: MetricsCollector::new(),
            active_tool: OperatorTool::None,
            tool_handler: None,
            default_operator_snapshot: OperatorSnapshot::default(),
            resize_pending: false,
            requested_window_size: None,
            pending_window_request: None,
        },
    }
}

fn hovered_with(tool: OperatorTool, cursor: Vec2D) -> Option<ObjectRef> {
    let mut state = build_state(PhysicsCoordinator::new_disconnected(), operators());
    state.select_operator_tool(tool);
    state.render.cursor_world_pos = cursor;
    state.hovered_object()
}

#[test]
fn wall_tool_highlights_the_whole_wall_from_any_piece() {
    let wall = Some(ObjectRef::Wall(4));
    assert_eq!(
        hovered_with(OperatorTool::LineSegment, Vec2D::new(-6.0, 0.1)),
        wall
    );
    assert_eq!(
        hovered_with(OperatorTool::LineSegment, Vec2D::new(6.0, -0.1)),
        wall
    );
    assert_eq!(
        hovered_with(OperatorTool::LineSegment, Vec2D::new(-10.3, 0.0)),
        wall
    );
    assert_eq!(
        hovered_with(OperatorTool::LineSegment, Vec2D::new(0.0, 5.0)),
        None
    );
}

#[test]
fn wall_tool_highlights_the_wall_when_hovering_its_valve() {
    assert_eq!(
        hovered_with(OperatorTool::LineSegment, Vec2D::new(0.0, 0.1)),
        Some(ObjectRef::Wall(4))
    );
}

#[test]
fn while_drawing_only_the_snap_target_wall_is_highlighted() {
    let mut state = build_state(PhysicsCoordinator::new_disconnected(), operators());
    state.select_operator_tool(OperatorTool::LineSegment);
    let start = Vec2D::new(0.0, 8.0);
    state.input.set_cursor_pos(start, [640.0, 360.0]);
    state.render.cursor_world_pos = start;
    InputOrchestrator::handle_mouse_input_event(
        &mut state,
        winit::event::MouseButton::Left,
        winit::event::ElementState::Pressed,
    );

    // Over the wall's body: drawing, not hovering it.
    state.render.cursor_world_pos = Vec2D::new(-6.0, 0.0);
    assert_eq!(state.hovered_object(), None);

    // Next to the wall's end point, where the new wall would snap to.
    state.render.cursor_world_pos = Vec2D::new(10.2, 0.1);
    assert_eq!(state.hovered_object(), Some(ObjectRef::Wall(4)));
}

#[test]
fn valve_tool_highlights_valves_but_not_walls() {
    assert_eq!(
        hovered_with(OperatorTool::ValveSegment, Vec2D::new(0.5, 0.0)),
        Some(ObjectRef::Valve(3))
    );
    assert_eq!(
        hovered_with(OperatorTool::ValveSegment, Vec2D::new(-6.0, 0.0)),
        None
    );
}

#[test]
fn thermostat_and_source_tools_highlight_their_own_objects_only() {
    assert_eq!(
        hovered_with(OperatorTool::Thermostat, THERMOSTAT_POS),
        Some(ObjectRef::Thermostat(5))
    );
    assert_eq!(hovered_with(OperatorTool::Thermostat, SPHERE_POS), None);
    assert_eq!(
        hovered_with(OperatorTool::SourceSphere, SPHERE_POS),
        Some(ObjectRef::SourceSphere(6))
    );
    assert_eq!(
        hovered_with(OperatorTool::SourceSphere, THERMOSTAT_POS),
        None
    );
}

#[test]
fn nothing_is_highlighted_without_an_operator_tool() {
    assert_eq!(hovered_with(OperatorTool::None, THERMOSTAT_POS), None);
    assert_eq!(
        hovered_with(OperatorTool::Ball, Vec2D::new(-6.0, 0.0)),
        None
    );
}

#[test]
fn frame_previews_carry_the_hovered_object() {
    let mut state = build_state(PhysicsCoordinator::new_disconnected(), operators());
    state.select_operator_tool(OperatorTool::Thermostat);
    state.render.cursor_world_pos = THERMOSTAT_POS;

    assert_eq!(
        state.operator_previews().hovered,
        Some(ObjectRef::Thermostat(5))
    );
}

#[test]
fn delete_and_backspace_map_to_delete_hovered_once_per_press() {
    let mut input = InputHandler::new();
    assert_eq!(
        input
            .handle_key_down(KeyCode::Delete, false)
            .and_then(|a| a.ui_action()),
        Some(UiAction::DeleteHovered)
    );
    assert!(input.handle_key_down(KeyCode::Delete, true).is_none());
    assert_eq!(
        input
            .handle_key_down(KeyCode::Backspace, false)
            .and_then(|a| a.ui_action()),
        Some(UiAction::DeleteHovered)
    );
}

#[test]
fn delete_hovered_sends_delete_for_the_highlighted_object() {
    let (physics, log) = PhysicsCoordinator::new_recording();
    let mut state = build_state(physics, operators());
    state.select_operator_tool(OperatorTool::LineSegment);
    state.render.cursor_world_pos = Vec2D::new(6.0, 0.0);

    InputOrchestrator::process_action(&mut state, delete_hovered(), None, None);

    let commands = log.lock().expect("command log lock");
    let deletes: Vec<&String> = commands
        .iter()
        .filter(|c| c.contains("DeleteObject"))
        .collect();
    assert_eq!(deletes.len(), 1, "got: {commands:?}");
    assert!(deletes[0].contains("Wall(4)"), "got: {commands:?}");
}

#[test]
fn delete_hovered_does_nothing_when_nothing_is_highlighted() {
    let (physics, log) = PhysicsCoordinator::new_recording();
    let mut state = build_state(physics, operators());
    state.select_operator_tool(OperatorTool::LineSegment);
    state.render.cursor_world_pos = Vec2D::new(0.0, 8.0);

    InputOrchestrator::process_action(&mut state, delete_hovered(), None, None);

    assert!(log.lock().expect("command log lock").is_empty());
}

#[test]
fn cursor_moving_onto_within_or_off_an_object_redraws() {
    let mut state = build_state(
        PhysicsCoordinator::new_disconnected(),
        OperatorSnapshot::default(),
    );
    state.select_operator_tool(OperatorTool::SourceSphere);

    // Put a sphere exactly under a known pixel.
    let px = [400.0, 300.0];
    let sphere_pos = state.render.viewport.px_to_world(&px);
    let mut snapshot = RendererSnapshot::new();
    snapshot.operators.source_spheres = vec![SimSourceSphere::new(6, sphere_pos)];
    state.render.last_snapshot = Some(Arc::new(snapshot));

    InputOrchestrator::handle_cursor_moved_event(&mut state, 900.0, 600.0);
    state.render.view_dirty = false;

    InputOrchestrator::handle_cursor_moved_event(&mut state, px[0], px[1]);
    assert!(
        state.render.view_dirty,
        "moving onto the sphere should redraw"
    );

    // The crosshair follows the cursor, also while paused when no snapshot
    // would trigger a redraw.
    state.render.view_dirty = false;
    InputOrchestrator::handle_cursor_moved_event(&mut state, px[0] + 1.0, px[1]);
    assert!(
        state.render.view_dirty,
        "moving within the sphere moves the crosshair"
    );

    state.render.view_dirty = false;
    InputOrchestrator::handle_cursor_moved_event(&mut state, 900.0, 600.0);
    assert!(
        state.render.view_dirty,
        "moving off the sphere should redraw"
    );
}

#[test]
fn switching_tools_mid_drag_clears_the_ghost_wall() {
    let (physics, log) = PhysicsCoordinator::new_recording();
    let mut state = build_state(physics, operators());
    state.select_operator_tool(OperatorTool::LineSegment);
    let start = Vec2D::new(0.0, 8.0);
    state.input.set_cursor_pos(start, [640.0, 360.0]);
    state.render.cursor_world_pos = start;
    InputOrchestrator::handle_mouse_input_event(
        &mut state,
        winit::event::MouseButton::Left,
        winit::event::ElementState::Pressed,
    );
    state.render.cursor_world_pos = Vec2D::new(5.0, 8.0);

    state.select_operator_tool(OperatorTool::Thermostat);

    let commands = log.lock().expect("command log lock");
    assert_eq!(
        commands.last().map(String::as_str),
        Some("Operator(SetGhostWall { ghost: None })"),
        "got: {commands:?}"
    );
}

fn delete_hovered() -> InputAction {
    InputAction::Command(command_by_id("tools.delete_hovered").expect("registered"))
}
