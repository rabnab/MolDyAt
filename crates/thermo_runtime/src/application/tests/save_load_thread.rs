// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Saving and loading through the physics thread (the world side is tested
//! in `thermo_physics` persistence tests).

use crate::application::SnapshotPublisher;
use crate::application::persistence::{LoadSource, SaveStore};
use crate::application::sim_config::PairOverrideConfig;
use crate::application::{MainContent, PhysicsEvent, PhysicsThread, SimConfig, WorldCommand};
use crate::objects2d::ball_impl::{Ball2D, SpeciesId};
use crate::objects2d::{Rect, Vec2D};
use std::fs;
use std::path::PathBuf;
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
    config.integration.time_step = 0.0025;
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
    mc.world_mut().thermostats_mut().find_mut(0).unwrap().chain[1] = 0.25;
    mc.world_mut().physics_mut().constants.gravity = 0.5;

    for (i, x) in [-6.0, -2.0, 2.0, 6.0].into_iter().enumerate() {
        let mut ball = Ball2D::new(10 + i, Vec2D::new(x, 0.0), SpeciesId::new((i % 3) as u16));
        ball.v = Vec2D::new(0.1 * i as f32, -0.2);
        ball.accel = Vec2D::new(0.01, -0.5);
        ball.pre_update_pos = Vec2D::new(x - 0.01, 0.01);
        mc.add_candidate_to_vec(ball);
    }
    mc
}

pub(super) fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mlj_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
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
    let thread = PhysicsThread::spawn(
        busy_world().with_save_store(store.clone()),
        SnapshotPublisher::new(),
        Box::new(|| {}),
    );

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
    let thread = PhysicsThread::spawn(empty_world(), SnapshotPublisher::new(), Box::new(|| {}));
    thread.send_command(WorldCommand::Save).unwrap();
    assert!(matches!(next_event(&thread), PhysicsEvent::SaveFailed(_)));
    thread
        .send_command(WorldCommand::Load(LoadSource::Latest))
        .unwrap();
    assert!(matches!(next_event(&thread), PhysicsEvent::LoadFailed(_)));
    thread.shutdown_and_join().unwrap();
}
