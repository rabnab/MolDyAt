// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! How balls meet walls (box walls, wall pieces, closed valves).
//!
//! The wall code asks the [`WallLaw`] how close a ball's centre may come to a
//! wall surface, and resolves the contact there (see `wall_contact`). Only the
//! hard contact exists: a ball is reflected at centre distance `σ_i / 2`, the
//! closest approach of two like balls at about T* = 1 (≈ 0.97·σ). A soft wall
//! potential (#280) adds a law with a force and keeps a hard contact at a
//! smaller distance as a backstop; the call sites stay as they are.

use crate::objects2d::ball_impl::species::Species;

/// The interaction of balls with walls.
pub(crate) trait WallLaw {
    /// Centre distance from a wall surface at which a ball of `species`
    /// touches the wall and is reflected.
    fn contact_distance(&self, species: &Species) -> f32;
}

/// Elastic (or damped, by the restitution) reflection at `σ_i / 2`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HardContact;

impl WallLaw for HardContact {
    fn contact_distance(&self, species: &Species) -> f32 {
        species.contact_radius()
    }
}
