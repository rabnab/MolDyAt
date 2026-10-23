// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::wgpu::{DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MARGINS};
use thermo_physics::application::sim_config::DEFAULT_WORLD_RECT;
use thermo_ui::graphics::renderer::ViewportTransform;

/// The default window shows the default world exactly, at the default world
/// units per pixel, so a first resize keeps its size (#351).
#[test]
fn default_window_fits_the_default_world() {
    let window = ViewportTransform::compute_world_bounds_from_window_size(
        DEFAULT_WINDOW_WIDTH as u32,
        DEFAULT_WINDOW_HEIGHT as u32,
        MARGINS,
    );
    assert_eq!(window, DEFAULT_WORLD_RECT);
}
