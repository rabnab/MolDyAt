// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! P8a (#344): how much f32 position rounding costs far from the origin.
//!
//! Runs the dense-gas scenario (hot, with projectiles) and the cool gas
//! (without) in drift worlds centred at growing distances from the origin,
//! each at dt, dt/2 and dt/4 over the same physical time, and prints one
//! `DRIFT_PRECISION` line per run. Velocity Verlet shrinks the energy
//! envelope as dt^2 (dt/4 ratio ≈ 0.06); position rounding does not shrink
//! with dt, so the ratio rises towards 1 where it dominates. The results and
//! the recommendation for #213 are in `docs/drift-test.md`.
//!
//! The only assertion is that the gas centred on the origin converges; the
//! far runs measure, they do not gate (#213 is open).

use super::dense_gas::{STEPS, run_dense_gas_at};
use super::*;
use crate::application::core::TestFaults;

/// Centres of the drift worlds: the origin, a default world's edge region,
/// a `--world-scale 8` world's edge region (~±450) and beyond.
const CENTERS: [f32; 4] = [0.0, 125.0, 500.0, 2000.0];
/// dt divisors of one measurement; the physical time stays `STEPS * DT_GOAL`.
const DT_DIVISORS: [usize; 3] = [1, 2, 4];
/// Same bound as `dense_gas.rs`: dt^2 convergence with ~2x margin.
const ORIGIN_MAX_DT_QUARTER_RATIO: f32 = 0.125;

/// Spacing of f32 values at `x` (one unit in the last place).
fn f32_ulp(x: f32) -> f32 {
    let x = x.abs().max(f32::MIN_POSITIVE);
    f32::from_bits(x.to_bits() + 1) - x
}

/// Max relative energy drift of one gas at `center` for each of [`DT_DIVISORS`].
fn drift_by_dt(center: f32, projectiles: bool) -> [f32; 3] {
    DT_DIVISORS.map(|div| {
        let run = run_dense_gas_at(
            Vec2D::new(center, center),
            TestFaults::default(),
            projectiles,
            DT_GOAL / div as f32,
            STEPS * div,
            25 * div,
        );
        assert_eq!(
            run.metrics.final_sample.ball_count, run.metrics.initial.ball_count,
            "balls lost at centre {center}, dt/{div}"
        );
        run.metrics.max_relative_energy_drift()
    })
}

#[test]
#[ignore = "suite:drift"]
fn test_dense_gas_precision_by_distance_from_origin() {
    for (gas, projectiles) in [("dense_gas", true), ("cool_gas", false)] {
        for center in CENTERS {
            let [d1, d2, d4] = drift_by_dt(center, projectiles);
            let ratio = d4 / d1.max(1e-12);
            // Positions reach about 20 beyond the centre.
            let ulp = f32_ulp(center + 20.0);
            println!(
                "DRIFT_PRECISION name={gas} center={center} position_ulp={ulp:.1e} max_rel_energy_drift_dt={d1:.3e} dt_half={d2:.3e} dt_quarter={d4:.3e} dt_quarter_ratio={ratio:.3}"
            );
            if center == 0.0 && projectiles {
                assert!(
                    ratio <= ORIGIN_MAX_DT_QUARTER_RATIO,
                    "{gas} at the origin does not converge with dt: ratio {ratio:.3}"
                );
            }
        }
    }
}
