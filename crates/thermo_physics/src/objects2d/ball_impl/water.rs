// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Mercedes-Benz (MB) water (#349): Silverstein, Haymet and Dill,
//! *J. Am. Chem. Soc.* 120, 3166 (1998).
//!
//! A water ball is a Lennard-Jones disk (the species' `σ`, `ε`; handled by
//! the pair kernel like any species) with three hydrogen-bond arms at 120°,
//! oriented by the ball's `θ`. Two water balls `i`, `j` at distance `d`,
//! with `u` the unit vector from `i` to `j`, add the hydrogen-bond energy
//!
//! `U = −ε_HB · G(d − r_HB; w_r) · Σ_k G(a_k·u − 1; w) · Σ_l G(b_l·u + 1; w)`
//!
//! with `G(x; w) = exp(−x² / (2 w²))`, `a_k` the arms of `i` and `b_l` those
//! of `j`: a bond needs an arm of `i` pointing at `j` and an arm of `j`
//! pointing back. The MB parameters in their own units are `ε_HB = 1`,
//! `r_HB = 1`, `σ_LJ = 0.7`, `ε_LJ = 0.1` and both widths 0.085 (`w_r` in
//! units of `r_HB`). Force and torque are the exact derivatives of `U`.
//! Water meets other species through its Lennard-Jones part only.

use crate::objects2d::Vec2D;

/// Hydrogen-bond parameters of a water species, in reduced (Ar) units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MbWater {
    /// `ε_HB`: depth of one hydrogen bond.
    pub epsilon_hb: f32,
    /// `r_HB`: length of a hydrogen bond.
    pub r_hb: f32,
    /// Width of the distance Gaussian, `w_r = 0.085 · r_HB`.
    pub width_r: f32,
    /// Width of the arm-alignment Gaussians (dimensionless).
    pub width_angle: f32,
}

/// The width of both Gaussians in MB units.
pub(crate) const MB_WIDTH: f32 = 0.085;

/// Hydrogen-bond energy of one pair and its derivatives for ball `i`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HbPair {
    pub energy: f32,
    /// Force on `i`.
    pub force: Vec2D,
    /// Torque on `i`.
    pub torque: f32,
}

impl MbWater {
    /// MB water with `ε_HB` and `r_HB`, both widths 0.085.
    pub(crate) fn new(epsilon_hb: f32, r_hb: f32) -> Self {
        Self {
            epsilon_hb,
            r_hb,
            width_r: MB_WIDTH * r_hb,
            width_angle: MB_WIDTH,
        }
    }

    /// Beyond this distance the bond energy is below `1e-30·ε_HB`.
    pub(crate) fn cutoff(&self) -> f32 {
        self.r_hb + 12.0 * self.width_r
    }

    /// Bond of ball `i` (orientation `theta_i`) with `j` (`theta_j`) at
    /// `r = r_j − r_i`: energy, force and torque on `i`. Zero beyond
    /// [`Self::cutoff`].
    #[cfg(test)]
    pub(crate) fn pair(&self, r: Vec2D, theta_i: f32, theta_j: f32) -> HbPair {
        self.pair_with_arms(r, &arms(theta_i), &arms(theta_j))
    }

    /// [`Self::pair`] with the arm directions of both balls ([`arms`]), so
    /// the force pass computes each ball's arms once, not once per pair.
    pub(crate) fn pair_with_arms(&self, r: Vec2D, arms_i: &Arms, arms_j: &Arms) -> HbPair {
        let zero = HbPair {
            energy: 0.0,
            force: Vec2D::zero(),
            torque: 0.0,
        };
        let d = r.norm();
        if d >= self.cutoff() || d <= f32::EPSILON {
            return zero;
        }
        let u = r.scaled(1.0 / d);
        let gauss = |x: f32, w: f32| (-x * x / (2.0 * w * w)).exp();
        let w = self.width_angle;
        let w2 = w * w;

        // Distance part.
        let dr = d - self.r_hb;
        let radial = gauss(dr, self.width_r);
        let d_radial = -dr / (self.width_r * self.width_r) * radial; // dR/dd

        // Arms of i: A = Σ G(x_k − 1); dA/dr and dA/dθ_i.
        let (mut a_sum, mut da_dr, mut da_dtheta) = (0.0f32, Vec2D::zero(), 0.0f32);
        // Arms of j: B = Σ G(y_l + 1); dB/dr.
        let (mut b_sum, mut db_dr) = (0.0f32, Vec2D::zero());
        for k in 0..3 {
            let arm_i = arms_i[k];
            let x = arm_i.dot(&u);
            let g = gauss(x - 1.0, w);
            let dg = -(x - 1.0) / w2 * g;
            a_sum += g;
            // ∂x/∂r = (a − x u) / d
            da_dr = da_dr.add_scaled(&arm_i.add_scaled(&u, -x), dg / d);
            // ∂x/∂θ_i = perp(a)·u
            da_dtheta += dg * Vec2D::new(-arm_i.y, arm_i.x).dot(&u);

            let arm_j = arms_j[k];
            let y = arm_j.dot(&u);
            let g = gauss(y + 1.0, w);
            let dg = -(y + 1.0) / w2 * g;
            b_sum += g;
            db_dr = db_dr.add_scaled(&arm_j.add_scaled(&u, -y), dg / d);
        }

        let eps = self.epsilon_hb;
        let energy = -eps * radial * a_sum * b_sum;
        // dU/dr (r = r_j − r_i); the force on i is −dU/dr_i = +dU/dr.
        let du_dr = u
            .scaled(d_radial * a_sum * b_sum)
            .add_scaled(&da_dr, radial * b_sum)
            .add_scaled(&db_dr, radial * a_sum)
            .scaled(-eps);
        HbPair {
            energy,
            force: du_dr,
            torque: eps * radial * b_sum * da_dtheta,
        }
    }
}

/// The three hydrogen-bond arm directions of a molecule.
pub(crate) type Arms = [Vec2D; 3];

/// The arms of a molecule at orientation `theta`: the first along `theta`,
/// the others turned by 120° and 240° (one `sin_cos`, then fixed rotations).
pub(crate) fn arms(theta: f32) -> Arms {
    let (sin, cos) = theta.sin_cos();
    let first = Vec2D::new(cos, sin);
    // cos and sin of ±120°.
    const C: f32 = -0.5;
    const S: f32 = 0.866_025_4;
    let turn = |s: f32| Vec2D::new(C * first.x - s * first.y, s * first.x + C * first.y);
    [first, turn(S), turn(-S)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn water() -> MbWater {
        MbWater::new(11.56, 0.8223)
    }

    fn energy(w: &MbWater, r: Vec2D, ti: f32, tj: f32) -> f32 {
        w.pair(r, ti, tj).energy
    }

    /// Two molecules in the ideal bond: an arm of each on the axis, facing.
    #[test]
    fn ideal_bond_has_depth_epsilon() {
        let w = water();
        let r = Vec2D::new(w.r_hb, 0.0);
        let bond = w.pair(r, 0.0, std::f32::consts::PI);
        assert!(
            (bond.energy + w.epsilon_hb).abs() < 1e-3 * w.epsilon_hb,
            "{bond:?}"
        );
        assert!(bond.force.norm() < 1e-2, "{bond:?}");
        assert!(bond.torque.abs() < 1e-2, "{bond:?}");
    }

    /// Force and torque are minus the gradient of the energy, checked by
    /// central differences (in f64 steps around several geometries).
    #[test]
    fn force_and_torque_match_finite_differences() {
        let w = water();
        let h = 1e-3;
        for (r, ti, tj) in [
            (Vec2D::new(0.83, 0.02), 0.05, 3.1),
            (Vec2D::new(0.78, -0.05), -0.08, 3.2),
            (Vec2D::new(0.6, 0.55), 0.7, -2.2),
            (Vec2D::new(-0.81, 0.04), 3.0, 0.1),
        ] {
            let p = w.pair(r, ti, tj);
            let fx = (energy(&w, r + Vec2D::new(h, 0.0), ti, tj)
                - energy(&w, r - &Vec2D::new(h, 0.0), ti, tj))
                / (2.0 * h);
            let fy = (energy(&w, r + Vec2D::new(0.0, h), ti, tj)
                - energy(&w, r - &Vec2D::new(0.0, h), ti, tj))
                / (2.0 * h);
            let torque = -(energy(&w, r, ti + h, tj) - energy(&w, r, ti - h, tj)) / (2.0 * h);
            let scale = p.force.norm().max(p.torque.abs()).max(1.0);
            assert!(
                (p.force.x - fx).abs() < 2e-2 * scale,
                "{r} fx {} vs {fx}",
                p.force.x
            );
            assert!(
                (p.force.y - fy).abs() < 2e-2 * scale,
                "{r} fy {} vs {fy}",
                p.force.y
            );
            assert!(
                (p.torque - torque).abs() < 2e-2 * scale,
                "{r} τ {} vs {torque}",
                p.torque
            );
        }
    }

    /// The pair seen from `j` has the same energy and the opposite force.
    #[test]
    fn pair_is_symmetric() {
        let w = water();
        let r = Vec2D::new(0.8, 0.1);
        let (ti, tj) = (0.1, 3.0);
        let a = w.pair(r, ti, tj);
        let b = w.pair(r.scaled(-1.0), tj, ti);
        assert!((a.energy - b.energy).abs() < 1e-5 * a.energy.abs());
        assert!((a.force + b.force).norm() < 1e-4 * a.force.norm().max(1.0));
    }

    #[test]
    fn no_bond_beyond_the_cutoff() {
        let w = water();
        let p = w.pair(
            Vec2D::new(w.cutoff() + 0.01, 0.0),
            0.0,
            std::f32::consts::PI,
        );
        assert_eq!(p.energy, 0.0);
    }
}
