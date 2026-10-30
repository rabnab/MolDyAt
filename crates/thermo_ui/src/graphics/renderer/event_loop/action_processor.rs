// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::command_registry::{CommandDescriptor, CommandKind, UiAction};
use super::unified_render_state::UnifiedRenderState;
use thermo_runtime::application::ViewCommand;

/// Runs a registry command: a domain command goes to the physics thread, a
/// UI command is handled here. Returns true if the view changed.
pub(super) fn run_command(state: &mut UnifiedRenderState, command: &CommandDescriptor) -> bool {
    match command.kind {
        CommandKind::Domain(_) => {
            if let Some(command) = command.domain_command() {
                state.physics.send_command(command);
            }
            false
        }
        CommandKind::Ui(action) => run_ui_action(state, action),
    }
}

/// Handles a renderer-side command. Returns true if the view changed.
fn run_ui_action(state: &mut UnifiedRenderState, action: UiAction) -> bool {
    match action {
        UiAction::PrintHelp => {
            log::info!("{}", state.input.key_map().help_text());
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
            // The backend opens the window (`take_plot_window_requests`).
            state.toggle_energy_window();
            false
        }
        UiAction::ResetPlotView => {
            state.render.energy_manager.reset_plot_view();
            false
        }
        UiAction::Plot(change) => {
            state.render.energy_manager.change_plots(change);
            false
        }
        UiAction::TogglePlotDistributions => {
            state.render.energy_manager.toggle_distributions();
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
