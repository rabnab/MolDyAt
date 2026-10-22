// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Snapshot tests moved from `thermo_physics` `core/tests/thermostats.rs` (#308).

use crate::application::CreateSnapshot;
use crate::application::MainContent;
use crate::application::SnapshotPublisher;
use crate::objects2d::ball_impl::BallFactory;
use crate::objects2d::{OperatorState, Rect, Vec2D};

fn make_content() -> MainContent {
    thermo_physics::test_support::new_main_content(Rect::new(-20.0, 20.0, 40.0, -40.0)).unwrap()
}

fn add_ball(mc: &mut MainContent, pos: Vec2D, velocity: Vec2D) {
    let factory = BallFactory::new();
    mc.add_candidate_to_vec(factory.create_massive_with_velocity(pos, velocity));
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

#[test]
fn rendering_off_measures_no_thermostat_readings() {
    use crate::application::scene_layers::SceneLayers;

    let mut publisher = SnapshotPublisher::new();
    let mut mc = make_content();
    mc.add_thermostat(Vec2D::new(0.0, 0.0));
    add_ball(&mut mc, Vec2D::new(-0.5, 0.0), Vec2D::new(1.0, 0.0));
    add_ball(&mut mc, Vec2D::new(0.5, 0.0), Vec2D::new(-1.0, 0.0));
    assert!(snapshot_reading(&mc) > 0.0);

    // F2 off: labels are not drawn, so nothing is measured; the snapshot
    // keeps the thermostat's stored value.
    publisher.set_layers(&mut mc, SceneLayers::ALL.difference(SceneLayers::HEAVY));
    let stored = mc.world().thermostats.thermostats()[0].current_local_temp;
    let reading = publisher.snapshot(&mc).operators.thermostats[0].current_local_temp;
    assert_eq!(reading.to_bits(), stored.to_bits());
}
