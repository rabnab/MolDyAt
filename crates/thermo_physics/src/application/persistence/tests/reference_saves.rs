// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reference saves: one file per format version, written by the build that
//! introduced that version. They are never edited or regenerated. A format
//! change adds a new fixture, and every older one must keep loading through
//! the real load path.

use super::{builtin_species, scratch_dir};
use crate::application::persistence::migrate;
use crate::application::persistence::{LoadSource, SaveStore, WORLD_FILE_VERSION, WorldFile};
use crate::application::sim_config::{
    FieldConfig, FieldKind, WallModel, WallOverrideConfig, WallsConfig,
};
use crate::application::units::{UnitScale, UnitSystem};
use crate::application::{MainContent, SimConfig};
use crate::objects2d::ball_impl::{SpeciesId, SpeciesTable};
use crate::objects2d::operator::OperatorState;
use crate::objects2d::wall_chain::Joint;
use crate::objects2d::{Rect, Vec2D};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// A reference save: its format version, file name and content checks.
struct Fixture {
    version: u32,
    file: &'static str,
    check: fn(&MainContent),
}

/// Every reference save, oldest first.
const FIXTURES: &[Fixture] = &[
    Fixture {
        version: 1,
        file: "v1.json",
        check: check_v1,
    },
    Fixture {
        version: 2,
        file: "v2.json",
        check: check_v2,
    },
    Fixture {
        version: 3,
        file: "v3.json",
        check: check_v2,
    },
    Fixture {
        version: 4,
        file: "v4.json",
        check: check_v4,
    },
    Fixture {
        version: 5,
        file: "v5.json",
        check: check_v5,
    },
    Fixture {
        version: 6,
        file: "v6.json",
        check: check_v5,
    },
    Fixture {
        version: 7,
        file: "v7.json",
        check: check_v5,
    },
    Fixture {
        version: 8,
        file: "v8.json",
        check: check_v5,
    },
    Fixture {
        version: 9,
        file: "v9.json",
        check: check_v5,
    },
];

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/application/persistence/tests/fixtures")
}

/// Loads `name` the way `--load` and Ctrl+O do (`SaveStore::load` validates,
/// then the world is built and swapped in).
fn load_fixture(name: &str) -> MainContent {
    let store = SaveStore::new(scratch_dir("reference"));
    let mut mc = MainContent::new(Rect::new(-10.0, 10.0, 100.0, -100.0)).unwrap();
    mc.load_world(&store, &LoadSource::Path(fixtures_dir().join(name)), true)
        .unwrap_or_else(|e| panic!("{name} does not load: {e:?}"));
    mc
}

#[test]
fn every_reference_save_loads() {
    for fixture in FIXTURES {
        (fixture.check)(&load_fixture(fixture.file));
    }
}

#[test]
fn reference_saves_have_the_version_they_are_listed_with() {
    for fixture in FIXTURES {
        let json = fs::read_to_string(fixtures_dir().join(fixture.file)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], fixture.version, "{}", fixture.file);
    }
}

#[test]
fn current_format_version_has_a_reference_save() {
    assert!(
        FIXTURES.iter().any(|f| f.version == WORLD_FILE_VERSION),
        "WORLD_FILE_VERSION {WORLD_FILE_VERSION} has no reference save: add one (see module doc)"
    );
}

#[test]
fn every_fixture_file_is_listed() {
    for entry in fs::read_dir(fixtures_dir()).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        assert!(
            FIXTURES.iter().any(|f| f.file == name),
            "{name} is not in FIXTURES"
        );
    }
}

#[test]
fn reference_saves_survive_a_save_and_load() {
    for fixture in FIXTURES {
        let mc = load_fixture(fixture.file);
        let file = WorldFile::capture(&mc);
        let reloaded = WorldFile::from_json(&file.to_json().unwrap()).unwrap();
        assert_eq!(reloaded, file, "{}", fixture.file);
    }
}

/// The newest format needs no migration, so its reference save must come
/// back field for field. Older ones are checked by their `check_vN`. Fields
/// added since (`config`, #231) come back with their defaults.
#[test]
fn current_reference_save_loads_every_field() {
    for fixture in FIXTURES.iter().filter(|f| f.version == WORLD_FILE_VERSION) {
        let json = fs::read_to_string(fixtures_dir().join(fixture.file)).unwrap();
        let expected = WorldFile::from_json(&json).unwrap();
        let mc = load_fixture(fixture.file);
        assert_eq!(WorldFile::capture(&mc), expected, "{}", fixture.file);
    }
}

/// `WorldFile` as a later build might extend it the additive way (#271): a
/// new field whose default reproduces today's behaviour and a new top-level
/// list. Old files lack both and must still load.
#[derive(Debug, Deserialize)]
struct ExtendedWorldFile {
    #[serde(flatten)]
    file: WorldFile,
    /// Today walls reflect elastically, so the default is 1.
    #[serde(default = "elastic")]
    wall_restitution: f32,
    /// A new operator kind; older files have none.
    #[serde(default)]
    sinks: Vec<SinkData>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct SinkData {
    id: usize,
    center: Vec2D,
}

fn elastic() -> f32 {
    1.0
}

fn read_fixture(file: &str) -> String {
    fs::read_to_string(fixtures_dir().join(file)).unwrap()
}

/// A save without a config runs with the defaults, but with the file's
/// top-level `time_step`, not the default one.
#[test]
fn config_less_save_keeps_its_time_step() {
    const TIME_STEP: f32 = 0.0025;
    assert_ne!(TIME_STEP, SimConfig::default().integration.time_step);
    let json = read_fixture("v2.json");
    assert!(json.contains("\"time_step\": 0.005,") && !json.contains("\"config\""));
    let json = json.replace(
        "\"time_step\": 0.005,",
        &format!("\"time_step\": {TIME_STEP},"),
    );
    let dir = scratch_dir("reference_time_step");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("v2_time_step.json");
    fs::write(&path, json).unwrap();

    let store = SaveStore::new(dir);
    let mut mc = MainContent::new(Rect::new(-10.0, 10.0, 100.0, -100.0)).unwrap();
    mc.load_world(&store, &LoadSource::Path(path), true)
        .unwrap();
    assert_eq!(mc.time_step(), TIME_STEP);
    let mut config = SimConfig {
        walls: WallsConfig::hard(),
        species: builtin_species(),
        ..SimConfig::default()
    };
    config.integration.time_step = TIME_STEP;
    assert_eq!(mc.config(), config);
}

/// A v5 save whose soft walls were at their default (no `walls` in its
/// config) keeps the physical `ε_w = 1 ε_N2` it ran with when converted to
/// Ar units (#405), like a save that wrote its `walls`.
#[test]
fn default_soft_walls_keep_their_epsilon_in_ar_units() {
    let mut file: serde_json::Value = serde_json::from_str(&read_fixture("v5.json")).unwrap();
    let config = file["config"].as_object_mut().expect("v5 has a config");
    assert!(config.remove("walls").is_some());
    let dir = scratch_dir("reference_default_walls");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("v5_default_walls.json");
    fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();

    let store = SaveStore::new(dir);
    let mut mc = MainContent::new(Rect::new(-10.0, 10.0, 100.0, -100.0)).unwrap();
    mc.load_world(&store, &LoadSource::Path(path), true)
        .unwrap();
    let s = UnitSystem::N2.scale_to(&UnitSystem::AR);
    let walls = mc.config().walls;
    assert_eq!(walls.model, WallModel::Soft);
    close(walls.epsilon, 1.0, s.energy, "default ε_w");
}

#[test]
fn old_saves_load_into_a_file_type_with_new_fields() {
    for fixture in FIXTURES {
        let json = read_fixture(fixture.file);
        let (extended, _): (ExtendedWorldFile, _) = migrate::parse(&json)
            .unwrap_or_else(|e| panic!("{} does not parse: {e:?}", fixture.file));
        extended.file.validate().unwrap();
        assert_eq!(extended.wall_restitution, 1.0, "{}", fixture.file);
        assert!(extended.sinks.is_empty(), "{}", fixture.file);
        // The defaulted fields change nothing about the rest of the file.
        assert_eq!(
            extended.file,
            WorldFile::from_json(&json).unwrap(),
            "{}",
            fixture.file
        );
    }
}

#[test]
fn new_fields_are_read_when_present() {
    let mut value: serde_json::Value = serde_json::from_str(&read_fixture("v2.json")).unwrap();
    value["wall_restitution"] = 0.5.into();
    value["sinks"] = serde_json::json!([{"id": 3, "center": {"x": 1.0, "y": -2.0}}]);
    let (extended, _): (ExtendedWorldFile, _) = migrate::parse(&value.to_string()).unwrap();
    assert_eq!(extended.wall_restitution, 0.5);
    assert_eq!(
        extended.sinks,
        vec![SinkData {
            id: 3,
            center: Vec2D::new(1.0, -2.0)
        }]
    );
}

fn species_name(id: SpeciesId) -> &'static str {
    &SpeciesTable::builtin().species(id).name
}

/// v1: written by the build that introduced the format. Its size classes
/// load as the species of the same name (`size_idx` 0 to 3: `XS` to `L`).
fn check_v1(mc: &MainContent) {
    check_world(
        mc,
        ["XS", "S", "M", "L", "XS", "S", "M", "L"],
        ["M", "S"],
        WallsConfig::hard(),
        Vec::new(),
    );
}

/// v2 (#234): the v1 world saved by the species build, with ball 14 and the
/// second source sphere changed to `N2` and ball 15 to `XL`. v3 (#274) is the
/// same world in the operator list layout, written by the build that
/// introduced it (`v2_upgrades_to_the_v3_reference_save`).
fn check_v2(mc: &MainContent) {
    check_world(
        mc,
        ["XS", "S", "M", "L", "N2", "XL", "M", "L"],
        ["M", "N2"],
        WallsConfig::hard(),
        Vec::new(),
    );
}

/// The walls of the v4 reference save: soft, with every key off its default.
fn v4_walls() -> WallsConfig {
    WallsConfig {
        model: WallModel::Soft,
        sigma: 0.5,
        epsilon: 2.0,
        attractive: true,
        backstop: 0.25,
        overrides: vec![WallOverrideConfig {
            species: "N2".into(),
            sigma: None,
            epsilon: Some(0.5),
        }],
    }
}

/// v4 (#280): the v3 world with a config whose `walls` set every key.
/// Written by the #280 build: `v3.json` loaded, `WorldFile::capture`d, its
/// `sim_config()` with `walls = v4_walls()` set as `config`, `to_json`.
fn check_v4(mc: &MainContent) {
    check_world(
        mc,
        ["XS", "S", "M", "L", "N2", "XL", "M", "L"],
        ["M", "N2"],
        v4_walls(),
        Vec::new(),
    );
}

/// The trap of the v5 reference save.
fn v5_fields() -> Vec<FieldConfig> {
    vec![FieldConfig {
        kind: FieldKind::Harmonic,
        center: [1.5, -2.0],
        stiffness: 0.05,
    }]
}

/// v5 (#347): the v4 world with a harmonic trap in `config.fields`. v6
/// (#405) is the same world in Ar units, written by the #405 build from
/// `v5.json` (loaded, `WorldFile::capture`d, `to_json`), and v7 (#409,
/// no `CH4` in it) by the #409 build from `v6.json` the same way, and v8
/// (#348, no rotating species in it) by the #348 build from `v7.json`.
/// Written by the #347 build: `v4.json` loaded, `WorldFile::capture`d, its
/// `sim_config()` with `fields = v5_fields()` set as `config`, `to_json`.
fn check_v5(mc: &MainContent) {
    check_world(
        mc,
        ["XS", "S", "M", "L", "N2", "XL", "M", "L"],
        ["M", "N2"],
        v4_walls(),
        v5_fields(),
    );
}

/// Content shared by the reference saves; they differ only in species and
/// walls.
fn check_world(
    mc: &MainContent,
    ball_species: [&str; 8],
    sphere_species: [&str; 2],
    walls: WallsConfig,
    fields: Vec<FieldConfig>,
) {
    let world = mc.world();
    // Every reference save holds the same world in N₂ units (v1 to v5) or
    // its rescaled form (v6); loaded, it is in Ar units (#405). The time step
    // keeps its number.
    let s = UnitSystem::N2.scale_to(&UnitSystem::AR);
    assert_eq!(mc.time_step(), 0.005);
    // Up to v3 none has a saved config (#231; v1 and v2 predate it, v3 is the
    // v2 world): they run with the defaults, hard walls (#280) and their own
    // time step. 0.005 is also the default now (#301);
    // `config_less_save_keeps_its_time_step` covers a step that differs.
    let mut config = SimConfig {
        walls: walls_in_ar(walls, &s),
        fields: fields_in_ar(fields, &s),
        species: builtin_species(),
        ..SimConfig::default()
    };
    config.integration.time_step = 0.005;
    assert_eq!(mc.config(), config);
    close(
        world.physics.constants.gravity,
        0.5,
        s.acceleration(),
        "gravity",
    );
    let rect = world.world_region();
    for (actual, n2) in [
        (rect.x, -20.0),
        (rect.y, 15.0),
        (rect.w, 40.0),
        (rect.h, -30.0),
    ] {
        close(actual, n2, s.length, "world_rect");
    }

    let balls: Vec<(usize, &str)> = world
        .balls
        .iter()
        .map(|b| (b.get_id(), species_name(b.species())))
        .collect();
    let expected: Vec<(usize, &str)> = (10..).zip(ball_species).collect();
    assert_eq!(balls, expected);
    // Ball i sits at (x_i, i - 4) with v = (0.1·i, -0.2 - 0.05·i); the saved
    // accelerations are the ones the build stored (gravity included).
    let xs = [-12.0, -6.0, 0.0, 6.0, 12.0, -3.0, 3.0, 8.0];
    let accels = [
        (0.01, -1.0),
        (0.02, -0.9375),
        (0.03, -0.875),
        (0.04, -0.8125),
        (0.049999997, -0.75),
        (0.06, -0.6875),
        (0.07, -0.625),
        (0.08, -0.5625),
    ];
    for (i, ball) in world.balls.iter().enumerate() {
        let f = i as f32;
        let what = format!("ball {i}");
        close_vec(
            *ball.get_position(),
            Vec2D::new(xs[i], f - 4.0),
            s.length,
            &what,
        );
        close_vec(
            ball.v,
            Vec2D::new(0.1 * f, -0.2 - 0.05 * f),
            s.speed(),
            &what,
        );
        close_vec(
            ball.accel,
            Vec2D::new(accels[i].0, accels[i].1),
            s.acceleration(),
            &what,
        );
        close_vec(
            ball.pre_update_pos,
            Vec2D::new(xs[i] - 0.01, f - 4.0 + 0.01),
            s.length,
            &what,
        );
    }

    let thicknesses: Vec<(usize, f32)> = world
        .line_segments
        .segments()
        .iter()
        .map(|seg| (seg.id, seg.thickness))
        .collect();
    let expected = [1.0, 1.0, 4.0, 4.0, 1.0, 1.0, 2.0, 1.5, 1.5, 1.5];
    assert_eq!(thicknesses.len(), expected.len());
    for ((id, thickness), (i, n2)) in thicknesses
        .into_iter()
        .zip(expected.into_iter().enumerate())
    {
        assert_eq!(id, i);
        close(thickness, n2, s.length, "thickness");
    }

    let valves: Vec<(usize, bool)> = world
        .valve_segments
        .valves()
        .iter()
        .map(|v| (v.id, v.is_closed))
        .collect();
    assert_eq!(valves, vec![(0, true), (1, false), (3, true)]);

    let chains: Vec<(usize, Vec<usize>, Vec<Joint>)> = world
        .wall_chains
        .chains()
        .map(|c| {
            let pieces = c.pieces.iter().map(|p| p.line_id).collect();
            (c.id, pieces, c.joints.clone())
        })
        .collect();
    assert_eq!(chains.len(), 5);
    assert_eq!(chains[0].1, vec![0, 1]);
    assert!(matches!(
        chains[0].2[..],
        [Joint::Valve { valve_id: 0, .. }]
    ));
    assert!(matches!(
        chains[1].2[..],
        [Joint::Valve { valve_id: 1, .. }]
    ));
    assert!(matches!(
        chains[2].2[..],
        [Joint::Gap { t_start, t_end }] if t_start == 0.25 && t_end == 0.75
    ));
    // A plain wall: one piece, no joints.
    assert_eq!(chains[3].1, vec![6]);
    assert!(chains[3].2.is_empty());
    // Three pieces: a closed valve, then a gap.
    assert_eq!(chains[4].1, vec![7, 8, 9]);
    assert_eq!(
        chains[4].2,
        vec![
            Joint::Valve {
                valve_id: 3,
                t_start: 0.1,
                t_end: 0.2
            },
            Joint::Gap {
                t_start: 0.6,
                t_end: 0.8
            }
        ]
    );
    let spans: Vec<(f32, f32)> = world
        .wall_chains
        .find(4)
        .unwrap()
        .pieces
        .iter()
        .map(|p| (p.t_start, p.t_end))
        .collect();
    assert_eq!(spans, vec![(0.0, 0.1), (0.2, 0.6), (0.8, 1.0)]);

    let spheres: Vec<_> = world
        .source_spheres
        .spheres()
        .iter()
        .map(|sphere| {
            (
                sphere.id,
                sphere.state,
                sphere.emit_period,
                species_name(sphere.ball_species),
            )
        })
        .collect();
    assert_eq!(
        spheres,
        vec![
            (0, OperatorState::On, 3000, sphere_species[0]),
            (1, OperatorState::Off, 1500, sphere_species[1])
        ]
    );
    for (sphere, n2) in world.source_spheres.spheres().iter().zip([0.05, 0.08]) {
        close(sphere.emission_speed, n2, s.speed(), "emission_speed");
    }
    close_vec(
        world.source_spheres.spheres()[0].center,
        Vec2D::new(-10.0, 0.0),
        s.length,
        "sphere centre",
    );

    let thermostats = world.thermostats.thermostats();
    let states: Vec<_> = thermostats.iter().map(|t| (t.id, t.state)).collect();
    assert_eq!(
        states,
        vec![(0, OperatorState::Off), (1, OperatorState::On)]
    );
    for (t, (temperature, radius, tau)) in
        thermostats.iter().zip([(0.25, 5.0, 0.5), (0.4, 3.5, 0.8)])
    {
        close(
            t.target_temperature,
            temperature,
            s.energy,
            "target_temperature",
        );
        close(t.radius, radius, s.length, "radius");
        close(t.tau, tau, s.time, "tau");
    }
    for (xi, n2) in thermostats[0]
        .chain
        .iter()
        .zip([0.0, 0.25, -0.5, 0.0, 0.125])
    {
        close(*xi as f32, n2, 1.0 / s.time, "chain");
    }
    close_vec(
        thermostats[1].position,
        Vec2D::new(-12.0, -3.0),
        s.length,
        "thermostat position",
    );
}

/// `actual` is `n2 · factor` (a value in N₂ units, converted to Ar units).
fn close(actual: f32, n2: f32, factor: f64, what: &str) {
    let expected = f64::from(n2) * factor;
    assert!(
        (f64::from(actual) - expected).abs() <= 1e-6 * expected.abs().max(1e-3),
        "{what}: {actual} vs {expected}"
    );
}

fn close_vec(actual: Vec2D, n2: Vec2D, factor: f64, what: &str) {
    close(actual.x, n2.x, factor, what);
    close(actual.y, n2.y, factor, what);
}

/// `walls` of a v4/v5 save after the migration to Ar units.
fn walls_in_ar(mut walls: WallsConfig, s: &UnitScale) -> WallsConfig {
    if walls.model == WallModel::Hard && walls == WallsConfig::hard() {
        return walls;
    }
    walls.sigma = (f64::from(walls.sigma) * s.length) as f32;
    walls.epsilon = (f64::from(walls.epsilon) * s.energy) as f32;
    for o in &mut walls.overrides {
        o.sigma = o.sigma.map(|x| (f64::from(x) * s.length) as f32);
        o.epsilon = o.epsilon.map(|x| (f64::from(x) * s.energy) as f32);
    }
    walls
}

/// `fields` of a v5 save after the migration to Ar units.
fn fields_in_ar(fields: Vec<FieldConfig>, s: &UnitScale) -> Vec<FieldConfig> {
    fields
        .into_iter()
        .map(|f| FieldConfig {
            center: f.center.map(|c| (f64::from(c) * s.length) as f32),
            stiffness: (f64::from(f.stiffness) * s.stiffness()) as f32,
            ..f
        })
        .collect()
}

/// Operators have their own file types since #270, in one list since #274.
/// The current reference save's operator list comes back unchanged after a
/// load and a save, so the layout does not move with the simulation types.
#[test]
fn operator_layout_is_unchanged_by_a_load_and_save() {
    let current = FIXTURES
        .iter()
        .find(|f| f.version == WORLD_FILE_VERSION)
        .expect("a reference save for the current version");
    let original: serde_json::Value = serde_json::from_str(&read_fixture(current.file)).unwrap();
    let mc = load_fixture(current.file);
    let written: serde_json::Value =
        serde_json::from_str(&WorldFile::capture(&mc).to_json().unwrap()).unwrap();
    let operators = original["operators"].as_array().expect("an operator list");
    for kind in ["wall_chain", "source_sphere", "thermostat"] {
        assert!(
            operators.iter().any(|entry| entry.get(kind).is_some()),
            "{} has a {kind}",
            current.file
        );
    }
    assert_eq!(written["operators"], original["operators"]);
}
