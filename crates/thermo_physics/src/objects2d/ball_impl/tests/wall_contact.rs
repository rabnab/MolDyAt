// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::wall_contact::{
    WallContact, normal_velocity_after_contact, resolve_normal_position,
};

const DT: f32 = 0.01;

fn contact_from(n: &crate::objects2d::ball_impl::wall_contact::NormalPosition) -> WallContact {
    WallContact {
        normal: Vec2D::new(0.0, 1.0),
        contact_fraction: n.contact_fraction,
        free_fraction: n.free_fraction,
        counts_for_momentum: true,
    }
}

/// Exact 1D motion with constant acceleration and an elastic wall at s = 0,
/// integrated with many tiny substeps.
fn reference_motion(mut s: f64, mut v: f64, a: f64, dt: f64) -> (f64, f64) {
    const SUBSTEPS: usize = 200_000;
    let h = dt / SUBSTEPS as f64;
    for _ in 0..SUBSTEPS {
        v += a * h;
        s += v * h;
        if s < 0.0 {
            s = -s;
            v = -v;
        }
    }
    (s, v)
}

#[test]
fn test_no_contact_when_the_ball_stays_clear() {
    assert_eq!(resolve_normal_position(1.0, -1.0, 0.0, DT, 1.0), None);
    assert_eq!(resolve_normal_position(0.0, 1.0, -1.0, DT, 1.0), None);
}

#[test]
fn test_force_free_bounce_mirrors_the_overshoot() {
    // 0.004 from the wall at -1: contact at 40 % of the step, 0.006 back out.
    let n = resolve_normal_position(0.004, -1.0, 0.0, DT, 1.0).unwrap();
    assert!((n.contact_fraction - 0.4).abs() < 1e-5);
    assert!((n.distance - 0.006).abs() < 1e-7);
    let v = normal_velocity_after_contact(-1.0, 0.0, &contact_from(&n), 1.0);
    assert!((v - 1.0).abs() < 1e-6);
}

#[test]
fn test_bounce_under_constant_force_matches_exact_motion() {
    // Pushed towards the wall (a < 0) and away from it (a > 0).
    for &(s0, v0, a) in &[
        (0.004f32, -1.0f32, -50.0f32),
        (0.004, -1.0, 50.0),
        (0.0, -0.3, -20.0),
    ] {
        let n = resolve_normal_position(s0, v0, a, DT, 1.0).unwrap();
        let v = normal_velocity_after_contact(v0, a * DT, &contact_from(&n), 1.0);
        let (s_ref, v_ref) = reference_motion(s0 as f64, v0 as f64, a as f64, DT as f64);
        assert!(
            (n.distance as f64 - s_ref).abs() < 1e-6,
            "s0={s0} v0={v0} a={a}: distance {} vs {s_ref}",
            n.distance
        );
        assert!(
            (v as f64 - v_ref).abs() < 1e-3,
            "s0={s0} v0={v0} a={a}: velocity {v} vs {v_ref}"
        );
        // Energy per unit mass is conserved exactly: ½v² - a·s.
        let e0 = 0.5 * v0 * v0 - a * s0;
        let e1 = 0.5 * v * v - a * n.distance;
        assert!(
            (e1 - e0).abs() < 1e-5 * e0.abs().max(1.0),
            "energy {e0} -> {e1}"
        );
    }
}

#[test]
fn test_several_bounces_in_one_step_follow_the_bounce_period() {
    // Slow ball on a strongly pushing wall: bounce period 2·0.1/100 = 0.002,
    // five bounces within the step.
    let n = resolve_normal_position(0.0, -0.1, -100.0, DT, 1.0).unwrap();
    let (s_ref, v_ref) = reference_motion(0.0, -0.1, -100.0, DT as f64);
    assert!(
        (n.distance as f64 - s_ref).abs() < 1e-6,
        "{} vs {s_ref}",
        n.distance
    );
    let v = normal_velocity_after_contact(-0.1, -100.0 * DT, &contact_from(&n), 1.0);
    assert!((v as f64 - v_ref).abs() < 1e-3, "{v} vs {v_ref}");
}

#[test]
fn test_ball_resting_on_a_pushing_wall_stays_at_rest() {
    let n = resolve_normal_position(0.0, 0.0, -9.0, DT, 1.0).unwrap();
    assert_eq!(n.distance, 0.0);
    assert_eq!(n.free_fraction, None);
    let v = normal_velocity_after_contact(0.0, -9.0 * DT, &contact_from(&n), 1.0);
    assert_eq!(v, 0.0);
}

#[test]
fn test_ball_already_inside_is_resolved_from_the_surface() {
    // Starts 0.002 inside, moving in: reflected from the surface at t = 0.
    let n = resolve_normal_position(-0.002, -1.0, 0.0, DT, 1.0).unwrap();
    assert_eq!(n.contact_fraction, 0.0);
    assert!((n.distance - 0.01).abs() < 1e-6);
}
