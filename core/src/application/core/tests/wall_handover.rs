//! Handover between the two wall mechanisms.
//!
//! Contacts are resolved exactly in the position update (`wall_contact`),
//! one per axis for box walls and one for line walls. A ball that meets a box
//! wall skips the line-wall sweep in that step, and a ball that starts a step
//! inside a line wall is left to the overlap fallback
//! (`resolve_wall_overlaps`), which projects it out. These tests pin that
//! handover down and check that the fallback stays rare.

use super::create_main_content_for_region;
use crate::application::{GameLogic, MainContent};
use crate::objects2d::ball_impl::BallFactory;
use crate::objects2d::{Rect, Vec2D};

/// World spanning x, y = -50..50.
const WORLD_TOP_LEFT: (f32, f32) = (-50.0, 50.0);
/// Balls touch a wall this far outside its surface (`COLLISION_EPSILON`).
const WALL_CONTACT_MARGIN: f32 = 0.01;

/// Deterministic gas among `walls` short walls: seeded ball placement and
/// fixed velocities (speed ~1), so wall statistics are reproducible.
fn seeded_gas_among_walls(balls: usize, walls: usize) -> MainContent {
    let (x0, y0) = WORLD_TOP_LEFT;
    let mut main_content = MainContent::new_with_seed(Rect::new(x0, y0, 100.0, -100.0), 2468)
        .expect("create seeded world");
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    for k in 0..walls {
        let cx = -45.0 + 90.0 * ((k as f32 * 0.618) % 1.0);
        let cy = -45.0 + 90.0 * ((k as f32 * 0.382) % 1.0);
        let angle = k as f32 * 0.7;
        main_content.add_line_segment(
            Vec2D::new(cx, cy),
            Vec2D::new(cx + 12.0 * angle.cos(), cy + 12.0 * angle.sin()),
        );
    }
    main_content.seed_random_balls(balls);
    for (i, ball) in main_content.world_mut().balls.iter_mut().enumerate() {
        let phase = i as f32 * 2.399; // golden angle: spread directions
        ball.v = Vec2D::new(phase.cos(), phase.sin());
    }
    main_content
}

fn world() -> MainContent {
    let (mut main_content, _) = create_main_content_for_region(WORLD_TOP_LEFT.0, WORLD_TOP_LEFT.1);
    main_content.world_mut().speed_limit_override = Some(f32::INFINITY);
    main_content
}

/// A ball in a box corner formed by the floor and a vertical line wall that
/// stands on it reaches both in the same step: the floor is resolved exactly,
/// the line wall by the overlap fallback. The ball must end outside both,
/// reflected in both directions, with its speed unchanged.
#[test]
fn test_box_wall_and_line_wall_in_the_same_step() {
    let mut main_content = world();
    const WALL_X: f32 = 45.0;
    main_content.add_line_segment(Vec2D::new(WALL_X, -50.0), Vec2D::new(WALL_X, -30.0));
    let half_thickness = 0.5 * main_content.world().line_segments.segments()[0].thickness;

    let factory = BallFactory::new();
    let probe = factory.create_massive_with_velocity(Vec2D::zero(), Vec2D::zero());
    let radius = probe.radius;
    let wall_surface_x = WALL_X - half_thickness - radius - WALL_CONTACT_MARGIN;
    let floor_y = -50.0 + radius;
    // 0.05 from both surfaces, moving 0.1 towards each within one step.
    let velocity = Vec2D::new(10.0, -10.0);
    let ball = factory
        .create_massive_with_velocity(Vec2D::new(wall_surface_x - 0.05, floor_y + 0.05), velocity);
    main_content.world_mut().add_ball_and_register(ball);
    main_content.collect_neighbours_by_cell(super::DT_GOAL);

    main_content.simulation_step().unwrap();

    let world = main_content.world();
    let ball = &world.balls[0];
    assert_eq!(world.sim.wall_contacts, 1, "floor contact resolved exactly");
    assert_eq!(
        world.sim.wall_overlap_corrections, 1,
        "line wall left to the overlap fallback"
    );
    assert!(
        ball.v.x < 0.0 && ball.v.y > 0.0,
        "reflected by both: v={:?}",
        ball.v
    );
    assert!(
        (ball.v.norm() - velocity.norm()).abs() < 1e-4,
        "speed changed: {} -> {}",
        velocity.norm(),
        ball.v.norm()
    );
    let pos = ball.get_position();
    assert!(
        pos.x <= wall_surface_x + 1e-4,
        "inside the line wall: {pos:?}"
    );
    assert!(pos.y >= floor_y - 1e-4, "inside the floor: {pos:?}");
}

/// In a gas among many walls the overlap fallback must stay rare: nearly
/// every contact starts clear of the walls and is resolved exactly.
#[test]
#[ignore = "suite:physics (slow)"]
fn test_overlap_fallback_stays_rare_in_a_gas_with_walls() {
    const WALLS: usize = 40;
    const STEPS: usize = 3_000;
    const MAX_FALLBACK_FRACTION: f64 = 0.01;
    const MIN_CONTACTS: u64 = 300;

    let mut main_content = seeded_gas_among_walls(600, WALLS);
    for _ in 0..STEPS {
        main_content.simulation_step().unwrap();
    }

    let stats = &main_content.world().sim;
    let fraction = stats.wall_overlap_corrections as f64 / stats.wall_contacts.max(1) as f64;
    println!(
        "WALL_HANDOVER contacts={} overlap_corrections={} fraction={fraction:.2e}",
        stats.wall_contacts, stats.wall_overlap_corrections
    );
    assert!(
        stats.wall_contacts >= MIN_CONTACTS,
        "scenario invalid: only {} wall contacts",
        stats.wall_contacts
    );
    assert!(
        fraction < MAX_FALLBACK_FRACTION,
        "overlap fallback fired for {fraction:.2e} of contacts (limit {MAX_FALLBACK_FRACTION})"
    );
}

/// Performance guard without timing: count the ball-wall pairs that need a
/// real distance computation (`SimStats::wall_distance_checks`). The
/// bounding-box reject and testing the end of a step first keep that to the
/// few pairs where a ball is near a wall; losing either makes every ball
/// check (nearly) every wall on every step, as before review of #206.
#[test]
fn test_wall_distance_checks_stay_local() {
    const WALLS: usize = 40;
    const BALLS: usize = 600;
    const STEPS: usize = 300;
    /// Checks per (ball, wall, step). Measured 1.10e-2 here (1.06e-2 to
    /// 1.13e-2 for 550..650 balls and 200..450 steps). Testing the start of
    /// the step first gives 1.65e-2; without the bounding-box reject, 2.0.
    const MAX_CHECKS_PER_PAIR_STEP: f64 = 1.4e-2;

    let mut main_content = seeded_gas_among_walls(BALLS, WALLS);
    let balls = main_content.world().balls.len();
    let before = main_content.world().sim.wall_distance_checks;
    for _ in 0..STEPS {
        main_content.simulation_step().unwrap();
    }

    let checks = main_content.world().sim.wall_distance_checks - before;
    let per_pair_step = checks as f64 / (balls * WALLS * STEPS) as f64;
    println!(
        "WALL_CHECKS balls={balls} walls={WALLS} steps={STEPS} checks={checks} per_pair_step={per_pair_step:.3e}"
    );
    assert!(
        per_pair_step < MAX_CHECKS_PER_PAIR_STEP,
        "{per_pair_step:.3e} distance checks per ball, wall and step (limit {MAX_CHECKS_PER_PAIR_STEP}); \
         did the bounding-box reject or the end-of-step-first order get lost?"
    );
}
