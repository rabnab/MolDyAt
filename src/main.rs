mod frame;
mod objects2d;

use crate::frame::MainContent;

use ggez::conf::{FullscreenType, WindowMode, WindowSetup};
use ggez::event::{self};
use ggez::graphics::ShaderBuilder;
use ggez::input::mouse;
use ggez::{ContextBuilder, GameResult};
use std::env;
use std::path;

fn main() -> GameResult {
    let resource_dir = if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let mut path = path::PathBuf::from(manifest_dir);
        path.push("resources");
        path
    } else {
        path::PathBuf::from("./resources")
    };

    let height_default: f32 = 1024.;
    let (mut ctx, event_loop) = ContextBuilder::new("ThermoRust", "Rainer Kaltseis")
        .add_resource_path(resource_dir)
        .window_setup(
            WindowSetup::default().title("Monatomic gas using modified Lenard Jones Potential"),
        )
        .window_mode(
            WindowMode {
                resizable: true,
                min_width: frame::MARGIN_PX_Y,
                min_height: frame::MARGIN_PX_X,
                width: height_default / 9. * 16.,
                height: height_default,
                ..Default::default()
            }
            .fullscreen_type(FullscreenType::Windowed),
        )
        .build()
        .expect("Failed to build context");

    mouse::set_cursor_hidden(&mut ctx, true);

    //let shader = Shader::from_fragment_shader ( &mut ctx, include_str!("objects2d/gradient.gls1"))?;
    let _shader = ShaderBuilder::new()
        //.vertex_code(include_str!("objects2d/gradient.wsgl"))
        //.fragment_code(include_str!("objects2d/gradient_frag.wgsl"))
        .vertex_path("/gradient_vert.wgsl")
        .fragment_path("/gradient_frag.wgsl")
        .build(&mut ctx.gfx)?;

    //let state = MainContent::new(800.,500., Some(shader))?;
    let state = MainContent::new(Some(&ctx), 800., 500., None)?;
    event::run(ctx, event_loop, state)
}
