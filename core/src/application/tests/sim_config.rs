// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `SimConfig` (#231): defaults, the TOML file, validation, and how a
//! `MainContent` takes it up.

use crate::application::MainContent;
use crate::application::core::SimulationEngine;
use crate::application::sim_config::SimConfig;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::BallPhysics;
use std::time::Duration;

fn rect() -> Rect {
    Rect::new(-50.0, 40.0, 100.0, -80.0)
}

fn error_text(text: &str) -> String {
    format!(
        "{:?}",
        SimConfig::from_toml(text).expect_err("config must be rejected")
    )
}

/// A config with every value changed from its default.
fn custom() -> SimConfig {
    let mut config = SimConfig::default();
    config.integration.time_step = 0.005;
    config.integration.speed_limit_factor = 0.3;
    config.potential.max_force = 80.0;
    config.potential.core_stiffness = 150.0;
    config.potential.cutoff_factor = 3.5;
    config.neighbours.skin_factor = 0.25;
    config.display.snapshot_interval_us = 33_333;
    config
}

#[test]
fn defaults_are_the_former_constants() {
    let config = SimConfig::default();
    assert_eq!(config.integration.time_step, 0.01);
    assert_eq!(config.integration.speed_limit_factor, 0.2);
    assert_eq!(config.potential.max_force, 100.0);
    assert_eq!(config.potential.core_stiffness, 200.0);
    assert_eq!(config.potential.cutoff_factor, 4.0);
    assert_eq!(config.neighbours.skin_factor, 0.2);
    assert_eq!(config.display.snapshot_interval_us, 16_667);
    assert_eq!(config.snapshot_interval(), Duration::from_micros(16_667));
    config.validate().expect("the defaults are valid");
}

#[test]
fn default_config_builds_the_default_physics() {
    let mc = MainContent::new(rect()).unwrap();
    assert_eq!(mc.config(), SimConfig::default());
    let physics = &mc.world().physics;
    let default = BallPhysics::default();
    assert_eq!(physics.cap_settings(), default.cap_settings());
    assert_eq!(
        physics.get_skin_thickness().to_bits(),
        default.get_skin_thickness().to_bits()
    );
    assert_eq!(physics.get_skin_thickness(), 0.2 * 4.0);
}

#[test]
fn main_content_runs_with_its_config() {
    let config = custom();
    let mc = MainContent::with_config(rect(), &config, Some(7)).unwrap();
    assert_eq!(mc.config(), config);
    assert_eq!(mc.time_step(), 0.005);
    let physics = &mc.world().physics;
    assert_eq!(physics.cap_settings().max_force, 80.0);
    assert_eq!(physics.get_skin_thickness(), 0.25 * 3.5);
    assert_eq!(
        SimulationEngine::default_speed_limit(&rect(), mc.world().speed_limit_factor),
        80.0 * 0.3
    );
    assert_eq!(
        mc.runtime().snapshot_interval(),
        Duration::from_micros(33_333)
    );
}

#[test]
fn config_follows_the_time_step_keys() {
    let mut mc = MainContent::with_config(rect(), &custom(), None).unwrap();
    mc.double_time_step();
    assert_eq!(mc.config().integration.time_step, 0.01);
}

#[test]
fn main_content_rejects_an_invalid_config() {
    let mut config = SimConfig::default();
    config.integration.time_step = -1.0;
    assert!(MainContent::with_config(rect(), &config, None).is_err());
}

#[test]
fn empty_file_is_the_default() {
    assert_eq!(SimConfig::from_toml("").unwrap(), SimConfig::default());
}

#[test]
fn missing_keys_keep_their_default() {
    let config = SimConfig::from_toml(
        "[potential]\n\
         max_force = 80.0\n",
    )
    .unwrap();
    let mut expected = SimConfig::default();
    expected.potential.max_force = 80.0;
    assert_eq!(config, expected);
}

#[test]
fn every_key_is_read() {
    let config = SimConfig::from_toml(
        "[integration]\n\
         time_step = 0.005\n\
         speed_limit_factor = 0.3\n\
         [potential]\n\
         max_force = 80.0\n\
         core_stiffness = 150.0\n\
         cutoff_factor = 3.5\n\
         [neighbours]\n\
         skin_factor = 0.25\n\
         [display]\n\
         snapshot_interval_us = 33333\n",
    )
    .unwrap();
    assert_eq!(config, custom());
}

/// The example in README.md ("Simulation config") must stay a valid file.
#[test]
fn readme_example_is_a_valid_config() {
    let readme = include_str!("../../../../README.md");
    let start = readme.find("```toml\n").expect("README has a TOML example") + "```toml\n".len();
    let end = start + readme[start..].find("```").unwrap();
    let config = SimConfig::from_toml(&readme[start..end]).unwrap();
    assert_eq!(
        config,
        SimConfig::default(),
        "the example lists the defaults"
    );
}

#[test]
fn unknown_keys_and_sections_are_rejected() {
    // A typo must not silently fall back to the default.
    assert!(error_text("[potential]\nmax_forc = 80.0\n").contains("max_forc"));
    assert!(error_text("[potentials]\nmax_force = 80.0\n").contains("potentials"));
    assert!(error_text("time_step = 0.005\n").contains("time_step"));
}

#[test]
fn wrong_types_are_rejected() {
    assert!(error_text("[integration]\ntime_step = \"fast\"\n").contains("invalid config"));
}

#[test]
fn values_out_of_range_are_rejected() {
    for (text, key) in [
        ("[integration]\ntime_step = 0.0", "time_step"),
        ("[integration]\ntime_step = nan", "time_step"),
        (
            "[integration]\nspeed_limit_factor = -0.1",
            "speed_limit_factor",
        ),
        ("[potential]\nmax_force = inf", "max_force"),
        ("[potential]\ncore_stiffness = -1.0", "core_stiffness"),
        ("[potential]\ncutoff_factor = 1.0", "cutoff_factor"),
        ("[neighbours]\nskin_factor = 0.0", "skin_factor"),
        (
            "[display]\nsnapshot_interval_us = 0",
            "snapshot_interval_us",
        ),
    ] {
        let error = error_text(text);
        assert!(error.contains(key), "{text}: {error}");
    }
}

#[test]
fn a_flat_core_is_allowed() {
    let config = SimConfig::from_toml("[potential]\ncore_stiffness = 0.0\n").unwrap();
    assert_eq!(config.potential.core_stiffness, 0.0);
}

#[test]
fn load_reads_a_file_and_reports_a_missing_one() {
    let dir = std::env::temp_dir().join(format!("mlj_sim_config_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, "[neighbours]\nskin_factor = 0.25\n").unwrap();
    assert_eq!(SimConfig::load(&path).unwrap().neighbours.skin_factor, 0.25);

    let missing = dir.join("missing.toml");
    let error = format!("{:?}", SimConfig::load(&missing).unwrap_err());
    assert!(error.contains("missing.toml"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}
