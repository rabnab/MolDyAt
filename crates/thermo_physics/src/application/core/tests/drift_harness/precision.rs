// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Precision far from the origin (P8a #344, gate since P8b #213).
//!
//! Runs the dense-gas scenario (hot, with projectiles) and the cool gas
//! (without) in drift worlds centred at growing distances from the origin,
//! each at dt, dt/2 and dt/4 over the same physical time, and prints one
//! `DRIFT_PRECISION` line per run. Velocity Verlet shrinks the energy
//! envelope as dt^2; position rounding does not shrink with dt, so with f32
//! positions the far runs stopped converging. With f64 positions (#213) the
//! energy drift at dt/4 far from the origin must stay close to the drift at
//! the origin. Results in `docs/drift-test.md`.

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
/// Max dt/4 drift far from the origin over the dt/4 drift at the origin.
/// Baselines (#213): dense gas up to 1.07 (f32 positions: 2.57 at 2000),
/// cool gas up to 2.0 (f32 positions: 22.6 at 500); ~1.5x and 2x margin.
const DENSE_FAR_OVER_ORIGIN: f32 = 1.6;
const COOL_FAR_OVER_ORIGIN: f32 = 4.0;

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
    let mut failures = Vec::new();
    for (gas, projectiles, max_far) in [
        ("dense_gas", true, DENSE_FAR_OVER_ORIGIN),
        ("cool_gas", false, COOL_FAR_OVER_ORIGIN),
    ] {
        let mut origin_d4 = f32::NAN;
        for center in CENTERS {
            let [d1, d2, d4] = drift_by_dt(center, projectiles);
            let ratio = d4 / d1.max(1e-12);
            if center == 0.0 {
                origin_d4 = d4;
            }
            let far = d4 / origin_d4.max(1e-12);
            // Positions reach about 20 beyond the centre.
            let ulp = f32_ulp(center + 20.0);
            println!(
                "DRIFT_PRECISION name={gas} center={center} position_ulp={ulp:.1e} max_rel_energy_drift_dt={d1:.3e} dt_half={d2:.3e} dt_quarter={d4:.3e} dt_quarter_ratio={ratio:.3} dt_quarter_over_origin={far:.2} threshold={max_far}"
            );
            warn_near_threshold(gas, "dt_quarter_over_origin", far, max_far);
            if center == 0.0 && projectiles && ratio > ORIGIN_MAX_DT_QUARTER_RATIO {
                failures.push(format!("{gas} at the origin: dt/4 ratio {ratio:.3}"));
            }
            if far > max_far {
                failures.push(format!(
                    "{gas} at {center}: dt/4 drift {d4:.3e} is {far:.2}x the origin's (max {max_far})"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
