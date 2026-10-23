// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::SnapshotPublisher;
use super::runtime::CommandHandler;
use crate::application::MainContent;
use crate::application::PhysicsEvent;
use crate::application::persistence::LoadSource;
use crate::application::{BallCommand, DomainCommand, SimControl, ViewCommand, WorldCommand};
use crate::objects2d::Rect;
use std::sync::mpsc::Sender;

pub(crate) struct DomainCommandHandler<'a> {
    main_content: &'a mut MainContent,
    publisher: &'a mut SnapshotPublisher,
    events: &'a Sender<PhysicsEvent>,
    /// The latest `Resize` not applied yet. Applying one costs O(balls) and a
    /// window drag queues one per mouse step, so a run of them is held back
    /// and only its last one is applied.
    pending_resize: Option<Rect>,
    resizes_applied: u32,
    commands_handled: u32,
}

impl<'a> DomainCommandHandler<'a> {
    pub(crate) fn new(
        main_content: &'a mut MainContent,
        publisher: &'a mut SnapshotPublisher,
        events: &'a Sender<PhysicsEvent>,
    ) -> Self {
        Self {
            main_content,
            publisher,
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

    /// How many commands this handler has handled. Any command may change
    /// what a snapshot shows (an edit, a load, a toggle).
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

impl DomainCommandHandler<'_> {
    fn handle_sim(&mut self, command: SimControl) {
        let mc = &mut *self.main_content;
        match command {
            SimControl::Pause => {
                if !mc.world().sim.is_paused() {
                    mc.toggle_pause();
                }
            }
            SimControl::Resume => {
                if mc.world().sim.is_paused() {
                    mc.toggle_pause();
                }
            }
            SimControl::TogglePause => mc.toggle_pause(),
            SimControl::ToggleStepwise => mc.toggle_stepwise(),
            SimControl::ToggleGravity => mc.toggle_gravity(),
            SimControl::DoubleTimeStep => mc.double_time_step(),
            SimControl::HalveTimeStep => mc.halve_time_step(),
            SimControl::IncreaseWaitTime => mc.increase_wait_time(),
            SimControl::DecreaseWaitTime => mc.decrease_wait_time(),
        }
    }

    fn handle_balls(&mut self, command: BallCommand) {
        let mc = &mut *self.main_content;
        match command {
            BallCommand::Add(pos) => {
                let _ = mc.add_candidate_at_pos(pos);
            }
            BallCommand::AddWithVelocity(ball) => {
                let _ = mc.add_candidate_ball(ball, 0.);
            }
            BallCommand::AddRandom(count) => mc.add_random_candidates(count),
            BallCommand::Clear => mc.clear_points(),
            BallCommand::StopAll => mc.stop_all_balls(),
            BallCommand::RandomizeSpeeds => mc.randomize_speeds(),
            BallCommand::EnableRandomSpeed => mc.enable_random_ball_speed(),
            BallCommand::NextSpecies => mc.next_species(),
            BallCommand::PreviousSpecies => mc.previous_species(),
        }
    }

    fn handle_world(&mut self, command: WorldCommand) {
        match command {
            WorldCommand::Resize { world_rect } => {
                self.pending_resize = Some(world_rect);
            }
            WorldCommand::ResumeAfterResize => self.main_content.unpause_after_resize(),
            WorldCommand::Save => {
                let event = self.save_world();
                self.report(event);
            }
            WorldCommand::Load(source) => {
                let event = self.load_world(&source);
                self.report(event);
            }
        }
    }

    fn handle_view(&mut self, command: ViewCommand) {
        let mc = &mut *self.main_content;
        match command {
            ViewCommand::ToggleRealData => mc.toggle_real_data(),
            ViewCommand::ToggleTimingDisplay => mc.toggle_timing_display(),
            ViewCommand::ToggleBallStats => mc.toggle_show_ball_stats(),
            ViewCommand::StartVerbosityInput => mc.start_handling_verbosity_input(),
            ViewCommand::SetSnapshotLayers(layers) => self.publisher.set_layers(mc, layers),
        }
    }
}

impl CommandHandler<DomainCommand> for DomainCommandHandler<'_> {
    fn handle_command(&mut self, command: DomainCommand) {
        self.commands_handled += 1;
        if !matches!(command, DomainCommand::World(WorldCommand::Resize { .. })) {
            self.apply_pending_resize();
        }
        match command {
            DomainCommand::Sim(command) => self.handle_sim(command),
            DomainCommand::Balls(command) => self.handle_balls(command),
            DomainCommand::World(command) => self.handle_world(command),
            DomainCommand::View(command) => self.handle_view(command),
            DomainCommand::Operator(command) => command.execute(self.main_content),
        }
    }

    fn flush(&mut self) {
        self.apply_pending_resize();
    }
}
