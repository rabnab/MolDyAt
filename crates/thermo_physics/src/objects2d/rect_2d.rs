// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    /// Create a new Rect from top-left coordinates, width and height
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Create a Rect from top-left and bottom-right coordinates
    pub(crate) fn from_tl_br(tl: (f32, f32), br: (f32, f32)) -> Self {
        let (x1, y1) = tl;
        let (x2, y2) = br;
        Self {
            x: x1,
            y: y1,
            w: x2 - x1,
            h: y2 - y1,
        }
    }

    /// Get top-left corner coordinates.
    ///
    /// # Returns
    /// Tuple of (x, y) for top-left corner
    pub fn get_tl(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    /// Get bottom-right corner coordinates.
    ///
    /// # Returns
    /// Tuple of (x + w, y + h) for bottom-right corner
    pub fn get_br(&self) -> (f32, f32) {
        (self.x + self.w, self.y + self.h)
    }

    /// Larger side of the rect, floored at 1.0. Operator decorations and
    /// hit-test slop are sized as a fraction of the world's extent so they
    /// keep their look when the world is resized.
    pub fn extent(&self) -> f32 {
        self.w.abs().max(self.h.abs()).max(1.0)
    }

    /// This rect with width and height multiplied by `scale`, around its
    /// centre. A scale of 1 returns it unchanged, bit for bit.
    pub fn scaled_about_centre(&self, scale: f32) -> Self {
        let (w, h) = (self.w * scale, self.h * scale);
        Self::new(
            self.x + (self.w - w) / 2.0,
            self.y + (self.h - h) / 2.0,
            w,
            h,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Rect;

    #[test]
    fn scaled_about_centre_keeps_the_centre() {
        let rect = Rect::new(-56.0, 31.0, 112.0, -62.0);
        let scaled = rect.scaled_about_centre(5.0);
        assert_eq!((scaled.w, scaled.h), (560.0, -310.0));
        assert_eq!(
            (scaled.x + scaled.w / 2.0, scaled.y + scaled.h / 2.0),
            (0.0, 0.0)
        );
        assert_eq!(rect.scaled_about_centre(1.0), rect);
    }
}
