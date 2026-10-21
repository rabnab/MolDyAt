// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reference saves: one file per format version, written by the build that
//! introduced that version. They are never edited or regenerated. A format
//! change adds a new fixture, and every older one must keep loading through
//! the real load path.

use super::scratch_dir;
use crate::application::persistence::migrate;
use crate::application::persistence::{LoadSource, SaveStore, WORLD_FILE_VERSION, WorldFile};
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
        let mut expected = WorldFile::from_json(&json).unwrap();
        expected.config = Some(expected.sim_config());
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
    let mut config = SimConfig::default();
    config.integration.time_step = TIME_STEP;
    assert_eq!(mc.config(), config);
}

#[test]
fn old_saves_load_into_a_file_type_with_new_fields() {
    for fixture in FIXTURES {
        let json = read_fixture(fixture.file);
        let extended: ExtendedWorldFile = migrate::parse(&json)
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
    let extended: ExtendedWorldFile = migrate::parse(&value.to_string()).unwrap();
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
    check_world(mc, ["XS", "S", "M", "L", "XS", "S", "M", "L"], ["M", "S"]);
}

/// v2 (#234): the v1 world saved by the species build, with ball 14 and the
/// second source sphere changed to `N2` and ball 15 to `XL`.
fn check_v2(mc: &MainContent) {
    check_world(mc, ["XS", "S", "M", "L", "N2", "XL", "M", "L"], ["M", "N2"]);
}

/// Content shared by the reference saves; they differ only in species.
fn check_world(mc: &MainContent, ball_species: [&str; 8], sphere_species: [&str; 2]) {
    let world = mc.world();
    assert_eq!(mc.time_step(), 0.005);
    // Both predate the saved config (#231): they run with the defaults and
    // their own time step. 0.005 is also the default now (#301);
    // `config_less_save_keeps_its_time_step` covers a step that differs.
    let mut config = SimConfig::default();
    config.integration.time_step = 0.005;
    assert_eq!(mc.config(), config);
    assert_eq!(world.physics.constants.gravity, 0.5);
    assert_eq!(world.world_region(), Rect::new(-20.0, 15.0, 40.0, -30.0));

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
        assert_eq!(*ball.get_position(), Vec2D::new(xs[i], f - 4.0), "ball {i}");
        assert_eq!(ball.v, Vec2D::new(0.1 * f, -0.2 - 0.05 * f), "ball {i}");
        assert_eq!(ball.accel, Vec2D::new(accels[i].0, accels[i].1), "ball {i}");
        assert_eq!(
            ball.pre_update_pos,
            Vec2D::new(xs[i] - 0.01, f - 4.0 + 0.01),
            "ball {i}"
        );
    }

    let thicknesses: Vec<(usize, f32)> = world
        .line_segments
        .segments()
        .iter()
        .map(|s| (s.id, s.thickness))
        .collect();
    assert_eq!(
        thicknesses,
        vec![
            (0, 1.0),
            (1, 1.0),
            (2, 4.0),
            (3, 4.0),
            (4, 1.0),
            (5, 1.0),
            (6, 2.0),
            (7, 1.5),
            (8, 1.5),
            (9, 1.5)
        ]
    );

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
        .map(|s| {
            (
                s.id,
                s.state,
                s.emit_period,
                species_name(s.ball_species),
                s.emission_speed,
            )
        })
        .collect();
    assert_eq!(
        spheres,
        vec![
            (0, OperatorState::On, 3000, sphere_species[0], 0.05),
            (1, OperatorState::Off, 1500, sphere_species[1], 0.08)
        ]
    );
    assert_eq!(
        world.source_spheres.spheres()[0].center,
        Vec2D::new(-10.0, 0.0)
    );

    let thermostats = world.thermostats.thermostats();
    let params: Vec<_> = thermostats
        .iter()
        .map(|t| (t.id, t.state, t.target_temperature, t.radius, t.tau))
        .collect();
    assert_eq!(
        params,
        vec![
            (0, OperatorState::Off, 0.25, 5.0, 0.5),
            (1, OperatorState::On, 0.4, 3.5, 0.8)
        ]
    );
    assert_eq!(thermostats[0].chain, vec![0.0, 0.25, -0.5, 0.0, 0.125]);
    assert_eq!(thermostats[1].position, Vec2D::new(-12.0, -3.0));
}

/// Walls, valves and wall chains have their own file types since #270; the
/// current reference save's JSON for them comes back unchanged after a load
/// and a save, so the layout did not move with the types.
#[test]
fn wall_layout_is_unchanged_by_the_file_types() {
    let current = FIXTURES
        .iter()
        .find(|f| f.version == WORLD_FILE_VERSION)
        .expect("a reference save for the current version");
    let original: serde_json::Value = serde_json::from_str(&read_fixture(current.file)).unwrap();
    let mc = load_fixture(current.file);
    let written: serde_json::Value =
        serde_json::from_str(&WorldFile::capture(&mc).to_json().unwrap()).unwrap();
    for key in ["line_segments", "valves", "wall_chains"] {
        assert!(
            original[key].as_array().is_some_and(|a| !a.is_empty()),
            "{} has {key}",
            current.file
        );
        assert_eq!(written[key], original[key], "{key}");
    }
}
