mod ball_management;
mod bootstrap;
mod drift_harness;
mod energy_conservation;
mod gravity;
mod physics_thread;
mod resize;
mod simulation;
mod simulation_engine;
mod wall_momentum;

mod ghost_wall;
mod line_segments;
mod object_deletion;
mod snapshot;
mod source_spheres;
mod thermostats;
mod valves;

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
