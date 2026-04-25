use super::DT_GOAL;
use crate::application::MainContent;
use crate::application::core::StatsAccumulator;
use crate::objects2d::ball_impl::BallFactory;
use crate::objects2d::{OperatorCollection, OperatorState, Rect, Vec2D};

fn make_content() -> MainContent {
    MainContent::new(Rect::new(-20.0, 20.0, 40.0, -40.0)).unwrap()
}

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

#[test]
fn add_thermostat_increases_count() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    mc.add_thermostat(Vec2D::new(5.0, 5.0));
    assert_eq!(mc.thermostats.thermostats().len(), 2);
}

#[test]
fn added_thermostat_starts_off() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat = &mc.thermostats.thermostats()[0];
    assert_eq!(thermostat.state, OperatorState::Off);
    assert!(!thermostat.is_active());
}

#[test]
fn thermostat_ids_reset_after_clear() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    mc.add_thermostat(Vec2D::new(1.0, 0.0));
    mc.thermostats.clear();
    mc.add_thermostat(Vec2D::new(2.0, 0.0));
    assert_eq!(mc.thermostats.thermostats()[0].id, 0);
}

#[test]
fn rescale_remaps_center_and_radius() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let old_rect = Rect::new(-20.0, 20.0, 40.0, -40.0);
    let new_rect = Rect::new(-40.0, 40.0, 80.0, -80.0);

    mc.thermostats.rescale(&old_rect, &new_rect);

    let thermostat = &mc.thermostats.thermostats()[0];
    assert!((thermostat.position.x - 0.0).abs() < 1e-4);
    assert!((thermostat.position.y - 0.0).abs() < 1e-4);
    assert!(thermostat.radius > 5.0);
}

#[test]
fn fewer_than_two_balls_is_noop() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat_id = mc.thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);
    add_ball(&mut mc, Vec2D::new(0.5, 0.0), Vec2D::new(1.0, 0.0));

    let velocity_before = mc.balls[0].v;
    let mut stats = zeroed_stats();
    mc.apply_thermostats(DT_GOAL, &mut stats);

    assert_eq!(mc.balls[0].v, velocity_before);
    assert!(
        mc.thermostats
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
    let thermostat_id = mc.thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);

    // Two balls with identical velocities have zero local temperature in the
    // bucket COM frame. A correct thermostat must leave this rigid drift
    // unchanged instead of heating or damping the absolute motion.
    let drift_velocity = Vec2D::new(0.7, -0.3);
    add_ball(&mut mc, Vec2D::new(-0.5, 0.0), drift_velocity);
    add_ball(&mut mc, Vec2D::new(0.5, 0.0), drift_velocity);

    let mut stats = zeroed_stats();
    mc.apply_thermostats(DT_GOAL, &mut stats);

    assert_eq!(mc.balls[0].v, drift_velocity);
    assert_eq!(mc.balls[1].v, drift_velocity);
    assert!(
        mc.thermostats
            .find(thermostat_id)
            .unwrap()
            .current_local_temp
            .abs()
            < 1e-6
    );
}

#[test]
fn zero_target_temperature_freezes_owned_balls() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    let thermostat_id = mc.thermostats.thermostats()[0].id;
    mc.toggle_thermostat(thermostat_id);
    let thermostat = mc.thermostats.find_mut(thermostat_id).unwrap();
    thermostat.target_temperature = 0.0;

    add_ball(&mut mc, Vec2D::new(0.5, 0.0), Vec2D::new(1.0, 0.5));
    add_ball(&mut mc, Vec2D::new(-0.5, 0.0), Vec2D::new(-0.5, 1.0));
    add_ball(&mut mc, Vec2D::new(10.0, 10.0), Vec2D::new(0.25, 0.25));

    let far_velocity_before = mc.balls[2].v;
    let mut stats = zeroed_stats();
    mc.apply_thermostats(DT_GOAL, &mut stats);

    assert_eq!(mc.balls[0].v, Vec2D::zero());
    assert_eq!(mc.balls[1].v, Vec2D::zero());
    assert_eq!(mc.balls[2].v, far_velocity_before);
}

#[test]
fn overlapping_thermostats_assign_ball_to_nearest_owner() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(-1.0, 0.0));
    mc.add_thermostat(Vec2D::new(1.0, 0.0));

    let left_id = mc.thermostats.thermostats()[0].id;
    let right_id = mc.thermostats.thermostats()[1].id;
    mc.toggle_thermostat(left_id);
    mc.toggle_thermostat(right_id);
    mc.thermostats.find_mut(left_id).unwrap().target_temperature = 0.0;
    mc.thermostats
        .find_mut(right_id)
        .unwrap()
        .target_temperature = 0.5;

    add_ball(&mut mc, Vec2D::new(-0.25, 0.0), Vec2D::new(1.0, 0.0));
    add_ball(&mut mc, Vec2D::new(-0.5, 0.1), Vec2D::new(0.8, 0.2));
    add_ball(&mut mc, Vec2D::new(0.25, 0.0), Vec2D::new(1.0, 0.0));
    add_ball(&mut mc, Vec2D::new(0.5, -0.1), Vec2D::new(0.8, -0.2));

    let right_velocity_before = mc.balls[2].v;
    let mut stats = zeroed_stats();
    mc.apply_thermostats(DT_GOAL, &mut stats);

    assert_eq!(mc.balls[0].v, Vec2D::zero());
    assert_eq!(mc.balls[1].v, Vec2D::zero());
    assert_ne!(mc.balls[2].v, Vec2D::zero());
    assert_ne!(mc.balls[3].v, Vec2D::zero());
    assert_ne!(mc.balls[2].v, right_velocity_before);
}

#[test]
fn snapshot_includes_thermostats() {
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(3.0, 4.0));
    mc.toggle_thermostat(mc.thermostats.thermostats()[0].id);

    let snapshot = mc.create_snapshot();
    assert_eq!(snapshot.operators.thermostats.len(), 1);
    assert_eq!(
        snapshot.operators.thermostats[0].position,
        Vec2D::new(3.0, 4.0)
    );
    assert_eq!(snapshot.operators.thermostats[0].state, OperatorState::On);
}
