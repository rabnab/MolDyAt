// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{MainContent, SimulationWorld};

/// Narrowest valve, as a fraction of its piece.
pub(crate) const MIN_VALVE_SPAN: f32 = 1e-4;

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

/// How a valve's span changes when it is dragged along its wall (#203), in
/// fractions of the wall (chain). The kind is explicit: inferring a move
/// from two equal edge shifts would depend on rounding (#383 review).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValveSpanChange {
    /// Shift the valve so it starts at `t_start`; its width stays as it is.
    Move { t_start: f32 },
    /// Set both edges (one of them usually unchanged).
    Resize { t_start: f32, t_end: f32 },
}

impl ValveSpanChange {
    pub(crate) fn is_finite(self) -> bool {
        match self {
            Self::Move { t_start } => t_start.is_finite(),
            Self::Resize { t_start, t_end } => t_start.is_finite() && t_end.is_finite(),
        }
    }
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

    /// Move or resize valve `valve_id` along its wall as `change` says,
    /// clamped so it stays between its neighbouring pieces (#203); see
    /// `SimulationWorld::set_valve_span`. Returns the span applied.
    pub fn set_valve_span(
        &mut self,
        valve_id: usize,
        change: ValveSpanChange,
    ) -> Option<(f32, f32)> {
        self.world.set_valve_span(valve_id, change)
    }

    /// Toggle a valve between open and closed.
    pub fn toggle_valve(&mut self, valve_id: usize) {
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
}
