// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The model of a species and the pair interactions it brings (#425).
//!
//! Every pair goes through the Lennard-Jones pair table (the SIMD kernel,
//! the fast path; a molecule's centre has `ε = 0` there). What a model adds
//! beyond it is an [`Interaction`], compiled per species pair by
//! [`pair_interactions`] when the physics is built; the force pass applies a
//! pair's interactions in the same walk over the neighbour list.

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::ForceResult;
use crate::objects2d::ball_impl::molecule::{LinearMolecule, Site, SitePairs};
use crate::objects2d::ball_impl::pair_potential::CapSettings;
use crate::objects2d::ball_impl::species::{Species, SpeciesTable};
use crate::objects2d::ball_impl::water::MbWater;

/// How a species interacts.
#[derive(Clone, Debug, PartialEq)]
pub enum Model {
    /// A single Lennard-Jones site; with `inertia` (#348) it has an
    /// orientation, which no force turns.
    Atom { inertia: Option<f32> },
    /// Lennard-Jones sites of a rigid body (#406).
    RigidLinear(LinearMolecule),
    /// Mercedes-Benz water (#349): its Lennard-Jones disk plus hydrogen bonds.
    MbWater(MbWater),
}

impl Model {
    /// Moment of inertia about the centre, `None` without orientation.
    pub(crate) fn inertia(&self) -> Option<f32> {
        match self {
            Model::Atom { inertia } => *inertia,
            Model::RigidLinear(m) => Some(m.inertia()),
            Model::MbWater(w) => Some(w.inertia),
        }
    }
}

impl Species {
    /// The Lennard-Jones sites of the species, which meet the walls and are
    /// drawn one by one: a molecule's, or one at the centre with the
    /// species' `σ`, `ε` and mass.
    pub fn sites(&self) -> Vec<Site> {
        match &self.model {
            Model::RigidLinear(m) => m.sites.clone(),
            Model::Atom { .. } | Model::MbWater(_) => vec![Site {
                offset: Vec2D::zero(),
                sigma: self.sigma,
                epsilon: self.epsilon,
                mass: self.mass,
            }],
        }
    }
}

/// An interaction of one species pair besides the pair table.
#[derive(Clone)]
pub(crate) enum Interaction {
    /// Site–site Lennard-Jones of a pair with a molecule (#406).
    Sites(SitePairs),
    /// Hydrogen bonds of two water balls (#349).
    HBond(MbWater),
}

impl Interaction {
    /// Centre distance at and beyond which it is zero.
    pub(crate) fn cutoff(&self) -> f32 {
        match self {
            Interaction::Sites(s) => s.cutoff,
            Interaction::HBond(w) => w.cutoff(),
        }
    }

    /// `ω²` of the fastest motion it drives between `a` and `b`, for the
    /// time step limit (`BallPhysics::time_step_limit`).
    pub(crate) fn omega_sq(&self, a: &Species, b: &Species) -> f32 {
        match self {
            Interaction::Sites(s) => s.omega_sq(a, b),
            Interaction::HBond(w) => w.omega_sq(a, b),
        }
    }

    /// Add the pair's energy (counted by both balls, like the pair energy),
    /// force, torque, virial and capped evaluations on ball `i` to `out`:
    /// `i` with axis `axis_i` (the unit vector at its `θ`), `j` at
    /// `r = r_j − r_i` with axis `axis_j`.
    pub(crate) fn eval(&self, r: Vec2D, axis_i: Vec2D, axis_j: Vec2D, out: &mut ForceResult) {
        match self {
            Interaction::Sites(s) => s.eval(r, axis_i, axis_j, out),
            Interaction::HBond(w) => w.eval(r, axis_i, axis_j, out),
        }
    }
}

/// The interactions of every species pair of `table`, row-major.
pub(crate) fn pair_interactions(table: &SpeciesTable, cap: CapSettings) -> Vec<Vec<Interaction>> {
    table
        .iter()
        .flat_map(|a| table.iter().map(move |b| (a, b)))
        .map(|(a, b)| match (&a.model, &b.model) {
            (Model::RigidLinear(_), _) | (_, Model::RigidLinear(_)) => {
                vec![Interaction::Sites(SitePairs::new(a, b, cap))]
            }
            // Only between equal parameters: the bond is not mixed.
            (Model::MbWater(w), Model::MbWater(w_b)) if w == w_b => vec![Interaction::HBond(*w)],
            _ => Vec::new(),
        })
        .collect()
}
