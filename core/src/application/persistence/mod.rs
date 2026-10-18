// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Saving and loading the simulation world (issue #50).
//!
//! - [`format`]: the file layout ([`WorldFile`]) and its validation.
//! - `convert`: live world ⇄ file.
//! - `migrate`: upgrading files of older format versions before they are read.
//! - [`store`]: where files live ([`SaveStore`]).
//!
//! [`MainContent::save_world`] and [`MainContent::load_world`] are the only
//! entry points: Ctrl+S / Ctrl+O reach them on the physics thread through
//! `DomainCommand`s, and `--load` calls `load_world` before the run starts.
//! The view (window size, zoom, pan) is not saved; a loaded world is shown in
//! the default view.
//!
//! # Reference saves
//!
//! `tests/fixtures/` holds one save per format version (`v1.json`, ...),
//! written by the build that introduced that version, and
//! `tests/reference_saves.rs` loads each one through [`MainContent::load_world`]
//! and checks its contents. An existing fixture is never edited or
//! regenerated: it stands for the files users already have. A change that
//! bumps [`WORLD_FILE_VERSION`] adds a new fixture, and every older one must
//! keep loading.
//!
//! # Additive and breaking changes (#271)
//!
//! A change to a file type ([`WorldFile`] and the types saved in it) is either
//! additive, which keeps [`WORLD_FILE_VERSION`], or breaking, which bumps it
//! and adds a migration (see `migrate`).
//!
//! Additive, no version bump: older files simply lack the new data, and serde
//! fills it in.
//! - A new field gets `#[serde(default)]` or `#[serde(default = "…")]` with a
//!   default that reproduces the behaviour of builds before the field
//!   existed, e.g. `#[serde(default = "default_friction")] friction: f32`
//!   where `default_friction()` returns the value that used to be hard-coded.
//! - A new top-level list (e.g. a new operator kind) is
//!   `#[serde(default)] sinks: Vec<SinkData>`: older files have none.
//! - [`WorldFile::validate`] must accept the default, so an old file that
//!   lacks the field passes validation.
//!
//! Breaking, bump the version and add a migration: renaming or removing a
//! field, changing its type, unit or meaning, or changing a default that old
//! files rely on. A plain `#[serde(default)]` would load such a file without
//! an error but with the wrong values.
//!
//! Exception: a save's `config` (`SimConfig`) rejects unknown keys, so a
//! config key added later makes the newer saves unreadable for older builds
//! of the same version (they report that the save is probably newer). And
//! builds from before `config` existed ignore it: they run such a save with
//! the default config. See #273 for the rule on newer files.
//!
//! `tests/reference_saves.rs` (`old_saves_load_into_a_file_type_with_new_fields`)
//! loads every reference save into a file type with extra defaulted fields
//! and checks the defaults.
//!
//! # Built-in species
//!
//! A file names each ball's species; mass, `σ` and `ε` come from the built-in
//! species table when the file is loaded. So the built-in names and their
//! mass, `σ` and `ε` are part of the file format: changing one is a breaking
//! change (bump [`WORLD_FILE_VERSION`] and add a migration), or add a new
//! species instead of changing an existing one. `builtin_table_is_pinned`
//! (in `objects2d/ball_impl/species.rs`) fails on any such change.

mod convert;
pub mod format;
mod migrate;
pub mod store;

#[cfg(test)]
mod tests;

pub use format::{WORLD_FILE_VERSION, WorldFile};
pub use store::SaveStore;

use crate::application::core::SimulationWorld;
use crate::application::{AppResult, MainContent};
use crate::objects2d::Rect;
use std::path::PathBuf;

/// Which save to load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadSource {
    /// The newest save in the store.
    Latest,
    /// A specific file.
    Path(PathBuf),
}

/// What a successful load produced, for whoever shows the world.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedWorld {
    pub path: PathBuf,
    pub world_rect: Rect,
}

impl MainContent {
    /// Write the world to a new save in `store`; returns its path.
    pub fn save_world(&self, store: &SaveStore) -> AppResult<PathBuf> {
        store.save(&WorldFile::capture(self))
    }

    /// Replace the world with the save `source` refers to.
    ///
    /// Everything that can fail (finding, reading, validating, building the
    /// new world) happens before the running world is touched, so on an error
    /// it is left as it was.
    pub fn load_world(
        &mut self,
        store: &SaveStore,
        source: &LoadSource,
        start_paused: bool,
    ) -> AppResult<LoadedWorld> {
        let path = store.resolve(source)?;
        let file = store.load(&path)?;
        let mut world = SimulationWorld::try_from(&file)?;
        world.sim.set_pause(start_paused);
        let config = file.sim_config();
        self.set_time_step(config.integration.time_step)?;
        self.runtime_mut()
            .set_snapshot_interval_us(config.display.snapshot_interval_us);

        *self.world_mut() = world;
        self.runtime_mut().next_world_generation();
        let (world, step_config) = self.world_and_step_config();
        step_config
            .get_ball_factory()
            .continue_after(world.balls.iter().map(|ball| ball.get_id()));
        self.collect_neighbours_by_cell(file.time_step);

        Ok(LoadedWorld {
            path,
            world_rect: file.world_rect,
        })
    }
}
