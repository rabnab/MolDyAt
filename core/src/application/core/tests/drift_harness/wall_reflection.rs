// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Scenario D: elastic wall/barrier reflection invariants.
//!
//! A single ball (no pair forces, no gravity) bounces around the world box,
//! optionally with a line-segment barrier. Every velocity change is a
//! reflection event and must be specular: speed conserved, normal component
//! flipped, tangential component kept, and no energy gained over time. The
//! ball must never leave the box or cross the barrier, and the number of
//! reflections must match an independent ray-traced reference.

use super::*;
use crate::application::core::TestFaults;
use crate::objects2d::line_segment::THICKNESS_TO_WORLD;

const SPEED: f32 = 12.0;
const STEPS: usize = 10_000;

// Thresholds: see docs/drift-test.md for the calibration process.
const MAX_REL_SPEED_CHANGE: f32 = 1.0e-5;
const MIN_NORMAL_ALIGNMENT: f32 = 0.999;
const MAX_REL_KINETIC_DEVIATION: f32 = 1.0e-5;
const MAX_REL_KINETIC_SLOPE_PER_EVENT: f32 = 1.0e-6;
const MIN_EVENTS: usize = 10;
/// Allowed difference between observed and ray-traced reflection counts.
const MAX_EVENT_COUNT_ERROR: usize = 1;
/// A reflection is resolved at most one step late, so the ball may sink this
/// far into a surface before being pushed out.
const PENETRATION_TOLERANCE: f32 = SPEED * DT_GOAL + 0.02;

#[derive(Debug, Clone, Copy)]
struct ReflectionEvent {
    step: usize,
    position: Vec2D,
    radius: f32,
    v_before: Vec2D,
    v_after: Vec2D,
}

fn dot(a: Vec2D, b: Vec2D) -> f32 {
    a.x * b.x + a.y * b.y
}

fn len(a: Vec2D) -> f32 {
    dot(a, a).sqrt()
}

fn unit(a: Vec2D) -> Vec2D {
    let l = len(a).max(1e-12);
    Vec2D::new(a.x / l, a.y / l)
}

/// An infinite line through `point` that the ball can reflect from. The ball
/// touches it when its centre is `radius + extra_reach` away (half the wall
/// thickness for line segments, 0 for world walls).
#[derive(Debug, Clone, Copy)]
struct Surface {
    point: Vec2D,
    normal: Vec2D,
    extra_reach: f32,
}

impl Surface {
    fn through(a: Vec2D, b: Vec2D) -> Self {
        Self {
            point: a,
            normal: unit(Vec2D::new(-(b.y - a.y), b.x - a.x)),
            extra_reach: 0.0,
        }
    }

    fn with_extra_reach(mut self, extra_reach: f32) -> Self {
        self.extra_reach = extra_reach;
        self
    }

    fn signed_distance(&self, p: Vec2D) -> f32 {
        dot(
            Vec2D::new(p.x - self.point.x, p.y - self.point.y),
            self.normal,
        )
    }

    /// Unit normal pointing from the surface towards `p`.
    fn inward_normal(&self, p: Vec2D) -> Vec2D {
        let s = self.signed_distance(p).signum();
        Vec2D::new(self.normal.x * s, self.normal.y * s)
    }
}

fn world_wall_surfaces(main_content: &MainContent) -> Vec<Surface> {
    let rect = main_content.world().world_region();
    let (tl_x, tl_y) = rect.get_tl();
    let (br_x, br_y) = rect.get_br();
    let tl = Vec2D::new(tl_x, tl_y);
    let tr = Vec2D::new(br_x, tl_y);
    let br = Vec2D::new(br_x, br_y);
    let bl = Vec2D::new(tl_x, br_y);
    vec![
        Surface::through(tl, tr),
        Surface::through(tr, br),
        Surface::through(br, bl),
        Surface::through(bl, tl),
    ]
}

/// Index of the surface an event reflected from: among surfaces parallel to
/// the velocity change, the one closest to the ball. `None` if no surface fits.
fn hit_surface(e: &ReflectionEvent, surfaces: &[Surface]) -> Option<usize> {
    let n = unit(Vec2D::new(
        e.v_after.x - e.v_before.x,
        e.v_after.y - e.v_before.y,
    ));
    surfaces
        .iter()
        .enumerate()
        .filter(|(_, s)| dot(n, s.normal).abs() >= MIN_NORMAL_ALIGNMENT)
        .min_by(|(_, a), (_, b)| {
            a.signed_distance(e.position)
                .abs()
                .total_cmp(&b.signed_distance(e.position).abs())
        })
        .map(|(i, _)| i)
}

/// Surfaces the ball is touching (within one step of travel) at an event.
fn touching_surfaces(e: &ReflectionEvent, surfaces: &[Surface]) -> usize {
    let reach_slack = len(e.v_before) * DT_GOAL + PENETRATION_TOLERANCE;
    surfaces
        .iter()
        .filter(|s| s.signed_distance(e.position).abs() <= e.radius + s.extra_reach + reach_slack)
        .count()
}

/// Independent reference: ray-trace a point ball of `radius` with specular
/// reflections for `total_time` and count the hits.
fn expected_event_count(
    start: Vec2D,
    velocity: Vec2D,
    radius: f32,
    surfaces: &[Surface],
    total_time: f32,
) -> usize {
    let inward: Vec<Vec2D> = surfaces.iter().map(|s| s.inward_normal(start)).collect();
    let (mut p, mut v, mut t_left, mut count) = (start, velocity, total_time, 0usize);
    loop {
        let mut next: Option<(f32, usize)> = None;
        for (k, s) in surfaces.iter().enumerate() {
            let vn = dot(v, inward[k]);
            if vn >= 0.0 {
                continue;
            }
            let gap = dot(Vec2D::new(p.x - s.point.x, p.y - s.point.y), inward[k])
                - (radius + s.extra_reach);
            let t_hit = (gap / -vn).max(0.0);
            if next.is_none_or(|(t, _)| t_hit < t) {
                next = Some((t_hit, k));
            }
        }
        let Some((t_hit, k)) = next else { break };
        if t_hit > t_left {
            break;
        }
        p = Vec2D::new(p.x + v.x * t_hit, p.y + v.y * t_hit);
        let vn = dot(v, inward[k]);
        v = Vec2D::new(v.x - 2.0 * vn * inward[k].x, v.y - 2.0 * vn * inward[k].y);
        t_left -= t_hit;
        count += 1;
    }
    count
}

/// Checks each event against the known surfaces and the kinetic-energy
/// history against the initial value. Returns all violations.
fn reflection_violations(
    events: &[ReflectionEvent],
    surfaces: &[Surface],
    initial_speed: f32,
) -> Vec<String> {
    let mut violations = Vec::new();
    if events.len() < MIN_EVENTS {
        violations.push(format!(
            "scenario invalid: only {} reflection events (need >= {MIN_EVENTS})",
            events.len()
        ));
    }

    for e in events {
        let context = format!(
            "step={} pos={:?} v_before={:?} v_after={:?}",
            e.step, e.position, e.v_before, e.v_after
        );
        let speed_before = len(e.v_before);
        let speed_after = len(e.v_after);
        let rel_speed_change = (speed_after - speed_before).abs() / speed_before.max(1e-12);
        if rel_speed_change > MAX_REL_SPEED_CHANGE {
            violations.push(format!(
                "speed not conserved: rel_change={rel_speed_change:.3e} threshold={MAX_REL_SPEED_CHANGE:.1e} {context}"
            ));
        }

        // A specular reflection changes velocity only along the surface
        // normal, so the tangential component is kept.
        let Some(surface) = hit_surface(e, surfaces).map(|i| surfaces[i]) else {
            if touching_surfaces(e, surfaces) >= 2 {
                violations.push(format!(
                    "scenario invalid: corner hit (two surfaces in one step), retune start/angle {context}"
                ));
            } else {
                violations.push(format!(
                    "impulse not along a surface normal (tangential velocity changed) {context}"
                ));
            }
            continue;
        };

        // The ball approaches the surface from its own side and leaves it.
        let n_in = surface.inward_normal(e.position);
        let vn_before = dot(e.v_before, n_in);
        let vn_after = dot(e.v_after, n_in);
        if !(vn_before < 0.0 && vn_after > 0.0) {
            violations.push(format!(
                "normal component did not flip away from surface: vn_before={vn_before} vn_after={vn_after} surface={surface:?} {context}"
            ));
        }
    }

    // Energy injection: every post-collision kinetic energy stays near the
    // initial one, and there is no systematic trend across events.
    let ke_rel: Vec<f32> = events
        .iter()
        .map(|e| dot(e.v_after, e.v_after) / (initial_speed * initial_speed) - 1.0)
        .collect();
    if let Some((idx, worst)) = ke_rel
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        && worst.abs() > MAX_REL_KINETIC_DEVIATION
    {
        violations.push(format!(
            "kinetic energy deviates after collision: rel={worst:.3e} threshold={MAX_REL_KINETIC_DEVIATION:.1e} step={}",
            events[idx].step
        ));
    }
    if ke_rel.len() >= 2 {
        let n = ke_rel.len() as f32;
        let mean_x = (n - 1.0) * 0.5;
        let mean_y = ke_rel.iter().sum::<f32>() / n;
        let (mut sxy, mut sxx) = (0.0f32, 0.0f32);
        for (i, y) in ke_rel.iter().enumerate() {
            let dx = i as f32 - mean_x;
            sxy += dx * (y - mean_y);
            sxx += dx * dx;
        }
        let slope = sxy / sxx;
        if slope > MAX_REL_KINETIC_SLOPE_PER_EVENT {
            violations.push(format!(
                "collision energy injection trend: slope={slope:.3e}/event threshold={MAX_REL_KINETIC_SLOPE_PER_EVENT:.1e} events={}",
                ke_rel.len()
            ));
        }
    }

    violations
}

/// Everything observed in one single-ball run.
struct ReflectionRun {
    metrics: DriftRunMetrics,
    events: Vec<ReflectionEvent>,
    /// Per-step invariant violations (containment, ball presence).
    step_violations: Vec<String>,
    expected_events: usize,
    /// Contacts resolved exactly in the position update.
    wall_contacts: u64,
    /// Balls pushed out of a line wall by the overlap fallback.
    overlap_corrections: u64,
}

/// Runs one ball with the given initial velocity, collects every step in
/// which its velocity changed, and checks containment after every step.
fn run_single_ball(
    mut main_content: MainContent,
    surfaces: &[Surface],
    start: Vec2D,
    velocity: Vec2D,
    faults: TestFaults,
) -> ReflectionRun {
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    main_content.world_mut().test_faults = faults;
    let factory = scenario_factory();
    main_content
        .add_candidate_ball(
            factory.create_massive_with_velocity(SpeciesTable::builtin(), start, velocity),
            0.0,
        )
        .expect("could not add reflection ball");
    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);

    let radius = main_content.world().balls[0].contact_radius;
    let inward: Vec<Vec2D> = surfaces.iter().map(|s| s.inward_normal(start)).collect();
    let expected_events =
        expected_event_count(start, velocity, radius, surfaces, STEPS as f32 * DT_GOAL);

    let mut events = Vec::new();
    let mut step_violations = Vec::new();
    let mut prev_v = main_content.world().balls[0].v;
    let metrics = run_drift_harness_with_observer(
        &mut main_content,
        DriftHarnessConfig {
            steps: STEPS,
            // Every step, so energy changes between collisions are seen too.
            sample_every: 1,
            dt: DT_GOAL,
        },
        |step, mc| {
            let balls = &mc.world().balls;
            if balls.len() != 1 {
                if step_violations.is_empty() {
                    step_violations.push(format!(
                        "ball removed or added: count={} at step {step}",
                        balls.len()
                    ));
                }
                return;
            }
            let ball = &balls[0];
            let pos = *ball.get_position();
            for (k, s) in surfaces.iter().enumerate() {
                let gap = dot(Vec2D::new(pos.x - s.point.x, pos.y - s.point.y), inward[k])
                    - (ball.contact_radius + s.extra_reach);
                if gap < -PENETRATION_TOLERANCE && step_violations.len() < 5 {
                    step_violations.push(format!(
                        "ball escaped/tunnelled through surface {k}: gap={gap:.4} tolerance={PENETRATION_TOLERANCE} step={step} pos={pos:?} v={:?}",
                        ball.v
                    ));
                }
            }
            let dv = Vec2D::new(ball.v.x - prev_v.x, ball.v.y - prev_v.y);
            if len(dv) > 1e-3 * len(prev_v) {
                events.push(ReflectionEvent {
                    step,
                    position: pos,
                    radius: ball.contact_radius,
                    v_before: prev_v,
                    v_after: ball.v,
                });
            }
            prev_v = ball.v;
        },
    );
    let stats = &main_content.world().sim;
    ReflectionRun {
        metrics,
        events,
        step_violations,
        expected_events,
        wall_contacts: stats.wall_contacts,
        overlap_corrections: stats.wall_overlap_corrections,
    }
}

impl ReflectionRun {
    /// All reflection-scenario checks, collected so the summary line is
    /// printed exactly once whatever fails.
    fn violations(&self, surfaces: &[Surface]) -> Vec<String> {
        let mut violations = self.step_violations.clone();
        violations.extend(reflection_violations(&self.events, surfaces, SPEED));

        let observed = self.events.len();
        if observed.abs_diff(self.expected_events) > MAX_EVENT_COUNT_ERROR {
            violations.push(format!(
                "reflection count differs from ray-traced reference: observed={observed} expected={} tolerance={MAX_EVENT_COUNT_ERROR}",
                self.expected_events
            ));
        }

        // Every contact here starts clear of the walls, so all of them must be
        // resolved exactly; the projecting overlap fallback must not fire.
        if self.overlap_corrections > 0 {
            violations.push(format!(
                "overlap fallback fired: overlap_corrections={} wall_contacts={}",
                self.overlap_corrections, self.wall_contacts
            ));
        }

        let drift = self.metrics.max_relative_energy_drift();
        if drift > MAX_REL_KINETIC_DEVIATION {
            violations.push(format!(
                "energy not conserved between samples: max_rel_energy_drift={drift:.3e} threshold={MAX_REL_KINETIC_DEVIATION:.1e}"
            ));
        }
        violations
    }
}

fn check_scenario(scenario: &str, run: &ReflectionRun, mut violations: Vec<String>) {
    run.metrics.report(scenario, violations.is_empty());
    let max_speed_change = run
        .events
        .iter()
        .map(|e| (len(e.v_after) - len(e.v_before)).abs() / len(e.v_before).max(1e-12))
        .fold(0.0f32, f32::max);
    warn_near_threshold(
        scenario,
        "max_rel_speed_change",
        max_speed_change,
        MAX_REL_SPEED_CHANGE,
    );
    warn_near_threshold(
        scenario,
        "max_rel_energy_drift",
        run.metrics.max_relative_energy_drift(),
        MAX_REL_KINETIC_DEVIATION,
    );
    println!(
        "{scenario} diagnostics: reflection_events={} expected_events={} wall_contacts={} overlap_corrections={}",
        run.events.len(),
        run.expected_events,
        run.wall_contacts,
        run.overlap_corrections
    );
    violations.dedup();
    assert!(
        violations.is_empty(),
        "{scenario} failed ({} events):\n  {}",
        run.events.len(),
        violations.join("\n  ")
    );
}

fn velocity_at(angle: f32) -> Vec2D {
    Vec2D::new(SPEED * angle.cos(), SPEED * angle.sin())
}

const WALL_ANGLE: f32 = 0.37;

fn run_world_wall(faults: TestFaults) -> (ReflectionRun, Vec<Surface>) {
    let main_content = create_dense_harness_with_seed(505);
    let surfaces = world_wall_surfaces(&main_content);
    let start = world_center(&main_content);
    let run = run_single_ball(
        main_content,
        &surfaces,
        start,
        velocity_at(WALL_ANGLE),
        faults,
    );
    (run, surfaces)
}

#[test]
#[ignore = "suite:drift"]
fn test_elastic_world_wall_reflections() {
    let (run, surfaces) = run_world_wall(TestFaults::default());
    let violations = run.violations(&surfaces);
    check_scenario("elastic_world_wall", &run, violations);
}

#[test]
#[ignore = "suite:drift"]
fn test_elastic_line_segment_barrier_reflections() {
    let mut main_content = create_dense_harness_with_seed(606);
    // Tilted barrier spanning the full world width, so the ball stays on one
    // side and only ever meets the segment face or the world walls.
    let center = world_center(&main_content);
    let seg_start = center + Vec2D::new(-50.0, -20.0);
    let seg_end = center + Vec2D::new(50.0, 20.0);
    main_content.add_line_segment(seg_start, seg_end);
    let half_thickness =
        0.5 * main_content.world().line_segments.segments()[0].thickness * THICKNESS_TO_WORLD;
    let mut surfaces = world_wall_surfaces(&main_content);
    let segment_idx = surfaces.len();
    surfaces.push(Surface::through(seg_start, seg_end).with_extra_reach(half_thickness));

    let run = run_single_ball(
        main_content,
        &surfaces,
        center + Vec2D::new(-20.0, 30.0),
        velocity_at(-1.1),
        TestFaults::default(),
    );
    let mut violations = run.violations(&surfaces);
    let segment_hits = run
        .events
        .iter()
        .filter(|e| hit_surface(e, &surfaces) == Some(segment_idx))
        .count();
    if segment_hits < 3 {
        violations.push(format!(
            "scenario invalid: barrier not exercised enough: segment_hits={segment_hits} events={}",
            run.events.len()
        ));
    }
    check_scenario("elastic_line_segment", &run, violations);
}

/// Real-simulation injected regression: a 0.2% restitution gain on world-wall
/// bounces must be rejected by the scenario.
#[test]
#[ignore = "suite:drift"]
fn test_world_wall_catches_injected_restitution_gain() {
    let (run, surfaces) = run_world_wall(TestFaults {
        wall_restitution: 1.002,
        ..TestFaults::default()
    });
    let joined = run.violations(&surfaces).join("\n");
    assert!(joined.contains("speed not conserved"), "{joined}");
    assert!(joined.contains("energy injection trend"), "{joined}");
}

#[test]
fn test_reference_event_count_matches_hand_calculation() {
    // Box [0, 10]^2, radius 1: the centre moves in [1, 9]. Starting at x = 5
    // with vx = 4, the centre reaches 9 at t=1, 1 at t=3 and 9 at t=5;
    // vy = 0 never hits. So 2 hits just before t=5 and 3 just after.
    let surfaces = unit_box_surfaces();
    let n = expected_event_count(
        Vec2D::new(5.0, 5.0),
        Vec2D::new(4.0, 0.0),
        1.0,
        &surfaces,
        4.99,
    );
    assert_eq!(n, 2);
    let n = expected_event_count(
        Vec2D::new(5.0, 5.0),
        Vec2D::new(4.0, 0.0),
        1.0,
        &surfaces,
        5.01,
    );
    assert_eq!(n, 3);
}

fn unit_box_surfaces() -> Vec<Surface> {
    vec![
        Surface::through(Vec2D::new(0.0, 10.0), Vec2D::new(10.0, 10.0)),
        Surface::through(Vec2D::new(10.0, 10.0), Vec2D::new(10.0, 0.0)),
        Surface::through(Vec2D::new(10.0, 0.0), Vec2D::new(0.0, 0.0)),
        Surface::through(Vec2D::new(0.0, 0.0), Vec2D::new(0.0, 10.0)),
    ]
}

/// Injected regressions: inelastic gain, a reflection towards the surface, a
/// tangential kick and a corner hit must each be rejected by the checks.
#[test]
fn test_reflection_checks_catch_injected_regressions() {
    // Box [0, 10] x [0, 10]; every event hits the right wall at x = 10.
    let surfaces = unit_box_surfaces();
    let good = |step: usize| ReflectionEvent {
        step,
        position: Vec2D::new(9.5, 5.0),
        radius: 0.5,
        v_before: Vec2D::new(SPEED * 0.6, SPEED * 0.8),
        v_after: Vec2D::new(-SPEED * 0.6, SPEED * 0.8),
    };
    let clean: Vec<_> = (0..MIN_EVENTS).map(good).collect();
    let v = reflection_violations(&clean, &surfaces, SPEED);
    assert!(v.is_empty(), "{v:?}");

    // Each bounce gains 0.5% speed.
    let mut injecting = clean.clone();
    let mut gain = 1.0f32;
    for e in &mut injecting {
        gain *= 1.005;
        e.v_after = Vec2D::new(e.v_after.x * gain, e.v_after.y * gain);
    }
    let joined = reflection_violations(&injecting, &surfaces, SPEED).join("\n");
    assert!(joined.contains("speed not conserved"), "{joined}");
    assert!(joined.contains("energy injection trend"), "{joined}");

    // Velocity flipped towards the right wall instead of away from it.
    let mut no_flip = clean.clone();
    no_flip[3].v_before = Vec2D::new(-SPEED * 0.6, SPEED * 0.8);
    no_flip[3].v_after = Vec2D::new(SPEED * 0.6, SPEED * 0.8);
    let v = reflection_violations(&no_flip, &surfaces, SPEED);
    assert!(v.iter().any(|s| s.contains("did not flip")), "{v:?}");

    // Speed-preserving rotation that is not specular.
    let mut kicked = clean.clone();
    kicked[5].v_after = Vec2D::new(-SPEED * 0.8, SPEED * 0.6);
    let v = reflection_violations(&kicked, &surfaces, SPEED);
    assert!(
        v.iter().any(|s| s.contains("not along a surface normal")),
        "{v:?}"
    );

    // Both components flipped in one step in the top-right corner.
    let mut corner = clean;
    corner[2].position = Vec2D::new(9.5, 9.5);
    corner[2].v_after = Vec2D::new(-SPEED * 0.6, -SPEED * 0.8);
    let v = reflection_violations(&corner, &surfaces, SPEED);
    assert!(v.iter().any(|s| s.contains("corner hit")), "{v:?}");
}
