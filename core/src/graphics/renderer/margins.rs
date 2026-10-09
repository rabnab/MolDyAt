// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/// Pixel margins around the simulation viewport.
///
/// Supports asymmetric margins (e.g. a wider left margin to accommodate
/// a sidebar panel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margins {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Margins {
    /// Create margins with explicit values for each edge.
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Create symmetric margins (same on left/right, same on top/bottom).
    pub const fn symmetric(x: f32, y: f32) -> Self {
        Self {
            left: x,
            top: y,
            right: x,
            bottom: y,
        }
    }

    /// Total horizontal margin (left + right).
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    /// Total vertical margin (top + bottom).
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}
