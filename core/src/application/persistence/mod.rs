//! Saving and loading the simulation world (issue #50).
//!
//! - [`format`]: the file layout ([`WorldFile`]) and its validation.
//! - `convert`: live world ⇄ file.
//! - [`store`]: where files live ([`SaveStore`]).
//!
//! [`MainContent::save_world`] and [`MainContent::load_world`] are the only
//! entry points: Ctrl+S / Ctrl+O reach them on the physics thread through
//! `DomainCommand`s, and `--load` calls `load_world` before the run starts.
//! The view (window size, zoom, pan) is not saved; a loaded world is shown in
//! the default view.

mod convert;
pub mod format;
pub mod store;

#[cfg(test)]
mod tests;

pub use format::{WORLD_FILE_VERSION, WorldFile};
pub use store::SaveStore;

use crate::application::core::SimulationWorld;
use crate::application::{AppResult, GameLogic, MainContent};
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
        store.save(&WorldFile::capture(self.world(), self.time_step()))
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
        self.set_time_step(file.time_step)?;

        *self.world_mut() = world;
        self.runtime_mut().next_world_generation();
        let (world, game_data) = self.world_and_game_data();
        game_data
            .get_ball_factory()
            .continue_after(world.balls.iter().map(|ball| ball.get_id()));
        self.collect_neighbours_by_cell(file.time_step);

        Ok(LoadedWorld {
            path,
            world_rect: file.world_rect,
        })
    }
}
