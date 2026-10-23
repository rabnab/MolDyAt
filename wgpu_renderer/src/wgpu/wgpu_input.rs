// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Input action execution for WGPU renderer.
//!
//! Handles actions that target a render pipeline (ring display mode).
//! All other actions are delegated to UnifiedRenderState.

use thermo_ui::graphics::renderer::InputAction;
use thermo_ui::graphics::renderer::event_loop::{UiAction, UnifiedRenderState};

use super::wgpu_pipelines::WgpuPipelines;

/// Execute input actions from the centralized InputHandler.
///
/// Only handles actions that target a render pipeline.
/// All other actions (including ToggleEnergyWindow) are delegated to UnifiedRenderState.
///
/// Returns `true` if the action affects the viewport.
pub fn execute_input_action(
    action: InputAction,
    state: &mut UnifiedRenderState,
    pipelines: &mut WgpuPipelines,
    window_target: &winit::event_loop::ActiveEventLoop,
    main_window: &winit::window::Window,
) -> bool {
    // Handle ONLY actions that target a render pipeline
    if action.ui_action() == Some(UiAction::CycleCutoffRings) {
        pipelines.cycle_ring_display_mode();
        return false; // doesn't affect view
    }

    // Delegate all other actions to UnifiedRenderState (single source of truth)
    // Pass window_target so ToggleEnergyWindow can be handled
    state.process_action(action, Some(window_target), Some(main_window))
}
