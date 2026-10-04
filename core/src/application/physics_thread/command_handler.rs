use super::runtime::CommandHandler;
use crate::application::DomainCommand;
use crate::application::MainContent;
use crate::application::PhysicsEvent;
use crate::application::persistence::LoadSource;
use crate::objects2d::Rect;
use std::sync::mpsc::Sender;

pub(crate) struct DomainCommandHandler<'a> {
    main_content: &'a mut MainContent,
    events: &'a Sender<PhysicsEvent>,
    /// The latest `Resize` not applied yet. Applying one costs O(balls) and a
    /// window drag queues one per mouse step, so a run of them is held back
    /// and only its last one is applied.
    pending_resize: Option<Rect>,
    resizes_applied: u32,
    commands_handled: u32,
}

impl<'a> DomainCommandHandler<'a> {
    pub(crate) fn new(main_content: &'a mut MainContent, events: &'a Sender<PhysicsEvent>) -> Self {
        Self {
            main_content,
            events,
            pending_resize: None,
            resizes_applied: 0,
            commands_handled: 0,
        }
    }

    /// How many resizes this handler has applied (a run of queued `Resize`
    /// commands counts once).
    pub(crate) fn resizes_applied(&self) -> u32 {
        self.resizes_applied
    }

    /// How many commands this handler has handled. Every command can change
    /// what is visible (edits, loads, toggles), so each one bumps the render
    /// revision.
    pub(crate) fn commands_handled(&self) -> u32 {
        self.commands_handled
    }

    /// Applies the held-back `Resize`, if any. It runs before every other
    /// command, so each command still sees the world as the commands before
    /// it left it.
    fn apply_pending_resize(&mut self) {
        if let Some(world_rect) = self.pending_resize.take() {
            self.main_content.resize(world_rect);
            self.resizes_applied += 1;
        }
    }

    fn save_world(&self) -> PhysicsEvent {
        let Some(store) = self.main_content.runtime().save_store() else {
            return PhysicsEvent::SaveFailed("no save directory configured".into());
        };
        match self.main_content.save_world(store) {
            Ok(path) => PhysicsEvent::WorldSaved(path),
            Err(e) => PhysicsEvent::SaveFailed(format!("{e:?}")),
        }
    }

    fn load_world(&mut self, source: &LoadSource) -> PhysicsEvent {
        let Some(store) = self.main_content.runtime().save_store().cloned() else {
            return PhysicsEvent::LoadFailed("no save directory configured".into());
        };
        // Interactive loads start paused, so the loaded state can be looked at.
        match self.main_content.load_world(&store, source, true) {
            Ok(loaded) => PhysicsEvent::WorldLoaded(loaded),
            Err(e) => PhysicsEvent::LoadFailed(format!("{e:?}")),
        }
    }

    fn report(&self, event: PhysicsEvent) {
        // The renderer may already be gone (shutdown); nothing to report to then.
        let _ = self.events.send(event);
    }
}

impl CommandHandler<DomainCommand> for DomainCommandHandler<'_> {
    fn handle_command(&mut self, command: DomainCommand) {
        self.commands_handled += 1;
        if !matches!(command, DomainCommand::Resize { .. }) {
            self.apply_pending_resize();
        }
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
            DomainCommand::ToggleGravity => {
                self.main_content.toggle_gravity();
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
                self.pending_resize = Some(world_rect);
            }
            DomainCommand::SetFullSnapshots(full) => {
                self.main_content.runtime_mut().set_light_snapshots(!full);
            }
            DomainCommand::ResumeAfterResize => {
                self.main_content.unpause_after_resize();
            }
            DomainCommand::SaveWorld => {
                let event = self.save_world();
                self.report(event);
            }
            DomainCommand::LoadWorld(source) => {
                let event = self.load_world(&source);
                self.report(event);
            }
            DomainCommand::Operator(cmd) => {
                cmd.execute(self.main_content);
            }
        }
    }

    fn flush(&mut self) {
        self.apply_pending_resize();
    }
}
