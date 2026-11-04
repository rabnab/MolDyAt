// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Soft walls (#280): the dense gas with its projectiles inside a square of
//! soft line walls. The harness energy includes the wall energy; the
//! projectiles soon reach the walls. The hard backstop must never be hit.

use super::dense_gas::{LineBox, STEPS, run_dense_gas_at};
use super::*;
use crate::application::core::TestFaults;

/// Half side of the square of line segments, as in the walled gas
/// (`precision.rs`).
const LINE_BOX_HALF_SIDE: f32 = 22.0;
/// Baseline 3.51e-3 (#280): the same envelope as the dense gas without
/// walls (the projectile pairs in the force cap dominate it); ~1.5x.
const MAX_REL_ENERGY_DRIFT: f32 = 5.5e-3;

#[test]
#[ignore = "suite:drift"]
fn test_soft_walled_gas_drift() {
    let name = "soft_walled_gas";
    let run = run_dense_gas_at(
        Vec2D::zero(),
        Some(LineBox {
            half_side: LINE_BOX_HALF_SIDE,
            soft: true,
        }),
        TestFaults::default(),
        true,
        DT_GOAL,
        STEPS,
        25,
    );
    let m = &run.metrics;
    let mut violations = Vec::new();
    if m.max_relative_energy_drift() > MAX_REL_ENERGY_DRIFT {
        violations.push(format!(
            "energy drift {:.3e} above {MAX_REL_ENERGY_DRIFT:.1e}",
            m.max_relative_energy_drift()
        ));
    }
    if run.wall_contacts != 0 {
        violations.push(format!("{} backstop hits", run.wall_contacts));
    }
    if m.final_sample.ball_count != m.initial.ball_count {
        violations.push("balls lost".into());
    }
    if run.line_pieces != 4 {
        violations.push("the line box is missing".into());
    }
    // 85 capped pairs (3.3e-4 per ball and step), no backstop hit (#422).
    violations.extend(m.safety_net_violations(
        name,
        SafetyNetLimits {
            capped_rate: 7.0e-4,
            backstop_hits: 0,
        },
    ));
    m.report(name, violations.is_empty());
    println!(
        "DRIFT_SOFT_WALLS name={name} backstop_hits={}",
        run.wall_contacts
    );
    warn_near_threshold(
        name,
        "max_rel_energy_drift",
        m.max_relative_energy_drift(),
        MAX_REL_ENERGY_DRIFT,
    );
    assert!(violations.is_empty(), "{name}: {}", violations.join("; "));
}
