// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod export_thread;
mod main_content_thread;
mod operator_commands;
mod physics_thread;
mod save_load_thread;
mod snapshot;

use crate::application::renderer_snapshot::RendererSnapshot;
use crate::application::{BallCommand, PhysicsThread};
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
/// Sends a marker command (`NextSpecies`, visible as the snapshot's
/// `cursor_species`) and waits for it. The thread applies commands in order,
/// so the commands sent before the marker have been applied by then. The
/// marker does change the species of new balls and of new source spheres
/// (#238): balls or spheres placed after it are not of the default species,
/// so tests that use this must not depend on their species.
pub(crate) fn sync_commands(
    physics_thread: &PhysicsThread,
    slot: &ArcSwapOption<RendererSnapshot>,
) -> Arc<RendererSnapshot> {
    let before = wait_for_snapshot(slot, "a first snapshot", |_| true).cursor_species;
    physics_thread
        .send_command(BallCommand::NextSpecies)
        .expect("send marker command");
    wait_for_snapshot(slot, "the marker command", |s| s.cursor_species != before)
}
