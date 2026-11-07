// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Physical units (#236, plan item I5).
//!
//! The simulation runs in reduced Lennard-Jones units: `σ = ε = m = k_B = 1`.
//! A `UnitSystem` says which substance those units stand for and converts
//! both ways: literature parameters (Å, K, u) into reduced values for new
//! species, and reduced results (temperature, time, energy, speed) into SI or
//! common units for display. The reference is argon (`UnitSystem::AR`,
//! #405), the built-in species `Ar` with `σ* = ε* = m* = 1`. N₂
//! (`UnitSystem::N2`) was the reference from #236 to #405; saves written
//! then are rescaled from it on load ([`UnitScale`]).
//!
//! Conversions use f64; the simulation stores f32. The parameters are fitted
//! to 3D gases, so in this 2D simulation the SI values are qualitative.
//!
//! Every save states the unit system it was written in (`WorldFile::units`),
//! and constants chosen in physical units are converted here, so changing
//! `REDUCED` needs no migration.

use crate::objects2d::ball_impl::{
    AR_EPSILON_KELVIN, AR_MASS_U, AR_SIGMA_ANGSTROM, N2_EPSILON_KELVIN, N2_MASS_U,
    N2_SIGMA_ANGSTROM,
};
use serde::{Deserialize, Serialize};

/// Boltzmann constant (exact, SI 2019).
pub(crate) const BOLTZMANN_J_PER_K: f64 = 1.380_649e-23;
/// Atomic mass constant (CODATA 2018).
pub(crate) const ATOMIC_MASS_KG: f64 = 1.660_539_066_60e-27;

/// The substance whose single-site Lennard-Jones parameters define the
/// reduced units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnitSystem {
    /// `σ` in Å: the reduced unit of length.
    pub sigma_angstrom: f64,
    /// `ε / k_B` in K: the reduced unit of temperature.
    pub epsilon_kelvin: f64,
    /// Molecular mass in u: the reduced unit of mass.
    pub mass_u: f64,
}

// `from_*` convert a physical value into this unit system, so they need it.
#[allow(clippy::wrong_self_convention)]
impl UnitSystem {
    /// Argon, the reference (#405), from second virial coefficients:
    /// Hirschfelder, Curtiss and Bird (1954), table I-A.
    pub const AR: UnitSystem = UnitSystem {
        sigma_angstrom: AR_SIGMA_ANGSTROM,
        epsilon_kelvin: AR_EPSILON_KELVIN,
        mass_u: AR_MASS_U,
    };

    /// The reference of the reduced units the simulation runs in.
    pub const REDUCED: UnitSystem = UnitSystem::AR;

    /// N₂, the reference from #236 to #405, from second virial coefficients: Hirschfelder, Curtiss and Bird,
    /// *Molecular Theory of Gases and Liquids* (Wiley, 1954), table I-A.
    /// Published single-site fits for N₂ range over about 70–100 K for
    /// `ε / k_B`, depending on the fitted property; see `docs/units.md`.
    pub const N2: UnitSystem = UnitSystem {
        sigma_angstrom: N2_SIGMA_ANGSTROM,
        epsilon_kelvin: N2_EPSILON_KELVIN,
        mass_u: N2_MASS_U,
    };

    /// Unit of length in m.
    pub(crate) fn length_m(&self) -> f64 {
        self.sigma_angstrom * 1e-10
    }

    /// Unit of energy in J.
    pub(crate) fn energy_j(&self) -> f64 {
        self.epsilon_kelvin * BOLTZMANN_J_PER_K
    }

    /// Unit of mass in kg.
    pub(crate) fn mass_kg(&self) -> f64 {
        self.mass_u * ATOMIC_MASS_KG
    }

    /// Unit of time `τ = σ·√(m/ε)` in s.
    pub(crate) fn time_s(&self) -> f64 {
        self.length_m() * (self.mass_kg() / self.energy_j()).sqrt()
    }

    // ── reduced → physical ─────────────────────────────────────────

    /// Reduced temperature (or energy per `k_B`) in K.
    pub fn to_kelvin(self, reduced: f32) -> f64 {
        reduced as f64 * self.epsilon_kelvin
    }

    /// Reduced time in ps.
    pub fn to_picoseconds(self, reduced: f32) -> f64 {
        reduced as f64 * self.time_s() * 1e12
    }

    /// Reduced time in fs.
    pub fn to_femtoseconds(self, reduced: f32) -> f64 {
        reduced as f64 * self.time_s() * 1e15
    }

    /// Reduced length in Å.
    #[cfg(test)]
    pub(crate) fn to_angstrom(self, reduced: f32) -> f64 {
        reduced as f64 * self.sigma_angstrom
    }

    /// Unit of moment of inertia in u·Å².
    pub(crate) const fn inertia_u_angstrom2(&self) -> f64 {
        self.mass_u * self.sigma_angstrom * self.sigma_angstrom
    }

    /// Reduced energy in J.
    pub fn to_joules(self, reduced: f32) -> f64 {
        reduced as f64 * self.energy_j()
    }

    /// Reduced speed in m/s.
    pub fn to_metres_per_second(self, reduced: f32) -> f64 {
        reduced as f64 * self.length_m() / self.time_s()
    }

    // ── physical → reduced ─────────────────────────────────────────

    /// A length in Å (e.g. a literature `σ`) in reduced units.
    pub(crate) const fn from_angstrom(&self, angstrom: f64) -> f32 {
        (angstrom / self.sigma_angstrom) as f32
    }

    /// A temperature in K (or a literature `ε / k_B`) in reduced units.
    pub(crate) const fn from_kelvin(&self, kelvin: f64) -> f32 {
        (kelvin / self.epsilon_kelvin) as f32
    }

    /// A mass in u in reduced units.
    pub(crate) const fn from_atomic_mass(&self, mass_u: f64) -> f32 {
        (mass_u / self.mass_u) as f32
    }

    /// A force in K/Å (`k_B`·K per Å) in reduced units.
    pub(crate) const fn from_kelvin_per_angstrom(&self, force: f64) -> f32 {
        (force / (self.epsilon_kelvin / self.sigma_angstrom)) as f32
    }

    /// A spring constant in K/Å² in reduced units.
    pub(crate) const fn from_kelvin_per_angstrom2(&self, stiffness: f64) -> f32 {
        (stiffness / (self.epsilon_kelvin / (self.sigma_angstrom * self.sigma_angstrom))) as f32
    }

    /// An acceleration in m/s² (e.g. gravity) in reduced units:
    /// `a* = a·τ² / σ`.
    #[cfg(test)]
    pub(crate) fn from_metres_per_second2(&self, accel: f64) -> f32 {
        (accel * self.time_s() * self.time_s() / self.length_m()) as f32
    }

    /// The factors that turn a reduced value of `self` into one of `to`.
    pub(crate) fn scale_to(&self, to: &UnitSystem) -> UnitScale {
        UnitScale {
            length: self.sigma_angstrom / to.sigma_angstrom,
            energy: self.epsilon_kelvin / to.epsilon_kelvin,
            mass: self.mass_u / to.mass_u,
            time: self.time_s() / to.time_s(),
        }
    }

    /// Reduced `(σ*, ε*, m*)` of a species from its literature single-site
    /// Lennard-Jones parameters.
    #[cfg(test)]
    pub(crate) fn reduced_lennard_jones(
        &self,
        sigma_angstrom: f64,
        epsilon_kelvin: f64,
        mass_u: f64,
    ) -> (f32, f32, f32) {
        (
            self.from_angstrom(sigma_angstrom),
            self.from_kelvin(epsilon_kelvin),
            self.from_atomic_mass(mass_u),
        )
    }
}

/// Factors between two unit systems (`UnitSystem::scale_to`): a reduced
/// length `x` of one is `x · length` in the other, and so on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UnitScale {
    pub length: f64,
    pub energy: f64,
    pub mass: f64,
    pub time: f64,
}

impl UnitScale {
    pub(crate) fn speed(&self) -> f64 {
        self.length / self.time
    }

    pub(crate) fn acceleration(&self) -> f64 {
        self.length / (self.time * self.time)
    }

    /// Spring constant (energy per length squared).
    pub(crate) fn stiffness(&self) -> f64 {
        self.energy / (self.length * self.length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N2: UnitSystem = UnitSystem::N2;
    const AR: UnitSystem = UnitSystem::AR;

    #[test]
    fn argon_is_the_unit_species() {
        assert_eq!(UnitSystem::REDUCED, AR);
        assert_eq!(
            AR.reduced_lennard_jones(3.405, 119.8, 39.948),
            (1.0, 1.0, 1.0)
        );
        approx(AR.time_s(), 2.1564e-12, 1e-4);
    }

    /// N₂ in Ar units, as listed in `docs/units.md`.
    #[test]
    fn n2_in_argon_units() {
        let (sigma, epsilon, mass) = AR.reduced_lennard_jones(3.698, 95.05, 28.0134);
        approx(sigma as f64, 1.0861, 1e-4);
        approx(epsilon as f64, 0.7934, 1e-4);
        approx(mass as f64, 0.7012, 1e-3);
        let scale = N2.scale_to(&AR);
        approx(scale.length, 1.0861, 1e-4);
        approx(scale.time, 1.0210, 1e-4);
        approx(scale.acceleration(), 1.0418, 1e-4);
    }

    #[test]
    fn gravity_is_about_1_34e_13_in_argon_units() {
        approx(AR.from_metres_per_second2(9.81) as f64, 1.3397e-13, 1e-4);
    }

    fn approx(a: f64, b: f64, rel: f64) {
        assert!((a - b).abs() <= rel * b.abs(), "{a} vs {b}");
    }

    #[test]
    fn n2_time_unit_is_about_2_2_ps() {
        approx(N2.time_s(), 2.2017e-12, 1e-4);
        approx(N2.to_picoseconds(1.0), 2.2017, 1e-4);
        approx(N2.to_femtoseconds(0.01), 22.017, 1e-4);
    }

    #[test]
    fn n2_is_the_unit_species() {
        assert_eq!(
            N2.reduced_lennard_jones(3.698, 95.05, 28.0134),
            (1.0, 1.0, 1.0)
        );
        approx(N2.to_kelvin(1.0), 95.05, 1e-12);
        approx(N2.to_angstrom(1.0), 3.698, 1e-12);
    }

    /// Argon in N₂ units, as listed in `docs/units.md`.
    #[test]
    fn literature_parameters_convert_to_reduced_units() {
        let (sigma, epsilon, mass) = N2.reduced_lennard_jones(3.405, 119.8, 39.948);
        approx(sigma as f64, 0.9208, 1e-3);
        approx(epsilon as f64, 1.2604, 1e-3);
        approx(mass as f64, 1.4260, 1e-3);
    }

    /// The built-in species are converted with the reduced unit system.
    #[test]
    fn library_species_use_these_units() {
        use crate::objects2d::ball_impl::{LIBRARY, SpeciesTable};
        let table = SpeciesTable::builtin();
        for lib in &LIBRARY {
            let s = table.species(table.by_name(lib.name).unwrap());
            assert_eq!(
                (s.sigma, s.epsilon, s.mass),
                UnitSystem::REDUCED.reduced_lennard_jones(
                    lib.sigma_angstrom,
                    lib.epsilon_kelvin,
                    lib.mass_u
                ),
                "{}",
                lib.name
            );
        }
    }

    #[test]
    fn conversions_round_trip() {
        for reduced in [0.0f32, 0.37, 1.0, 2.5, 120.0] {
            approx(
                N2.from_angstrom(N2.to_angstrom(reduced)) as f64,
                reduced as f64,
                1e-6,
            );
            approx(
                N2.from_kelvin(N2.to_kelvin(reduced)) as f64,
                reduced as f64,
                1e-6,
            );
        }
        for kelvin in [10.0, 77.0, 95.05, 300.0] {
            approx(N2.to_kelvin(N2.from_kelvin(kelvin)), kelvin, 1e-6);
        }
        for angstrom in [1.0, 3.405, 3.698, 10.0] {
            approx(N2.to_angstrom(N2.from_angstrom(angstrom)), angstrom, 1e-6);
        }
        for mass_u in [2.016, 28.0134, 196.967] {
            approx(N2.from_atomic_mass(mass_u) as f64 * N2.mass_u, mass_u, 1e-6);
        }
    }

    #[test]
    fn speed_and_energy_units_are_consistent() {
        // ½·m·v² with m* = v* = 1 is half the energy unit.
        let v = N2.to_metres_per_second(1.0);
        approx(0.5 * N2.mass_kg() * v * v, 0.5 * N2.energy_j(), 1e-12);
        approx(N2.to_joules(2.0), 2.0 * N2.energy_j(), 1e-12);
    }

    #[test]
    fn gravity_is_about_1_3e_13_in_n2_units() {
        approx(N2.from_metres_per_second2(9.81) as f64, 1.2859e-13, 1e-4);
    }
}
