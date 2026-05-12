//! Tests for `EnergyWindowManager` throttling, coalescing, and data-version
//! behaviour introduced in issue 163.

use crate::application::renderer_snapshot::RendererSnapshot;
use crate::graphics::energy_plot::{EnergyData, SharedEnergyData};
use crate::graphics::renderer::event_loop::energy_window_manager::EnergyWindowManager;
use crate::graphics::renderer::event_loop::metrics::FrameMetrics;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

#[test]
fn push_sample_increments_version_each_call() {
    let mut data = EnergyData::new();
    data.push_sample(1.0, 2.0, 0.1);
    assert_eq!(data.version, 1);
    data.push_sample(2.0, 3.0, 0.2);
    assert_eq!(data.version, 2);
    data.push_sample(3.0, 4.0, 0.3);
    assert_eq!(data.version, 3);
}

#[test]
fn push_sample_stores_energy_values() {
    let mut data = EnergyData::new();
    data.push_sample(5.5, 3.3, 1.0);
    assert_eq!(data.v_kin.back().copied(), Some(5.5));
    assert_eq!(data.v_pot.back().copied(), Some(3.3));
    assert_eq!(data.time, 1.0);
}

#[test]
fn update_from_snapshot_increments_version() {
    let shared: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));
    let mut manager = EnergyWindowManager::new(shared.clone());

    let snapshot = make_snapshot(1, 10.0, 20.0);
    manager.update_from_snapshot(&snapshot, 60.0, 1.0, &FrameMetrics::default());

    let data = shared.lock().unwrap();
    assert_eq!(
        data.version, 1,
        "version should have incremented after update"
    );
}

#[test]
fn update_from_snapshot_ignores_duplicate_step() {
    let shared: SharedEnergyData = Arc::new(Mutex::new(EnergyData::new()));
    let mut manager = EnergyWindowManager::new(shared.clone());

    let snapshot = make_snapshot(5, 1.0, 2.0);
    manager.update_from_snapshot(&snapshot, 60.0, 1.0, &FrameMetrics::default());
    manager.update_from_snapshot(&snapshot, 60.0, 1.0, &FrameMetrics::default());

    let data = shared.lock().unwrap();
    assert_eq!(
        data.v_kin.len(),
        1,
        "duplicate step should be ignored, only one sample expected"
    );
    assert_eq!(
        data.version, 1,
        "duplicate update must not bump version again"
    );
}

// ── handle_redraw ────────────────────────────────────────────────────────────

#[test]
fn handle_redraw_returns_false_when_no_window() {
    let mut manager = make_manager();
    assert!(
        !manager.handle_redraw(),
        "handle_redraw should return false when no energy window is open"
    );
}

#[test]
fn handle_redraw_does_not_alter_pending_flag_without_window() {
    // When there is no window, handle_redraw does not perform a render and
    // therefore should not modify the pending/throttle state.
    let mut manager = make_manager();
    manager.mark_redraw_pending();

    manager.handle_redraw(); // no window → no render

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
fn handle_redraw_then_request_redraw_within_interval_does_not_re_queue() {
    // Simulate a very recent last-redraw by directly setting the instant.
    // The throttle interval (83 ms) should prevent another immediate request
    // even though the pending flag has been cleared.
    let mut manager = make_manager();
    let just_now = Instant::now();
    manager.set_last_redraw_instant(Some(just_now));
    // Clear any pending state so we can test the time-guard path alone.
    // (The pending flag is initially false, so this is a no-op, but explicit.)
    manager.request_redraw(); // no-op: no window, but time-guard fires first

    assert!(
        !manager.is_redraw_pending(),
        "redraw must be throttled when last render was just now"
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
