//! Valve segment operator tool — drawing, toggling, and dragging valves on line segments.
//!
//! Extends [`UnifiedRenderState`] with the valve-segment interaction
//! state machine, hit-testing on existing line segments, and preview queries.

use crate::objects2d::Vec2D;

use super::unified_render_state::UnifiedRenderState;

/// State machine for valve segment interactions.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ValveDrawState {
    /// Waiting for user interaction.
    #[default]
    Idle,
    /// Drawing a new valve: user pressed on a line segment and is
    /// dragging to define the valve region.
    Drawing { line_id: usize, t_start: f32 },
    /// Dragging an existing valve along its parent segment.
    Moving {
        valve_id: usize,
        line_id: usize,
        /// Half-width of the valve in parametric space, preserved during drag.
        t_half_width: f32,
        /// Parametric center at press time, used to distinguish toggle from drag.
        t_center_at_press: f32,
    },
}

/// Snap threshold for parametric distance on a line segment.
/// Valve endpoints within this distance of 0.0 or 1.0 snap to the full extent.
const T_SNAP_THRESHOLD: f32 = 0.05;

/// Maximum perpendicular distance (in world units) from cursor to a line
/// segment for the cursor to be considered "on" that segment.
const LINE_HIT_DISTANCE: f32 = 0.8;

/// Maximum distance (in world units) from cursor to a valve indicator
/// for toggle/drag hit detection.
const VALVE_HIT_DISTANCE: f32 = 1.0;

impl UnifiedRenderState {
    /// Find the line segment closest to `pos` within [`LINE_HIT_DISTANCE`]
    /// and return `(line_id, t)` where `t` is the parametric coordinate.
    pub(super) fn find_line_at(&self, pos: Vec2D) -> Option<(usize, f32)> {
        let segments = self
            .last_snapshot
            .as_ref()
            .map(|s| s.operators.line_segments.as_slice())
            .unwrap_or(&[]);
        let mut best_dist = LINE_HIT_DISTANCE;
        let mut best: Option<(usize, f32)> = None;
        for seg in segments {
            let t = seg.closest_t(&pos);
            let closest = seg.point_at_t(t);
            let dist = pos.dist(&closest).norm();
            if dist < best_dist {
                best_dist = dist;
                best = Some((seg.id, t));
            }
        }
        best
    }

    /// Find a valve whose center is near `pos`.
    /// Returns `(valve_id, line_id, t_start, t_end, is_closed)`.
    pub(super) fn find_valve_at(&self, pos: Vec2D) -> Option<(usize, usize, f32, f32, bool)> {
        let snapshot = self.last_snapshot.as_ref()?;
        let mut best_dist = VALVE_HIT_DISTANCE;
        let mut best: Option<(usize, usize, f32, f32, bool)> = None;
        for valve in &snapshot.operators.valve_segments {
            let Some(seg) = snapshot
                .operators
                .line_segments
                .iter()
                .find(|s| s.id == valve.line_id)
            else {
                continue;
            };
            let t_mid = (valve.t_start + valve.t_end) * 0.5;
            let center = seg.point_at_t(t_mid);
            let dist = pos.dist(&center).norm();
            if dist < best_dist {
                best_dist = dist;
                best = Some((
                    valve.id,
                    valve.line_id,
                    valve.t_start,
                    valve.t_end,
                    valve.is_closed,
                ));
            }
        }
        best
    }

    /// Snap parametric coordinate to 0.0 or 1.0 if within threshold.
    pub(super) fn snap_t(t: f32) -> f32 {
        if t < T_SNAP_THRESHOLD {
            0.0
        } else if t > 1.0 - T_SNAP_THRESHOLD {
            1.0
        } else {
            t
        }
    }

    /// Returns the current ghost-preview valve segment while the user is
    /// drawing, as `(start_world, end_world)` in world coordinates.
    pub fn valve_segment_preview(&self) -> Option<(usize, f32, f32)> {
        if let ValveDrawState::Drawing { line_id, t_start } = self.valve_draw_state {
            if let Some((hit_line, t_end)) = self.find_line_at(self.cursor_world_pos)
                && hit_line == line_id
            {
                let t_end = Self::snap_t(t_end);
                return Some((line_id, t_start, t_end));
            }
            // Even if cursor drifts off, show preview to the t_start
            Some((line_id, t_start, t_start))
        } else {
            None
        }
    }
}
