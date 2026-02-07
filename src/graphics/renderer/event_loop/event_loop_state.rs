use super::{InputAction, InputHandler, PhysicsCoordinator};
use crate::application::physics_thread::PhysicsCommand;
use crate::application::{LogStuff, MainContent};
use crate::graphics::renderer::state_layers::RendererConfig;

/// Shared state for any event loop implementation
pub struct EventLoopState {
    pub input: InputHandler,
    pub physics: PhysicsCoordinator,
    pub config: RendererConfig,
    pub view_dirty: bool,
}

impl EventLoopState {
    pub fn new(application: MainContent) -> Self {
        Self {
            input: InputHandler::new(),
            physics: PhysicsCoordinator::new(application),
            config: RendererConfig::default(),
            view_dirty: true,
        }
    }

    /// Process an input action and update state accordingly
    /// Returns true if the action affects viewport (caller should mark view_dirty)
    pub fn process_action(&mut self, action: &InputAction) -> bool {
        let affects_view = process_action_common(action, &self.physics, &mut self.config);
        if affects_view {
            self.view_dirty = true;
        }
        affects_view
    }

    /// Check if rendering should occur this frame
    pub fn should_render(&self) -> bool {
        self.config.render_drawing && (self.physics.is_new_step_available() || self.view_dirty)
    }

    /// Mark view as clean after rendering
    pub fn mark_rendered(&mut self) {
        self.view_dirty = false;
        if let Some(snapshot) = self.physics.get_latest_snapshot() {
            self.physics.mark_step_displayed(snapshot.simulation_step);
        }
    }
}

/// Shared mapping from InputAction to physics commands and renderer state flags.
/// Returns true if the action affects the viewport (caller should refresh view).
pub fn process_action_common(
    action: &InputAction,
    physics: &PhysicsCoordinator,
    config: &mut RendererConfig,
) -> bool {
    let mut affects_view = false;

    match action {
        InputAction::TogglePause => {
            physics.send_command(PhysicsCommand::TogglePause);
        }
        InputAction::ZoomIn { .. } | InputAction::ZoomOut { .. } => {
            // Viewport changes are handled by the event loop
            affects_view = true;
        }
        InputAction::PanUpdate { .. } => {
            affects_view = true;
        }
        InputAction::ResetView => {
            affects_view = true;
        }
        InputAction::ToggleRendering => {
            config.toggle_rendering();
        }
        InputAction::AddBall { position } => {
            physics.send_command(PhysicsCommand::AddBall(*position));
        }
        InputAction::NextBallSize => {
            physics.send_command(PhysicsCommand::NextBallSize);
        }
        InputAction::AddRandomBalls { count } => {
            physics.send_command(PhysicsCommand::AddRandomBalls(*count));
        }
        InputAction::StopAllBalls => {
            physics.send_command(PhysicsCommand::StopAllBalls);
        }
        InputAction::ClearBalls => {
            physics.send_command(PhysicsCommand::ClearBalls);
        }
        InputAction::ToggleStepwise => {
            physics.send_command(PhysicsCommand::ToggleStepwise);
        }
        InputAction::RandomizeSpeeds => {
            physics.send_command(PhysicsCommand::RandomizeSpeeds);
        }
        InputAction::DoubleTimeStep => {
            physics.send_command(PhysicsCommand::DoubleTimeStep);
        }
        InputAction::HalveTimeStep => {
            physics.send_command(PhysicsCommand::HalveTimeStep);
        }
        InputAction::IncreaseWaitTime => {
            physics.send_command(PhysicsCommand::IncreaseWaitTime);
        }
        InputAction::DecreaseWaitTime => {
            physics.send_command(PhysicsCommand::DecreaseWaitTime);
        }
        InputAction::EnableRandomBallSpeed => {
            physics.send_command(PhysicsCommand::EnableRandomBallSpeed);
        }
        InputAction::ToggleRealData => {
            physics.send_command(PhysicsCommand::ToggleRealData);
        }
        InputAction::ToggleTimingDisplay => {
            physics.send_command(PhysicsCommand::ToggleTimingDisplay);
        }
        InputAction::ToggleBallStats => {
            physics.send_command(PhysicsCommand::ToggleBallStats);
        }
        InputAction::StartVerbosityInput => {
            physics.send_command(PhysicsCommand::StartVerbosityInput);
        }
        InputAction::PrintHelp => {
            MainContent::print_help_message();
        }
        // Actions handled elsewhere (renderer-specific)
        InputAction::PanStart { .. }
        | InputAction::PanEnd
        | InputAction::UpdateCursor { .. }
        | InputAction::EnableCursor { .. }
        | InputAction::DisableCursor
        | InputAction::CyclePlotContent
        | InputAction::ToggleEnergyWindow
        | InputAction::ToggleCutoffRings => {}
    }

    affects_view
}
