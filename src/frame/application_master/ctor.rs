use super::*;
use crate::frame::GameLogic;
use ggez::graphics::Shader;
use ggez::{Context, GameResult};

impl<'a> Default for MainContent<'a> {
    fn default() -> Self {
        Self {
            physics: BallPhysics::default(),
            game_data: <Self as GameLogic>::init_game_logic(),
            win_definition: WindowDefinition {
                ..Default::default()
            },
            sim: SimStats::default(),
            new_ball: NewBallDef::default(),
            num_balls: 0,
            v_points: Vec::new(),
            real_data: false,
            step_by_step: false,
            step_count: 0,
            show_timings: false,
            show_plot: PlotContent::NONE,
            show_ball_stats: false,
            shader: None,
            crosshair_mesh: None,
            resize_resume_at: None,
            was_paused_for_resize: false,
        }
    }
}

impl<'a> MainContent<'a> {
    pub fn new(
        ctx: Option<&Context>,
        reg_x: f32,
        reg_y: f32,
        shader_in: Option<Shader>,
    ) -> GameResult<Self> {
        let physics = BallPhysics::default();
        Ok(Self {
            win_definition: WindowDefinition {
                region_px: (reg_x, reg_y),
                margin_px: (super::MARGIN_PX_X, super::MARGIN_PX_Y),
                ..Default::default()
            },
            v_points: Vec::new(),
            shader: shader_in,
            new_ball: if let Some(the_ctx) = ctx {
                NewBallDef::new(the_ctx, &physics)
            } else {
                NewBallDef::default()
            },
            crosshair_mesh: if let Some(the_ctx) = ctx {
                super::MainContent::create_crosshair(the_ctx)
            } else {
                None
            },
            physics,
            ..Default::default()
        })
    }
}
