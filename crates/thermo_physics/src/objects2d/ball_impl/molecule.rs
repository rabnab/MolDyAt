// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Rigid linear molecules (#406): a ball whose species is a molecule
//! carries Lennard-Jones sites at fixed offsets along its axis, turned by
//! the ball's orientation `θ` (#348). The ball is the molecule's centre of
//! mass; its mass and moment of inertia are the sum over the sites.
//!
//! Molecules interact site by site: every site of one ball with every site
//! of the other (an atomic species is a single site at its centre), with
//! Lorentz–Berthelot mixing of the site parameters, capped and cut off like
//! a pair. Forces at the sites give the ball's force and torque.

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::ForceResult;
use crate::objects2d::ball_impl::ball_physics::REFERENCE_ENERGY;
use crate::objects2d::ball_impl::pair_potential::{
    CapSettings, CappedPotential, LennardJones, PairPotential,
};
use crate::objects2d::ball_impl::species::Species;

/// One Lennard-Jones site of a [`LinearMolecule`], in reduced units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoleculeSite {
    /// Signed distance from the centre of mass along the axis.
    pub offset: f32,
    pub sigma: f32,
    pub epsilon: f32,
    pub mass: f32,
}

/// A rigid linear molecule: its sites along the axis at angle `θ`.
#[derive(Clone, Debug, PartialEq)]
pub struct LinearMolecule {
    pub sites: Vec<MoleculeSite>,
}

impl LinearMolecule {
    /// Two equal sites `bond` apart, centred.
    pub(crate) fn diatomic(bond: f32, sigma: f32, epsilon: f32, site_mass: f32) -> Self {
        let site = |offset| MoleculeSite {
            offset,
            sigma,
            epsilon,
            mass: site_mass,
        };
        Self {
            sites: vec![site(-0.5 * bond), site(0.5 * bond)],
        }
    }

    pub(crate) fn mass(&self) -> f32 {
        self.sites.iter().map(|s| s.mass).sum()
    }

    /// `Σ m x²` about the centre of mass.
    pub(crate) fn inertia(&self) -> f32 {
        self.sites
            .iter()
            .map(|s| s.mass * s.offset * s.offset)
            .sum()
    }

    /// Largest distance of a site centre from the centre of mass.
    pub(crate) fn half_length(&self) -> f32 {
        self.sites
            .iter()
            .map(|s| s.offset.abs())
            .fold(0.0, f32::max)
    }

    /// Largest site `σ`.
    pub(crate) fn max_site_sigma(&self) -> f32 {
        self.sites.iter().map(|s| s.sigma).fold(0.0, f32::max)
    }

    /// Diameter of the disk that holds every site's contact disk: what
    /// placement and the centre-based checks use as the species' `σ`.
    pub(crate) fn envelope_sigma(&self) -> f32 {
        2.0 * self
            .sites
            .iter()
            .map(|s| s.offset.abs() + 0.5 * s.sigma)
            .fold(0.0, f32::max)
    }
}

/// Unit vector of the molecular axis at angle `theta`.
#[inline]
pub(crate) fn axis(theta: f32) -> Vec2D {
    let (sin, cos) = theta.sin_cos();
    Vec2D::new(cos, sin)
}

/// 2D cross product `a × b` (the z component): the torque of force `b` at
/// lever `a`.
#[inline]
pub(crate) fn cross(a: Vec2D, b: Vec2D) -> f32 {
    a.x * b.y - a.y * b.x
}

/// The site–site Lennard-Jones of a species pair with a molecule (#406):
/// every site of one with every site of the other (an atomic species is one
/// site at its centre with its own `σ`, `ε`), mixed by Lorentz–Berthelot.
#[derive(Clone)]
pub(crate) struct SitePairs {
    /// (offset in `a`, offset in `b`, potential) of each site pair.
    pairs: Vec<(f32, f32, CappedPotential<LennardJones>)>,
    pub cutoff: f32,
}

impl SitePairs {
    pub(crate) fn new(a: &Species, b: &Species, cap: CapSettings) -> Self {
        let sites = |s: &Species| match s.model.molecule() {
            Some(m) => m
                .sites
                .iter()
                .map(|x| (x.offset, x.sigma, x.epsilon))
                .collect(),
            None => vec![(0.0, s.sigma, s.epsilon)],
        };
        let sites_b = sites(b);
        let pairs: Vec<_> = sites(a)
            .into_iter()
            .flat_map(|(oa, sa, ea)| {
                sites_b.iter().map(move |&(ob, sb, eb)| {
                    let law = LennardJones {
                        sigma: 0.5 * (sa + sb),
                        epsilon: (ea * eb).sqrt(),
                    };
                    (oa, ob, CappedPotential::new(law, cap))
                })
            })
            .collect();
        let cutoff = pairs
            .iter()
            .map(|(oa, ob, p)| oa.abs() + ob.abs() + p.cutoff())
            .fold(0.0, f32::max);
        Self { pairs, cutoff }
    }

    /// The stiffest site pair on the reduced mass of its two sites; a site at
    /// lever `L` moves across the axis as `1/(1/M + L²/I)`.
    pub(crate) fn omega_sq(&self, a: &Species, b: &Species) -> f32 {
        let site_mass = |s: &Species, offset: f32| match s.model.molecule() {
            Some(m) => 1.0 / (1.0 / m.mass() + offset * offset / m.inertia()),
            None => s.mass,
        };
        self.pairs
            .iter()
            .map(|(oa, ob, p)| {
                let (ma, mb) = (site_mass(a, *oa), site_mass(b, *ob));
                p.stiffness(REFERENCE_ENERGY) / (ma * mb / (ma + mb))
            })
            .fold(0.0, f32::max)
    }

    pub(crate) fn eval(&self, r: Vec2D, axis_i: Vec2D, axis_j: Vec2D, out: &mut ForceResult) {
        for (oa, ob, pot) in &self.pairs {
            let pa = axis_i.scaled(*oa);
            // From site a to site b.
            let d = r.add_scaled(&axis_j, *ob) - &pa;
            let dist = d.norm();
            if dist >= pot.cutoff() || dist <= f32::EPSILON {
                continue;
            }
            // Repulsive force pushes a away from b.
            let f = d.scaled(-pot.force(dist) / dist);
            out.capped_pairs += u32::from(pot.is_capped(dist));
            out.potential_energy += pot.potential(dist);
            out.force += f;
            out.torque += cross(pa, f);
            out.virial -= d.dot(&f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects2d::ball_impl::species::{SpeciesId, SpeciesTable};

    fn table() -> &'static SpeciesTable {
        SpeciesTable::builtin()
    }

    /// The site pairs of species `a` (angle `ti`) with `b` (`tj`) at `r`.
    fn pair(a: SpeciesId, ti: f32, b: SpeciesId, tj: f32, r: Vec2D) -> ForceResult {
        let (a, b) = (table().species(a), table().species(b));
        let mut out = ForceResult::default();
        SitePairs::new(a, b, CapSettings::default()).eval(r, axis(ti), axis(tj), &mut out);
        out
    }

    fn id(name: &str) -> SpeciesId {
        table().by_name(name).unwrap()
    }

    /// N2_2S: two TraPPE sites, mass and inertia from them, envelope `σ`.
    #[test]
    fn n2_dumbbell_is_built_from_its_sites() {
        let s = table().species(id("N2_2S"));
        let m = s.model.molecule().unwrap();
        assert_eq!(m.sites.len(), 2);
        let bond = (1.10 / 3.405) as f32;
        assert!((m.sites[1].offset - m.sites[0].offset - bond).abs() < 1e-6);
        assert!((s.mass - (2.0 * 14.007 / 39.948) as f32).abs() < 1e-6);
        assert!((s.model.inertia().unwrap() - s.mass * 0.25 * bond * bond).abs() < 1e-6);
        assert_eq!(s.epsilon, 0.0, "the centre itself does not interact");
        assert!((s.sigma - (bond + (3.31 / 3.405) as f32)).abs() < 1e-5);
    }

    /// Force and torque on `i` are minus the gradient of the site–site
    /// energy, for molecule–molecule and molecule–atom pairs.
    #[test]
    fn force_and_torque_match_finite_differences() {
        let (n2, ar) = (id("N2_2S"), id("Ar"));
        let h = 1e-3;
        for (sj, r, ti, tj) in [
            (n2, Vec2D::new(1.35, 0.2), 0.3, -1.1),
            (n2, Vec2D::new(-0.9, 1.0), 2.0, 0.4),
            (ar, Vec2D::new(1.2, -0.3), -0.7, 0.0),
        ] {
            let e = |r: Vec2D, ti: f32| pair(n2, ti, sj, tj, r).potential_energy;
            let p = pair(n2, ti, sj, tj, r);
            let fx = (e(r + Vec2D::new(h, 0.0), ti) - e(r - &Vec2D::new(h, 0.0), ti)) / (2.0 * h);
            let fy = (e(r + Vec2D::new(0.0, h), ti) - e(r - &Vec2D::new(0.0, h), ti)) / (2.0 * h);
            let torque = -(e(r, ti + h) - e(r, ti - h)) / (2.0 * h);
            let scale = p.force.norm().max(p.torque.abs()).max(1.0);
            assert!(
                (p.force.x - fx).abs() < 2e-2 * scale,
                "{r}: fx {} vs {fx}",
                p.force.x
            );
            assert!(
                (p.force.y - fy).abs() < 2e-2 * scale,
                "{r}: fy {} vs {fy}",
                p.force.y
            );
            assert!(
                (p.torque - torque).abs() < 2e-2 * scale,
                "{r}: τ {} vs {torque}",
                p.torque
            );
            assert!(p.potential_energy != 0.0, "{r}: inside the cutoff");
        }
    }

    /// CO2_3S: O–C–O with the TraPPE sites, symmetric, inertia from the
    /// oxygens only (#408).
    #[test]
    fn co2_chain_is_built_from_its_sites() {
        let s = table().species(id("CO2_3S"));
        let m = s.model.molecule().unwrap();
        let (o, c) = (m.sites[0], m.sites[1]);
        assert_eq!(m.sites.len(), 3);
        assert_eq!(c.offset, 0.0);
        assert_eq!(m.sites[2].offset, -o.offset);
        assert!((o.offset.abs() - (1.16 / 3.405) as f32).abs() < 1e-6);
        assert!((o.sigma - (3.05 / 3.405) as f32).abs() < 1e-6);
        assert!((c.epsilon - (27.0 / 119.8) as f32).abs() < 1e-6);
        assert!((s.mass - ((2.0 * 15.999 + 12.011) / 39.948) as f32).abs() < 1e-6);
        assert!((s.model.inertia().unwrap() - 2.0 * o.mass * o.offset * o.offset).abs() < 1e-6);
    }

    /// Force and torque of the three-site chain vs finite differences.
    #[test]
    fn co2_force_and_torque_match_finite_differences() {
        let (co2, ar) = (id("CO2_3S"), id("Ar"));
        let h = 1e-3;
        for (sj, r, ti, tj) in [
            (co2, Vec2D::new(1.6, 0.4), 0.3, 1.2),
            (co2, Vec2D::new(-0.6, 1.5), -1.0, 0.2),
            (ar, Vec2D::new(1.3, -0.5), 0.8, 0.0),
        ] {
            let e = |r: Vec2D, ti: f32| pair(co2, ti, sj, tj, r).potential_energy;
            let p = pair(co2, ti, sj, tj, r);
            let fx = (e(r + Vec2D::new(h, 0.0), ti) - e(r - &Vec2D::new(h, 0.0), ti)) / (2.0 * h);
            let fy = (e(r + Vec2D::new(0.0, h), ti) - e(r - &Vec2D::new(0.0, h), ti)) / (2.0 * h);
            let torque = -(e(r, ti + h) - e(r, ti - h)) / (2.0 * h);
            let scale = p.force.norm().max(p.torque.abs()).max(1.0);
            assert!(
                (p.force.x - fx).abs() < 2e-2 * scale,
                "{r}: fx {} vs {fx}",
                p.force.x
            );
            assert!(
                (p.force.y - fy).abs() < 2e-2 * scale,
                "{r}: fy {} vs {fy}",
                p.force.y
            );
            assert!(
                (p.torque - torque).abs() < 2e-2 * scale,
                "{r}: τ {} vs {torque}",
                p.torque
            );
            assert!(p.potential_energy != 0.0, "{r}: inside the cutoff");
        }
    }

    /// The pair seen from the other ball: same energy, opposite force.
    #[test]
    fn pair_is_symmetric() {
        let n2 = id("N2_2S");
        let r = Vec2D::new(1.3, 0.25);
        let a = pair(n2, 0.4, n2, -0.8, r);
        let b = pair(n2, -0.8, n2, 0.4, r.scaled(-1.0));
        assert!(
            (a.potential_energy - b.potential_energy).abs()
                < 1e-5 * a.potential_energy.abs().max(1.0)
        );
        assert!((a.force + b.force).norm() < 1e-4 * a.force.norm().max(1.0));
    }
}
