//! Scenario D: elastic wall/barrier reflection invariants.
//!
//! A single ball (no pair forces, no gravity) bounces around the world box,
//! optionally with a line-segment barrier. Every velocity change is a
//! reflection event and must be specular: speed conserved, normal component
//! flipped, tangential component kept, and no energy gained over time.

use super::*;

const SPEED: f32 = 12.0;
const STEPS: usize = 10_000;

// Thresholds: see docs/drift-test.md for the calibration process.
const MAX_REL_SPEED_CHANGE: f32 = 1.0e-5;
const MIN_NORMAL_ALIGNMENT: f32 = 0.999;
const MAX_REL_KINETIC_DEVIATION: f32 = 1.0e-5;
const MAX_REL_KINETIC_SLOPE_PER_EVENT: f32 = 1.0e-6;
const MIN_EVENTS: usize = 10;

#[derive(Debug, Clone, Copy)]
struct ReflectionEvent {
    step: usize,
    position: Vec2D,
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

/// An infinite line `point + t * tangent` that the ball can reflect from.
#[derive(Debug, Clone, Copy)]
struct Surface {
    point: Vec2D,
    normal: Vec2D,
}

impl Surface {
    fn through(a: Vec2D, b: Vec2D) -> Self {
        Self {
            point: a,
            normal: unit(Vec2D::new(-(b.y - a.y), b.x - a.x)),
        }
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
            violations.push(format!(
                "impulse not along a surface normal (tangential velocity changed) {context}"
            ));
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

/// Runs one ball with the given initial velocity and collects every step in
/// which its velocity changed.
fn run_single_ball(
    mut main_content: MainContent,
    start: Vec2D,
    velocity: Vec2D,
) -> (DriftRunMetrics, Vec<ReflectionEvent>) {
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    let factory = BallFactory::new();
    main_content
        .add_candidate_ball(factory.create_massive_with_velocity(start, velocity), 0.0)
        .expect("could not add reflection ball");
    main_content.world_mut().rebuild_cell_grid();
    main_content.collect_neighbours_by_cell(DT_GOAL);

    let mut events = Vec::new();
    let mut prev_v = main_content.world().balls[0].v;
    let metrics = run_drift_harness_with_observer(
        &mut main_content,
        DriftHarnessConfig {
            steps: STEPS,
            sample_every: 100,
            dt: DT_GOAL,
        },
        |step, mc| {
            let ball = &mc.world().balls[0];
            let dv = Vec2D::new(ball.v.x - prev_v.x, ball.v.y - prev_v.y);
            if len(dv) > 1e-3 * len(prev_v) {
                events.push(ReflectionEvent {
                    step,
                    position: *ball.get_position(),
                    v_before: prev_v,
                    v_after: ball.v,
                });
            }
            prev_v = ball.v;
        },
    );
    (metrics, events)
}

fn assert_reflections(
    scenario: &str,
    metrics: &DriftRunMetrics,
    events: &[ReflectionEvent],
    surfaces: &[Surface],
) {
    let violations = reflection_violations(events, surfaces, SPEED);
    metrics.report(scenario, violations.is_empty());
    println!("{scenario} diagnostics: reflection_events={}", events.len());
    assert!(
        violations.is_empty(),
        "{scenario} failed ({} events):\n  {}",
        events.len(),
        violations.join("\n  ")
    );
}

fn velocity_at(angle: f32) -> Vec2D {
    Vec2D::new(SPEED * angle.cos(), SPEED * angle.sin())
}

#[test]
#[ignore = "suite:drift"]
fn test_elastic_world_wall_reflections() {
    let main_content = create_dense_harness_with_seed(505);
    let surfaces = world_wall_surfaces(&main_content);
    let (metrics, events) =
        run_single_ball(main_content, Vec2D::new(550.0, 450.0), velocity_at(0.37));
    assert_reflections("elastic_world_wall", &metrics, &events, &surfaces);
}

#[test]
#[ignore = "suite:drift"]
fn test_elastic_line_segment_barrier_reflections() {
    let mut main_content = create_dense_harness_with_seed(606);
    // Tilted barrier spanning the full world width, so the ball stays on one
    // side and only ever meets the segment face or the world walls.
    let seg_start = Vec2D::new(500.0, 430.0);
    let seg_end = Vec2D::new(600.0, 470.0);
    main_content.add_line_segment(seg_start, seg_end);
    let mut surfaces = world_wall_surfaces(&main_content);
    let segment_idx = surfaces.len();
    surfaces.push(Surface::through(seg_start, seg_end));

    let (metrics, events) =
        run_single_ball(main_content, Vec2D::new(530.0, 480.0), velocity_at(-1.1));
    let segment_hits = events
        .iter()
        .filter(|e| hit_surface(e, &surfaces) == Some(segment_idx))
        .count();
    assert!(
        segment_hits >= 3,
        "barrier scenario did not exercise the segment enough: segment_hits={segment_hits} events={}",
        events.len()
    );
    assert_reflections("elastic_line_segment", &metrics, &events, &surfaces);
}

/// Injected regressions: inelastic gain, a reflection towards the surface and
/// a tangential kick must each be rejected by the reflection checks.
#[test]
fn test_reflection_checks_catch_injected_regressions() {
    // Unit box [0, 10] x [0, 10]; every event hits the right wall at x = 10.
    let surfaces = vec![
        Surface::through(Vec2D::new(0.0, 10.0), Vec2D::new(10.0, 10.0)),
        Surface::through(Vec2D::new(10.0, 10.0), Vec2D::new(10.0, 0.0)),
        Surface::through(Vec2D::new(10.0, 0.0), Vec2D::new(0.0, 0.0)),
        Surface::through(Vec2D::new(0.0, 0.0), Vec2D::new(0.0, 10.0)),
    ];
    let good = |step: usize| ReflectionEvent {
        step,
        position: Vec2D::new(9.5, 5.0),
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
    let mut kicked = clean;
    kicked[5].v_after = Vec2D::new(-SPEED * 0.8, SPEED * 0.6);
    let v = reflection_violations(&kicked, &surfaces, SPEED);
    assert!(
        v.iter().any(|s| s.contains("not along a surface normal")),
        "{v:?}"
    );
}
