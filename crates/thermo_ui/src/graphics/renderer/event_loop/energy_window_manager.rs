// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Energy window manager: creating and closing the energy plot window, its
//! keys and mouse gestures, data updates, and when it is redrawn. The
//! backend draws the [`PlotFrame`] that [`EnergyWindowManager::handle_redraw`]
//! hands out.
//!
//! Mouse (#367): the scroll wheel zooms the plot's time range around the
//! pointer, a middle-button drag pans it, a middle-button double click (or
//! the key of `view.reset_plot_view`) returns to the default view.

use super::command_registry::{KeyInput, UiAction, command_by_id};
use super::input_handler::key_input;
use super::key_map::{HeldModifiers, KeyMap, KeyPhase};
use super::operator_tool_handler::scroll_y_from_delta;
use crate::graphics::energy_plot::{
    DisplayMode, EnergyPlotWindow, PlotRect, PlotScene, SharedEnergyData, TimeHistory, TimeView,
    split_screen_layout_for_monitor,
};
use crate::graphics::renderer::event_loop::metrics::FrameMetrics;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thermo_runtime::application::renderer_snapshot::RendererSnapshot;
use winit::event::{ElementState, MouseButton, WindowEvent};
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
    /// Return the plot to its default time view.
    ResetView,
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
        UiAction::ResetPlotView => Some(EnergyWindowKey::ResetView),
        _ => None,
    }
}

/// The key hints the energy window shows, from `key_map`: `Esc` closes it
/// (fixed), and the keys of the plot commands cycle what it plots (see
/// [`energy_window_key`]), so a rebinding in `ui.toml` shows here too.
pub(super) fn energy_window_key_hints(key_map: &KeyMap) -> String {
    let mode_keys: Vec<String> = ["view.toggle_energy_window", "view.cycle_plot"]
        .into_iter()
        .filter_map(command_by_id)
        .flat_map(|command| key_map.binding_display(command))
        .fold(Vec::new(), |mut keys, key| {
            if !keys.contains(&key) {
                keys.push(key);
            }
            keys
        });
    let mut hints = "Esc: close".to_string();
    if !mode_keys.is_empty() {
        hints.push_str(&format!("  {}: mode", mode_keys.join(" / ")));
    }
    let reset_keys = command_by_id("view.reset_plot_view")
        .map(|command| key_map.binding_display(command))
        .unwrap_or_default();
    if !reset_keys.is_empty() {
        hints.push_str(&format!("  {}: reset", reset_keys.join(" / ")));
    }
    hints
}

/// Two middle-button presses at most this far apart in time and space are
/// a double click (reset the view).
const DOUBLE_CLICK_TIME: Duration = Duration::from_millis(400);
const DOUBLE_CLICK_DISTANCE_PX: f32 = 6.0;

/// Where the plot was in the last frame: turns pointer pixels into times.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PlotGeometry {
    pub(super) area: PlotRect,
    pub(super) history: TimeHistory,
}

/// The pointer over the energy window.
#[derive(Debug, Default)]
struct PlotPointer {
    /// Last cursor position, window pixels.
    cursor: Option<[f32; 2]>,
    /// x of the cursor at the last pan step while the middle button is held.
    drag_x: Option<f32>,
    /// Time and place of the last middle-button press, for double clicks.
    last_middle_press: Option<(Instant, [f32; 2])>,
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
    /// The plot's time range; kept when the window is closed and reopened.
    view: TimeView,
    /// The plot geometry of the last frame, for the mouse gestures.
    geometry: Option<PlotGeometry>,
    pointer: PlotPointer,
    /// `EnergyData::history_resets` the view last saw: a new world history
    /// returns the view to its default.
    seen_history_resets: u64,
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
            view: TimeView::default(),
            geometry: None,
            pointer: PlotPointer::default(),
            seen_history_resets: 0,
        }
    }

    /// The plot's time range.
    pub fn plot_view(&self) -> &TimeView {
        &self.view
    }

    /// Return the plot to its default time view and show it now.
    pub fn reset_plot_view(&mut self) {
        if !self.view.is_default() {
            self.view.reset();
            self.redraw_now();
        }
    }

    /// Zoom the plot's time range by `notches` wheel notches (positive: in)
    /// around the pointer at `cursor_x` (window pixels), in the geometry of
    /// the last frame. Returns whether the view changed.
    pub(super) fn zoom_plot(&mut self, notches: f64, cursor_x: Option<f32>) -> bool {
        let Some(geometry) = self.geometry else {
            return false;
        };
        let area = geometry.area;
        let anchor = cursor_x.map_or(1.0, |x| {
            (f64::from(x) - f64::from(area.left)) / f64::from(area.width())
        });
        self.change_view(|view| view.zoom(notches, anchor, &geometry.history))
    }

    /// Pan the plot by a pointer move of `dx_px` window pixels: the curves
    /// follow the pointer, so moving right shows earlier times. Returns
    /// whether the view changed.
    pub(super) fn pan_plot(&mut self, dx_px: f32) -> bool {
        let Some(geometry) = self.geometry else {
            return false;
        };
        let dt = -f64::from(dx_px) / f64::from(geometry.area.width()) * self.view.span();
        self.change_view(|view| view.pan(dt, &geometry.history))
    }

    /// The geometry of the last frame (tests).
    #[cfg(test)]
    pub(super) fn set_plot_geometry(&mut self, geometry: Option<PlotGeometry>) {
        self.geometry = geometry;
    }

    fn change_view(&mut self, change: impl FnOnce(&mut TimeView)) -> bool {
        let before = self.view;
        change(&mut self.view);
        let changed = self.view != before;
        if changed {
            self.redraw_now();
        }
        changed
    }

    /// Redraw promptly after a view change, not waiting for the throttle that
    /// paces redraws for new samples.
    fn redraw_now(&mut self) {
        if let Some(window) = &self.window {
            window.request_redraw();
            self.redraw_pending = true;
        }
    }

    /// A middle-button press at `at`: a double click resets the view,
    /// otherwise a pan starts.
    pub(super) fn middle_pressed(&mut self, at: [f32; 2], now: Instant) {
        let double = self.pointer.last_middle_press.is_some_and(|(time, place)| {
            now.duration_since(time) <= DOUBLE_CLICK_TIME
                && (at[0] - place[0]).hypot(at[1] - place[1]) <= DOUBLE_CLICK_DISTANCE_PX
        });
        if double {
            self.pointer.last_middle_press = None;
            self.pointer.drag_x = None;
            self.reset_plot_view();
        } else {
            self.pointer.last_middle_press = Some((now, at));
            self.pointer.drag_x = Some(at[0]);
        }
    }

    /// The pointer moved to `at`; while the middle button is held this pans.
    pub(super) fn pointer_moved(&mut self, at: [f32; 2]) {
        self.pointer.cursor = Some(at);
        if let Some(from) = self.pointer.drag_x {
            self.pointer.drag_x = Some(at[0]);
            self.pan_plot(at[0] - from);
        }
    }

    /// The mouse gestures of the energy window. Returns whether the event
    /// was one.
    fn handle_mouse_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::MouseWheel { delta, .. } => {
                let notches = f64::from(scroll_y_from_delta(*delta));
                let cursor_x = self.pointer.cursor.map(|[x, _]| x);
                self.zoom_plot(notches, cursor_x);
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Middle,
                ..
            } => match state {
                ElementState::Pressed => {
                    let at = self.pointer.cursor.unwrap_or_default();
                    self.middle_pressed(at, Instant::now());
                }
                ElementState::Released => self.pointer.drag_x = None,
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_moved([position.x as f32, position.y as f32]);
            }
            WindowEvent::CursorLeft { .. } => self.pointer.cursor = None,
            // A release while the window had no focus never arrives.
            WindowEvent::Focused(false) => self.pointer.drag_x = None,
            _ => return false,
        }
        true
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
                    Some(EnergyWindowKey::ResetView) => self.reset_plot_view(),
                    None => return false,
                }
                true
            }
            event => self.handle_mouse_event(&event),
        }
    }

    /// Handle a redraw request for the energy window: the frame to draw.
    ///
    /// Records the time and clears the pending flag so subsequent
    /// [`Self::request_redraw`] calls can schedule the next redraw. Both
    /// state updates happen only while a window is open, so a spurious call
    /// with no window leaves the throttle state clean.
    ///
    /// The key hints come from `key_map` (see [`energy_window_key_hints`]).
    ///
    /// # Returns
    /// `None` without a window, or while it is minimised.
    pub fn handle_redraw(&mut self, key_map: &KeyMap) -> Option<PlotFrame> {
        log::trace!("EnergyWindowManager: handle_redraw called");
        let window = self.window.as_ref()?;
        let handle = Arc::clone(window.window_handle()?);
        self.redraw_pending = false;
        self.last_redraw_instant = Some(Instant::now());
        let scene = window.scene(&energy_window_key_hints(key_map), &self.view)?;
        self.geometry = scene
            .plot_area
            .zip(scene.history)
            .map(|(area, history)| PlotGeometry { area, history });
        Some(PlotFrame {
            window: handle,
            scene,
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
            // A new world starts a new history: back to the default view.
            if data.history_resets != self.seen_history_resets {
                self.seen_history_resets = data.history_resets;
                self.view.reset();
                self.geometry = None;
            }

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
