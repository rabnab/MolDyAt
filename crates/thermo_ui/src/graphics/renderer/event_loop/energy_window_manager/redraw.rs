// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! When the energy window is redrawn, and the frame it is drawn with.

use super::gestures::{ClickTargets, PlotGeometry};
use super::keys::{distribution_key_hints, energy_window_key_hints};
use super::{EnergyWindowManager, PlotWindow};
use crate::graphics::energy_plot::{PlotScene, PlotWindowRequest};
use crate::graphics::renderer::event_loop::key_map::KeyMap;
use std::time::{Duration, Instant};

/// One redraw of the plot window: what to draw into the backend's window.
pub struct PlotFrame {
    pub scene: PlotScene,
}

/// Minimum interval between energy-window redraws (≈12 Hz).
///
/// Every `RedrawRequested` event lays the plot out anew (up to a few thousand
/// segments) and presents a frame. Throttling to this interval keeps that
/// from stealing frame time from the simulation window when the OS services
/// the energy window more aggressively (e.g., when it has input focus).
const ENERGY_REDRAW_MIN_INTERVAL: Duration = Duration::from_millis(83);

impl EnergyWindowManager {
    /// Redraw promptly after a view change, not waiting for the throttle that
    /// paces redraws for new samples.
    pub(super) fn redraw_now(&mut self) {
        if self.window.is_some() {
            self.requests.push(PlotWindowRequest::Redraw);
            self.redraw_pending = true;
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
        let window = *self.window.as_ref()?;
        self.redraw_pending = false;
        self.last_redraw_instant = Some(Instant::now());
        let hints = if self.show_distributions {
            distribution_key_hints(key_map)
        } else {
            energy_window_key_hints(key_map)
        };
        let scene = self.scene(window, &hints)?;
        self.geometry = scene
            .plot_area
            .zip(scene.history)
            .map(|(area, history)| PlotGeometry { area, history });
        self.targets = ClickTargets::of(&scene);
        Some(PlotFrame { scene })
    }

    /// What the window shows now, laid out for its size; `None` while its
    /// size is not known yet or it is minimised.
    fn scene(&self, window: PlotWindow, key_hints: &str) -> Option<PlotScene> {
        let (width, height) = window.size;
        if width == 0 || height == 0 {
            return None;
        }
        if self.show_distributions {
            return Some(PlotScene::build_distributions(
                self.distributions.as_ref().map(|(data, ..)| data),
                &self.hidden_pairs,
                window.size,
                window.scale_factor,
                key_hints,
            ));
        }
        let data = self.energy_data.lock().unwrap_or_else(|e| e.into_inner());
        Some(PlotScene::build(
            &data,
            &self.selection,
            window.size,
            window.scale_factor,
            key_hints,
            &self.view,
        ))
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
        self.requests.push(PlotWindowRequest::Redraw);
        self.redraw_pending = true;
    }
}
