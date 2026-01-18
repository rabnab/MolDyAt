use super::{InputAction, InputHandler, PhysicsCoordinator};
use crate::application::MainContent;
use crate::application::physics_thread::PhysicsCommand;

/// Shared state for any event loop implementation
pub struct EventLoopState {
    pub input: InputHandler,
    pub physics: PhysicsCoordinator,
    pub render_drawing: bool,
    pub view_dirty: bool,
}

impl EventLoopState {
    pub fn new(application: MainContent) -> Self {
        Self {
            input: InputHandler::new(),
            physics: PhysicsCoordinator::new(application),
            render_drawing: true,
            view_dirty: true,
        }
    }

    /// Process an input action and update state accordingly
    /// Returns true if the action affects viewport (caller should mark view_dirty)
    pub fn process_action(&mut self, action: &InputAction) -> bool {
        let mut affects_view = false;

        match action {
            InputAction::TogglePause => {
                self.physics.send_command(PhysicsCommand::TogglePause);
            }
            InputAction::ZoomIn { .. } | InputAction::ZoomOut { .. } => {
                // Viewport changes are handled by the event loop, just mark as dirty
                affects_view = true;
            }
            InputAction::PanUpdate { .. } => {
                affects_view = true;
            }
            InputAction::ResetView => {
                affects_view = true;
            }
            InputAction::ToggleRendering => {
                self.render_drawing = !self.render_drawing;
            }
            InputAction::AddBall { position } => {
                self.physics
                    .send_command(PhysicsCommand::AddBall(*position));
            }
            InputAction::RemoveBallAt { position } => {
                // Note: The current PhysicsCommand doesn't have RemoveBallAt
                // This would need to be added to PhysicsCommand in the future
                // For now, we can leave this as a placeholder or implement a workaround
                // TODO: Add RemoveBallAt to PhysicsCommand
                let _ = position; // Suppress unused warning
            }
            InputAction::NextBallSize => {
                self.physics.send_command(PhysicsCommand::NextBallSize);
            }
            InputAction::AddRandomBalls { count } => {
                self.physics
                    .send_command(PhysicsCommand::AddRandomBalls(*count));
            }
            InputAction::StopAllBalls => {
                self.physics.send_command(PhysicsCommand::StopAllBalls);
            }
            InputAction::ClearBalls => {
                self.physics.send_command(PhysicsCommand::ClearBalls);
            }
            InputAction::ToggleStepwise => {
                self.physics.send_command(PhysicsCommand::ToggleStepwise);
            }
            InputAction::RandomizeSpeeds => {
                self.physics.send_command(PhysicsCommand::RandomizeSpeeds);
            }
            InputAction::DoubleTimeStep => {
                self.physics.send_command(PhysicsCommand::DoubleTimeStep);
            }
            InputAction::HalveTimeStep => {
                self.physics.send_command(PhysicsCommand::HalveTimeStep);
            }
            InputAction::IncreaseWaitTime => {
                self.physics.send_command(PhysicsCommand::IncreaseWaitTime);
            }
            InputAction::DecreaseWaitTime => {
                self.physics.send_command(PhysicsCommand::DecreaseWaitTime);
            }
            InputAction::EnableRandomBallSpeed => {
                self.physics
                    .send_command(PhysicsCommand::EnableRandomBallSpeed);
            }
            InputAction::ToggleRealData => {
                self.physics.send_command(PhysicsCommand::ToggleRealData);
            }
            InputAction::ToggleTimingDisplay => {
                self.physics
                    .send_command(PhysicsCommand::ToggleTimingDisplay);
            }
            InputAction::ToggleBallStats => {
                self.physics.send_command(PhysicsCommand::ToggleBallStats);
            }
            InputAction::StartVerbosityInput => {
                self.physics
                    .send_command(PhysicsCommand::StartVerbosityInput);
            }
            // Actions that don't affect physics or viewport state
            InputAction::PanStart { .. }
            | InputAction::PanEnd
            | InputAction::UpdateCursor { .. }
            | InputAction::EnableCursor { .. }
            | InputAction::DisableCursor
            | InputAction::CyclePlotContent
            | InputAction::ToggleEnergyWindow
            | InputAction::PrintHelp => {
                // These are handled separately by the event loop implementation
            }
        }

        if affects_view {
            self.view_dirty = true;
        }

        affects_view
    }

    /// Check if rendering should occur this frame
    pub fn should_render(&self) -> bool {
        self.render_drawing && (self.physics.is_new_step_available() || self.view_dirty)
    }

    /// Mark view as clean after rendering
    pub fn mark_rendered(&mut self) {
        self.view_dirty = false;
        if let Some(snapshot) = self.physics.get_latest_snapshot() {
            self.physics.mark_step_displayed(snapshot.simulation_step);
        }
    }
}
