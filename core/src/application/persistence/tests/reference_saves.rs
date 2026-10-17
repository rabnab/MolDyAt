// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Reference saves: one file per format version, written by the build that
//! introduced that version. They are never edited or regenerated. A format
//! change adds a new fixture, and every older one must keep loading through
//! the real load path.

use super::scratch_dir;
use crate::application::MainContent;
use crate::application::persistence::{LoadSource, SaveStore, WORLD_FILE_VERSION, WorldFile};
use crate::objects2d::operator::OperatorState;
use crate::objects2d::wall_chain::Joint;
use crate::objects2d::{Rect, Vec2D};
use std::fs;
use std::path::PathBuf;

/// A reference save: its format version, file name and content checks.
struct Fixture {
    version: u32,
    file: &'static str,
    check: fn(&MainContent),
}

/// Every reference save, oldest first.
const FIXTURES: &[Fixture] = &[Fixture {
    version: 1,
    file: "v1.json",
    check: check_v1,
}];

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
        let file = WorldFile::capture(mc.world(), mc.time_step());
        let reloaded = WorldFile::from_json(&file.to_json().unwrap()).unwrap();
        assert_eq!(reloaded, file, "{}", fixture.file);
    }
}

/// The newest format needs no migration, so its reference save must come
/// back field for field. Older ones are checked by their `check_vN`.
#[test]
fn current_reference_save_loads_every_field() {
    for fixture in FIXTURES.iter().filter(|f| f.version == WORLD_FILE_VERSION) {
        let json = fs::read_to_string(fixtures_dir().join(fixture.file)).unwrap();
        let expected = WorldFile::from_json(&json).unwrap();
        let mc = load_fixture(fixture.file);
        assert_eq!(
            WorldFile::capture(mc.world(), mc.time_step()),
            expected,
            "{}",
            fixture.file
        );
    }
}

/// v1: written by the build that introduced the format.
fn check_v1(mc: &MainContent) {
    let world = mc.world();
    assert_eq!(mc.time_step(), 0.005);
    assert_eq!(world.physics.constants.gravity, 0.5);
    assert_eq!(world.world_region(), Rect::new(-20.0, 15.0, 40.0, -30.0));

    let balls: Vec<(usize, usize)> = world
        .balls
        .iter()
        .map(|b| (b.get_id(), b.get_size_idx()))
        .collect();
    assert_eq!(
        balls,
        vec![
            (10, 0),
            (11, 1),
            (12, 2),
            (13, 3),
            (14, 0),
            (15, 1),
            (16, 2),
            (17, 3)
        ]
    );
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
                s.ball_size_idx,
                s.emission_speed,
            )
        })
        .collect();
    assert_eq!(
        spheres,
        vec![
            (0, OperatorState::On, 3000, 2, 0.05),
            (1, OperatorState::Off, 1500, 1, 0.08)
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
