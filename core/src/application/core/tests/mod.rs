// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
mod thermostats;
mod valves;
mod verbose;

use crate::application::MainContent;
use crate::graphics::renderer::ViewportTransform;
use crate::graphics::renderer::margins::Margins;
use crate::objects2d::Rect;

const DT_GOAL: f32 = 0.01;

fn create_main_content_for_region(tl_x: f32, tl_y: f32) -> (MainContent, ViewportTransform) {
    let world = Rect::new(tl_x, tl_y, 100.0, -100.0);
    let transform = ViewportTransform::new(world, (800.0, 600.0), Margins::symmetric(50.0, 50.0));
    create_main_content_with_world(transform)
}

fn create_main_content_with_world(
    transform: ViewportTransform,
) -> (MainContent, ViewportTransform) {
    let world_rect = transform.world_bounds();
    let main_content = MainContent::new(world_rect).expect("Could not create main content");

    println!("Created main content with world rect {:?}", world_rect,);
    (main_content, transform)
}

fn create_main_content() -> (MainContent, ViewportTransform) {
    create_main_content_for_region(-10., 10.)
}
