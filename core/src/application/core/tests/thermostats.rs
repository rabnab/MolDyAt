use super::DT_GOAL;
use crate::application::MainContent;
use crate::application::core::StatsAccumulator;
use crate::application::thermo_field::LocalKineticStats;
use crate::objects2d::ball_impl::BallFactory;
use crate::objects2d::{
    DEFAULT_THERMOSTAT_CHAIN_LENGTH, OperatorCollection, OperatorState, Rect, Vec2D,
};

fn make_content() -> MainContent {
    MainContent::new(Rect::new(-20.0, 20.0, 40.0, -40.0)).unwrap()
}

/// Threshold below which trailing chain values are treated as effectively zero.
const MIN_SIGNIFICANT_CHAIN_VALUE: f32 = 1.0e-6;
/// Coarse upper bound used to detect runaway heating in long-run stability checks.
const MAX_STABLE_BUCKET_TEMP: f32 = 10.0;

fn zeroed_stats() -> StatsAccumulator {
    StatsAccumulator {
        pot: 0.0,
        kin: 0.0,
        v_com: Vec2D::zero(),
        sum_mass: 0.0,
        max_v: 0.0,
        max_id: 0,
    }
}

fn add_ball(mc: &mut MainContent, pos: Vec2D, velocity: Vec2D) {
    let factory = BallFactory::new();
    mc.add_candidate_to_vec(factory.create_massive_with_velocity(pos, velocity));
}

fn add_ball_with_size(mc: &mut MainContent, pos: Vec2D, velocity: Vec2D, size_idx: usize) {
    let factory = BallFactory::new();
    mc.add_candidate_to_vec(factory.create_with_size_and_velocity(pos, size_idx, velocity));
}

fn cluster_kinetic_stats(mc: &MainContent) -> LocalKineticStats {
    let mut stats = LocalKineticStats::new();
    for ball in &mc.world().balls {
        stats.add_ball(ball);
    }
    stats
}

#[test]
fn add_thermostat_increases_count() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    mc.add_thermostat(Vec2D::new(5.0, 5.0));
    assert_eq!(mc.world().thermostats.thermostats().len(), 2);
}

#[test]
fn added_thermostat_starts_off() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat = &mc.world().thermostats.thermostats()[0];
    assert_eq!(thermostat.state, OperatorState::Off);
    assert!(!thermostat.is_active());
}

#[test]
fn added_thermostat_starts_with_five_chain_variables() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat = &mc.world().thermostats.thermostats()[0];
    assert_eq!(thermostat.chain.len(), DEFAULT_THERMOSTAT_CHAIN_LENGTH);
    assert!(thermostat.chain.iter().all(|&value| value == 0.0));
}

#[test]
fn thermostat_ids_reset_after_clear() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    mc.add_thermostat(Vec2D::new(1.0, 0.0));
    mc.world_mut().thermostats.clear();
    mc.add_thermostat(Vec2D::new(2.0, 0.0));
    assert_eq!(mc.world().thermostats.thermostats()[0].id, 0);
}

#[test]
fn rescale_remaps_center_and_radius() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let old_rect = Rect::new(-20.0, 20.0, 40.0, -40.0);
    let new_rect = Rect::new(-40.0, 40.0, 80.0, -80.0);

    mc.world_mut().thermostats.rescale(&old_rect, &new_rect);

    let thermostat = &mc.world().thermostats.thermostats()[0];
    assert!((thermostat.position.x - 0.0).abs() < 1e-4);
    assert!((thermostat.position.y - 0.0).abs() < 1e-4);
    assert!(thermostat.radius > 5.0);
}

#[test]
fn fewer_than_two_balls_is_noop() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat_id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);
    add_ball(&mut mc, Vec2D::new(0.5, 0.0), Vec2D::new(1.0, 0.0));

    let velocity_before = mc.world().balls[0].v;
    let mut stats = zeroed_stats();
    mc.apply_thermostats(DT_GOAL, &mut stats);

    assert_eq!(mc.world().balls[0].v, velocity_before);
    assert!(
        mc.world()
            .thermostats
            .find(thermostat_id)
            .unwrap()
            .current_local_temp
            .is_nan()
    );
}

#[test]
fn rigid_bucket_drift_is_preserved() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat_id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);

    // Two balls with identical velocities have zero local temperature in the
    // bucket COM frame. A correct thermostat must leave this rigid drift
    // unchanged instead of heating or damping the absolute motion.
    let drift_velocity = Vec2D::new(0.7, -0.3);
    add_ball(&mut mc, Vec2D::new(-0.5, 0.0), drift_velocity);
    add_ball(&mut mc, Vec2D::new(0.5, 0.0), drift_velocity);

    let mut stats = zeroed_stats();
    mc.apply_thermostats(DT_GOAL, &mut stats);

    assert_eq!(mc.world().balls[0].v, drift_velocity);
    assert_eq!(mc.world().balls[1].v, drift_velocity);
    assert!(
        mc.world()
            .thermostats
            .find(thermostat_id)
            .unwrap()
            .current_local_temp
            .abs()
            < 1e-6
    );
}

#[test]
fn hot_thermostat_heats_cold_cluster_without_changing_com_drift() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(5.0, 0.0));
    let thermostat_id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);
    let thermostat = mc.world_mut().thermostats.find_mut(thermostat_id).unwrap();
    thermostat.target_temperature = 1.0;
    thermostat.radius = 12.0;

    // Ten equal-mass particles form a cold cluster with T = 0.25 in the
    // local COM frame while the full cluster drifts toward the thermostat
    // center. Five +u and five -u thermal velocities give zero thermal COM and
    // `sum |u_i|^2 = 10 * u^2 = 4.5`, which matches T = 4.5 / (2 * (10 - 1)).
    let drift_velocity = Vec2D::new(0.8, 0.0);
    let rel_speed = 0.45_f32.sqrt();
    let positions = [
        Vec2D::new(-3.0, 1.8),
        Vec2D::new(-2.0, 1.8),
        Vec2D::new(-1.0, 1.8),
        Vec2D::new(0.0, 1.8),
        Vec2D::new(1.0, 1.8),
        Vec2D::new(-3.0, -1.8),
        Vec2D::new(-2.0, -1.8),
        Vec2D::new(-1.0, -1.8),
        Vec2D::new(0.0, -1.8),
        Vec2D::new(1.0, -1.8),
    ];

    for (idx, pos) in positions.into_iter().enumerate() {
        let thermal_velocity = if idx < 5 {
            Vec2D::new(rel_speed, 0.0)
        } else {
            Vec2D::new(-rel_speed, 0.0)
        };
        add_ball_with_size(&mut mc, pos, drift_velocity + thermal_velocity, 0);
    }

    assert_eq!(mc.world().balls.len(), 10);

    let initial_stats = cluster_kinetic_stats(&mc);
    let initial_temp = initial_stats.temperature().unwrap();
    let initial_v_com = initial_stats.center_of_mass_velocity().unwrap();

    assert!((initial_temp - 0.25).abs() < 1e-5);
    assert!((initial_v_com.x - drift_velocity.x).abs() < 1e-6);
    assert!(initial_v_com.y.abs() < 1e-6);

    let mut stats = zeroed_stats();
    for _ in 0..200 {
        mc.apply_thermostats(DT_GOAL, &mut stats);
    }

    let final_stats = cluster_kinetic_stats(&mc);
    let final_temp = final_stats.temperature().unwrap();
    let final_v_com = final_stats.center_of_mass_velocity().unwrap();

    assert!(final_temp > initial_temp);
    assert!((final_v_com.x - initial_v_com.x).abs() < 1e-5);
    assert!((final_v_com.y - initial_v_com.y).abs() < 1e-5);
}

#[test]
fn thermostat_preserves_bucket_com_drift_when_damping_noncollinear_velocities() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat_id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);
    let thermostat = mc.world_mut().thermostats.find_mut(thermostat_id).unwrap();
    thermostat.target_temperature = 0.01;
    thermostat.radius = 5.0;

    // The cluster drifts along +x while each ball has a vertical peculiar
    // velocity component. A COM-frame thermostat should preserve the drift
    // velocity while damping the vertical thermal motion.
    add_ball(&mut mc, Vec2D::new(-0.5, 0.0), Vec2D::new(1.0, 1.0));
    add_ball(&mut mc, Vec2D::new(0.5, 0.0), Vec2D::new(1.0, -1.0));

    let initial_stats = cluster_kinetic_stats(&mc);
    let initial_temp = initial_stats.temperature().unwrap();
    let initial_v_com = initial_stats.center_of_mass_velocity().unwrap();

    let mut stats = zeroed_stats();
    mc.apply_thermostats(DT_GOAL, &mut stats);

    let final_stats = cluster_kinetic_stats(&mc);
    let final_temp = final_stats.temperature().unwrap();
    let final_v_com = final_stats.center_of_mass_velocity().unwrap();
    let velocity_after = mc.world().balls[0].v;

    assert!(final_temp < initial_temp);
    assert!((final_v_com.x - initial_v_com.x).abs() < 1e-6);
    assert!((final_v_com.y - initial_v_com.y).abs() < 1e-6);
    assert!((velocity_after.x - initial_v_com.x).abs() < 1e-6);
    assert!(velocity_after.y > 0.0);
    assert!(velocity_after.y < 1.0);
}

#[test]
fn thermostat_does_not_flip_peculiar_velocity_when_damping_exceeds_one() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat_id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);
    let thermostat = mc.world_mut().thermostats.find_mut(thermostat_id).unwrap();
    thermostat.target_temperature = 0.01;
    thermostat.radius = 5.0;
    thermostat.chain = vec![10.0, 0.0, 0.0, 0.0, 0.0];

    // With a large enough `dt`, the current implementation can make
    // `1.0 - friction` negative and invert the peculiar velocity.
    add_ball(&mut mc, Vec2D::new(-0.5, 0.0), Vec2D::new(1.0, 1.0));
    add_ball(&mut mc, Vec2D::new(0.5, 0.0), Vec2D::new(1.0, -1.0));

    let mut stats = zeroed_stats();
    mc.apply_thermostats(0.2, &mut stats);

    let velocity_after = mc.world().balls[0].v;
    assert!(velocity_after.y >= 0.0);
}

#[test]
fn thermostat_updates_all_chain_variables() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat_id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);
    let thermostat = mc.world_mut().thermostats.find_mut(thermostat_id).unwrap();
    thermostat.target_temperature = 0.02;
    thermostat.radius = 8.0;

    add_ball(&mut mc, Vec2D::new(-1.0, -0.2), Vec2D::new(1.2, 1.0));
    add_ball(&mut mc, Vec2D::new(1.0, 0.2), Vec2D::new(0.8, -1.0));
    add_ball(&mut mc, Vec2D::new(-0.2, 1.0), Vec2D::new(1.0, 0.9));
    add_ball(&mut mc, Vec2D::new(0.2, -1.0), Vec2D::new(1.0, -0.9));

    let mut stats = zeroed_stats();
    for _ in 0..500 {
        mc.apply_thermostats(DT_GOAL, &mut stats);
    }

    let chain = &mc.world().thermostats.find(thermostat_id).unwrap().chain;
    assert_eq!(chain.len(), DEFAULT_THERMOSTAT_CHAIN_LENGTH);
    assert!(chain.iter().all(|v| v.is_finite()));
    assert!(
        chain
            .iter()
            .skip(1)
            .any(|v| v.abs() > MIN_SIGNIFICANT_CHAIN_VALUE)
    );
}

#[test]
fn thermostat_remains_stable_in_small_bucket_long_run() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat_id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);
    let thermostat = mc.world_mut().thermostats.find_mut(thermostat_id).unwrap();
    thermostat.target_temperature = 0.05;
    thermostat.radius = 6.0;

    add_ball(&mut mc, Vec2D::new(-0.5, 0.0), Vec2D::new(0.8, 1.0));
    add_ball(&mut mc, Vec2D::new(0.5, 0.0), Vec2D::new(1.2, -1.0));

    let mut stats = zeroed_stats();
    for _ in 0..2000 {
        mc.apply_thermostats(DT_GOAL, &mut stats);
    }

    let kinetic_stats = cluster_kinetic_stats(&mc);
    let final_temp = kinetic_stats.temperature().unwrap();
    let chain = &mc.world().thermostats.find(thermostat_id).unwrap().chain;
    assert!(final_temp.is_finite());
    assert!(final_temp < MAX_STABLE_BUCKET_TEMP);
    assert!(chain.iter().all(|v| v.is_finite()));
}

#[test]
fn zero_target_temperature_freezes_owned_balls() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat_id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);
    let thermostat = mc.world_mut().thermostats.find_mut(thermostat_id).unwrap();
    thermostat.target_temperature = 0.0;

    add_ball(&mut mc, Vec2D::new(0.5, 0.0), Vec2D::new(1.0, 0.5));
    add_ball(&mut mc, Vec2D::new(-0.5, 0.0), Vec2D::new(-0.5, 1.0));
    add_ball(&mut mc, Vec2D::new(10.0, 10.0), Vec2D::new(0.25, 0.25));

    let far_velocity_before = mc.world().balls[2].v;
    let mut stats = zeroed_stats();
    mc.apply_thermostats(DT_GOAL, &mut stats);

    assert_eq!(mc.world().balls[0].v, Vec2D::zero());
    assert_eq!(mc.world().balls[1].v, Vec2D::zero());
    assert_eq!(mc.world().balls[2].v, far_velocity_before);
    assert!(
        mc.world()
            .thermostats
            .find(thermostat_id)
            .unwrap()
            .current_local_temp
            .abs()
            < 1e-6
    );
}

#[test]
fn overlapping_thermostats_assign_ball_to_nearest_owner() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(-1.0, 0.0));
    mc.add_thermostat(Vec2D::new(1.0, 0.0));

    let left_id = mc.world().thermostats.thermostats()[0].id;
    let right_id = mc.world().thermostats.thermostats()[1].id;
    mc.toggle_thermostat(left_id);
    mc.toggle_thermostat(right_id);
    mc.world_mut()
        .thermostats
        .find_mut(left_id)
        .unwrap()
        .target_temperature = 0.0;
    mc.world_mut()
        .thermostats
        .find_mut(right_id)
        .unwrap()
        .target_temperature = 0.5;

    add_ball(&mut mc, Vec2D::new(-0.25, 0.0), Vec2D::new(1.0, 0.0));
    add_ball(&mut mc, Vec2D::new(-0.5, 0.1), Vec2D::new(0.8, 0.2));
    add_ball(&mut mc, Vec2D::new(0.25, 0.0), Vec2D::new(1.0, 0.0));
    add_ball(&mut mc, Vec2D::new(0.5, -0.1), Vec2D::new(0.8, -0.2));

    let right_velocity_before = mc.world().balls[2].v;
    let mut stats = zeroed_stats();
    mc.apply_thermostats(DT_GOAL, &mut stats);

    assert_eq!(mc.world().balls[0].v, Vec2D::zero());
    assert_eq!(mc.world().balls[1].v, Vec2D::zero());
    assert_ne!(mc.world().balls[2].v, Vec2D::zero());
    assert_ne!(mc.world().balls[3].v, Vec2D::zero());
    assert_ne!(mc.world().balls[2].v, right_velocity_before);
}

#[test]
fn snapshot_includes_thermostats() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(3.0, 4.0));
    mc.toggle_thermostat(mc.world().thermostats.thermostats()[0].id);

    let snapshot = mc.create_snapshot();
    assert_eq!(snapshot.operators.thermostats.len(), 1);
    assert_eq!(
        snapshot.operators.thermostats[0].position,
        Vec2D::new(3.0, 4.0)
    );
    assert_eq!(snapshot.operators.thermostats[0].state, OperatorState::On);
}

fn snapshot_reading(mc: &MainContent) -> f32 {
    mc.create_snapshot().operators.thermostats[0].current_local_temp
}

#[test]
fn deactivated_thermostat_snapshot_reading_is_live_and_read_only() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    assert!(!mc.world().thermostats.thermostats()[0].is_active());

    add_ball(&mut mc, Vec2D::new(-0.5, 0.0), Vec2D::new(1.0, 0.0));
    add_ball(&mut mc, Vec2D::new(0.5, 0.0), Vec2D::new(-1.0, 0.0));
    // A ball outside the radius does not count.
    add_ball(&mut mc, Vec2D::new(15.0, 0.0), Vec2D::new(9.0, 9.0));

    let first = snapshot_reading(&mc);
    assert!(first.is_finite() && first > 0.0);

    // Faster balls (an edit while paused, no step): the reading follows.
    mc.world_mut().balls[0].v = Vec2D::new(3.0, 0.0);
    mc.world_mut().balls[1].v = Vec2D::new(-3.0, 0.0);
    let second = snapshot_reading(&mc);
    assert!(second > first);

    // Measuring changes nothing: an inactive thermostat never regulates.
    assert_eq!(mc.world().balls[0].v, Vec2D::new(3.0, 0.0));
}

#[test]
fn active_thermostat_snapshot_reading_follows_edits_while_paused() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat_id = mc.world().thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);
    add_ball(&mut mc, Vec2D::new(-0.5, 0.0), Vec2D::new(1.0, 0.0));
    add_ball(&mut mc, Vec2D::new(0.5, 0.0), Vec2D::new(-1.0, 0.0));
    mc.world_mut().sim.set_pause(true);

    let first = snapshot_reading(&mc);
    assert!(first.is_finite() && first > 0.0);
    mc.world_mut().balls[0].v = Vec2D::new(4.0, 0.0);
    assert!(snapshot_reading(&mc) > first);

    // Moved away from its balls: no reading.
    mc.move_thermostat(thermostat_id, Vec2D::new(15.0, 15.0));
    assert!(snapshot_reading(&mc).is_nan());
}

#[test]
fn snapshot_reading_of_active_thermostats_uses_step_ownership() {
    // Two overlapping active thermostats: each ball counts for the nearer
    // one only, as in the step.
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(-1.0, 0.0));
    mc.add_thermostat(Vec2D::new(1.0, 0.0));
    for thermostat_id in [0, 1].map(|i| mc.world().thermostats.thermostats()[i].id) {
        mc.toggle_thermostat(thermostat_id);
    }
    add_ball(&mut mc, Vec2D::new(-1.2, 0.0), Vec2D::new(1.0, 0.0));
    add_ball(&mut mc, Vec2D::new(-0.8, 0.0), Vec2D::new(-1.0, 0.0));
    add_ball(&mut mc, Vec2D::new(0.8, 0.0), Vec2D::new(3.0, 0.0));
    add_ball(&mut mc, Vec2D::new(1.2, 0.0), Vec2D::new(-3.0, 0.0));

    let readings = mc.world().thermostat_readings();
    assert_eq!(readings.len(), 2);
    assert!(readings[1] > readings[0], "the right pair is hotter");
}
