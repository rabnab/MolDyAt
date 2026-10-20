// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `SimConfig` (#231): defaults, the TOML file, validation, and how a
//! `MainContent` takes it up.

use crate::application::MainContent;
use crate::application::core::SimulationEngine;
use crate::application::sim_config::*;
use crate::error::AppError;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::pair_potential::PairOverride;
use crate::objects2d::ball_impl::{BallPhysics, DEFAULT_SPECIES, SpeciesTable};
use std::time::Duration;

fn rect() -> Rect {
    Rect::new(-50.0, 40.0, 100.0, -80.0)
}

/// The message of the error that rejects `text`, unescaped.
fn error_message(text: &str) -> String {
    let AppError::CustomError(message) =
        SimConfig::from_toml(text).expect_err("config must be rejected");
    message
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
    config.integration.time_step = 0.0025;
    config.integration.speed_limit_factor = 0.3;
    config.potential.max_force = 80.0;
    config.potential.core_stiffness = 150.0;
    config.potential.cutoff_factor = 3.5;
    config.neighbours.skin_factor = 0.25;
    config.display.snapshot_interval_us = 33_333;
    config
}

#[test]
fn defaults_are_the_documented_values() {
    let config = SimConfig::default();
    assert_eq!(config.integration.time_step, 0.005);
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
    assert_eq!(mc.time_step(), 0.0025);
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
    assert_eq!(mc.config().integration.time_step, 0.005);
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
         time_step = 0.0025\n\
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

/// Each key with its accepted range: just outside is rejected, the bounds
/// themselves are accepted.
#[test]
fn every_key_has_a_range() {
    type Set = fn(&mut SimConfig, f64);
    let keys: [(&str, f64, f64, Set); 7] = [
        (
            "integration.time_step",
            MIN_TIME_STEP.into(),
            MAX_TIME_STEP.into(),
            |c, v| c.integration.time_step = v as f32,
        ),
        (
            "integration.speed_limit_factor",
            MIN_SPEED_LIMIT_FACTOR.into(),
            MAX_SPEED_LIMIT_FACTOR.into(),
            |c, v| c.integration.speed_limit_factor = v as f32,
        ),
        (
            "potential.max_force",
            MIN_MAX_FORCE.into(),
            MAX_MAX_FORCE.into(),
            |c, v| c.potential.max_force = v as f32,
        ),
        (
            "potential.core_stiffness",
            MIN_CORE_STIFFNESS.into(),
            MAX_CORE_STIFFNESS.into(),
            |c, v| c.potential.core_stiffness = v as f32,
        ),
        (
            "potential.cutoff_factor",
            MIN_CUTOFF_FACTOR.into(),
            MAX_CUTOFF_FACTOR.into(),
            |c, v| c.potential.cutoff_factor = v as f32,
        ),
        (
            "neighbours.skin_factor",
            MIN_SKIN_FACTOR.into(),
            MAX_SKIN_FACTOR.into(),
            |c, v| c.neighbours.skin_factor = v as f32,
        ),
        (
            "display.snapshot_interval_us",
            MIN_SNAPSHOT_INTERVAL_US as f64,
            MAX_SNAPSHOT_INTERVAL_US as f64,
            |c, v| c.display.snapshot_interval_us = v as u64,
        ),
    ];
    for (key, min, max, set) in keys {
        for value in [min, max] {
            let mut config = SimConfig::default();
            set(&mut config, value);
            assert!(
                config.validate().is_ok(),
                "{key} = {value} must be accepted"
            );
        }
        // Below the minimum (or -1 when it is 0), and 1% above the maximum.
        let below = if min > 0.0 { min * 0.99 } else { -1.0 };
        for value in [below, max * 1.01] {
            let mut config = SimConfig::default();
            set(&mut config, value);
            let error = format!("{:?}", config.validate().expect_err(key));
            assert!(error.contains(key), "{key} = {value}: {error}");
        }
    }
}

/// The review's examples of values that would make a run crawl or freeze.
#[test]
fn extreme_values_are_rejected() {
    for (text, key) in [
        ("[integration]\ntime_step = 5.0", "time_step"),
        ("[potential]\ncutoff_factor = 1000.0", "cutoff_factor"),
        ("[potential]\nmax_force = 1e30", "max_force"),
        ("[neighbours]\nskin_factor = 500.0", "skin_factor"),
        ("[neighbours]\nskin_factor = 1e-9", "skin_factor"),
        (
            "[display]\nsnapshot_interval_us = 18446744073709551615",
            "snapshot_interval_us",
        ),
    ] {
        let error = error_text(text);
        assert!(error.contains(key), "{text}: {error}");
    }
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

// ── Pair overrides (#235) ────────────────────────────────────────

fn pair(a: &str, b: &str, sigma: Option<f32>, epsilon: Option<f32>) -> PairOverrideConfig {
    PairOverrideConfig {
        a: a.into(),
        b: b.into(),
        sigma,
        epsilon,
    }
}

fn with_pairs(pairs: Vec<PairOverrideConfig>) -> SimConfig {
    let mut config = SimConfig::default();
    config.potential.pair_overrides = pairs;
    config
}

#[test]
fn pair_overrides_are_read() {
    let config = SimConfig::from_toml(
        "[[potential.pair_override]]\n\
         a = \"N2\"\n\
         b = \"XS\"\n\
         sigma = 1.1\n\
         epsilon = 1.2\n\
         [[potential.pair_override]]\n\
         a = \"XS\"\n\
         b = \"XS\"\n\
         epsilon = 1.5\n",
    )
    .unwrap();
    assert_eq!(
        config,
        with_pairs(vec![
            pair("N2", "XS", Some(1.1), Some(1.2)),
            pair("XS", "XS", None, Some(1.5)),
        ])
    );
}

#[test]
fn main_content_runs_with_its_pair_overrides() {
    let config = with_pairs(vec![
        pair("XS", "N2", Some(1.1), Some(1.2)),
        pair("M", "M", None, Some(0.5)),
    ]);
    let mc = MainContent::with_config(rect(), &config, None).unwrap();
    assert_eq!(mc.config(), config);
    let species = SpeciesTable::builtin();
    let id = |name| species.by_name(name).unwrap();
    assert_eq!(
        mc.world().physics.pair_overrides(),
        [
            PairOverride {
                a: id("XS"),
                b: id("N2"),
                sigma: Some(1.1),
                epsilon: Some(1.2)
            },
            PairOverride {
                a: id("M"),
                b: id("M"),
                sigma: None,
                epsilon: Some(0.5)
            },
        ]
    );
    assert_eq!(mc.world().physics.cutoff(id("N2"), id("XS")), 4.0 * 1.1);
}

#[test]
fn no_pair_overrides_are_not_written() {
    let json = serde_json::to_string(&SimConfig::default()).unwrap();
    assert!(!json.contains("pair_override"), "{json}");
    let json = serde_json::to_string(&with_pairs(vec![pair("N2", "N2", Some(1.1), None)])).unwrap();
    assert!(json.contains("pair_override"), "{json}");
    assert!(
        !json.contains("epsilon"),
        "an unset value is left out: {json}"
    );
}

#[test]
fn invalid_pair_overrides_are_rejected() {
    for (pairs, expected) in [
        (
            vec![pair("N2", "Kr", Some(1.1), None)],
            "potential.pair_override[0].b: unknown species \"Kr\" (known: N2, XS, S, M, L, XL, Ar, Ne, CH4, O2)",
        ),
        (
            vec![pair("n2", "N2", Some(1.1), None)],
            "potential.pair_override[0].a: unknown species \"n2\"",
        ),
        (
            vec![pair("N2", "XS", None, None)],
            "potential.pair_override[0] (N2 – XS) sets neither sigma nor epsilon",
        ),
        (
            vec![pair("N2", "XS", Some(f32::NAN), None)],
            "potential.pair_override[0].sigma must be within",
        ),
        (
            vec![pair("N2", "XS", None, Some(-1.0))],
            "potential.pair_override[0].epsilon must be within",
        ),
        (
            vec![pair("N2", "XS", None, Some(f32::INFINITY))],
            "potential.pair_override[0].epsilon must be within",
        ),
        (
            vec![
                pair("N2", "N2", Some(1.1), None),
                pair("N2", "XS", Some(1.1), None),
                pair("XS", "N2", None, Some(2.0)),
            ],
            "potential.pair_override[2] overrides the pair XS – N2 again (first in potential.pair_override[1])",
        ),
    ] {
        let config = with_pairs(pairs);
        let AppError::CustomError(error) = config.validate().expect_err(expected);
        assert!(error.contains(expected), "expected {expected:?} in {error}");
        assert!(MainContent::with_config(rect(), &config, None).is_err());
    }
}

#[test]
fn pair_override_values_have_a_range() {
    for (sigma, epsilon) in [
        (MIN_PAIR_SIGMA, MIN_PAIR_EPSILON),
        (MAX_PAIR_SIGMA, MAX_PAIR_EPSILON),
    ] {
        let config = with_pairs(vec![pair("N2", "XS", Some(sigma), Some(epsilon))]);
        assert!(config.validate().is_ok(), "{sigma}, {epsilon}");
    }
    for (sigma, epsilon, key) in [
        (MIN_PAIR_SIGMA * 0.99, 1.0, "sigma"),
        (MAX_PAIR_SIGMA * 1.01, 1.0, "sigma"),
        (1.0, MIN_PAIR_EPSILON * 0.99, "epsilon"),
        (1.0, MAX_PAIR_EPSILON * 1.01, "epsilon"),
    ] {
        let config = with_pairs(vec![pair("N2", "XS", Some(sigma), Some(epsilon))]);
        let error = format!("{:?}", config.validate().expect_err(key));
        assert!(
            error.contains(&format!("potential.pair_override[0].{key}")),
            "{error}"
        );
    }
}

#[test]
fn unknown_keys_in_a_pair_override_are_rejected() {
    let error = error_text(
        "[[potential.pair_override]]\n\
         a = \"N2\"\n\
         b = \"XS\"\n\
         sigma_ij = 1.1\n",
    );
    assert!(error.contains("sigma_ij"), "{error}");
    let error = error_text("[[potential.pair_override]]\nsigma = 1.1\n");
    assert!(error.contains("missing field"), "{error}");
}

/// The commented-out override in the README example is a valid config.
#[test]
fn readme_pair_override_example_is_valid() {
    let readme = include_str!("../../../../README.md");
    let start = readme.find("```toml\n").expect("README has a TOML example") + "```toml\n".len();
    let end = start + readme[start..].find("```").unwrap();
    let uncommented: String = readme[start..end]
        .lines()
        .map(|line| line.strip_prefix("# ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    let config = SimConfig::from_toml(&uncommented).unwrap();
    assert_eq!(config.potential.pair_overrides.len(), 1, "{uncommented}");
    let names: Vec<_> = config.species.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["N2", "Ar"], "{uncommented}");
}

// ── [[species]] (#237) ───────────────────────────────────────────────

const N2_AR: &str = "[[species]]\n\
     name = \"N2\"\n\
     sigma_angstrom = 3.698\n\
     epsilon_kelvin = 95.05\n\
     mass_u = 28.0134\n\
     colour = [0.0, 1.0, 1.0]\n\
     source = \"Hirschfelder, Curtiss & Bird (1954)\"\n\
     [[species]]\n\
     name = \"Ar\"\n\
     sigma_angstrom = 3.405\n\
     epsilon_kelvin = 119.8\n\
     mass_u = 39.948\n\
     colour = [0.6, 0.45, 1.0, 0.5]\n\
     display_radius_angstrom = 1.849\n";

#[test]
fn species_list_replaces_the_table() {
    let config = SimConfig::from_toml(N2_AR).unwrap();
    let table = config.species_table().unwrap();
    let names: Vec<_> = table.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["N2", "Ar"]);
    let n2 = table.species(table.by_name("N2").unwrap());
    assert_eq!(n2, SpeciesTable::builtin().species(DEFAULT_SPECIES));
    let ar = table.species(table.by_name("Ar").unwrap());
    let builtin_ar = SpeciesTable::builtin().by_name("Ar").unwrap();
    let builtin_ar = SpeciesTable::builtin().species(builtin_ar);
    assert_eq!(
        (ar.sigma, ar.epsilon, ar.mass),
        (builtin_ar.sigma, builtin_ar.epsilon, builtin_ar.mass),
        "same literature values as the library"
    );
    assert_eq!(ar.colour, [0.6, 0.45, 1.0, 0.5]);
    assert_eq!(ar.display_radius, 0.5, "1.849 Å is half the N2 sigma");
    assert_eq!(n2.colour[3], 1.0, "RGB gets an opaque alpha");
}

#[test]
fn without_species_the_builtin_library_applies() {
    let table = SimConfig::default().species_table().unwrap();
    assert_eq!(&*table, SpeciesTable::builtin());
    for name in ["N2", "XS", "S", "M", "L", "XL", "Ar", "Ne", "CH4", "O2"] {
        assert!(table.by_name(name).is_some(), "{name}");
    }
}

/// The world, its new balls and `config()` follow the species list.
#[test]
fn main_content_runs_with_its_species() {
    let config = SimConfig::from_toml(N2_AR).unwrap();
    let mut mc = MainContent::with_config(rect(), &config, Some(3)).unwrap();
    assert_eq!(mc.config(), config, "config() returns the list as given");
    mc.seed_random_mixture(40, 0.0, &[("N2".into(), 1), ("Ar".into(), 1)])
        .unwrap();
    let snapshot = mc.create_snapshot();
    let count = |name: &str| {
        let id = snapshot.species.by_name(name).unwrap();
        snapshot.balls.iter().filter(|b| b.species == id).count()
    };
    assert_eq!((count("N2"), count("Ar")), (20, 20));
    let ar = config.species_table().unwrap();
    let ar = ar.species(ar.by_name("Ar").unwrap()).clone();
    for ball in mc.world().balls.iter().filter(|b| b.species().index() == 1) {
        assert_eq!(ball.get_mass(), ar.mass);
        assert_eq!(ball.get_contact_radius(), ar.contact_radius());
    }
    let AppError::CustomError(error) = mc
        .seed_random_mixture(1, 0.0, &[("XS".into(), 1)])
        .expect_err("XS is not in the list");
    assert!(
        error.contains("unknown species \"XS\" (known: N2, Ar)"),
        "{error}"
    );
}

#[test]
fn species_entries_need_every_required_key() {
    for key in [
        "name",
        "sigma_angstrom",
        "epsilon_kelvin",
        "mass_u",
        "colour",
    ] {
        let text: String = N2_AR
            .lines()
            .take(7)
            .filter(|line| !line.starts_with(&format!("{key} =")))
            .map(|line| format!("{line}\n"))
            .collect();
        let error = error_message(&text);
        assert!(
            error.contains(&format!("missing field `{key}`")),
            "{key}: {error}"
        );
    }
}

#[test]
fn invalid_species_entries_are_rejected() {
    let entry = |name: &str, sigma: f64, epsilon: f64, mass: f64, colour: &str| {
        format!(
            "[[species]]\nname = \"{name}\"\nsigma_angstrom = {sigma}\n\
             epsilon_kelvin = {epsilon}\nmass_u = {mass}\ncolour = {colour}\n"
        )
    };
    let ok = entry("N2", 3.698, 95.05, 28.0134, "[0.0, 1.0, 1.0]");
    for (text, expected) in [
        (
            entry("", 3.698, 95.05, 28.0134, "[0.0, 1.0, 1.0]"),
            "species[0].name must not be empty",
        ),
        (
            format!("{ok}{ok}"),
            "species[1].name \"N2\" is used twice (first in species[0])",
        ),
        (
            entry("Tiny", 1.0, 95.05, 28.0134, "[0.0, 1.0, 1.0]"),
            "species[0].sigma_angstrom must be within 1.8490..=29.5840, got 1",
        ),
        (
            entry("Hot", 3.698, 5000.0, 28.0134, "[0.0, 1.0, 1.0]"),
            "species[0].epsilon_kelvin must be within",
        ),
        (
            entry("Light", 3.698, 95.05, 1.0, "[0.0, 1.0, 1.0]"),
            "species[0].mass_u must be within",
        ),
        (
            entry("Nan", f64::NAN, 95.05, 28.0134, "[0.0, 1.0, 1.0]").replace("NaN", "nan"),
            "species[0].sigma_angstrom must be within",
        ),
        (
            entry("Grey", 3.698, 95.05, 28.0134, "[0.5, 0.5]"),
            "species[0].colour needs 3 (RGB) or 4 (RGBA) values, got 2",
        ),
        (
            entry("Bright", 3.698, 95.05, 28.0134, "[2.0, 0.5, 0.5]"),
            "species[0].colour values must be within 0..=1",
        ),
        (
            format!("{ok}display_radius_angstrom = 0.0\n"),
            "species[0].display_radius_angstrom must be within",
        ),
        (format!("{ok}radius = 1.0\n"), "unknown field `radius`"),
    ] {
        let error = error_message(&text);
        assert!(error.contains(expected), "expected {expected:?} in {error}");
    }
}

#[test]
fn a_species_table_holds_at_most_64_species() {
    let entry = |i: usize| {
        format!(
            "[[species]]\nname = \"S{i}\"\nsigma_angstrom = 3.698\n\
             epsilon_kelvin = 95.05\nmass_u = 28.0134\ncolour = [1.0, 1.0, 1.0]\n"
        )
    };
    let text = |n: usize| (0..n).map(entry).collect::<String>();
    assert!(SimConfig::from_toml(&text(64)).is_ok());
    assert!(error_message(&text(65)).contains("at most 64"));
}

/// Pair overrides name species of the config's own table.
#[test]
fn pair_overrides_resolve_against_the_species_list() {
    let ok = format!("{N2_AR}[[potential.pair_override]]\na = \"N2\"\nb = \"Ar\"\nsigma = 1.0\n");
    let config = SimConfig::from_toml(&ok).unwrap();
    let resolved = config.validated_species().unwrap();
    assert_eq!(resolved.pair_overrides.len(), 1);
    assert_eq!(resolved.table.len(), 2);

    let missing =
        format!("{N2_AR}[[potential.pair_override]]\na = \"N2\"\nb = \"XS\"\nsigma = 1.0\n");
    assert!(
        error_message(&missing)
            .contains("potential.pair_override[0].b: unknown species \"XS\" (known: N2, Ar)")
    );
}

#[test]
fn no_species_are_not_written() {
    let json = serde_json::to_string(&SimConfig::default()).unwrap();
    assert!(!json.contains("species"), "{json}");
    let config = SimConfig::from_toml(N2_AR).unwrap();
    let json = serde_json::to_string(&config).unwrap();
    let back: SimConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back, config);
}
