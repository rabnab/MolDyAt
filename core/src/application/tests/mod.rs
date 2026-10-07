mod physics_thread;
mod thermo_field;

use crate::application::renderer_snapshot::RendererSnapshot;
use crate::application::{DomainCommand, PhysicsThread};
use arc_swap::ArcSwapOption;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a test waits for the physics thread before it fails. Only a hung
/// thread should hit it: the waits below poll for a condition, so a slow
/// runner makes a test slower, never wrong.
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll `slot` until a published snapshot satisfies `done`, and return it.
pub(crate) fn wait_for_snapshot(
    slot: &ArcSwapOption<RendererSnapshot>,
    what: &str,
    done: impl Fn(&RendererSnapshot) -> bool,
) -> Arc<RendererSnapshot> {
    let deadline = Instant::now() + SNAPSHOT_TIMEOUT;
    loop {
        if let Some(snapshot) = slot.load_full().filter(|s| done(s)) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {SNAPSHOT_TIMEOUT:?} waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Wait until every command sent to `physics_thread` so far has been applied
/// and published, and return that snapshot.
///
/// Sends a marker command (`NextBallSize`, visible as the snapshot's cursor
/// ball size) and waits for it. The thread applies commands in order, so the
/// commands sent before the marker have been applied by then. The marker
/// does change the next ball size; tests that use this must not depend on it.
pub(crate) fn sync_commands(
    physics_thread: &PhysicsThread,
    slot: &ArcSwapOption<RendererSnapshot>,
) -> Arc<RendererSnapshot> {
    let before = wait_for_snapshot(slot, "a first snapshot", |_| true).cursor_ball_size_idx;
    physics_thread
        .send_command(DomainCommand::NextBallSize)
        .expect("send marker command");
    wait_for_snapshot(slot, "the marker command", |s| {
        s.cursor_ball_size_idx != before
    })
}
