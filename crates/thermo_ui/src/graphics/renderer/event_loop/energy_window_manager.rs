// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Energy window manager: creating and closing the energy plot window, its
//! keys and data updates, and when it is redrawn. The backend draws the
//! [`PlotFrame`] that [`EnergyWindowManager::handle_redraw`] hands out.

use super::command_registry::{KeyInput, UiAction};
use super::input_handler::key_input;
use super::key_map::{HeldModifiers, KeyMap, KeyPhase};
use crate::graphics::energy_plot::{
    DisplayMode, EnergyPlotWindow, PlotScene, SharedEnergyData, split_screen_layout_for_monitor,
};
use crate::graphics::renderer::event_loop::metrics::FrameMetrics;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thermo_runtime::application::renderer_snapshot::RendererSnapshot;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::KeyCode;
use winit::window::{Window, WindowId};

/// What a key does while the energy window has the focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EnergyWindowKey {
    /// Close the window.
    Close,
    /// Cycle what the window plots.
    CycleDisplay,
}

/// The energy window's action for a key event. The keys come from the key
/// map, so a `ui.toml` rebinds them here too (#355): the keys of the plot
/// commands (`view.cycle_plot`, `view.toggle_energy_window`; `P` and `TAB`
/// by default) cycle what the window plots, each on its own trigger. `ESC`
/// closes the window, as the close shortcut of the window itself, not a
/// registry command (in the main window `ESC` clears the balls).
pub(super) fn energy_window_key(
    key_map: &KeyMap,
    input: KeyInput,
    phase: KeyPhase,
) -> Option<EnergyWindowKey> {
    if phase == KeyPhase::Down && input.code == KeyCode::Escape {
        return Some(EnergyWindowKey::Close);
    }
    let (command, _) = key_map.command_for_key(input, phase, HeldModifiers::default())?;
    match command.ui_action()? {
        UiAction::CyclePlotContent | UiAction::ToggleEnergyWindow => {
            Some(EnergyWindowKey::CycleDisplay)
        }
        _ => None,
    }
}

/// One redraw of the energy window: the window to draw into and what to draw.
pub struct PlotFrame {
    pub window: Arc<Window>,
    pub scene: PlotScene,
}

/// Manages the energy plot window lifecycle and data updates.
pub struct EnergyWindowManager {
    /// The energy plot window (if open)
    window: Option<EnergyPlotWindow>,
    /// Shared energy data (shared with window)
    energy_data: SharedEnergyData,
    /// Instant of the most recent energy window render completion.
    last_redraw_instant: Option<Instant>,
    /// `true` while a `request_redraw()` has been sent but the corresponding
    /// `RedrawRequested` event has not yet been handled.  Prevents queuing
    /// more than one pending redraw at a time.
    redraw_pending: bool,
}

/// Minimum interval between energy-window redraws (≈12 Hz).
///
/// Every `RedrawRequested` event lays the plot out anew (up to a few thousand
/// segments) and presents a frame. Throttling to this interval keeps that
/// from stealing frame time from the simulation window when the OS services
/// the energy window more aggressively (e.g., when it has input focus).
const ENERGY_REDRAW_MIN_INTERVAL: Duration = Duration::from_millis(83);

impl EnergyWindowManager {
    /// Create a new energy window manager.
    ///
    /// # Arguments
    /// * `energy_data` - Shared energy data for the energy plot
    pub fn new(energy_data: SharedEnergyData) -> Self {
        Self {
            window: None,
            energy_data,
            last_redraw_instant: None,
            redraw_pending: false,
        }
    }

    /// Get the window ID if the energy window is open.
    pub fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().and_then(|w| w.window_id())
    }

    /// Check if this window event is for the energy window.
    pub fn is_energy_window_event(&self, window_id: WindowId) -> bool {
        log::trace!(
            "Checking if event for window {:?} matches energy window {:?}",
            window_id,
            self.window_id()
        );
        self.window_id() == Some(window_id)
    }

    /// Toggle or cycle the energy window.
    ///
    /// If no window exists, creates one with default mode.
    /// If window exists and is open, cycles display mode.
    /// If window was closed, reopens with last mode.
    pub fn toggle(&mut self, window_target: &ActiveEventLoop, main_window: Option<&Window>) {
        let split_layout = main_window
            .and_then(|window| window.current_monitor())
            .or_else(|| window_target.primary_monitor())
            .map(|monitor| split_screen_layout_for_monitor(&monitor));

        if self.window.is_none() {
            // Create and open energy window with default mode
            let mut window =
                EnergyPlotWindow::new(self.energy_data.clone(), DisplayMode::default());
            if let Err(e) = window.create(window_target, split_layout.map(|layout| layout.energy)) {
                eprintln!("Failed to create energy window: {}", e);
                return;
            }
            self.window = Some(window);
        } else if let Some(window) = &mut self.window {
            if window.is_open() {
                // Cycle display mode instead of closing
                window.cycle_display_mode();
                window.request_redraw();
            } else {
                // Reopen if closed with last mode
                let last_mode = window.get_display_mode();
                let mut new_window = EnergyPlotWindow::new(self.energy_data.clone(), last_mode);
                if let Err(e) =
                    new_window.create(window_target, split_layout.map(|layout| layout.energy))
                {
                    eprintln!("Failed to create energy window: {}", e);
                    return;
                }
                self.window = Some(new_window);
            }
        }
    }

    /// Close the energy window.
    pub fn close(&mut self) {
        self.window = None;
    }

    /// Handle a window event for the energy window.
    ///
    /// # Returns
    /// `true` if the event was handled, `false` if not an energy window event.
    /// Keys are looked up in `key_map` (see [`energy_window_key`]).
    pub fn handle_event(&mut self, event: WindowEvent, key_map: &KeyMap) -> bool {
        match event {
            WindowEvent::CloseRequested => {
                self.window = None;
                true
            }
            WindowEvent::Resized(size) => {
                if let Some(window) = &mut self.window {
                    window.handle_resize(size.width, size.height);
                }
                true
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let Some(input) = key_input(&event) else {
                    return false;
                };
                let phase = match event.state {
                    ElementState::Pressed => KeyPhase::Down,
                    ElementState::Released => KeyPhase::Up,
                };
                match energy_window_key(key_map, input, phase) {
                    Some(EnergyWindowKey::Close) => self.window = None,
                    Some(EnergyWindowKey::CycleDisplay) => {
                        if let Some(window) = &mut self.window {
                            window.cycle_display_mode();
                            window.request_redraw();
                        }
                    }
                    None => return false,
                }
                true
            }
            _ => false,
        }
    }

    /// Handle a redraw request for the energy window: the frame to draw.
    ///
    /// Records the time and clears the pending flag so subsequent
    /// [`Self::request_redraw`] calls can schedule the next redraw. Both
    /// state updates happen only while a window is open, so a spurious call
    /// with no window leaves the throttle state clean.
    ///
    /// # Returns
    /// `None` without a window, or while it is minimised.
    pub fn handle_redraw(&mut self) -> Option<PlotFrame> {
        log::trace!("EnergyWindowManager: handle_redraw called");
        let window = self.window.as_ref()?;
        let handle = Arc::clone(window.window_handle()?);
        self.redraw_pending = false;
        self.last_redraw_instant = Some(Instant::now());
        Some(PlotFrame {
            window: handle,
            scene: window.scene()?,
        })
    }

    /// Request a redraw for the energy window if it is open.
    ///
    /// Requests are **throttled** to at most [`ENERGY_REDRAW_MIN_INTERVAL`] and
    /// **coalesced** so that at most one redraw is pending at any time.  This
    /// prevents the OS (especially when the energy window has input focus) from
    /// driving the plot redraws faster than needed.
    pub fn request_redraw(&mut self) {
        if self.window.is_none() {
            return;
        }
        // Skip if a redraw is already queued but not yet serviced.
        if self.redraw_pending {
            return;
        }
        // Skip if the minimum interval since the last completed render hasn't
        // elapsed yet.
        if let Some(last) = self.last_redraw_instant
            && last.elapsed() < ENERGY_REDRAW_MIN_INTERVAL
        {
            // This intentionally drops the redraw instead of deferring it.
            // If snapshots stop arriving immediately after a render, the most
            // recent sample may not be shown until a future update triggers
            // another request. That tradeoff is acceptable here because the
            // energy window is best-effort diagnostics, and throttling avoids
            // stealing frame time from the main simulation.
            return;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
            self.redraw_pending = true;
        }
    }

    /// Update energy data from a snapshot.
    ///
    /// The status text follows every snapshot (also while paused, when edits
    /// change ball counts and toggles). The plot history comes from the
    /// MetricsRegistry, which samples a step once and starts afresh for a
    /// loaded world, so a paused world adds no duplicate points.
    ///
    /// # Arguments
    /// * `snapshot` - The renderer snapshot with energy data
    /// * `render_fps` - Smoothed render FPS for status text
    /// * `steps_per_frame` - Physics steps per render frame for status text
    /// * `frame_metrics` - Per-frame metrics (draw calls, frame time, etc.)
    pub fn update_from_snapshot(
        &mut self,
        snapshot: &RendererSnapshot,
        render_fps: f32,
        steps_per_frame: f32,
        frame_metrics: &FrameMetrics,
    ) {
        if let Ok(mut data) = self.energy_data.lock() {
            data.sync_from_metrics();

            // The time is not a metrics series: it comes with the snapshot.
            data.time = snapshot.time;

            // Update status text with FPS and per-frame metrics
            if let Some(ref status) = snapshot.status_text {
                let fps_line = format_metrics_line(
                    render_fps,
                    snapshot.physics_fps,
                    steps_per_frame,
                    frame_metrics,
                );
                let updated_status = status.replace("fps 0", &fps_line);
                data.status_text = Some(updated_status);
            } else {
                data.status_text = None;
            }

            // Release lock before requesting redraw
            drop(data);
        }
    }

    /// Get a reference to the shared energy data.
    pub fn energy_data(&self) -> &SharedEnergyData {
        &self.energy_data
    }

    /// Returns `true` when a `request_redraw()` call has been sent to the OS
    /// but the corresponding `RedrawRequested` event has not yet been handled.
    ///
    /// Exposed primarily for testing.
    #[cfg(test)]
    pub fn is_redraw_pending(&self) -> bool {
        self.redraw_pending
    }

    /// Override the last-redraw instant (test helper).
    ///
    /// Allows tests to simulate a recent or past render without needing a real
    /// OS window.
    #[cfg(test)]
    pub fn set_last_redraw_instant(&mut self, instant: Option<Instant>) {
        self.last_redraw_instant = instant;
    }

    /// Manually put the manager into the "redraw pending" state (test helper).
    ///
    /// Simulates the effect of a successful `request_redraw()` call so that
    /// coalescing behaviour can be verified independently of OS windows.
    #[cfg(test)]
    pub fn mark_redraw_pending(&mut self) {
        self.redraw_pending = true;
    }
}

/// Format the metrics replacement for the `"fps 0"` placeholder.
///
/// Produces two lines:
/// 1. FPS & throughput:  `render: 60 fps (16.7ms) | sim: 1200 fps | steps/frame: 20.0`
/// 2. Draw-call stats:   `draws: 3 | balls: 100`
fn format_metrics_line(
    render_fps: f32,
    physics_fps: f32,
    steps_per_frame: f32,
    m: &FrameMetrics,
) -> String {
    format!(
        "render: {} fps ({:.1}ms) | sim: {:.0} fps | steps/frame: {:.1}\n\
         draws: {} | balls: {}",
        render_fps as u32,
        m.frame_time_ms,
        physics_fps,
        steps_per_frame,
        m.draw_calls,
        m.balls_rendered,
    )
}
