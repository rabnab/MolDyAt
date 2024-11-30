mod objects2d;
mod frame;

use crate::frame::MainContent;

use ggez::{ContextBuilder, GameResult};
use ggez::event::{self};
use ggez::conf::WindowMode;
use ggez::input::mouse;

fn main() -> GameResult { 
    let height_default:f32 = 1024.;
    let (mut ctx, event_loop) = ContextBuilder::new("ThermoRust", "Rainer Kaltseis")
        .window_mode(WindowMode {
            resizable: true,
            min_width: frame::MARGIN_Y,
            min_height: frame::MARGIN_X,
            width: height_default/9.*16.,
            height: height_default,
            ..Default::default()
        })
        .build()
        .expect("Failed to build context");

    mouse::set_cursor_hidden(&mut ctx, true);
    let state = MainContent::new(800.,500.)?;
    event::run(ctx, event_loop, state)
}

