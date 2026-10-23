// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::command_registry::{CommandDescriptor, CommandKind, UiAction, help_text};
use super::unified_render_state::UnifiedRenderState;
use thermo_runtime::application::ViewCommand;

/// Runs a registry command: a domain command goes to the physics thread, a
/// UI command is handled here. Returns true if the view changed.
pub(super) fn run_command(
    state: &mut UnifiedRenderState,
    command: &CommandDescriptor,
    window_target: Option<&winit::event_loop::ActiveEventLoop>,
    main_window: Option<&winit::window::Window>,
) -> bool {
    match command.kind {
        CommandKind::Domain(make) => {
            state.physics.send_command(make());
            false
        }
        CommandKind::Ui(action) => run_ui_action(state, action, window_target, main_window),
    }
}

/// Handles a renderer-side command. Returns true if the view changed.
fn run_ui_action(
    state: &mut UnifiedRenderState,
    action: UiAction,
    window_target: Option<&winit::event_loop::ActiveEventLoop>,
    main_window: Option<&winit::window::Window>,
) -> bool {
    match action {
        UiAction::PrintHelp => {
            log::info!("{}", help_text());
            false
        }
        UiAction::ResetView => {
            state.reset_view();
            true
        }
        UiAction::ToggleRendering => {
            let config = &mut state.render.config;
            config.toggle_rendering();
            // The physics thread builds only the layers that are drawn.
            state
                .physics
                .send_command(ViewCommand::SetSnapshotLayers(config.scene_layers()));
            // Redraw now with the new layers (back on: from the next full
            // snapshot; until it arrives, the light one is drawn light).
            true
        }
        UiAction::CycleThermoDisplay => {
            let config = &mut state.render.config;
            config.cycle_thermo_display();
            // The thermo fields are recorded only while shown.
            state
                .physics
                .send_command(ViewCommand::SetSnapshotLayers(config.scene_layers()));
            true
        }
        UiAction::CyclePlotContent => {
            state.render.config.cycle_plot_content();
            false
        }
        UiAction::ToggleEnergyWindow => {
            if let Some(target) = window_target {
                state.toggle_energy_window(target, main_window);
            }
            false
        }
        UiAction::DeleteHovered => {
            state.delete_hovered_object();
            false
        }
        // The ring display lives in the backend's ball pipeline, which
        // handles this command before it gets here (`wgpu_input`).
        UiAction::CycleCutoffRings => false,
    }
}
