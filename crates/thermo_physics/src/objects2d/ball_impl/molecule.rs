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
use crate::objects2d::ball_impl::pair_potential::{
    CapSettings, CappedPotential, LennardJones, PairPotential,
};
use crate::objects2d::ball_impl::species::{SpeciesId, SpeciesTable};

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

/// The site–site interactions of a species table with molecules (#406):
/// every species as a list of sites (an atomic species is one site at its
/// centre with its own `σ`, `ε`), and one capped Lennard-Jones potential
/// per pair of site types (Lorentz–Berthelot). Only pairs that involve a
/// molecule use it; atomic pairs stay in the pair table (with their
/// overrides).
#[derive(Clone, Debug)]
pub(crate) struct SiteTable {
    /// Per species: (site type, offset along the axis) of each site.
    species_sites: Vec<Vec<(usize, f32)>>,
    is_molecule: Vec<bool>,
    potentials: Vec<CappedPotential<LennardJones>>,
    type_count: usize,
}

/// Energy, force and torque on ball `i` of one molecule pair, and its
/// virial `Σ (r_a − r_b)·F_a`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SitePair {
    pub energy: f32,
    pub force: Vec2D,
    pub torque: f32,
    pub virial: f32,
    /// Site pairs inside the force cap's core (#422).
    pub capped: u32,
}

impl SiteTable {
    /// `None` if no species of `table` is a molecule.
    pub(crate) fn new(table: &SpeciesTable, cap: CapSettings) -> Option<Self> {
        if table.iter().all(|s| s.molecule.is_none()) {
            return None;
        }
        let mut types: Vec<(f32, f32)> = Vec::new();
        let mut type_of = |sigma: f32, epsilon: f32| {
            types
                .iter()
                .position(|&t| t == (sigma, epsilon))
                .unwrap_or_else(|| {
                    types.push((sigma, epsilon));
                    types.len() - 1
                })
        };
        let species_sites: Vec<Vec<(usize, f32)>> = table
            .iter()
            .map(|s| match &s.molecule {
                Some(m) => m
                    .sites
                    .iter()
                    .map(|site| (type_of(site.sigma, site.epsilon), site.offset))
                    .collect(),
                None => vec![(type_of(s.sigma, s.epsilon), 0.0)],
            })
            .collect();
        let type_count = types.len();
        let potentials = types
            .iter()
            .flat_map(|&a| types.iter().map(move |&b| (a, b)))
            .map(|((sa, ea), (sb, eb))| {
                CappedPotential::new(
                    LennardJones {
                        sigma: 0.5 * (sa + sb),
                        epsilon: (ea * eb).sqrt(),
                    },
                    cap,
                )
            })
            .collect();
        Some(Self {
            species_sites,
            is_molecule: table.iter().map(|s| s.molecule.is_some()).collect(),
            potentials,
            type_count,
        })
    }

    #[inline]
    pub(crate) fn is_molecule(&self, species: SpeciesId) -> bool {
        self.is_molecule[species.index()]
    }

    /// (offset in `a`, offset in `b`, potential) of every site pair.
    pub(crate) fn site_pairs(
        &self,
        a: SpeciesId,
        b: SpeciesId,
    ) -> impl Iterator<Item = (f32, f32, &CappedPotential<LennardJones>)> {
        self.species_sites[a.index()]
            .iter()
            .flat_map(move |&(ta, oa)| {
                self.species_sites[b.index()]
                    .iter()
                    .map(move |&(tb, ob)| (oa, ob, &self.potentials[ta * self.type_count + tb]))
            })
    }

    /// Site–site interaction of ball `i` (species `si`, angle `theta_i`)
    /// with ball `j` at `r = r_j − r_i`.
    pub(crate) fn pair(
        &self,
        si: SpeciesId,
        theta_i: f32,
        sj: SpeciesId,
        theta_j: f32,
        r: Vec2D,
    ) -> SitePair {
        let (axis_i, axis_j) = (axis(theta_i), axis(theta_j));
        let mut out = SitePair {
            energy: 0.0,
            force: Vec2D::zero(),
            torque: 0.0,
            virial: 0.0,
            capped: 0,
        };
        for &(ta, oa) in &self.species_sites[si.index()] {
            let pa = axis_i.scaled(oa);
            for &(tb, ob) in &self.species_sites[sj.index()] {
                let pot = &self.potentials[ta * self.type_count + tb];
                // From site a to site b.
                let d = r.add_scaled(&axis_j, ob) - &pa;
                let dist = d.norm();
                if dist >= pot.cutoff() || dist <= f32::EPSILON {
                    continue;
                }
                // Repulsive force pushes a away from b.
                let f = d.scaled(-pot.force(dist) / dist);
                out.capped += u32::from(pot.is_capped(dist));
                out.energy += pot.potential(dist);
                out.force += f;
                out.torque += cross(pa, f);
                out.virial -= d.dot(&f);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> &'static SpeciesTable {
        SpeciesTable::builtin()
    }

    fn sites() -> SiteTable {
        SiteTable::new(table(), CapSettings::default()).expect("N2_2S is a molecule")
    }

    fn id(name: &str) -> SpeciesId {
        table().by_name(name).unwrap()
    }

    /// N2_2S: two TraPPE sites, mass and inertia from them, envelope `σ`.
    #[test]
    fn n2_dumbbell_is_built_from_its_sites() {
        let s = table().species(id("N2_2S"));
        let m = s.molecule.as_ref().unwrap();
        assert_eq!(m.sites.len(), 2);
        let bond = (1.10 / 3.405) as f32;
        assert!((m.sites[1].offset - m.sites[0].offset - bond).abs() < 1e-6);
        assert!((s.mass - (2.0 * 14.007 / 39.948) as f32).abs() < 1e-6);
        assert!((s.inertia.unwrap() - s.mass * 0.25 * bond * bond).abs() < 1e-6);
        assert_eq!(s.epsilon, 0.0, "the centre itself does not interact");
        assert!((s.sigma - (bond + (3.31 / 3.405) as f32)).abs() < 1e-5);
    }

    /// Force and torque on `i` are minus the gradient of the site–site
    /// energy, for molecule–molecule and molecule–atom pairs.
    #[test]
    fn force_and_torque_match_finite_differences() {
        let sites = sites();
        let (n2, ar) = (id("N2_2S"), id("Ar"));
        let h = 1e-3;
        for (sj, r, ti, tj) in [
            (n2, Vec2D::new(1.35, 0.2), 0.3, -1.1),
            (n2, Vec2D::new(-0.9, 1.0), 2.0, 0.4),
            (ar, Vec2D::new(1.2, -0.3), -0.7, 0.0),
        ] {
            let e = |r: Vec2D, ti: f32| sites.pair(n2, ti, sj, tj, r).energy;
            let p = sites.pair(n2, ti, sj, tj, r);
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
            assert!(p.energy != 0.0, "{r}: inside the cutoff");
        }
    }

    /// CO2_3S: O–C–O with the TraPPE sites, symmetric, inertia from the
    /// oxygens only (#408).
    #[test]
    fn co2_chain_is_built_from_its_sites() {
        let s = table().species(id("CO2_3S"));
        let m = s.molecule.as_ref().unwrap();
        let (o, c) = (m.sites[0], m.sites[1]);
        assert_eq!(m.sites.len(), 3);
        assert_eq!(c.offset, 0.0);
        assert_eq!(m.sites[2].offset, -o.offset);
        assert!((o.offset.abs() - (1.16 / 3.405) as f32).abs() < 1e-6);
        assert!((o.sigma - (3.05 / 3.405) as f32).abs() < 1e-6);
        assert!((c.epsilon - (27.0 / 119.8) as f32).abs() < 1e-6);
        assert!((s.mass - ((2.0 * 15.999 + 12.011) / 39.948) as f32).abs() < 1e-6);
        assert!((s.inertia.unwrap() - 2.0 * o.mass * o.offset * o.offset).abs() < 1e-6);
    }

    /// Force and torque of the three-site chain vs finite differences.
    #[test]
    fn co2_force_and_torque_match_finite_differences() {
        let sites = sites();
        let (co2, ar) = (id("CO2_3S"), id("Ar"));
        let h = 1e-3;
        for (sj, r, ti, tj) in [
            (co2, Vec2D::new(1.6, 0.4), 0.3, 1.2),
            (co2, Vec2D::new(-0.6, 1.5), -1.0, 0.2),
            (ar, Vec2D::new(1.3, -0.5), 0.8, 0.0),
        ] {
            let e = |r: Vec2D, ti: f32| sites.pair(co2, ti, sj, tj, r).energy;
            let p = sites.pair(co2, ti, sj, tj, r);
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
            assert!(p.energy != 0.0, "{r}: inside the cutoff");
        }
    }

    /// The pair seen from the other ball: same energy, opposite force.
    #[test]
    fn pair_is_symmetric() {
        let sites = sites();
        let n2 = id("N2_2S");
        let r = Vec2D::new(1.3, 0.25);
        let a = sites.pair(n2, 0.4, n2, -0.8, r);
        let b = sites.pair(n2, -0.8, n2, 0.4, r.scaled(-1.0));
        assert!((a.energy - b.energy).abs() < 1e-5 * a.energy.abs().max(1.0));
        assert!((a.force + b.force).norm() < 1e-4 * a.force.norm().max(1.0));
    }
}
