// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Mouse input of the energy window: the scroll wheel zooms the
//! plot's time range around the pointer, a middle-button drag pans it, a
//! middle-button double click (or the key of `view.reset_plot_view`) returns
//! to the default view.

use super::EnergyWindowManager;
use crate::graphics::energy_plot::{
    LegendEntry, PanelArea, PlotRect, PlotScene, TimeHistory, TimeView,
};
use crate::graphics::renderer::event_loop::operator_tool_handler::scroll_y_from_delta;
use std::time::{Duration, Instant};
use winit::event::{ElementState, MouseButton, WindowEvent};

/// Two middle-button presses at most this far apart in time and space are
/// a double click (reset the view).
const DOUBLE_CLICK_TIME: Duration = Duration::from_millis(400);
const DOUBLE_CLICK_DISTANCE_PX: f32 = 6.0;

/// A middle-button drag pans only once the pointer has moved this far from
/// the press, so a click with a little jitter (or the first click of a
/// double click) does not pause the plot.
const PAN_THRESHOLD_PX: f32 = 4.0;

/// A middle-button drag in progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Drag {
    /// x of the press.
    origin_x: f32,
    /// x at the last pan step, once the drag passed [`PAN_THRESHOLD_PX`].
    panned_to: Option<f32>,
}

/// Where the plot was in the last frame: turns pointer pixels into times.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in super::super) struct PlotGeometry {
    pub(in super::super) area: PlotRect,
    pub(in super::super) history: TimeHistory,
}

/// Where the plots and their legend entries were in the last frame, for
/// left clicks.
#[derive(Debug, Clone, Default, PartialEq)]
pub(in super::super) struct ClickTargets {
    pub(in super::super) panels: Vec<PanelArea>,
    pub(in super::super) legend: Vec<LegendEntry>,
}

impl ClickTargets {
    pub(super) fn of(scene: &PlotScene) -> Self {
        Self {
            panels: scene.panels.clone(),
            legend: scene.legend.clone(),
        }
    }
}

/// The pointer over the energy window.
#[derive(Debug, Default)]
pub(super) struct PlotPointer {
    /// Last cursor position, window pixels.
    cursor: Option<[f32; 2]>,
    /// The drag while the middle button is held.
    pub(super) drag: Option<Drag>,
    /// Time and place of the last middle-button press, for double clicks.
    last_middle_press: Option<(Instant, [f32; 2])>,
}

impl EnergyWindowManager {
    /// The plot's time range.
    pub fn plot_view(&self) -> &TimeView {
        &self.view
    }

    /// Return the plot to its default time view and show it now.
    pub fn reset_plot_view(&mut self) {
        if !self.view.is_default() {
            let was_following = self.view.is_following();
            self.view.reset();
            self.resume_if_following(was_following);
            self.redraw_now();
        }
    }

    /// Zoom the plot's time range by `notches` wheel notches (positive: in)
    /// around the pointer at `cursor_x` (window pixels), in the geometry of
    /// the last frame. Returns whether the view changed.
    pub(in super::super) fn zoom_plot(&mut self, notches: f64, cursor_x: Option<f32>) -> bool {
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
    pub(in super::super) fn pan_plot(&mut self, dx_px: f32) -> bool {
        let Some(geometry) = self.geometry else {
            return false;
        };
        let dt = -f64::from(dx_px) / f64::from(geometry.area.width()) * self.view.span();
        self.change_view(|view| view.pan(dt, &geometry.history))
    }

    /// The geometry of the last frame (tests).
    #[cfg(test)]
    pub(in super::super) fn set_plot_geometry(&mut self, geometry: Option<PlotGeometry>) {
        self.geometry = geometry;
    }

    /// The click targets of the last frame (tests).
    #[cfg(test)]
    pub(in super::super) fn set_click_targets(&mut self, targets: ClickTargets) {
        self.targets = targets;
    }

    fn change_view(&mut self, change: impl FnOnce(&mut TimeView)) -> bool {
        let before = self.view;
        change(&mut self.view);
        let changed = self.view != before;
        if changed {
            self.resume_if_following(before.is_following());
            self.redraw_now();
        }
        changed
    }

    /// A paused plot keeps the history it had when the pause began (see
    /// [`Self::update_from_snapshot`]); back in follow mode it takes the
    /// current series at once, not only with the next snapshot (a paused
    /// simulation sends none).
    fn resume_if_following(&mut self, was_following: bool) {
        if was_following || !self.view.is_following() {
            return;
        }
        if let Ok(mut data) = self.energy_data.lock() {
            data.sync_from_metrics();
            self.seen_history_resets = data.history_resets;
        }
    }

    /// A middle-button press at `at`: a double click resets the view,
    /// otherwise a pan starts.
    pub(in super::super) fn middle_pressed(&mut self, at: [f32; 2], now: Instant) {
        let double = self.pointer.last_middle_press.is_some_and(|(time, place)| {
            now.duration_since(time) <= DOUBLE_CLICK_TIME
                && (at[0] - place[0]).hypot(at[1] - place[1]) <= DOUBLE_CLICK_DISTANCE_PX
        });
        if double {
            self.pointer.last_middle_press = None;
            self.pointer.drag = None;
            self.reset_plot_view();
        } else {
            self.pointer.last_middle_press = Some((now, at));
            self.pointer.drag = Some(Drag {
                origin_x: at[0],
                panned_to: None,
            });
        }
    }

    /// The pointer moved to `at`. While the middle button is held this pans,
    /// once the pointer is more than [`PAN_THRESHOLD_PX`] from the press;
    /// the first pan step covers the whole move from the press.
    pub(in super::super) fn pointer_moved(&mut self, at: [f32; 2]) {
        self.pointer.cursor = Some(at);
        let Some(drag) = self.pointer.drag else {
            return;
        };
        let from = match drag.panned_to {
            Some(last) => last,
            None if (at[0] - drag.origin_x).abs() > PAN_THRESHOLD_PX => drag.origin_x,
            None => return,
        };
        self.pointer.drag = Some(Drag {
            panned_to: Some(at[0]),
            ..drag
        });
        self.pan_plot(at[0] - from);
    }

    /// The mouse gestures of the energy window. Returns whether the event
    /// was one.
    pub(super) fn handle_mouse_event(&mut self, event: &WindowEvent) -> bool {
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
                ElementState::Released => self.pointer.drag = None,
            },
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(at) = self.pointer.cursor {
                    self.left_clicked(at);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_moved([position.x as f32, position.y as f32]);
            }
            WindowEvent::CursorLeft { .. } => self.pointer.cursor = None,
            // A release while the window had no focus never arrives.
            WindowEvent::Focused(false) => self.pointer.drag = None,
            _ => return false,
        }
        true
    }
}
