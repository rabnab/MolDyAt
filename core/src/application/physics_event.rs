// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Results the physics thread reports back to the renderer.
//!
//! Snapshots only carry the latest state and may be skipped; one-shot results
//! such as "the world was loaded" go through this channel instead, so none
//! is lost.

use crate::application::persistence::LoadedWorld;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum PhysicsEvent {
    WorldSaved(PathBuf),
    SaveFailed(String),
    /// The world was replaced; the view should show `world_rect`.
    WorldLoaded(LoadedWorld),
    LoadFailed(String),
}
