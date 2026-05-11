use super::runtime::CommandHandler;
use crate::application::DomainCommand;
use crate::application::MainContent;

pub(crate) struct DomainCommandHandler<'a> {
    main_content: &'a mut MainContent,
}

impl<'a> DomainCommandHandler<'a> {
    pub(crate) fn new(main_content: &'a mut MainContent) -> Self {
        Self { main_content }
    }
}

impl CommandHandler<DomainCommand> for DomainCommandHandler<'_> {
    fn handle_command(&mut self, command: DomainCommand) {
        match command {
            DomainCommand::Pause => {
                if !self.main_content.world().sim.is_paused() {
                    self.main_content.toggle_pause();
                }
            }
            DomainCommand::Resume => {
                if self.main_content.world().sim.is_paused() {
                    self.main_content.toggle_pause();
                }
            }
            DomainCommand::TogglePause => {
                self.main_content.toggle_pause();
            }
            DomainCommand::AddBall(pos) => {
                let _ = self.main_content.add_candidate_at_pos(pos);
            }
            DomainCommand::AddBallWithVelocity(ball) => {
                let _ = self.main_content.add_candidate_ball(ball, 0.);
            }
            DomainCommand::AddRandomBalls(count) => {
                self.main_content.add_random_candidates(count);
            }
            DomainCommand::ClearBalls => {
                self.main_content.clear_points();
            }
            DomainCommand::StopAllBalls => {
                self.main_content.stop_all_balls();
            }
            DomainCommand::RandomizeSpeeds => {
                self.main_content.randomize_speeds();
            }
            DomainCommand::EnableRandomBallSpeed => {
                self.main_content.enable_random_ball_speed();
            }
            DomainCommand::NextBallSize => {
                self.main_content.next_ball_size();
            }
            DomainCommand::IncreaseWaitTime => {
                self.main_content.increase_wait_time();
            }
            DomainCommand::DecreaseWaitTime => {
                self.main_content.decrease_wait_time();
            }
            DomainCommand::DoubleTimeStep => {
                self.main_content.double_time_step();
            }
            DomainCommand::HalveTimeStep => {
                self.main_content.halve_time_step();
            }
            DomainCommand::ToggleRealData => {
                self.main_content.toggle_real_data();
            }
            DomainCommand::ToggleStepwise => {
                self.main_content.toggle_stepwise();
            }
            DomainCommand::ToggleTimingDisplay => {
                self.main_content.toggle_timing_display();
            }
            DomainCommand::ToggleBallStats => {
                self.main_content.toggle_show_ball_stats();
            }
            DomainCommand::StartVerbosityInput => {
                self.main_content.start_handling_verbosity_input();
            }
            DomainCommand::Resize { world_rect } => {
                self.main_content.resize(world_rect);
            }
            DomainCommand::ResumeAfterResize => {
                self.main_content.unpause_after_resize();
            }
            DomainCommand::Operator(cmd) => {
                cmd.execute(self.main_content);
            }
        }
    }
}
