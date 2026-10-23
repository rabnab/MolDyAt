// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::scratch_dir;
use crate::application::persistence::{LoadSource, SaveStore, WorldFile};
use crate::application::sim_config::PairOverrideConfig;
use crate::application::{MainContent, SimConfig};
use crate::objects2d::ball_impl::{Ball2D, SpeciesId, SpeciesTable};
use crate::objects2d::{Rect, Vec2D};
use std::fs;

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

#[test]
fn every_load_starts_a_new_world_generation() {
    let store = SaveStore::new(scratch_dir("generation"));
    busy_world().save_world(&store).unwrap();
    let mut target = empty_world();
    assert_eq!(target.runtime().world_generation(), 0);

    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    assert_eq!(target.runtime().world_generation(), 1);
    // Loading the same save again is still a new world for the renderer,
    // although its step count (0) is the same.
    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    assert_eq!(target.runtime().world_generation(), 2);

    // A failed load keeps the current world and its generation.
    assert!(
        target
            .load_world(&store, &LoadSource::Path("missing.json".into()), true)
            .is_err()
    );
    assert_eq!(target.runtime().world_generation(), 2);
    let _ = fs::remove_dir_all(store.dir());
}

/// A config with its own species list: N2 and Ar.
fn n2_ar_config() -> SimConfig {
    SimConfig::from_toml(
        "[[species]]\n\
         name = \"N2\"\n\
         sigma_angstrom = 3.698\n\
         epsilon_kelvin = 95.05\n\
         mass_u = 28.0134\n\
         colour = [0.0, 1.0, 1.0]\n\
         [[species]]\n\
         name = \"Ar\"\n\
         sigma_angstrom = 3.405\n\
         epsilon_kelvin = 119.8\n\
         mass_u = 39.948\n\
         colour = [0.6, 0.45, 1.0]\n\
         source = \"Hirschfelder, Curtiss & Bird (1954)\"\n",
    )
    .unwrap()
}

/// A save carries the run's species list; loading it into a world with the
/// built-in library switches to that list, for balls, source spheres, the
/// ball factory and the snapshot.
#[test]
fn species_list_survives_a_save_and_load() {
    let store = SaveStore::new(scratch_dir("species"));
    let mut source =
        MainContent::with_config(world_rect(), &n2_ar_config(), Some(5)).expect("create");
    source
        .seed_random_mixture(30, 0.0, &[("N2".into(), 2), ("Ar".into(), 1)])
        .unwrap();
    source.add_source_sphere(Vec2D::new(-5.0, -5.0));
    source.world_mut().cycle_source_sphere_species(0);
    let path = source.save_world(&store).unwrap();
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains("\"species\": \"Ar\""), "balls name Ar");
    assert!(
        text.contains("\"ball_species\": \"Ar\""),
        "the sphere emits Ar"
    );

    let mut target = empty_world();
    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    assert_eq!(capture(&target), capture(&source));
    assert_eq!(target.config().species, n2_ar_config().species);

    let world = target.world();
    let names: Vec<_> = world.species().iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["N2", "Ar"]);
    let ar = world.species().by_name("Ar").unwrap();
    let ar_mass = world.species().species(ar).mass;
    let ars: Vec<_> = world.balls.iter().filter(|b| b.species() == ar).collect();
    assert_eq!(ars.len(), 10);
    assert!(ars.iter().all(|b| b.get_mass() == ar_mass));
    assert_eq!(world.source_spheres.spheres()[0].ball_species, ar);
    assert_eq!(target.world().species().len(), 2);

    // Cycling the sphere's species wraps within the loaded table.
    target.world_mut().cycle_source_sphere_species(0);
    let n2 = target.world().species().by_name("N2").unwrap();
    assert_eq!(target.world().source_spheres.spheres()[0].ball_species, n2);
    let _ = fs::remove_dir_all(store.dir());
}

/// Loading a world with the built-in library after one with its own list
/// switches the ball factory back.
#[test]
fn loading_a_builtin_save_restores_the_builtin_library() {
    let store = SaveStore::new(scratch_dir("species-back"));
    busy_world().save_world(&store).unwrap();
    let mut target = MainContent::with_config(world_rect(), &n2_ar_config(), None).expect("create");
    target.next_species();
    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    assert_eq!(&**target.world().species(), SpeciesTable::builtin());
    assert_eq!(
        target.new_ball_species(),
        SpeciesId::new(0),
        "back to the default"
    );
    let _ = fs::remove_dir_all(store.dir());
}

/// A load with the same species table keeps the species of new balls.
#[test]
fn loading_with_the_same_table_keeps_the_species_of_new_balls() {
    let store = SaveStore::new(scratch_dir("species-same"));
    busy_world().save_world(&store).unwrap();
    let mut target = empty_world();
    target.next_species();
    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    assert_eq!(target.new_ball_species(), SpeciesId::new(1));
    let _ = fs::remove_dir_all(store.dir());
}

/// Display and stepping options belong to the run, not the world (#305): a
/// load keeps them, and saves do not carry them.
#[test]
fn runtime_options_survive_a_load() {
    let store = SaveStore::new(scratch_dir("options"));
    let path = busy_world().save_world(&store).unwrap();
    let text = fs::read_to_string(path).unwrap();
    for key in [
        "real_data",
        "step_by_step",
        "show_timings",
        "show_ball_stats",
    ] {
        assert!(!text.contains(key), "{key} must not be saved");
    }

    let mut target = empty_world();
    target.toggle_real_data();
    target.toggle_stepwise();
    target.toggle_timing_display();
    target.toggle_show_ball_stats();
    let before = *target.options();
    assert_ne!(before, Default::default());
    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    assert_eq!(*target.options(), before);
    let _ = fs::remove_dir_all(store.dir());
}

#[test]
fn load_during_a_resize_pause_keeps_the_loaded_world_paused() {
    let dir = scratch_dir("load_during_resize");
    let store = SaveStore::new(&dir);
    empty_world().save_world(&store).unwrap();

    let mut target = empty_world();
    if target.world().sim.is_paused() {
        target.toggle_pause();
    }
    target.pause_for_resize();
    target
        .load_world(&store, &LoadSource::Latest, true)
        .unwrap();
    // The resize ends after the load: it must not resume the loaded world.
    target.unpause_after_resize();
    assert!(target.world().sim.is_paused());
    let _ = fs::remove_dir_all(&dir);
}
