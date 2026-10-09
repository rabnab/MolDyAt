// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::MainContent;
use crate::graphics::renderer::Margins;
use float_cmp::approx_eq;

#[test]
fn test_bootstrap_world_rect_matches_current_wgpu_startup_geometry() {
    let world_rect =
        MainContent::bootstrap_world_rect(1280, 720, Margins::new(110.0, 50.0, 50.0, 50.0));

    assert!(approx_eq!(f32, world_rect.x, -56.0, epsilon = 1.0e-6));
    assert!(approx_eq!(f32, world_rect.y, 31.0, epsilon = 1.0e-6));
    assert!(approx_eq!(f32, world_rect.w, 112.0, epsilon = 1.0e-6));
    assert!(approx_eq!(f32, world_rect.h, -62.0, epsilon = 1.0e-6));
}
