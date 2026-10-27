// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::margins::Margins;
use crate::graphics::renderer::state_layers::RendererConfig;
use std::sync::{Arc, Mutex};
use thermo_physics::objects2d::{Rect, Vec2D};
use thermo_runtime::application::renderer_snapshot::OperatorSnapshot;
use thermo_runtime::application::scene_layers::SceneLayers;

use crate::graphics::renderer::event_loop::energy_window_manager::EnergyWindowManager;
use crate::graphics::renderer::event_loop::input_handler::{
    InputAction, InputHandler, OperatorTool,
};
use crate::graphics::renderer::event_loop::metrics::MetricsCollector;
use crate::graphics::renderer::event_loop::physics_coordinator::PhysicsCoordinator;

use crate::graphics::renderer::event_loop::command_registry::command_by_id;
use crate::graphics::renderer::event_loop::unified_render_state::{
    RenderStateData, UnifiedRenderState,
};
use thermo_runtime::application::SimControl;
use thermo_runtime::application::{DomainCommand, ViewCommand};

// ── Helpers ──────────────────────────────────────────────────────

/// Build a minimal `UnifiedRenderState` without spawning a real
/// physics thread (which needs a full `MainContent`).  We test the
/// pure-logic helpers here; integration tests cover the full path.
fn dummy_viewport() -> ViewportTransform {
    let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
    ViewportTransform::new(world, (1280.0, 720.0), Margins::symmetric(50.0, 50.0))
}

// ── Action dispatch ──────────────────────────────────────────────

fn command(id: &str) -> InputAction {
    InputAction::Command(command_by_id(id).expect("registered command"))
}

#[test]
fn zoom_and_pan_mark_view_dirty() {
    let actions = [
        InputAction::ZoomIn {
            center: Vec2D::zero(),
        },
        InputAction::ZoomOut {
            center: Vec2D::zero(),
        },
        InputAction::PanUpdate {
            delta: Vec2D::new(5.0, 3.0),
        },
    ];
    for action in actions {
        let mut state = build_test_state();
        let label = format!("{action:?}");
        assert!(state.process_action(action), "{label}");
        assert!(state.render.view_dirty, "{label}");
    }
}

#[test]
fn reset_view_marks_view_dirty() {
    let mut state = build_test_state();
    assert!(state.process_action(command("view.reset")));
    assert!(state.render.view_dirty);
}

#[test]
fn toggle_rendering_flips_flag_and_tells_the_physics_thread() {
    let (physics, log) = PhysicsCoordinator::new_recording();
    let mut state = build_state_with(physics);
    // Redraws at once with the new layers.
    let affects = state.process_action(command("view.toggle_rendering"));
    assert!(affects);
    assert!(!state.render.config.render_drawing);
    // Toggle back.
    state.process_action(command("view.toggle_rendering"));
    assert!(state.render.config.render_drawing);

    // The physics thread builds only the layers that are drawn.
    let sent = log.lock().unwrap().clone();
    // Back on with the thermo overlay off: everything but the thermo fields.
    let expected = [
        SceneLayers::BASE,
        SceneLayers::ALL.difference(SceneLayers::THERMO),
    ]
    .map(|layers| {
        format!(
            "{:?}",
            DomainCommand::View(ViewCommand::SetSnapshotLayers(layers))
        )
    });
    assert_eq!(sent, expected);
}

#[test]
fn cycling_the_thermo_overlay_requests_thermo_fields_only_while_shown() {
    let (physics, log) = PhysicsCoordinator::new_recording();
    let mut state = build_state_with(physics);

    // Off -> Grid -> Temperature -> Pressure -> Off
    for _ in 0..4 {
        state.process_action(command("view.cycle_thermo_overlay"));
    }

    let without_thermo = SceneLayers::ALL.difference(SceneLayers::THERMO);
    let expected = [
        without_thermo,
        SceneLayers::ALL,
        SceneLayers::ALL,
        without_thermo,
    ]
    .map(|layers| {
        format!(
            "{:?}",
            DomainCommand::View(ViewCommand::SetSnapshotLayers(layers))
        )
    });
    assert_eq!(log.lock().unwrap().clone(), expected);
}

#[test]
fn renderer_specific_commands_do_not_affect_view() {
    for id in [
        "view.cycle_plot",
        "view.toggle_energy_window",
        "view.cycle_cutoff_rings",
        "help.show",
    ] {
        let (physics, log) = PhysicsCoordinator::new_recording();
        let mut state = build_state_with(physics);
        assert!(
            !state.process_action(command(id)),
            "{id} should not affect view"
        );
        assert!(
            log.lock().unwrap().is_empty(),
            "{id} must not reach physics"
        );
    }
}

#[test]
fn domain_commands_go_to_the_physics_thread() {
    let (physics, log) = PhysicsCoordinator::new_recording();
    let mut state = build_state_with(physics);
    assert!(!state.process_action(command("sim.toggle_pause")));
    assert_eq!(
        log.lock().unwrap().clone(),
        [format!("{:?}", DomainCommand::Sim(SimControl::TogglePause))]
    );
}

// ── Viewport helper tests ───────────────────────────────────────

#[test]
fn zoom_at_point_marks_dirty() {
    let mut vp = dummy_viewport();
    let initial_zoom = vp.zoom();
    vp.zoom_at_point(&Vec2D::zero(), 2.0);
    assert!((vp.zoom() - initial_zoom * 2.0).abs() < 0.01);
}

#[test]
fn reset_restores_defaults() {
    let mut vp = dummy_viewport();
    vp.zoom_at_point(&Vec2D::zero(), 5.0);
    vp.reset();
    assert!((vp.zoom() - 1.0).abs() < 0.01);
    assert!(vp.pan_offset().norm_sq() < 0.01);
}

// ── Stub coordinator (no real physics thread) ───────────────────

/// A `PhysicsCoordinator` whose command channel is disconnected so
/// that `send_command` silently drops.  Good enough for unit tests
/// that only exercise action-dispatch logic.
fn make_disconnected_coordinator() -> PhysicsCoordinator {
    PhysicsCoordinator::new_disconnected()
}

fn build_test_state() -> UnifiedRenderState {
    build_state_with(make_disconnected_coordinator())
}

fn build_state_with(physics: PhysicsCoordinator) -> UnifiedRenderState {
    let energy_data: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));
    UnifiedRenderState {
        input: InputHandler::new(),
        physics,
        render: RenderStateData {
            viewport: dummy_viewport(),
            energy_manager: EnergyWindowManager::new(energy_data),
            config: RendererConfig::default(),
            view_dirty: false,
            last_snapshot: None,
            cursor_world_pos: Vec2D::zero(),
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

#[test]
fn selecting_ball_tool_enables_cursor_preview_and_marks_view_dirty() {
    let mut state = build_test_state();

    let affects_view = state.process_action(InputAction::SelectOperatorTool(OperatorTool::Ball));

    assert!(!affects_view);
    assert_eq!(state.render.active_tool, OperatorTool::Ball);
    assert!(state.render.config.draw_cursor);
    assert!(state.render.view_dirty);
}

#[test]
fn switching_away_from_ball_tool_disables_cursor_preview() {
    let mut state = build_test_state();
    state.render.active_tool = OperatorTool::Ball;
    state.render.config.draw_cursor = true;

    let affects_view =
        state.process_action(InputAction::SelectOperatorTool(OperatorTool::LineSegment));

    assert!(!affects_view);
    assert_eq!(state.render.active_tool, OperatorTool::LineSegment);
    assert!(!state.render.config.draw_cursor);
    assert!(state.render.view_dirty);
}

#[test]
fn next_species_marks_view_dirty_when_ball_tool_is_active() {
    let mut state = build_test_state();
    state.render.active_tool = OperatorTool::Ball;

    let affects_view = state.process_action(InputAction::NextSpecies);

    assert!(!affects_view);
    assert!(state.render.view_dirty);
}
