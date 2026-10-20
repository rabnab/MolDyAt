// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod alloc_count;
mod ball_management;
mod barrier_energy;
mod bootstrap;
mod drift_harness;
mod energy_conservation;
mod gravity;
mod overlap_resolution;
mod physics_thread;
mod placement;
mod pressure;
mod resize;
mod simulation;
mod simulation_engine;
mod wall_handover;
mod wall_momentum;

mod ghost_wall;
mod line_segments;
mod object_deletion;
mod snapshot;
mod source_spheres;
mod species;
mod thermostats;
mod valves;
mod verbose;

use crate::application::MainContent;
use crate::application::sim_config::SimConfig;
use crate::error::AppResult;
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::margins::Margins;
use crate::objects2d::Rect;

const DT_GOAL: f32 = 0.01;

/// The default config with the time step set to `DT_GOAL`: the tests and
/// their thresholds are written for that step, not for the default.
fn test_config() -> SimConfig {
    let mut config = SimConfig::default();
    config.integration.time_step = DT_GOAL;
    config
}

/// A world built with [`test_config`]. Core tests build their worlds here,
/// not with `MainContent::new`, so they run at `DT_GOAL` whatever the default.
fn new_main_content(world_rect: Rect) -> AppResult<MainContent> {
    MainContent::with_config(world_rect, &test_config(), None)
}

/// [`new_main_content`] with seeded random placement.
fn new_main_content_seeded(world_rect: Rect, seed: u64) -> AppResult<MainContent> {
    MainContent::with_config(world_rect, &test_config(), Some(seed))
}

fn create_main_content_for_region(tl_x: f32, tl_y: f32) -> (MainContent, ViewportTransform) {
    let world = Rect::new(tl_x, tl_y, 100.0, -100.0);
    let transform = ViewportTransform::new(world, (800.0, 600.0), Margins::symmetric(50.0, 50.0));
    create_main_content_with_world(transform)
}

fn create_main_content_with_world(
    transform: ViewportTransform,
) -> (MainContent, ViewportTransform) {
    let world_rect = transform.world_bounds();
    let main_content = new_main_content(world_rect).expect("Could not create main content");

    println!("Created main content with world rect {:?}", world_rect,);
    (main_content, transform)
}

fn create_main_content() -> (MainContent, ViewportTransform) {
    create_main_content_for_region(-10., 10.)
}
