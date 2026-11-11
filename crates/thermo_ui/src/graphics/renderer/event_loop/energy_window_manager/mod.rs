// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Energy window manager: whether the plot window is open, data updates, and
//! when it is redrawn. The backend owns the OS window: it carries out
//! the [`PlotWindowRequest`]s this manager queues (open, close, title,
//! redraw), reports the window's size back, and draws the [`PlotFrame`] that
//! [`EnergyWindowManager::handle_redraw`] hands out.
//!
//! Input is in `keys` (the key map) and `gestures` (mouse, time view), what
//! is shown in `plots` (plots, distributions) and the redraw throttle in
//! `redraw`; this file holds the state they share.

pub(super) mod gestures;
pub(super) mod keys;
mod plots;
mod redraw;

use super::command_registry::PlotChange;
use super::input_handler::key_input;
use super::key_map::{HeldModifiers, KeyMap, KeyPhase};
use crate::graphics::energy_plot::status_text::status_text;
use crate::graphics::energy_plot::{
    DistributionData, PlotSelection, PlotWindowRequest, SharedEnergyData, TimeView,
};
use crate::graphics::renderer::event_loop::metrics::FrameMetrics;
use gestures::{ClickTargets, PlotGeometry, PlotPointer};
use keys::{EnergyWindowKey, energy_window_key};
pub use redraw::PlotFrame;
use std::time::Instant;
use thermo_physics::application::observables::Obs;
use thermo_runtime::application::renderer_snapshot::RendererSnapshot;
use winit::event::{ElementState, WindowEvent};

/// The plot window while it is open, as the backend reported it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PlotWindow {
    /// Inner size in physical pixels; `(0, 0)` until the backend reports
    /// it, and while the window is minimised.
    size: (u32, u32),
    scale_factor: f64,
}

/// Manages the energy plot window lifecycle and data updates.
pub struct EnergyWindowManager {
    /// The plot window while it is open (the backend owns the OS window).
    window: Option<PlotWindow>,
    /// What the backend is to do with the window, oldest first.
    requests: Vec<PlotWindowRequest>,
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
    /// The plots and legend entries of the last frame, for left clicks.
    targets: ClickTargets,
    /// The plots shown; kept when the window is closed and reopened.
    selection: PlotSelection,
    /// Ctrl and Alt as held over the energy window, for its chords
    /// (`Ctrl + P`, …).
    modifiers: HeldModifiers,
    pointer: PlotPointer,
    /// `EnergyData::history_resets` the view last saw: a new world history
    /// returns the view to its default.
    seen_history_resets: u64,
    /// The window shows the distributions instead of the time series.
    show_distributions: bool,
    /// The distributions shown and when they were computed.
    distributions: Option<(DistributionData, Instant)>,
    /// g(r) of the distributions view by the local density, not the mean.
    rdf_local: bool,
    /// The `rdf_local` the shown distributions were computed with (they fall
    /// back to the mean in a small world, whatever was asked).
    distributions_local: bool,
    /// g(r) pairs hidden in the distributions view, by name (`"N2-Ar"`);
    /// a click on a pair's legend entry hides or shows it. Kept when the
    /// window is closed and reopened, cleared with a new world history.
    hidden_pairs: Vec<String>,
}

impl EnergyWindowManager {
    /// Create a new energy window manager.
    ///
    /// # Arguments
    /// * `energy_data` - Shared energy data for the energy plot
    pub fn new(energy_data: SharedEnergyData) -> Self {
        Self {
            window: None,
            requests: Vec::new(),
            energy_data,
            last_redraw_instant: None,
            redraw_pending: false,
            view: TimeView::default(),
            geometry: None,
            targets: ClickTargets::default(),
            selection: PlotSelection::default(),
            modifiers: HeldModifiers::default(),
            pointer: PlotPointer::default(),
            seen_history_resets: 0,
            show_distributions: false,
            distributions: None,
            rdf_local: false,
            distributions_local: false,
            hidden_pairs: Vec::new(),
        }
    }

    /// Whether the plot window is open (or asked for).
    pub fn is_open(&self) -> bool {
        self.window.is_some()
    }

    /// What the backend is to do with the plot window, oldest first; the
    /// queue is empty afterwards.
    pub fn take_requests(&mut self) -> Vec<PlotWindowRequest> {
        std::mem::take(&mut self.requests)
    }

    /// The backend created the window asked for with
    /// [`PlotWindowRequest::Open`]: its inner size (physical pixels) and
    /// scale factor. It is drawn at once.
    pub fn plot_window_opened(&mut self, size: (u32, u32), scale_factor: f64) {
        if let Some(window) = &mut self.window {
            *window = PlotWindow { size, scale_factor };
            self.redraw_now();
        }
    }

    /// The backend could not create the window: it counts as closed.
    pub fn plot_window_failed(&mut self) {
        self.window = None;
        self.redraw_pending = false;
    }

    /// Open the plot window, or give the focused plot the next energy
    /// preset while it is open. A reopened window shows the plots it had.
    /// Opening queues a [`PlotWindowRequest::Open`] for the backend, which
    /// places the window on the split screen.
    pub fn toggle(&mut self) {
        if self.window.is_some() {
            self.change_plots(PlotChange::CyclePreset);
            return;
        }
        self.open();
    }

    fn open(&mut self) {
        self.window = Some(PlotWindow {
            size: (0, 0),
            scale_factor: 1.0,
        });
        self.redraw_pending = false;
        self.requests.push(PlotWindowRequest::Open {
            title: self.title(),
        });
    }

    /// Close the plot window (`ESC`, its close button): the backend drops
    /// it. The plots and the time view stay for the next opening.
    pub fn close(&mut self) {
        if self.window.take().is_some() {
            self.requests.push(PlotWindowRequest::Close);
        }
        self.redraw_pending = false;
        self.pointer.drag = None;
    }

    /// Handle a window event for the energy window.
    ///
    /// # Returns
    /// `true` if the event was handled, `false` if not an energy window event.
    /// Keys are looked up in `key_map` (see [`energy_window_key`]).
    pub fn handle_event(&mut self, event: WindowEvent, key_map: &KeyMap) -> bool {
        match event {
            WindowEvent::CloseRequested => {
                self.close();
                true
            }
            WindowEvent::Resized(size) => {
                if let Some(window) = &mut self.window {
                    window.size = (size.width, size.height);
                }
                self.redraw_now();
                true
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(window) = &mut self.window {
                    window.scale_factor = scale_factor;
                }
                self.redraw_now();
                true
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.modifiers = HeldModifiers {
                    ctrl: state.control_key(),
                    alt: state.alt_key(),
                };
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
                match energy_window_key(key_map, input, phase, self.modifiers) {
                    Some(EnergyWindowKey::Close) => self.close(),
                    Some(EnergyWindowKey::Plot(change)) => {
                        self.change_plots(change);
                    }
                    Some(EnergyWindowKey::ResetView) => self.reset_plot_view(),
                    Some(EnergyWindowKey::Distributions) => self.toggle_distributions(),
                    Some(EnergyWindowKey::RdfLocal) => self.toggle_rdf_local(),
                    None => return false,
                }
                true
            }
            event => self.handle_mouse_event(&event),
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
        self.update_distributions(snapshot);
        if let Ok(mut data) = self.energy_data.lock() {
            // A paused plot keeps its history so it can be examined: only a
            // following one takes new samples. A loaded world (a cleared
            // registry) ends the pause.
            if self.view.is_following() || data.registry_history_changed() {
                data.sync_from_metrics();
            }
            // A new world starts a new history: back to the default view,
            // and every g(r) pair shown again (its species may differ).
            if data.history_resets != self.seen_history_resets {
                self.seen_history_resets = data.history_resets;
                self.view.reset();
                self.geometry = None;
                self.hidden_pairs.clear();
            }

            // The time is not a metrics series: it comes with the snapshot.
            data.time = snapshot.frame.time;

            // The status lines, from the snapshot's numbers and the rates.
            data.status_text = snapshot.frame.status.is_some().then(|| {
                let metrics_line = format_metrics_line(
                    render_fps,
                    snapshot.frame.values[Obs::PhysicsFps] as f32,
                    steps_per_frame,
                    frame_metrics,
                );
                status_text(snapshot, &metrics_line).unwrap_or_default()
            });

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
