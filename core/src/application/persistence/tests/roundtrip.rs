// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::scratch_dir;
use crate::application::persistence::{LoadSource, SaveStore, WorldFile};
use crate::application::sim_config::PairOverrideConfig;
use crate::application::{MainContent, PhysicsEvent, PhysicsThread, SimConfig, WorldCommand};
use crate::objects2d::ball_impl::{Ball2D, SpeciesId, SpeciesTable};
use crate::objects2d::{Rect, Vec2D};
use std::fs;
use std::time::{Duration, Instant};

fn world_rect() -> Rect {
    Rect::new(-10.0, 10.0, 100.0, -100.0)
}

fn empty_world() -> MainContent {
    MainContent::new(world_rect()).expect("create MainContent")
}

/// A config with every value changed from its default.
fn custom_config() -> SimConfig {
    let mut config = SimConfig::default();
    config.integration.time_step = 0.005;
    config.integration.speed_limit_factor = 0.3;
    config.potential.max_force = 80.0;
    config.potential.core_stiffness = 150.0;
    config.potential.cutoff_factor = 3.5;
    config.neighbours.skin_factor = 0.25;
    config.display.snapshot_interval_us = 33_333;
    config.potential.pair_overrides.push(PairOverrideConfig {
        a: "N2".into(),
        b: "M".into(),
        sigma: Some(2.0),
        epsilon: Some(1.5),
    });
    config
}

/// A world with every kind of saved object: balls, a split wall, a closed
/// valve, a running source sphere and a thermostat, run with a custom config.
fn busy_world() -> MainContent {
    let mut mc =
        MainContent::with_config(world_rect(), &custom_config(), None).expect("create MainContent");
    mc.add_line_segment(Vec2D::new(-8.0, 5.0), Vec2D::new(8.0, 5.0));
    mc.add_valve_segment(0, 0.4, 0.6, true);
    mc.add_source_sphere(Vec2D::new(-5.0, -5.0));
    mc.toggle_source_sphere(0);
    mc.add_thermostat(Vec2D::new(5.0, -5.0));
    mc.world_mut().thermostats.find_mut(0).unwrap().chain[1] = 0.25;
    mc.world_mut().physics.constants.gravity = 0.5;

    for (i, x) in [-6.0, -2.0, 2.0, 6.0].into_iter().enumerate() {
        let mut ball = Ball2D::new(10 + i, Vec2D::new(x, 0.0), SpeciesId::new((i % 3) as u16));
        ball.v = Vec2D::new(0.1 * i as f32, -0.2);
        ball.accel = Vec2D::new(0.01, -0.5);
        ball.pre_update_pos = Vec2D::new(x - 0.01, 0.01);
        mc.add_candidate_to_vec(ball);
    }
    mc
}

fn capture(mc: &MainContent) -> WorldFile {
    WorldFile::capture(mc)
}

#[test]
fn world_survives_a_save_and_load() {
    let store = SaveStore::new(scratch_dir("roundtrip"));
    let source = busy_world();
    source.save_world(&store).unwrap();

    let mut target = empty_world();
    // Something else is running there first.
    target.add_candidate_at_pos(Vec2D::new(1.0, 1.0)).unwrap();
    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();

    assert_eq!(capture(&target), capture(&source));
    assert!(target.world().sim.is_paused());
    let _ = fs::remove_dir_all(store.dir());
}

#[test]
fn config_survives_a_save_and_load() {
    let store = SaveStore::new(scratch_dir("config"));
    let source = busy_world();
    assert_eq!(source.config(), custom_config());
    let path = source.save_world(&store).unwrap();
    let saved = WorldFile::from_json(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(saved.config, Some(custom_config()));

    let mut target = empty_world();
    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    assert_eq!(target.config(), custom_config());
    let physics = &target.world().physics;
    assert_eq!(physics.cap_settings().cutoff_factor, 3.5);
    assert_eq!(physics.get_skin_thickness(), 0.25 * 3.5);
    let [n2, m] = ["N2", "M"].map(|name| SpeciesTable::builtin().by_name(name).unwrap());
    assert_eq!(physics.cutoff(m, n2), 3.5 * 2.0);
    let _ = fs::remove_dir_all(store.dir());
}

#[test]
fn pause_after_load_is_the_callers_choice() {
    let store = SaveStore::new(scratch_dir("pause"));
    busy_world().save_world(&store).unwrap();
    let mut target = empty_world();
    target
        .load_world(&store, &LoadSource::Latest, false)
        .unwrap();
    assert!(!target.world().sim.is_paused());
    let _ = fs::remove_dir_all(store.dir());
}

#[test]
fn loaded_world_starts_fresh_and_keeps_running() {
    let store = SaveStore::new(scratch_dir("fresh"));
    let mut source = busy_world();
    for _ in 0..20 {
        source.simulation_step().unwrap();
    }
    assert!(source.world().step_count > 0);
    source.save_world(&store).unwrap();

    let mut target = empty_world();
    target
        .load_world(&store, &LoadSource::Latest, false)
        .unwrap();
    assert_eq!(
        target.world().step_count,
        0,
        "run-time state is not restored"
    );
    for ball in &target.world().balls {
        assert!(
            ball.current_cell.is_some(),
            "ball {} not in a cell",
            ball.get_id()
        );
    }
    for _ in 0..50 {
        target.simulation_step().unwrap();
    }
    for ball in &target.world().balls {
        assert!(ball.get_position().is_finite());
    }
    let _ = fs::remove_dir_all(store.dir());
}

#[test]
fn ids_continue_after_the_restored_ones() {
    let store = SaveStore::new(scratch_dir("ids"));
    busy_world().save_world(&store).unwrap();
    let mut target = empty_world();
    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();

    target.add_candidate_at_pos(Vec2D::new(0.0, -8.0)).unwrap();
    let mut ids: Vec<usize> = target.world().balls.iter().map(|b| b.get_id()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![10, 11, 12, 13, 14]);

    target.add_thermostat(Vec2D::new(0.0, 8.0));
    assert_eq!(target.world().thermostats.thermostats()[1].id, 1);
    let _ = fs::remove_dir_all(store.dir());
}

#[test]
fn invalid_file_leaves_the_running_world_untouched() {
    let store = SaveStore::new(scratch_dir("invalid"));
    let mut file = capture(&busy_world());
    let duplicate = file.balls[0].clone();
    file.balls.push(duplicate);
    fs::create_dir_all(store.dir()).unwrap();
    let path = store.dir().join("world_invalid.json");
    fs::write(&path, file.to_json().unwrap()).unwrap();

    let mut target = empty_world();
    target.add_candidate_at_pos(Vec2D::new(1.0, 1.0)).unwrap();
    let before = capture(&target);
    assert!(
        target
            .load_world(&store, &LoadSource::Path(path), true)
            .is_err()
    );
    assert_eq!(capture(&target), before);
    let _ = fs::remove_dir_all(store.dir());
}

#[test]
fn latest_and_explicit_path() {
    let store = SaveStore::new(scratch_dir("paths"));
    let first = busy_world().save_world(&store).unwrap();
    let mut changed = busy_world();
    changed.add_candidate_at_pos(Vec2D::new(0.0, -8.0)).unwrap();
    let second = changed.save_world(&store).unwrap();

    let mut target = empty_world();
    let loaded = target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    assert_eq!(loaded.path, second);
    assert_eq!(target.world().balls.len(), 5);
    let loaded = target
        .load_world(&store, &LoadSource::Path(first.clone()), true)
        .unwrap();
    assert_eq!(loaded.path, first);
    assert_eq!(loaded.world_rect, target.world().world_region());
    assert_eq!(target.world().balls.len(), 4);
    let _ = fs::remove_dir_all(store.dir());
}

#[test]
fn loading_without_any_save_reports_an_error() {
    let store = SaveStore::new(scratch_dir("empty"));
    let error = empty_world()
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap_err();
    assert!(format!("{error:?}").contains("no saved world"));
}

fn next_event(thread: &PhysicsThread) -> PhysicsEvent {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(event) = thread.try_recv_event() {
            return event;
        }
        assert!(
            Instant::now() < deadline,
            "no event from the physics thread"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn physics_thread_reports_save_and_load_results() {
    let store = SaveStore::new(scratch_dir("thread"));
    let thread = PhysicsThread::spawn(busy_world().with_save_store(store.clone()), Box::new(|| {}));

    thread.send_command(WorldCommand::Save).unwrap();
    let PhysicsEvent::WorldSaved(path) = next_event(&thread) else {
        panic!("expected WorldSaved");
    };
    assert!(path.starts_with(store.dir()));

    thread
        .send_command(WorldCommand::Load(LoadSource::Latest))
        .unwrap();
    let PhysicsEvent::WorldLoaded(loaded) = next_event(&thread) else {
        panic!("expected WorldLoaded");
    };
    assert_eq!(loaded.path, path);

    thread
        .send_command(WorldCommand::Load(LoadSource::Path(
            store.dir().join("missing.json"),
        )))
        .unwrap();
    assert!(matches!(next_event(&thread), PhysicsEvent::LoadFailed(_)));

    thread.shutdown_and_join().unwrap();
    let _ = fs::remove_dir_all(store.dir());
}

#[test]
fn without_a_save_store_save_and_load_fail_cleanly() {
    let thread = PhysicsThread::spawn(empty_world(), Box::new(|| {}));
    thread.send_command(WorldCommand::Save).unwrap();
    assert!(matches!(next_event(&thread), PhysicsEvent::SaveFailed(_)));
    thread
        .send_command(WorldCommand::Load(LoadSource::Latest))
        .unwrap();
    assert!(matches!(next_event(&thread), PhysicsEvent::LoadFailed(_)));
    thread.shutdown_and_join().unwrap();
}

#[test]
fn every_load_starts_a_new_world_generation() {
    let store = SaveStore::new(scratch_dir("generation"));
    busy_world().save_world(&store).unwrap();
    let mut target = empty_world();
    assert_eq!(target.create_snapshot().world_generation, 0);

    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    assert_eq!(target.create_snapshot().world_generation, 1);
    // Loading the same save again is still a new world for the renderer,
    // although its step count (0) is the same.
    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    assert_eq!(target.create_snapshot().world_generation, 2);

    // A failed load keeps the current world and its generation.
    assert!(
        target
            .load_world(&store, &LoadSource::Path("missing.json".into()), true)
            .is_err()
    );
    assert_eq!(target.create_snapshot().world_generation, 2);
    let _ = fs::remove_dir_all(store.dir());
}
