use super::PhysicsCommand;
use super::runtime::CommandHandler;
use crate::application::MainContent;

pub(crate) struct PhysicsCommandHandler<'a> {
    main_content: &'a mut MainContent,
}

impl<'a> PhysicsCommandHandler<'a> {
    pub(crate) fn new(main_content: &'a mut MainContent) -> Self {
        Self { main_content }
    }
}

impl CommandHandler<PhysicsCommand> for PhysicsCommandHandler<'_> {
    fn handle_command(&mut self, command: PhysicsCommand) {
        match command {
            PhysicsCommand::Pause => {
                if !self.main_content.sim.is_paused() {
                    self.main_content.toggle_pause();
                }
            }
            PhysicsCommand::Resume => {
                if self.main_content.sim.is_paused() {
                    self.main_content.toggle_pause();
                }
            }
            PhysicsCommand::TogglePause => {
                self.main_content.toggle_pause();
            }
            PhysicsCommand::AddBall(pos) => {
                let _ = self.main_content.add_candidate_at_pos(pos);
            }
            PhysicsCommand::AddBallWithVelocity(ball) => {
                let _ = self.main_content.add_candidate_ball(ball, 0.);
            }
            PhysicsCommand::AddRandomBalls(count) => {
                self.main_content.add_random_candidates(count);
            }
            PhysicsCommand::ClearBalls => {
                self.main_content.clear_points();
            }
            PhysicsCommand::StopAllBalls => {
                self.main_content.stop_all_balls();
            }
            PhysicsCommand::RandomizeSpeeds => {
                self.main_content.randomize_speeds();
            }
            PhysicsCommand::EnableRandomBallSpeed => {
                self.main_content.enable_random_ball_speed();
            }
            PhysicsCommand::NextBallSize => {
                self.main_content.next_ball_size();
            }
            PhysicsCommand::IncreaseWaitTime => {
                self.main_content.increase_wait_time();
            }
            PhysicsCommand::DecreaseWaitTime => {
                self.main_content.decrease_wait_time();
            }
            PhysicsCommand::DoubleTimeStep => {
                self.main_content.double_time_step();
            }
            PhysicsCommand::HalveTimeStep => {
                self.main_content.halve_time_step();
            }
            PhysicsCommand::ToggleRealData => {
                self.main_content.toggle_real_data();
            }
            PhysicsCommand::ToggleStepwise => {
                self.main_content.toggle_stepwise();
            }
            PhysicsCommand::ToggleTimingDisplay => {
                self.main_content.toggle_timing_display();
            }
            PhysicsCommand::ToggleBallStats => {
                self.main_content.toggle_show_ball_stats();
            }
            PhysicsCommand::StartVerbosityInput => {
                self.main_content.start_handling_verbosity_input();
            }
            PhysicsCommand::Resize { world_rect } => {
                self.main_content.resize(world_rect);
            }
            PhysicsCommand::ResumeAfterResize => {
                self.main_content.unpause_after_resize();
            }
            PhysicsCommand::Operator(cmd) => {
                cmd.execute(self.main_content);
            }
        }
    }
}
