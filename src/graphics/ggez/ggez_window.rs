use crate::application::MainContent;
use crate::graphics::ggez::WindowDefinitionRef;
use crate::graphics::ggez::custom_event_loop::CustomEventLoop;
use crate::graphics::ggez::ggez_renderer::GgezRenderer;
use crate::graphics::ggez::ggez_renderer_builder::GgezRendererBuilder;
use crate::graphics::window::window_definition::WindowDefinition;
use crate::graphics::window::{MARGIN_PX_X, MARGIN_PX_Y, Window};
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
    /// Create new ggez window configuration.
    ///
    /// # Arguments
    /// * `resource_dir` - Path to shader resources directory
    /// * `reg_x` - Region width in pixels
    /// * `reg_y` - Region height in pixels
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
    fn get_size(&self) -> (u32, u32) {
        let win_def = self.win_definition.borrow();
        // Implementation for getting the window size
        (win_def.region_px.0 as u32, win_def.region_px.1 as u32)
    }

    /// Create and run ggez window with threaded physics architecture.
    /// Main thread handles rendering (ggez requirement), separate physics thread runs simulation.
    /// Physics thread publishes snapshots for rendering thread to consume.
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
        let ball_physics = BallPhysics::default();
        let ball_defs = {
            let all_radii = BallFactory::create_all_radii();

            all_radii
                .into_iter()
                .map(|r| (r, ball_physics.equilibrium_of_radius(r)))
                .collect()
        };
        let cutoff = ball_physics.get_cutoff_distance();

        let renderer = GgezRendererBuilder::new()
            .context_and_ball_def(&ctx, cutoff, ball_defs)
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
