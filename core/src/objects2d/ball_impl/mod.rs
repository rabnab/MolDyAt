// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub use self::ball_factory::*;
pub use self::ball_physics::*;
// Re-export core components for external use
#[cfg(test)]
pub(crate) use self::species::SPECIES_M;
pub use self::species::{
    DEFAULT_SPECIES, LIBRARY, LibrarySpecies, N2_EPSILON_KELVIN, N2_MASS_U, N2_SIGMA_ANGSTROM,
    Species, SpeciesId, SpeciesSet, SpeciesTable,
};
pub use crate::objects2d::ball_impl::ball_core::Ball2D;

use std::sync::atomic::{AtomicUsize, Ordering};

// Module declarations
pub mod ball_core;
mod ball_factory;
pub mod ball_physics;
pub mod pair_potential;
pub mod simd_forces;
pub(crate) mod species;
pub mod verlet;
pub mod wall_contact;
pub mod wall_law;

/// Neighbour-list rebuilds so far; rebuild checks run in parallel.
static UPDATE_HOOD_CNT: AtomicUsize = AtomicUsize::new(0);

/// get the count of how often update_hood was actually collecting the neighbors
pub fn get_update_hood_call_cnt() -> usize {
    UPDATE_HOOD_CNT.load(Ordering::Relaxed)
}

// Type definitions and constants that are common across ball modules
pub type BallId = usize;

/// Common trait for ball identification
pub trait Named {
    fn get_name(&self) -> String;
}

/// Result type for ball-related operations
pub type BallResult<T> = Result<T, BallError>;

/// Error types for ball-related operations
#[derive(Debug, PartialEq)]
pub enum BallError {
    DistanceUnderflow,
    PhysicsError,
    InvalidParameter,
    // Add other error types as needed
}

// Common validation functions if needed
#[inline]
#[cfg(test)]
pub(crate) fn validate_radius(radius: f32) -> BallResult<()> {
    if radius <= 0.0 {
        Err(BallError::InvalidParameter)
    } else {
        Ok(())
    }
}

#[inline]
#[cfg(test)]
pub(crate) fn validate_mass(mass: f32) -> BallResult<()> {
    if mass <= 0.0 {
        Err(BallError::InvalidParameter)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
