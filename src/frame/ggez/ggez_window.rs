use crate::frame::application_master::WindowDefinitionRef;
use crate::frame::ggez::custom_event_loop::CustomEventLoop;
use crate::frame::ggez::ggez_renderer::GgezRenderer;
use crate::frame::ggez::ggez_renderer_builder::GgezRendererBuilder;
use crate::frame::window::window_definition::WindowDefinition;
use crate::frame::window::{Point2D, Window, WindowEvent, MARGIN_PX_X, MARGIN_PX_Y};
use crate::frame::MainContent;
use crate::objects2d::ball_impl::{BallFactory, BallPhysics};
use crate::objects2d::Rect;
use ggez::conf::{FullscreenType, WindowMode, WindowSetup};
use ggez::graphics::ShaderBuilder;
use ggez::input::mouse;
use ggez::ContextBuilder;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub struct GgezWindow {
    resource_dir: PathBuf,
    win_definition: WindowDefinitionRef,
}

impl GgezWindow {
    pub(crate) fn new(resource_dir: PathBuf, reg_x: f32, reg_y: f32) -> Self {
        let my_win_definition = WindowDefinition {
            region_px: (reg_x, reg_y),
            margin_px: (MARGIN_PX_X, MARGIN_PX_Y),
        };

        GgezWindow {
            resource_dir,
            win_definition: Rc::new(RefCell::new(my_win_definition)),
        }
    }
}
impl Window<GgezRenderer> for GgezWindow {
    fn set_title(&mut self, _title: &str) {
        // Implementation for setting the window title
    }

    fn get_title(&self) -> &str {
        todo!()
    }

    fn set_size(&mut self, _width: u32, _height: u32) {
        // Implementation for setting the window size
    }

    fn get_size(&self) -> (u32, u32) {
        let win_def = self.win_definition.borrow();
        // Implementation for getting the window size
        (win_def.region_px.0 as u32, win_def.region_px.1 as u32)
    }

    fn set_fullscreen(&mut self, _fullscreen: bool) {
        todo!()
    }

    fn is_fullscreen(&self) -> bool {
        todo!()
    }

    fn close(&mut self) {
        todo!()
    }

    fn poll_events(&mut self) -> Vec<WindowEvent> {
        todo!()
    }

    fn set_cursor_visible(&mut self, _visible: bool) {
        todo!()
    }

    fn set_cursor_position(&mut self, _pos: Point2D) {
        todo!()
    }

    fn create_window(&self) {
        let height_default: f32 = 1024.;
        let (mut ctx, event_loop) = ContextBuilder::new("ThermoRust", "Rainer Kaltseis")
            .add_resource_path(self.resource_dir.clone())
            .window_setup(
                WindowSetup::default().title("Monatomic gas using modified Lenard Jones Potential"),
            )
            .window_mode(
                WindowMode {
                    resizable: true,
                    min_width: MARGIN_PX_Y,
                    min_height: MARGIN_PX_X,
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
            .build(&ctx.gfx)
            .expect("Failed to build shader");

        let ball_defs = {
            let all_radii = BallFactory::create_all_radii();
            let ball_physics = BallPhysics::default();
            all_radii
                .into_iter()
                .map(|r| (r, ball_physics.equilibrium_of_radius(r)))
                .collect()
        };

        let renderer = GgezRendererBuilder::new()
            .context(&ctx, ball_defs)
            .window_def_ref(self.win_definition.clone())
            .build();
        let world_rect = self.win_definition.borrow().compute_world_bounds_rect();
        let world_rect = Rect::new(world_rect.x, world_rect.y, world_rect.w, world_rect.h);

        let application = MainContent::new(world_rect).expect("Failed to create MainContent");

        // Use threaded physics architecture
        println!("\n========================================");
        println!("THREADED RENDERER ARCHITECTURE");
        println!("========================================");
        println!("Main thread ID: {:?}", std::thread::current().id());
        println!("Spawning physics thread...");
        let custom_loop = CustomEventLoop::new(application, renderer);
        println!("✓ Physics thread spawned successfully");
        println!("  - Main thread: Rendering (ggez requirement)");
        println!("  - Physics thread: Simulation + snapshot publishing");
        println!("========================================\n");

        custom_loop.run(ctx, event_loop);
    }
}
