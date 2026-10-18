// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{MainContent, SimulationWorld};
use crate::application::renderer_snapshot::ValveSpan;
use crate::objects2d::wall_chain::Joint;

/// Narrowest valve, as a fraction of its piece.
pub const MIN_VALVE_SPAN: f32 = 1e-4;

/// Why a valve spanning the piece-local fractions `t_start..t_end`
/// (`t_start <= t_end`) cannot be placed, or `None` if it can.
///
/// A valve flush against only ONE of the piece's edges would split it into
/// one normal piece plus a permanent zero-length piece (start == end) that
/// nothing ever merges or prunes. The UI's `snap_t` snaps a dragged endpoint
/// to exactly 0.0/1.0 near a piece boundary, so this is reachable from an
/// ordinary drag. A valve flush against BOTH edges (the whole piece becomes
/// the valve) strands nothing and is allowed.
pub fn valve_span_problem(t_start: f32, t_end: f32) -> Option<String> {
    if t_end - t_start < MIN_VALVE_SPAN {
        return Some(format!(
            "valve span is narrower than {MIN_VALVE_SPAN} of the wall piece"
        ));
    }
    let start_flush = t_start < MIN_VALVE_SPAN;
    let end_flush = t_end > 1.0 - MIN_VALVE_SPAN;
    (start_flush != end_flush)
        .then(|| "valve may touch both ends of the wall piece or neither, not only one".to_string())
}

impl MainContent {
    /// Add a valve on wall piece `line_id`, given as piece-local fractions
    /// `t_start`/`t_end`, and return its id, or `None` if the piece is
    /// unknown or the span is rejected (see [`valve_span_problem`]). Splits
    /// the piece; see `SimulationWorld::add_valve_on_piece`.
    pub fn add_valve_segment(
        &mut self,
        line_id: usize,
        t_start: f32,
        t_end: f32,
        is_closed: bool,
    ) -> Option<usize> {
        self.world
            .add_valve_on_piece(line_id, t_start, t_end, is_closed)
    }

    /// Toggle a valve between open and closed.
    pub(crate) fn toggle_valve(&mut self, valve_id: usize) {
        self.world.toggle_valve(valve_id);
    }

    /// Open or close a valve. Balls inside it when it closes are not moved;
    /// they leave through either side on their own.
    #[cfg(test)]
    pub(crate) fn set_valve_closed(&mut self, valve_id: usize, is_closed: bool) {
        self.world.set_valve_closed(valve_id, is_closed);
    }
}

impl SimulationWorld {
    pub(crate) fn set_valve_closed(&mut self, valve_id: usize, is_closed: bool) {
        let Some(idx) = self.valve_segments.position(valve_id) else {
            return;
        };
        self.valve_segments.get_mut(idx).is_closed = is_closed;
        // Balls in the span when it closes are not moved: they leave on
        // their own (see `SimulationWorld::resolve_wall_overlaps_with`).
        if is_closed {
            self.flag_balls_overlapping_valve(valve_id);
        }
    }

    pub(crate) fn toggle_valve(&mut self, valve_id: usize) {
        let Some(idx) = self.valve_segments.position(valve_id) else {
            return;
        };
        let new_closed = !self.valve_segments.get(idx).is_closed;
        self.set_valve_closed(valve_id, new_closed);
    }

    /// Resolved world-space geometry of every valve, in chain order.
    ///
    /// Computed once per snapshot so renderer and hit-testing never repeat
    /// the chain math.
    pub(crate) fn valve_spans(&self) -> Vec<ValveSpan> {
        let mut spans = Vec::new();
        for chain in self.wall_chains.chains() {
            for (index, joint) in chain.joints.iter().enumerate() {
                let Joint::Valve {
                    valve_id,
                    t_start,
                    t_end,
                } = joint
                else {
                    continue;
                };
                let Some(valve) = self.valve_segments.find(*valve_id) else {
                    continue;
                };
                spans.push(ValveSpan {
                    valve_id: *valve_id,
                    p0: chain.point_at(*t_start),
                    p1: chain.point_at(*t_end),
                    thickness: self.joint_thickness(chain, index),
                    is_closed: valve.is_closed,
                });
            }
        }
        spans
    }
}
