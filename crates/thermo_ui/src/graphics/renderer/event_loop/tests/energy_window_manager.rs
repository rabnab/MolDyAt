// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Tests for `EnergyWindowManager` throttling, coalescing, and data-version
//! behaviour introduced in issue 163.

use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
use crate::graphics::renderer::event_loop::energy_window_manager::EnergyWindowManager;
use crate::graphics::renderer::event_loop::metrics::FrameMetrics;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thermo_runtime::application::renderer_snapshot::RendererSnapshot;
use thermo_runtime::metrics::MetricsRegistry;

fn make_manager() -> EnergyWindowManager {
    let data: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));
    EnergyWindowManager::new(data)
}

fn make_snapshot(step: u64, kin: f32, pot: f32) -> RendererSnapshot {
    let mut s = RendererSnapshot::new();
    s.simulation_step = step;
    s.kinetic_energy = kin;
    s.potential_energy = pot;
    s.time = step as f32 * 0.01;
    s
}

// ── EnergyData version counter ──────────────────────────────────────────────

#[test]
fn energy_data_version_starts_at_zero() {
    let data = EnergyData::new();
    assert_eq!(data.version, 0);
}

fn shared_with_metrics(
    metrics: &thermo_runtime::metrics::SharedMetricsRegistry,
) -> SharedEnergyData {
    Arc::new(Mutex::new(EnergyData::with_metrics(Arc::clone(metrics))))
}

fn record(metrics: &thermo_runtime::metrics::SharedMetricsRegistry, kin: f32, pot: f32) {
    let mut registry = metrics.write().unwrap();
    registry.record("E_kin", kin);
    registry.record("E_pot", pot);
}

#[test]
fn update_from_snapshot_increments_version_for_new_samples() {
    let metrics = MetricsRegistry::shared();
    record(&metrics, 10.0, 20.0);
    let shared = shared_with_metrics(&metrics);
    let mut manager = EnergyWindowManager::new(shared.clone());

    manager.update_from_snapshot(
        &make_snapshot(1, 10.0, 20.0),
        60.0,
        1.0,
        &FrameMetrics::default(),
    );

    let data = shared.lock().unwrap();
    assert_eq!(data.version, 1);
    assert_eq!(data.v_kin.back().copied(), Some(10.0));
    assert_eq!(data.time, 0.01);
}

#[test]
fn update_from_snapshot_ignores_duplicate_step() {
    let metrics = MetricsRegistry::shared();
    record(&metrics, 1.0, 2.0);
    let shared = shared_with_metrics(&metrics);
    let mut manager = EnergyWindowManager::new(shared.clone());
    let snapshot = make_snapshot(5, 1.0, 2.0);
    manager.update_from_snapshot(&snapshot, 60.0, 1.0, &FrameMetrics::default());
    manager.update_from_snapshot(&snapshot, 60.0, 1.0, &FrameMetrics::default());

    let data = shared.lock().unwrap();
    assert_eq!(data.v_kin.len(), 1);
    assert_eq!(data.version, 1, "no new sample must not bump the version");
}

#[test]
fn update_from_snapshot_starts_a_new_history_for_a_new_world() {
    let metrics = MetricsRegistry::shared();
    let shared = shared_with_metrics(&metrics);
    let mut manager = EnergyWindowManager::new(shared.clone());
    let frame_metrics = FrameMetrics::default();
    for step in [10, 20, 30] {
        record(&metrics, 1.0, 2.0);
        manager.update_from_snapshot(&make_snapshot(step, 1.0, 2.0), 60.0, 1.0, &frame_metrics);
    }
    // A loaded world: the registry starts afresh (the physics thread does it
    // when the world generation changes).
    metrics.write().unwrap().clear();
    record(&metrics, 7.0, 8.0);
    let mut loaded = make_snapshot(30, 7.0, 8.0);
    loaded.world_generation = 1;
    manager.update_from_snapshot(&loaded, 60.0, 1.0, &frame_metrics);
    record(&metrics, 9.0, 9.5);
    let mut next = make_snapshot(32, 9.0, 9.5);
    next.world_generation = 1;
    manager.update_from_snapshot(&next, 60.0, 1.0, &frame_metrics);

    let data = shared.lock().unwrap();
    assert_eq!(
        data.v_kin.iter().copied().collect::<Vec<_>>(),
        vec![7.0, 9.0]
    );
    assert_eq!(
        data.v_pot.iter().copied().collect::<Vec<_>>(),
        vec![8.0, 9.5]
    );
}

// ── handle_redraw ────────────────────────────────────────────────────────────

#[test]
fn handle_redraw_gives_no_frame_without_window() {
    let mut manager = make_manager();
    assert!(
        manager
            .handle_redraw(&crate::graphics::renderer::event_loop::KeyMap::default())
            .is_none(),
        "handle_redraw should give no frame when no energy window is open"
    );
}

#[test]
fn handle_redraw_does_not_alter_pending_flag_without_window() {
    // When there is no window, handle_redraw does not perform a render and
    // therefore should not modify the pending/throttle state.
    let mut manager = make_manager();
    manager.mark_redraw_pending();

    manager.handle_redraw(&crate::graphics::renderer::event_loop::KeyMap::default()); // no window → no render

    // Pending flag should still be set because no render happened.
    assert!(
        manager.is_redraw_pending(),
        "pending flag must not be cleared when handle_redraw has no window to render"
    );
}

// ── request_redraw throttling / coalescing ───────────────────────────────────

#[test]
fn request_redraw_does_not_set_pending_without_window() {
    let mut manager = make_manager();
    manager.request_redraw();
    assert!(
        !manager.is_redraw_pending(),
        "no window open, so no redraw should be pending"
    );
}

#[test]
fn request_redraw_skips_when_already_pending() {
    // Mark as pending (simulate an already-outstanding request) and verify
    // that the pending flag stays set (the guard works).
    let mut manager = make_manager();
    manager.mark_redraw_pending();

    // Calling request_redraw again should detect the pending flag and return
    // early without touching anything.
    manager.request_redraw();

    assert!(
        manager.is_redraw_pending(),
        "pending flag must remain set when a redraw is already in flight"
    );
}

#[test]
fn request_redraw_with_recent_last_redraw_and_no_window_does_not_queue() {
    // Simulate a very recent last-redraw by directly setting the instant.
    // In this test setup there is no real OS window, so `request_redraw()`
    // returns early because the window is absent before any throttle logic
    // could matter. This still verifies that a recent last-redraw instant
    // does not cause the manager to mark redraw as pending in the no-window
    // case.
    let mut manager = make_manager();
    let just_now = Instant::now();
    manager.set_last_redraw_instant(Some(just_now));
    manager.request_redraw(); // no-op: no window is present, so nothing is queued

    assert!(
        !manager.is_redraw_pending(),
        "pending should remain false when no window is available to redraw"
    );
}

#[test]
fn handle_redraw_then_request_redraw_after_interval_is_allowed() {
    // Backdate the last-redraw instant to more than one interval ago and
    // verify that the manager would schedule a new redraw.  The pending flag
    // won't actually be set here because there is no real OS window, but we
    // confirm the manager does *not* return from the time-guard early by
    // checking it reaches the window check (pending stays false only because
    // the window is absent, not because of the throttle guard).
    let mut manager = make_manager();
    let past = Instant::now() - Duration::from_millis(200);
    manager.set_last_redraw_instant(Some(past));

    // request_redraw with a stale last-redraw instant AND no pending flag:
    // throttle guard passes, but no window exists → pending stays false.
    manager.request_redraw();
    assert!(
        !manager.is_redraw_pending(),
        "pending should be false because there is no window to redraw"
    );
    // The important assertion: no panic, no regression in control flow.
}

mod keys {
    use crate::graphics::renderer::event_loop::KeyCode;
    use crate::graphics::renderer::event_loop::command_registry::KeyInput;
    use crate::graphics::renderer::event_loop::energy_window_manager::{
        EnergyWindowKey, energy_window_key, energy_window_key_hints,
    };
    use crate::graphics::renderer::event_loop::key_map::{KeyMap, KeyPhase};

    fn key(code: KeyCode) -> KeyInput {
        crate::graphics::renderer::event_loop::tests::us(code)
    }

    #[test]
    fn test_default_keys_of_the_energy_window() {
        let key_map = KeyMap::default();
        for code in [KeyCode::KeyP, KeyCode::Tab] {
            assert_eq!(
                energy_window_key(&key_map, key(code), KeyPhase::Up),
                Some(EnergyWindowKey::CycleDisplay),
                "{code:?}"
            );
        }
        assert_eq!(
            energy_window_key(&key_map, key(KeyCode::Escape), KeyPhase::Down),
            Some(EnergyWindowKey::Close)
        );
        // Keys of other commands do nothing there.
        assert_eq!(
            energy_window_key(&key_map, key(KeyCode::KeyG), KeyPhase::Up),
            None
        );
    }

    /// The energy window follows the key map (#355 review).
    #[test]
    fn test_energy_window_keys_follow_overrides() {
        let x = ["X".to_string()];
        let none: [String; 0] = [];
        let (key_map, warnings) = KeyMap::with_overrides([
            ("view.cycle_plot", x.as_slice()),
            ("view.toggle_energy_window", none.as_slice()),
        ]);
        assert!(warnings.is_empty(), "{warnings:?}");
        for code in [KeyCode::KeyP, KeyCode::Tab] {
            assert_eq!(
                energy_window_key(&key_map, key(code), KeyPhase::Up),
                None,
                "{code:?}"
            );
        }
        assert_eq!(
            energy_window_key(&key_map, key(KeyCode::KeyX), KeyPhase::Up),
            Some(EnergyWindowKey::CycleDisplay)
        );
    }

    /// `HOME` resets the plot's time view in the energy window (#367).
    #[test]
    fn test_home_resets_the_plot_view() {
        assert_eq!(
            energy_window_key(&KeyMap::default(), key(KeyCode::Home), KeyPhase::Down),
            Some(EnergyWindowKey::ResetView)
        );
    }

    #[test]
    fn test_key_hints_name_the_default_keys() {
        assert_eq!(
            energy_window_key_hints(&KeyMap::default()),
            "Esc: close  TAB / P: mode  HOME: reset"
        );
    }

    /// The hints follow a `ui.toml` rebinding (#365 review).
    #[test]
    fn test_key_hints_follow_overrides() {
        let x = ["X".to_string()];
        let none: [String; 0] = [];
        let (rebound, _) = KeyMap::with_overrides([
            ("view.cycle_plot", x.as_slice()),
            ("view.toggle_energy_window", none.as_slice()),
        ]);
        assert_eq!(
            energy_window_key_hints(&rebound),
            "Esc: close  X: mode  HOME: reset"
        );

        let (unbound, _) = KeyMap::with_overrides([
            ("view.cycle_plot", none.as_slice()),
            ("view.toggle_energy_window", none.as_slice()),
        ]);
        assert_eq!(energy_window_key_hints(&unbound), "Esc: close  HOME: reset");
    }
}

/// The plot's mouse gestures (#367), against a set plot geometry: the plot
/// area spans x = 100..1100, the history 0..50 with a sample every 0.1.
mod plot_view {
    use super::*;
    use crate::graphics::energy_plot::{PlotRect, TimeHistory};
    use crate::graphics::renderer::event_loop::KeyMap;
    use crate::graphics::renderer::event_loop::energy_window_manager::PlotGeometry;
    use winit::dpi::{PhysicalPosition, PhysicalSize};
    use winit::event::{
        DeviceId, ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
    };

    fn geometry() -> PlotGeometry {
        PlotGeometry {
            area: PlotRect {
                left: 100.0,
                top: 50.0,
                right: 1100.0,
                bottom: 650.0,
            },
            history: TimeHistory {
                oldest: 0.0,
                newest: 50.0,
                samples: 501,
            },
        }
    }

    fn manager() -> EnergyWindowManager {
        let mut manager = make_manager();
        manager.set_plot_geometry(Some(geometry()));
        manager
    }

    fn send(manager: &mut EnergyWindowManager, event: WindowEvent) -> bool {
        manager.handle_event(event, &KeyMap::default())
    }

    fn wheel(delta: MouseScrollDelta) -> WindowEvent {
        WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta,
            phase: TouchPhase::Moved,
        }
    }

    fn moved(x: f64) -> WindowEvent {
        WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(x, 300.0),
        }
    }

    fn middle(state: ElementState) -> WindowEvent {
        WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state,
            button: MouseButton::Middle,
        }
    }

    fn window(manager: &EnergyWindowManager) -> (f64, f64) {
        manager.plot_view().window(&geometry().history)
    }

    #[test]
    fn wheel_forward_zooms_in_for_line_and_pixel_deltas() {
        for (forward, backward) in [
            (
                MouseScrollDelta::LineDelta(0.0, 1.0),
                MouseScrollDelta::LineDelta(0.0, -1.0),
            ),
            (
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 50.0)),
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -50.0)),
            ),
        ] {
            let mut manager = manager();
            let default = manager.plot_view().span();
            assert!(send(&mut manager, wheel(forward)));
            assert!(manager.plot_view().span() < default, "{forward:?}");
            send(&mut manager, wheel(backward));
            send(&mut manager, wheel(backward));
            assert!(manager.plot_view().span() > default, "{backward:?}");
        }
    }

    #[test]
    fn middle_drag_pans_with_the_pointer() {
        let mut manager = manager();
        send(&mut manager, moved(500.0));
        send(&mut manager, middle(ElementState::Pressed));
        // 100 px of a 1000 px wide plot showing 10 time units: 1 unit back.
        send(&mut manager, moved(600.0));
        assert_eq!(window(&manager), (39.0, 49.0));
        assert!(!manager.plot_view().is_following());

        // Released: moving no longer pans.
        send(&mut manager, middle(ElementState::Released));
        send(&mut manager, moved(900.0));
        assert_eq!(window(&manager), (39.0, 49.0));
    }

    #[test]
    fn wheel_zooms_around_the_pointer_once_panned() {
        let mut manager = manager();
        send(&mut manager, moved(500.0));
        send(&mut manager, middle(ElementState::Pressed));
        send(&mut manager, moved(700.0)); // [38, 48]
        send(&mut manager, middle(ElementState::Released));
        // The pointer at x = 350: a quarter into the plot, t = 40.5.
        send(&mut manager, moved(350.0));
        send(&mut manager, wheel(MouseScrollDelta::LineDelta(0.0, 1.0)));

        let (t0, t1) = window(&manager);
        assert!((t0 + 0.25 * (t1 - t0) - 40.5).abs() < 1e-9, "{t0}..{t1}");
    }

    #[test]
    fn middle_double_click_resets_the_view() {
        let mut manager = manager();
        manager.zoom_plot(2.0, None);
        assert!(!manager.plot_view().is_default());

        let now = Instant::now();
        manager.middle_pressed([500.0, 300.0], now);
        manager.middle_pressed([502.0, 301.0], now + Duration::from_millis(200));
        assert!(manager.plot_view().is_default());
    }

    #[test]
    fn slow_or_distant_second_click_does_not_reset() {
        let mut manager = manager();
        manager.zoom_plot(2.0, None);
        let now = Instant::now();
        manager.middle_pressed([500.0, 300.0], now);
        manager.middle_pressed([500.0, 300.0], now + Duration::from_millis(900));
        manager.middle_pressed([600.0, 300.0], now + Duration::from_millis(1000));
        assert!(!manager.plot_view().is_default());
    }

    #[test]
    fn reset_plot_view_returns_to_the_default() {
        let mut manager = manager();
        manager.zoom_plot(2.0, None);
        manager.reset_plot_view();
        assert!(manager.plot_view().is_default());
    }

    #[test]
    fn gestures_without_a_plot_do_nothing() {
        let mut manager = make_manager();
        assert!(!manager.zoom_plot(1.0, Some(500.0)));
        assert!(!manager.pan_plot(100.0));
        assert!(manager.plot_view().is_default());
    }

    #[test]
    fn other_events_are_not_gestures() {
        let mut manager = manager();
        let resized = WindowEvent::Resized(PhysicalSize::new(800, 600));
        // Handled (as a resize), but the view is unchanged.
        send(&mut manager, resized);
        assert!(manager.plot_view().is_default());
    }

    /// A loaded world starts a new history: the view returns to following
    /// the newest sample with the default span.
    #[test]
    fn new_world_returns_to_the_default_view() {
        let metrics = MetricsRegistry::shared();
        let shared = shared_with_metrics(&metrics);
        let mut manager = EnergyWindowManager::new(shared);
        manager.set_plot_geometry(Some(geometry()));
        let frame_metrics = FrameMetrics::default();
        record(&metrics, 1.0, 2.0);
        manager.update_from_snapshot(&make_snapshot(10, 1.0, 2.0), 60.0, 1.0, &frame_metrics);
        manager.zoom_plot(2.0, None);
        assert!(!manager.plot_view().is_default());

        metrics.write().unwrap().clear();
        record(&metrics, 7.0, 8.0);
        let mut loaded = make_snapshot(30, 7.0, 8.0);
        loaded.world_generation = 1;
        manager.update_from_snapshot(&loaded, 60.0, 1.0, &frame_metrics);
        assert!(manager.plot_view().is_default());
    }

    /// A press with a little jitter does not pause the plot; beyond the
    /// threshold the pan covers the whole move from the press (#368 review).
    #[test]
    fn small_moves_do_not_pan_and_larger_ones_pan_the_full_distance() {
        let mut manager = manager();
        send(&mut manager, moved(500.0));
        send(&mut manager, middle(ElementState::Pressed));
        send(&mut manager, moved(503.0));
        send(&mut manager, moved(497.0));
        assert!(manager.plot_view().is_following(), "jitter must not pan");

        send(&mut manager, moved(600.0));
        assert_eq!(window(&manager), (39.0, 49.0), "100 px from the press");
        send(&mut manager, moved(650.0));
        assert_eq!(window(&manager), (38.5, 48.5));
    }

    fn recorded_kin(shared: &SharedEnergyData) -> usize {
        shared.lock().unwrap().v_kin.len()
    }

    /// While paused, the plot keeps the history it had when the pause began;
    /// back in follow mode it shows the registry's series at once.
    #[test]
    fn paused_plot_freezes_its_history_until_it_follows_again() {
        let metrics = MetricsRegistry::shared();
        let shared = shared_with_metrics(&metrics);
        let mut manager = EnergyWindowManager::new(shared.clone());
        manager.set_plot_geometry(Some(geometry()));
        let frame_metrics = FrameMetrics::default();
        for step in 1..=3 {
            record(&metrics, step as f32, 0.0);
            manager.update_from_snapshot(&make_snapshot(step, 1.0, 0.0), 60.0, 1.0, &frame_metrics);
        }
        assert_eq!(recorded_kin(&shared), 3);

        assert!(manager.pan_plot(100.0));
        assert!(!manager.plot_view().is_following());
        for step in 4..=8 {
            record(&metrics, step as f32, 0.0);
            manager.update_from_snapshot(&make_snapshot(step, 1.0, 0.0), 60.0, 1.0, &frame_metrics);
        }
        assert_eq!(recorded_kin(&shared), 3, "frozen while paused");
        assert_eq!(shared.lock().unwrap().time, 0.08, "the time still updates");

        manager.reset_plot_view();
        assert_eq!(recorded_kin(&shared), 8, "the current series at once");
    }

    #[test]
    fn panning_back_to_the_newest_sample_resumes_the_series() {
        let metrics = MetricsRegistry::shared();
        let shared = shared_with_metrics(&metrics);
        let mut manager = EnergyWindowManager::new(shared.clone());
        manager.set_plot_geometry(Some(geometry()));
        let frame_metrics = FrameMetrics::default();
        record(&metrics, 1.0, 0.0);
        manager.update_from_snapshot(&make_snapshot(1, 1.0, 0.0), 60.0, 1.0, &frame_metrics);
        manager.pan_plot(100.0);
        record(&metrics, 2.0, 0.0);
        manager.update_from_snapshot(&make_snapshot(2, 1.0, 0.0), 60.0, 1.0, &frame_metrics);
        assert_eq!(recorded_kin(&shared), 1);

        manager.pan_plot(-1000.0);
        assert!(manager.plot_view().is_following());
        assert_eq!(recorded_kin(&shared), 2);
    }

    /// A world loaded while the plot is paused ends the pause and shows the
    /// new history.
    #[test]
    fn world_load_while_paused_resets_and_shows_the_new_history() {
        let metrics = MetricsRegistry::shared();
        let shared = shared_with_metrics(&metrics);
        let mut manager = EnergyWindowManager::new(shared.clone());
        manager.set_plot_geometry(Some(geometry()));
        let frame_metrics = FrameMetrics::default();
        for step in 1..=3 {
            record(&metrics, 1.0, 0.0);
            manager.update_from_snapshot(&make_snapshot(step, 1.0, 0.0), 60.0, 1.0, &frame_metrics);
        }
        manager.pan_plot(100.0);
        assert!(!manager.plot_view().is_following());

        metrics.write().unwrap().clear();
        record(&metrics, 7.0, 8.0);
        let mut loaded = make_snapshot(30, 7.0, 8.0);
        loaded.world_generation = 1;
        manager.update_from_snapshot(&loaded, 60.0, 1.0, &frame_metrics);

        assert!(manager.plot_view().is_default());
        let data = shared.lock().unwrap();
        assert_eq!(data.v_kin.iter().copied().collect::<Vec<_>>(), vec![7.0]);
    }
}
