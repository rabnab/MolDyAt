// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub use self::ball_factory::*;
pub use self::ball_physics::*;
// Re-export core components for external use
#[cfg(any(test, feature = "test-support"))]
pub use self::species::SPECIES_M;
pub use self::species::{
    DEFAULT_SPECIES, LIBRARY, LibrarySpecies, N2_EPSILON_KELVIN, N2_MASS_U, N2_SIGMA_ANGSTROM,
    Species, SpeciesId, SpeciesSet, SpeciesTable,
};
pub use crate::objects2d::ball_impl::ball_core::Ball2D;

use std::sync::atomic::{AtomicUsize, Ordering};

// Module declarations
pub(crate) mod ball_core;
mod ball_factory;
pub(crate) mod ball_physics;
pub(crate) mod pair_potential;
pub(crate) mod simd_forces;
pub(crate) mod species;
pub(crate) mod verlet;
pub(crate) mod wall_contact;
pub(crate) mod wall_law;

/// Neighbour-list rebuilds so far; rebuild checks run in parallel.
static UPDATE_HOOD_CNT: AtomicUsize = AtomicUsize::new(0);

/// get the count of how often update_hood was actually collecting the neighbors
pub(crate) fn get_update_hood_call_cnt() -> usize {
    UPDATE_HOOD_CNT.load(Ordering::Relaxed)
}

// Type definitions and constants that are common across ball modules
pub(crate) type BallId = usize;

/// Common trait for ball identification
pub(crate) trait Named {
    fn get_name(&self) -> String;
}

/// Result type for ball-related operations
#[cfg(test)]
pub(crate) type BallResult<T> = Result<T, BallError>;

/// Error types for ball-related operations
#[derive(Debug, PartialEq)]
#[cfg(test)]
pub(crate) enum BallError {
    InvalidParameter,
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
