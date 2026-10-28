// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The time range the energy plot shows, and how the mouse changes it
//! (#367): the scroll wheel zooms (changes the span), a middle-button drag
//! pans (moves the range in time).
//!
//! In **follow** mode (the default) the right edge is the newest sample, so
//! a running simulation scrolls the plot; while the history is shorter than
//! the span, the range starts at the oldest sample instead (the curves fill
//! the plot from the left). Panning to earlier times leaves follow mode;
//! panning back to the newest sample, or [`TimeView::reset`], returns to it.
//! Zooming never does: a paused view stays paused.
//!
//! The view knows nothing of pixels: callers pass the time history and,
//! for zooming around the pointer, where the pointer is as a fraction of the
//! plot's width.

use super::plot_layout::TIME_SPAN;

/// Span change per wheel notch.
pub const ZOOM_FACTOR: f64 = 1.25;

/// The shortest span shows this many sample intervals.
const MIN_SAMPLE_INTERVALS: f64 = 4.0;

/// The shortest span when the sample interval is not known.
const MIN_SPAN: f64 = 1e-3;

/// Relative tolerance below which two spans are the same: zooming in and
/// out by the same notches lands back on the default span.
const SPAN_TOLERANCE: f64 = 1e-9;

fn same_span(a: f64, b: f64) -> bool {
    (a - b).abs() <= SPAN_TOLERANCE * a.abs().max(b.abs())
}

/// What there is to show: the times of the oldest and the newest sample and
/// the number of samples in between (both included).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeHistory {
    pub oldest: f64,
    pub newest: f64,
    pub samples: usize,
}

impl TimeHistory {
    /// The time the history covers.
    pub fn length(&self) -> f64 {
        self.newest - self.oldest
    }

    /// Zooming in stops at a few sample intervals.
    pub fn min_span(&self) -> f64 {
        let interval = if self.samples > 1 {
            self.length() / (self.samples - 1) as f64
        } else {
            0.0
        };
        (interval * MIN_SAMPLE_INTERVALS).max(MIN_SPAN)
    }

    /// Zooming out stops at the whole history, or the default span when the
    /// history is shorter; never below [`Self::min_span`] (a coarse history,
    /// few samples far apart).
    pub fn max_span(&self) -> f64 {
        self.length().max(TIME_SPAN).max(self.min_span())
    }
}

/// The time range of the plot: its span, its right edge and whether it
/// follows the newest sample. Two views are equal when they show the same
/// range: spans within a rounding tolerance, and the right edge only
/// counts while not following.
#[derive(Debug, Clone, Copy)]
pub struct TimeView {
    span: f64,
    /// The right edge while not following; unused while following.
    right: f64,
    follow: bool,
}

impl PartialEq for TimeView {
    fn eq(&self, other: &Self) -> bool {
        self.follow == other.follow
            && same_span(self.span, other.span)
            && (self.follow || self.right == other.right)
    }
}

impl Default for TimeView {
    fn default() -> Self {
        Self {
            span: TIME_SPAN,
            right: 0.0,
            follow: true,
        }
    }
}

impl TimeView {
    pub fn span(&self) -> f64 {
        self.span
    }

    pub fn is_following(&self) -> bool {
        self.follow
    }

    /// Whether this is the view a new window starts with: following, with
    /// the default span.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Back to the default: follow the newest sample with the default span.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The right edge in follow mode: the newest sample, or the end of the
    /// span from the oldest sample while the history is shorter.
    fn follow_right(&self, history: &TimeHistory) -> f64 {
        history.newest.max(history.oldest + self.span)
    }

    /// The range `(t0, t1)` shown for `history`.
    pub fn window(&self, history: &TimeHistory) -> (f64, f64) {
        let right = if self.follow {
            self.follow_right(history)
        } else {
            self.clamped_right(self.right, history)
        };
        (right - self.span, right)
    }

    /// `right` kept so that the range stays within the history: not left of
    /// the oldest sample, not right of the follow position.
    fn clamped_right(&self, right: f64, history: &TimeHistory) -> f64 {
        let hi = self.follow_right(history);
        let lo = (history.oldest + self.span).min(hi);
        right.clamp(lo, hi)
    }

    /// Zoom by `notches` wheel notches (positive: in, less time shown), with
    /// the time at `anchor` (the pointer, as a fraction 0–1 of the plot's
    /// width) kept where it is. In follow mode the newest sample stays at
    /// the right edge instead. The span stays within the history's limits.
    /// A paused view stays paused, even when the range reaches the newest
    /// sample.
    pub fn zoom(&mut self, notches: f64, anchor: f64, history: &TimeHistory) {
        if !notches.is_finite() || notches == 0.0 {
            return;
        }
        let mut new_span =
            (self.span * ZOOM_FACTOR.powf(-notches)).clamp(history.min_span(), history.max_span());
        // In and out by the same notches lands exactly on the default span.
        if same_span(new_span, TIME_SPAN) {
            new_span = TIME_SPAN;
        }
        if self.follow {
            self.span = new_span;
            return;
        }
        let anchor = anchor.clamp(0.0, 1.0);
        let (t0, _) = self.window(history);
        let anchor_time = t0 + anchor * self.span;
        self.span = new_span;
        // Kept within the history, but a zoom never ends a pause: it looks
        // closer or wider at the paused data (only a reset follows again).
        self.right = self.clamped_right(anchor_time + (1.0 - anchor) * new_span, history);
    }

    /// Move the range by `dt` time units (positive: to later times), within
    /// the history. A pan that moves a following range to earlier times
    /// pauses it; a pan never ends a pause, also when the range reaches the
    /// newest sample (only a reset follows again: `HOME` or a middle double
    /// click). A following range has nothing later to show, so a pan to
    /// later times, or one the history is too short for, leaves it
    /// following.
    pub fn pan(&mut self, dt: f64, history: &TimeHistory) {
        if !dt.is_finite() || dt == 0.0 {
            return;
        }
        let (_, right) = self.window(history);
        let moved = self.clamped_right(right + dt, history);
        if self.follow && moved >= right - self.span * 1e-9 {
            return;
        }
        self.right = moved;
        self.follow = false;
    }
}
