use crate::frame::window::{MARGIN_PX_X, MARGIN_PX_Y};
use crate::objects2d::{Rect, Vec2D};

const LEN_TO_PX: f32 = 1. / 10.; // 50 px are one LEN_SCALE (SIG is about 0.5 to 1 LEN_SCALE)

#[derive(Debug, Clone)]
pub struct WindowDefinition {
    pub region_px: (f32, f32),
    pub margin_px: (f32, f32),
}

impl WindowDefinition {
    pub fn new(region_px: (f32, f32)) -> Self {
        WindowDefinition {
            region_px,
            margin_px: (MARGIN_PX_X, MARGIN_PX_Y),
        }
    }

    pub fn px_to_coord(&self, px_x: f32, px_y: f32) -> Vec2D {
        //window top left (0,0) px
        //margin margin_px_[xy]
        //region region_px_[xy]
        //window bottom [xy] = region_px_[xy]+2*margin_px[xy]
        //pos = px_to_len(px_pos - (region_px_[xy]+2*margin_px_[xy])*0.5)
        Vec2D::new(
            Self::px_to_len(px_x - self.region_px.0 * 0.5 - self.margin_px.0),
            -Self::px_to_len(px_y - self.region_px.1 * 0.5 - self.margin_px.1),
        )
    }

    pub fn coord_to_px(&self, coord: &Vec2D) -> Vec2D {
        Self::coord_to_px_raw(self.region_px, self.margin_px, coord)
    }

    pub fn coord_to_px_raw(region_px: (f32, f32), margin_px: (f32, f32), coord: &Vec2D) -> Vec2D {
        let x = Self::len_to_px(coord.x) + region_px.0 * 0.5 + margin_px.0;
        let y = -Self::len_to_px(coord.y) + region_px.1 * 0.5 + margin_px.1;
        Vec2D::new(x, y)
    }

    fn clamp_to_bounds_internal(value: f32, margin: f32, region: f32) -> f32 {
        f32::max(margin, f32::min(margin + region, value))
    }

    pub(crate) fn clamp_to_bounds(&self, value: &Vec2D) -> Vec2D {
        let clamped_x = Self::clamp_to_bounds_internal(value.x, self.margin_px.0, self.region_px.0);
        let clamped_y = Self::clamp_to_bounds_internal(value.y, self.margin_px.1, self.region_px.1);
        Vec2D::new(clamped_x, clamped_y)
    }

    pub fn len_to_px(coord_len: f32) -> f32 {
        coord_len / LEN_TO_PX
    }

    pub fn px_to_len(coord_px: f32) -> f32 {
        coord_px * LEN_TO_PX
    }

    pub fn world_region_inset(&self, radius: f32) -> Rect {
        let tl = self.px_to_coord(self.margin_px.0, self.margin_px.1);
        let w = WindowDefinition::px_to_len(self.region_px.0) - 2.0 * radius;
        let h = WindowDefinition::px_to_len(self.region_px.1) - 2.0 * radius;
        Rect::new(tl.x + radius, tl.y - radius, w, -h)
    }

    pub fn compute_world_bounds_rect(&self) -> Rect {
        let tl = self.px_to_coord(self.margin_px.0, self.margin_px.1);
        let w = WindowDefinition::px_to_len(self.region_px.0);
        let h = WindowDefinition::px_to_len(self.region_px.1);
        Rect::new(tl.x, tl.y, w, -h)
    }
    pub fn resize(&mut self, width: f32, height: f32) {
        self.region_px.0 = width - 2. * self.margin_px.0;
        self.region_px.1 = height - 2. * self.margin_px.1;
    }
    #[cfg(test)]
    pub fn get_len_to_px() -> f32 {
        LEN_TO_PX
    }
}

impl Default for WindowDefinition {
    fn default() -> WindowDefinition {
        WindowDefinition {
            // curs_pos: Vec2D::new(0., 0.),
            region_px: (100., 100.),
            margin_px: (10., 10.),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        let diff = (a - b).abs();
        if diff <= eps {
            return true;
        }
        let max_ab = a.abs().max(b.abs());
        diff <= eps * (1.0 + max_ab)
    }

    #[test]
    fn test_len_px_idempotent() {
        let eps = 1.0e-6;
        let values: [f32; 7] = [0.0, 1.0, -1.0, 0.5, -0.5, 123.456, -9876.5];
        for &v in &values {
            let px = WindowDefinition::len_to_px(v);
            let back = WindowDefinition::px_to_len(px);
            assert!(
                approx_eq(back, v, eps),
                "px_to_len(len_to_px({})) -> {} != {}",
                v,
                back,
                v
            );

            let len = WindowDefinition::px_to_len(v);
            let back_px = WindowDefinition::len_to_px(len);
            assert!(
                approx_eq(back_px, v, eps),
                "len_to_px(px_to_len({})) -> {} != {}",
                v,
                back_px,
                v
            );
        }

        // Also verify constant relation explicitly
        let k = WindowDefinition::get_len_to_px();
        assert!(approx_eq(WindowDefinition::len_to_px(k), 1.0, eps));
        assert!(approx_eq(WindowDefinition::px_to_len(1.0), k, eps));
    }

    #[test]
    fn test_origin_is_center_of_region() {
        // Using defaults: region 100x100, margin 10x10
        let wd = WindowDefinition::default();
        let center_px_x = wd.margin_px.0 + wd.region_px.0 * 0.5; // 10 + 50 = 60
        let center_px_y = wd.margin_px.1 + wd.region_px.1 * 0.5; // 10 + 50 = 60
        let origin = wd.px_to_coord(center_px_x, center_px_y);
        let eps = 1.0e-6;
        assert!(
            approx_eq(origin.x, 0.0, eps),
            "origin.x should be 0 but is {}",
            origin.x
        );
        assert!(
            approx_eq(origin.y, 0.0, eps),
            "origin.y should be 0 but is {}",
            origin.y
        );
    }

    #[test]
    fn test_corner_signs_and_scales() {
        // Configure a custom size to check scaling explicitly
        let mut wd = WindowDefinition::default();
        wd.region_px = (200.0, 100.0);
        wd.margin_px = (8.0, 12.0);

        // Compute expected half sizes in world length units
        let half_x_len = WindowDefinition::px_to_len(wd.region_px.0 * 0.5);
        let half_y_len = WindowDefinition::px_to_len(wd.region_px.1 * 0.5);

        // Top-left corner of the drawable region in pixels
        let tl_px = (wd.margin_px.0, wd.margin_px.1);
        let tl = wd.px_to_coord(tl_px.0, tl_px.1);
        // Expect negative X (left) and positive Y (top due to inversion)
        let eps = 1.0e-5;
        assert!(
            approx_eq(tl.x, -half_x_len, eps),
            "tl.x = {} expected {}",
            tl.x,
            -half_x_len
        );
        assert!(
            approx_eq(tl.y, half_y_len, eps),
            "tl.y = {} expected {}",
            tl.y,
            half_y_len
        );

        // Bottom-right corner of the drawable region in pixels
        let br_px = (
            wd.margin_px.0 + wd.region_px.0,
            wd.margin_px.1 + wd.region_px.1,
        );
        let br = wd.px_to_coord(br_px.0, br_px.1);
        // Expect positive X (right) and negative Y (bottom due to inversion)
        assert!(
            approx_eq(br.x, half_x_len, eps),
            "br.x = {} expected {}",
            br.x,
            half_x_len
        );
        assert!(
            approx_eq(br.y, -half_y_len, eps),
            "br.y = {} expected {}",
            br.y,
            -half_y_len
        );
    }

    // Helper reproducing the coord->px mapping used in MainContent::coord_to_px

    #[test]
    fn test_px_coord_roundtrip_idempotent() {
        let mut wd = WindowDefinition::default();
        // Try multiple configurations
        let configs = [
            ((100.0, 100.0), (10.0, 10.0)),
            ((320.0, 240.0), (0.0, 0.0)),
            ((800.0, 600.0), (5.0, 7.0)),
        ];
        let eps = 1.0e-4; // allow some fp tolerance after multiple ops
        for (region, margin) in configs {
            wd.region_px = region;
            wd.margin_px = margin;
            let samples = [
                // center
                (margin.0 + region.0 * 0.5, margin.1 + region.1 * 0.5),
                // corners
                (margin.0, margin.1),
                (margin.0 + region.0, margin.1 + region.1),
                // some random-ish interior points
                (margin.0 + region.0 * 0.25, margin.1 + region.1 * 0.75),
                (margin.0 + region.0 * 0.9, margin.1 + region.1 * 0.1),
            ];
            for (px_x, px_y) in samples {
                let coord = wd.px_to_coord(px_x, px_y);
                let back_px = wd.coord_to_px(&coord);
                assert!(
                    approx_eq(back_px.x, px_x, eps),
                    "roundtrip x: {} vs {}",
                    back_px.x,
                    px_x
                );
                assert!(
                    approx_eq(back_px.y, px_y, eps),
                    "roundtrip y: {} vs {}",
                    back_px.y,
                    px_y
                );
            }
        }
    }

    #[test]
    fn test_coord_px_roundtrip_idempotent() {
        let mut wd = WindowDefinition::default();
        let configs = [((100.0, 50.0), (3.0, 4.0)), ((1024.0, 768.0), (0.0, 0.0))];
        let eps = 1.0e-4;
        for (region, margin) in configs {
            wd.region_px = region;
            wd.margin_px = margin;

            // define some world coords around center and edges in world units corresponding to px edges
            let half_x_len = WindowDefinition::px_to_len(region.0 * 0.5);
            let half_y_len = WindowDefinition::px_to_len(region.1 * 0.5);

            let coords = [
                Vec2D::new(0.0, 0.0),
                Vec2D::new(half_x_len, 0.0),
                Vec2D::new(-half_x_len, 0.0),
                Vec2D::new(0.0, half_y_len),
                Vec2D::new(0.0, -half_y_len),
                Vec2D::new(half_x_len * 0.3, -half_y_len * 0.7),
            ];
            for c in &coords {
                let px = wd.coord_to_px(c);
                let back = wd.px_to_coord(px.x, px.y);
                assert!(
                    approx_eq(back.x, c.x, eps),
                    "roundtrip x: {} vs {}",
                    back.x,
                    c.x
                );
                assert!(
                    approx_eq(back.y, c.y, eps),
                    "roundtrip y: {} vs {}",
                    back.y,
                    c.y
                );
            }
        }
    }

    #[test]
    fn test_clamp_to_bounds() {
        let mut wd = WindowDefinition::default();
        // Custom region/margin to test
        wd.region_px = (200.0, 150.0);
        wd.margin_px = (8.0, 12.0);

        // Below margins -> clamp up to margin
        let below = Vec2D::new(-1000.0, -1000.0);
        let clamped_below = wd.clamp_to_bounds(&below);
        assert!(approx_eq(clamped_below.x, wd.margin_px.0, 1.0e-6));
        assert!(approx_eq(clamped_below.y, wd.margin_px.1, 1.0e-6));

        // Above region+margin -> clamp down to margin+region
        let above = Vec2D::new(1.0e6, 1.0e6);
        let clamped_above = wd.clamp_to_bounds(&above);
        assert!(approx_eq(
            clamped_above.x,
            wd.margin_px.0 + wd.region_px.0,
            1.0e-6
        ));
        assert!(approx_eq(
            clamped_above.y,
            wd.margin_px.1 + wd.region_px.1,
            1.0e-6
        ));

        // Inside region -> unchanged
        let inside = Vec2D::new(wd.margin_px.0 + 10.0, wd.margin_px.1 + 20.0);
        let clamped_inside = wd.clamp_to_bounds(&inside);
        assert!(approx_eq(clamped_inside.x, inside.x, 1.0e-6));
        assert!(approx_eq(clamped_inside.y, inside.y, 1.0e-6));
    }

    #[test]
    fn test_world_region_rects() {
        let mut wd = WindowDefinition::default();
        wd.region_px = (300.0, 100.0);
        wd.margin_px = (5.0, 7.0);
        let radius = 0.75; // in world-length units

        // Compute expected world widths/heights
        let world_w = WindowDefinition::px_to_len(wd.region_px.0);
        let world_h = WindowDefinition::px_to_len(wd.region_px.1);

        let bounds = wd.compute_world_bounds_rect();
        // Width is positive, height stored negative in Rect (because of y inversion)
        assert!(approx_eq(bounds.w, world_w, 1.0e-6));
        assert!(approx_eq(bounds.h, -world_h, 1.0e-6));

        // Top-left of drawable region in world coords
        let tl_world = wd.px_to_coord(wd.margin_px.0, wd.margin_px.1);
        assert!(approx_eq(bounds.x, tl_world.x, 1.0e-6));
        assert!(approx_eq(bounds.y, tl_world.y, 1.0e-6));

        let inset = wd.world_region_inset(radius);
        // Inset dimensions reduced by 2*radius; height remains negative
        assert!(approx_eq(inset.w, world_w - 2.0 * radius, 1.0e-6));
        assert!(approx_eq(inset.h, -(world_h - 2.0 * radius), 1.0e-6));
        // Inset x moves right by radius; y moves up by radius (increase in world y),
        // but because stored y is world coords, we expect tl.y - radius as per implementation.
        assert!(approx_eq(inset.x, tl_world.x + radius, 1.0e-6));
        assert!(approx_eq(inset.y, tl_world.y - radius, 1.0e-6));
    }
}
