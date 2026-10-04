use super::unified_render_state::UnifiedRenderState;

/// How much a frame draws. Pausing the simulation does not change this:
/// pause freezes physics only, and the render loop keeps running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDetail {
    /// Nothing changed: draw nothing.
    Skip,
    /// Rendering is switched off (F2): only what operators, the mouse/tool
    /// preview, the sidebar and the labels need. No ball instance building or
    /// upload, no ball draw pass, no render-only visualisation data.
    Light,
    /// The full scene.
    Full,
}

/// Coordinates per-frame snapshot handoff and renderer bookkeeping.
pub(super) struct FrameOrchestrator;

impl FrameOrchestrator {
    /// Apply results reported by the physics thread (save/load), then pull
    /// the latest physics snapshot into render state and update
    /// snapshot-derived views such as the energy plot.
    pub(super) fn begin_frame(state: &mut UnifiedRenderState) {
        while let Some(event) = state.physics.try_recv_event() {
            state.handle_physics_event(event);
        }
        if let Some(snapshot) = state.physics.get_latest_snapshot() {
            state.render.last_snapshot = Some(snapshot);
            Self::update_energy_from_snapshot(state);
        }
    }

    /// Start render metrics collection for the current frame.
    pub(super) fn begin_render(state: &mut UnifiedRenderState) {
        state.render.metrics.begin_render();
    }

    /// Decide what the backend draws this frame.
    pub(super) fn frame_detail(state: &UnifiedRenderState) -> FrameDetail {
        if !(state.physics.is_new_step_available() || state.render.view_dirty) {
            FrameDetail::Skip
        } else if state.render.config.render_drawing
            && state
                .render
                .last_snapshot
                .as_ref()
                .is_none_or(|snapshot| snapshot.full_scene)
        {
            // After F2 on, the first snapshots may still be light (no balls):
            // draw those light until the full one arrives.
            FrameDetail::Full
        } else {
            FrameDetail::Light
        }
    }

    /// Determine whether the backend should draw a frame (full or light).
    pub(super) fn should_render(state: &UnifiedRenderState) -> bool {
        Self::frame_detail(state) != FrameDetail::Skip
    }

    /// Finalize frame bookkeeping after drawing completes.
    pub(super) fn end_frame(state: &mut UnifiedRenderState) {
        state.render.view_dirty = false;
        state.render.metrics.end_frame();
        if let Some(snapshot) = &state.render.last_snapshot {
            state.physics.mark_snapshot_displayed(snapshot.revision);
            state.physics.mark_step_displayed(
                snapshot.simulation_step,
                snapshot.world_generation,
                snapshot.cell_grid_bounds,
            );
        }
    }

    fn update_energy_from_snapshot(state: &mut UnifiedRenderState) {
        if let Some(snapshot) = &state.render.last_snapshot {
            let frame_metrics = state.render.metrics.current().clone();
            state.render.energy_manager.update_from_snapshot(
                snapshot,
                state.render.metrics.average_fps(),
                state.physics.steps_per_frame(),
                &frame_metrics,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameDetail, FrameOrchestrator};
    use crate::application::renderer_snapshot::OperatorSnapshot;
    use crate::application::renderer_snapshot::RendererSnapshot;
    use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
    use crate::graphics::renderer::ViewportTransform;
    use crate::graphics::renderer::event_loop::energy_window_manager::EnergyWindowManager;
    use crate::graphics::renderer::event_loop::input_handler::{InputHandler, OperatorTool};
    use crate::graphics::renderer::event_loop::metrics::MetricsCollector;
    use crate::graphics::renderer::event_loop::physics_coordinator::PhysicsCoordinator;
    use crate::graphics::renderer::event_loop::unified_render_state::{
        RenderStateData, UnifiedRenderState,
    };
    use crate::graphics::renderer::margins::Margins;
    use crate::graphics::renderer::state_layers::RendererConfig;
    use crate::objects2d::{Rect, Vec2D};
    use std::sync::{Arc, Mutex};

    fn dummy_viewport() -> ViewportTransform {
        let world = Rect::new(-18.0, 12.0, 36.0, -24.0);
        ViewportTransform::new(world, (1280.0, 720.0), Margins::symmetric(50.0, 50.0))
    }

    fn build_test_state_with_physics(physics: PhysicsCoordinator) -> UnifiedRenderState {
        let energy_data: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));
        UnifiedRenderState {
            input: InputHandler::new(),
            physics,
            render: RenderStateData {
                viewport: dummy_viewport(),
                energy_manager: EnergyWindowManager::new(energy_data),
                config: RendererConfig::default(),
                view_dirty: false,
                last_snapshot: None,
                cursor_world_pos: Vec2D::zero(),
                current_ball_size_idx: 2,
                metrics: MetricsCollector::new(),
                active_tool: OperatorTool::None,
                tool_handler: None,
                default_operator_snapshot: OperatorSnapshot::default(),
                resize_pending: false,
                requested_window_size: None,
                pending_window_request: None,
            },
        }
    }

    fn build_test_state() -> UnifiedRenderState {
        build_test_state_with_physics(PhysicsCoordinator::new_disconnected())
    }

    #[test]
    fn world_loaded_switches_to_the_default_view_of_the_loaded_world() {
        use crate::application::PhysicsEvent;
        use crate::application::persistence::LoadedWorld;

        let mut state = build_test_state();
        state.render.viewport.zoom_at_point(&Vec2D::zero(), 2.0);
        let loaded_rect = Rect::new(-50.0, 30.0, 100.0, -60.0);

        state.handle_physics_event(PhysicsEvent::LoadFailed("bad file".into()));
        assert_ne!(state.render.viewport.world_bounds(), loaded_rect);
        assert!(
            !state.render.view_dirty,
            "a failed load leaves the view alone"
        );

        state.handle_physics_event(PhysicsEvent::WorldLoaded(LoadedWorld {
            path: "saves/world.json".into(),
            world_rect: loaded_rect,
        }));
        assert_eq!(state.render.viewport.world_bounds(), loaded_rect);
        assert_eq!(state.render.viewport.zoom(), ViewportTransform::MIN_ZOOM);
        assert!(state.render.view_dirty);
    }

    fn load(state: &mut UnifiedRenderState, world_rect: Rect) {
        use crate::application::PhysicsEvent;
        use crate::application::persistence::LoadedWorld;
        state.handle_physics_event(PhysicsEvent::WorldLoaded(LoadedWorld {
            path: "saves/world.json".into(),
            world_rect,
        }));
    }

    #[test]
    fn loaded_world_of_another_shape_requests_a_matching_window() {
        // Window 1280x720 with 50 px margins: drawing area 1180x620.
        let mut state = build_test_state();
        load(&mut state, Rect::new(-50.0, 50.0, 100.0, -100.0));
        // 1:1 size for a 100x100 world (LEN_TO_PX = 0.1) plus the margins.
        assert_eq!(state.take_requested_window_size(), Some((1100, 1100)));
        assert_eq!(state.take_requested_window_size(), None, "asked once");
        let pending = state
            .render
            .pending_window_request
            .expect("the OS's answer is awaited");
        assert_eq!(pending.size, (1100, 1100));
    }

    #[test]
    fn an_immediate_os_answer_of_the_current_size_ends_the_wait() {
        let mut state = build_test_state();
        load(&mut state, Rect::new(-50.0, 50.0, 100.0, -100.0));
        assert!(state.take_requested_window_size().is_some());
        // The OS keeps the window as it is (maximised): same size back.
        let (w, h) = state.render.viewport.window_size_px();
        let margins = state.render.viewport.margins();
        state.apply_window_size_answer(w, h, margins);
        assert!(
            state.render.pending_window_request.is_none(),
            "the next user resize must refit the world again"
        );
    }

    #[test]
    fn loaded_world_of_the_window_shape_keeps_the_window() {
        let mut state = build_test_state();
        load(&mut state, Rect::new(-59.0, 31.0, 118.0, -62.0));
        assert_eq!(state.take_requested_window_size(), None);
        assert!(state.render.pending_window_request.is_none());
    }

    #[test]
    fn begin_frame_pulls_snapshot_and_updates_energy_window() {
        let mut snapshot = RendererSnapshot::new();
        snapshot.simulation_step = 7;
        snapshot.kinetic_energy = 1.5;
        snapshot.potential_energy = 2.5;
        snapshot.time = 3.0;
        snapshot.physics_fps = 120.0;
        snapshot.status_text = Some("fps 0".to_string());

        let physics = PhysicsCoordinator::with_snapshot(snapshot);
        let mut state = build_test_state_with_physics(physics);

        FrameOrchestrator::begin_frame(&mut state);

        let snapshot = state
            .render
            .last_snapshot
            .as_ref()
            .expect("snapshot should be stored");
        assert_eq!(snapshot.simulation_step, 7);

        let energy_data = state
            .render
            .energy_manager
            .energy_data()
            .lock()
            .expect("energy data lock should succeed");
        assert_eq!(energy_data.v_kin.len(), 1);
        assert_eq!(energy_data.v_pot.len(), 1);
        assert_eq!(energy_data.v_kin[0], 1.5);
        assert_eq!(energy_data.v_pot[0], 2.5);
        assert_eq!(energy_data.time, 3.0);
        assert!(
            energy_data
                .status_text
                .as_deref()
                .is_some_and(|status| status.contains("render: 0 fps"))
        );
    }

    #[test]
    fn should_render_uses_view_dirty_when_rendering_enabled() {
        let mut state = build_test_state();
        state.render.view_dirty = true;

        assert!(FrameOrchestrator::should_render(&state));
    }

    #[test]
    fn should_render_respects_rendering_toggle() {
        let mut state = build_test_state();
        state.render.view_dirty = true;
        state.render.config.render_drawing = false;

        assert_eq!(FrameOrchestrator::frame_detail(&state), FrameDetail::Light);
        assert!(FrameOrchestrator::should_render(&state));
    }

    #[test]
    fn nothing_new_skips_the_frame_whatever_the_toggle() {
        for render_drawing in [true, false] {
            let mut state = build_test_state();
            state.render.config.render_drawing = render_drawing;
            assert_eq!(FrameOrchestrator::frame_detail(&state), FrameDetail::Skip);
        }
    }

    #[test]
    fn a_new_snapshot_of_the_same_step_is_drawn_once_when_paused() {
        let mut snapshot = RendererSnapshot::new();
        snapshot.simulation_step = 5;
        let mut state = build_test_state_with_physics(PhysicsCoordinator::with_snapshot(snapshot));
        FrameOrchestrator::begin_frame(&mut state);
        FrameOrchestrator::end_frame(&mut state);
        assert_eq!(FrameOrchestrator::frame_detail(&state), FrameDetail::Skip);

        // The paused simulation publishes the same step again, with new
        // operator data.
        let mut next = RendererSnapshot::new();
        next.simulation_step = 5;
        next.revision = 1;
        state.physics.store_snapshot_for_test(next);
        assert_eq!(FrameOrchestrator::frame_detail(&state), FrameDetail::Full);

        FrameOrchestrator::begin_frame(&mut state);
        FrameOrchestrator::end_frame(&mut state);
        assert_eq!(FrameOrchestrator::frame_detail(&state), FrameDetail::Skip);
    }

    /// What changed since the last drawn frame.
    #[derive(Clone, Copy, Debug)]
    enum Change {
        /// Nothing: a paused world republishing the same revision.
        Nothing,
        /// The simulation advanced (running only).
        Step,
        /// A paused edit: same step, new revision.
        PausedEdit,
        /// Mouse, zoom, pan, resize: `view_dirty`.
        View,
    }

    /// A state that drew snapshot (`step`, revision 0) already.
    fn drawn_state(render_drawing: bool, step: u64) -> UnifiedRenderState {
        let mut snapshot = RendererSnapshot::new();
        snapshot.simulation_step = step;
        let mut state = build_test_state_with_physics(PhysicsCoordinator::with_snapshot(snapshot));
        state.render.config.render_drawing = render_drawing;
        FrameOrchestrator::begin_frame(&mut state);
        FrameOrchestrator::end_frame(&mut state);
        state
    }

    fn apply(state: &mut UnifiedRenderState, change: Change) {
        let mut next = RendererSnapshot::new();
        next.simulation_step = 5;
        next.full_scene = state
            .render
            .last_snapshot
            .as_ref()
            .is_none_or(|s| s.full_scene);
        match change {
            Change::Nothing => return,
            Change::Step => {
                next.simulation_step = 6;
                next.revision = 1;
            }
            Change::PausedEdit => next.revision = 1,
            Change::View => {
                state.render.view_dirty = true;
                return;
            }
        }
        state.physics.store_snapshot_for_test(next);
    }

    /// Paused/running x F2 on/off x what changed. Pause freezes physics only:
    /// the frame follows the changes, and F2 only picks the detail.
    #[test]
    fn frame_matrix_paused_running_by_rendering_on_off() {
        use Change::*;
        let cases = [
            // (paused, change, draws)
            (false, Nothing, false),
            (false, Step, true),
            (false, View, true),
            (true, Nothing, false), // a paused, unchanged world stays idle
            (true, PausedEdit, true),
            (true, View, true),
        ];
        for render_drawing in [true, false] {
            for (paused, change, draws) in cases {
                let mut state = drawn_state(render_drawing, 5);
                apply(&mut state, change);

                let expected = match (draws, render_drawing) {
                    (false, _) => FrameDetail::Skip,
                    (true, true) => FrameDetail::Full,
                    (true, false) => FrameDetail::Light,
                };
                assert_eq!(
                    FrameOrchestrator::frame_detail(&state),
                    expected,
                    "paused={paused} F2={render_drawing} change={change:?}"
                );

                // Drawn once, then idle again.
                FrameOrchestrator::begin_frame(&mut state);
                FrameOrchestrator::end_frame(&mut state);
                assert_eq!(
                    FrameOrchestrator::frame_detail(&state),
                    FrameDetail::Skip,
                    "paused={paused} F2={render_drawing} change={change:?}: redraws forever"
                );
            }
        }
    }

    #[test]
    fn light_snapshot_is_drawn_light_until_the_full_scene_arrives() {
        // F2 was just switched on, the physics thread still sends light snapshots.
        let mut light = RendererSnapshot::new();
        light.full_scene = false;
        let mut state = build_test_state_with_physics(PhysicsCoordinator::with_snapshot(light));
        FrameOrchestrator::begin_frame(&mut state);
        assert_eq!(FrameOrchestrator::frame_detail(&state), FrameDetail::Light);

        let mut full = RendererSnapshot::new();
        full.revision = 1;
        state.physics.store_snapshot_for_test(full);
        FrameOrchestrator::begin_frame(&mut state);
        assert_eq!(FrameOrchestrator::frame_detail(&state), FrameDetail::Full);
    }

    #[test]
    fn end_frame_clears_view_dirty() {
        let mut state = build_test_state();
        state.render.view_dirty = true;

        FrameOrchestrator::end_frame(&mut state);

        assert!(!state.render.view_dirty);
    }

    #[test]
    fn end_frame_advances_displayed_simulation_step() {
        let mut snapshot = RendererSnapshot::new();
        snapshot.simulation_step = 42;

        let mut state = build_test_state_with_physics(PhysicsCoordinator::with_snapshot(snapshot));
        // Load the snapshot into render state so end_frame can mark it displayed.
        FrameOrchestrator::begin_frame(&mut state);

        FrameOrchestrator::end_frame(&mut state);

        assert_eq!(state.physics.last_displayed_step(), 42);
    }
}
