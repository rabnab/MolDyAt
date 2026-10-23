// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! World builders for tests, here and in dependent crates (feature
//! `test-support`, enabled by their dev-dependencies only).

use crate::application::MainContent;
use crate::application::sim_config::SimConfig;
use crate::error::AppResult;
use crate::objects2d::Rect;

pub const DT_GOAL: f32 = 0.01;

/// The default config with the time step set to `DT_GOAL`: the tests and
/// their thresholds are written for that step, not for the default.
pub fn test_config() -> SimConfig {
    let mut config = SimConfig::default();
    config.integration.time_step = DT_GOAL;
    config
}

/// A world built with [`test_config`]. Tests build their worlds here,
/// not with `MainContent::new`, so they run at `DT_GOAL` whatever the default.
pub fn new_main_content(world_rect: Rect) -> AppResult<MainContent> {
    MainContent::with_config(world_rect, &test_config(), None)
}

/// [`new_main_content`] with seeded random placement.
pub fn new_main_content_seeded(world_rect: Rect, seed: u64) -> AppResult<MainContent> {
    MainContent::with_config(world_rect, &test_config(), Some(seed))
}

/// A 100 x 100 world with its top left corner at `(tl_x, tl_y)`, and its
/// rectangle.
pub fn create_main_content_for_region(tl_x: f32, tl_y: f32) -> (MainContent, Rect) {
    let world_rect = Rect::new(tl_x, tl_y, 100.0, -100.0);
    let main_content = new_main_content(world_rect).expect("Could not create main content");
    (main_content, world_rect)
}

/// [`create_main_content_for_region`] centred on the origin.
pub fn create_main_content() -> (MainContent, Rect) {
    create_main_content_for_region(-10., 10.)
}
