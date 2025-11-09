use crate::frame::MainContent;
use crate::frame::application_master::WindowDefinitionRef;
use crate::frame::ggez::ggez_event_handler::GgezEventHandler;
use crate::frame::ggez::ggez_renderer::GgezRenderer;
use crate::frame::ggez::ggez_renderer_builder::GgezRendererBuilder;
use crate::frame::window::window_definition::WindowDefinition;
use crate::frame::window::{MARGIN_PX_X, MARGIN_PX_Y, Point2D, Window, WindowEvent};
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::{BallFactory, BallPhysics};
use ggez::ContextBuilder;
use ggez::conf::{FullscreenType, WindowMode, WindowSetup};
use ggez::graphics::ShaderBuilder;
use ggez::input::mouse;
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
            ..Default::default()
        };

        GgezWindow {
            resource_dir,
            win_definition: Rc::new(RefCell::new(my_win_definition)),
        }
    }
}
impl Window<GgezRenderer> for GgezWindow {
    fn set_title(&mut self, title: &str) {
        // Implementation for setting the window title
    }

    fn get_title(&self) -> &str {
        todo!()
    }

    fn set_size(&mut self, width: u32, height: u32) {
        // Implementation for setting the window size
    }

    fn get_size(&self) -> (u32, u32) {
        let win_def = self.win_definition.borrow();
        // Implementation for getting the window size
        (win_def.region_px.0 as u32, win_def.region_px.1 as u32)
    }

    fn set_fullscreen(&mut self, fullscreen: bool) {
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

    fn set_cursor_visible(&mut self, visible: bool) {
        todo!()
    }

    fn set_cursor_position(&mut self, pos: Point2D) {
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
            .build(&mut ctx.gfx)
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

        let rend_ref = Rc::new(RefCell::new(renderer));

        let application =
            MainContent::new(world_rect, rend_ref.clone()).expect("Failed to create MainContent");

        // rend_ref.borrow_mut().initialize(&ctx, application.cell_grid.get_cell_size());

        let event_handler = GgezEventHandler::new(application, rend_ref.clone());

        ggez::event::run(ctx, event_loop, event_handler);
    }
}
