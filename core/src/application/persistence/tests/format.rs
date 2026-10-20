// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::sample_file;
use crate::application::SimConfig;
use crate::application::persistence::WorldFile;
use crate::application::persistence::format::{
    BallData, MAX_ID, SourceSphereData, ThermostatData, WORLD_FILE_VERSION,
};
use crate::application::sim_config::{MAX_TIME_STEP, MIN_TIME_STEP};
use crate::objects2d::wall_chain::{ChainPiece, WallChain};
use crate::objects2d::{SimLineSegment, Vec2D};

fn error_text(file: &WorldFile) -> String {
    format!("{:?}", file.validate().expect_err("file must be rejected"))
}

#[test]
fn json_round_trip_is_exact() {
    let mut file = sample_file();
    file.balls[0].pos = Vec2D::new(0.1, 1.0 / 3.0);
    let parsed = WorldFile::from_json(&file.to_json().unwrap()).unwrap();
    assert_eq!(parsed, file);
}

#[test]
fn rejects_unsupported_version() {
    let mut file = sample_file();
    file.version = WORLD_FILE_VERSION + 1;
    assert!(error_text(&file).contains("version"));
}

#[test]
fn rejects_duplicate_ball_ids() {
    let mut file = sample_file();
    let ball: BallData = file.balls[0].clone();
    file.balls.push(ball);
    assert!(error_text(&file).contains("duplicate ball id"));
}

#[test]
fn rejects_unknown_species() {
    let mut file = sample_file();
    file.balls[0].species = "Unobtainium".to_string();
    assert!(error_text(&file).contains("unknown species"));
}

/// With a `[[species]]` list in its config, a file may only use those.
#[test]
fn rejects_species_missing_from_the_files_species_list() {
    let mut file = sample_file();
    let config = SimConfig::from_toml(
        "[[species]]\nname = \"Ar\"\nsigma_angstrom = 3.405\nepsilon_kelvin = 119.8\n\
         mass_u = 39.948\ncolour = [0.6, 0.45, 1.0]\n",
    )
    .unwrap();
    file.config = Some(SimConfig {
        integration: file.sim_config().integration,
        ..config
    });
    file.balls[0].species = "N2".to_string();
    assert!(error_text(&file).contains("unknown species"));
    file.balls[0].species = "Ar".to_string();
    file.source_spheres.clear();
    file.validate().expect("Ar is in the list");
}

#[test]
fn rejects_non_finite_ball_values() {
    let mut file = sample_file();
    file.balls[0].v.x = f32::NAN;
    assert!(error_text(&file).contains("non-finite"));
}

#[test]
fn rejects_degenerate_world_rect() {
    let mut file = sample_file();
    file.world_rect.w = 0.0;
    assert!(error_text(&file).contains("w > 0"));
}

#[test]
fn rejects_ball_outside_the_world() {
    let mut file = sample_file();
    // world_rect is x -20..20, y 20..-20 (top-left first).
    for pos in [
        [25.0, 0.0],
        [-25.0, 0.0],
        [0.0, 25.0],
        [0.0, -25.0],
        [20.0, 0.0],
    ] {
        file.balls[0].pos = Vec2D::new(pos[0], pos[1]);
        assert!(error_text(&file).contains("outside world_rect"), "{pos:?}");
    }
    file.balls[0].pos = Vec2D::new(19.9, -19.9);
    assert!(file.validate().is_ok());
}

#[test]
fn rejects_world_rect_with_wrong_orientation() {
    let mut file = sample_file();
    file.world_rect.h = 40.0;
    assert!(error_text(&file).contains("h < 0"));
}

fn sample_sphere() -> SourceSphereData {
    SourceSphereData {
        id: 0,
        center: Vec2D::zero(),
        active: true,
        emit_period: 3000,
        ball_species: "M".to_string(),
        emission_speed: 0.05,
    }
}

#[test]
fn rejects_source_sphere_emit_period_out_of_range() {
    for period in [0, 4, 30_001] {
        let mut file = sample_file();
        let mut sphere = sample_sphere();
        sphere.emit_period = period;
        file.source_spheres.push(sphere);
        assert!(error_text(&file).contains("emit_period"), "{period}");
    }
    let mut file = sample_file();
    file.source_spheres.push(sample_sphere());
    assert!(file.validate().is_ok());
}

#[test]
fn rejects_source_sphere_emission_speed_out_of_range() {
    let mut file = sample_file();
    let mut sphere = sample_sphere();
    sphere.emission_speed = 50.0;
    file.source_spheres.push(sphere);
    assert!(error_text(&file).contains("emission_speed"));
}

#[test]
fn rejects_thermostat_with_wrong_chain_or_ranges() {
    let good = ThermostatData {
        id: 0,
        position: Vec2D::zero(),
        active: true,
        target_temperature: 0.3,
        radius: 5.0,
        tau: 0.5,
        chain: vec![0.0; 5],
    };
    let mut file = sample_file();
    file.thermostats.push(good.clone());
    assert!(file.validate().is_ok());

    for bad in [
        ThermostatData {
            chain: vec![0.0; 2],
            ..good.clone()
        },
        ThermostatData {
            chain: vec![99.0; 5],
            ..good.clone()
        },
        ThermostatData {
            radius: 0.0,
            ..good.clone()
        },
        ThermostatData {
            target_temperature: -1.0,
            ..good.clone()
        },
        ThermostatData {
            tau: 0.0,
            ..good.clone()
        },
    ] {
        let mut file = sample_file();
        file.thermostats.push(bad);
        assert!(file.validate().is_err());
    }
}

#[test]
fn rejects_line_segment_without_thickness() {
    let mut file = sample_file();
    let mut segment = SimLineSegment::new(0, Vec2D::zero(), Vec2D::new(1.0, 0.0));
    segment.thickness = 0.0;
    file.line_segments.push(segment);
    assert!(error_text(&file).contains("thickness"));
}

#[test]
fn rejects_malformed_json() {
    assert!(WorldFile::from_json("{ not json").is_err());
}

#[test]
fn rejects_chain_with_missing_line_segment() {
    let mut file = sample_file();
    file.wall_chains.push(WallChain {
        id: 0,
        start: Vec2D::zero(),
        end: Vec2D::new(1.0, 0.0),
        pieces: vec![ChainPiece {
            line_id: 7,
            t_start: 0.0,
            t_end: 1.0,
        }],
        joints: vec![],
    });
    assert!(error_text(&file).contains("missing line segment"));
}

#[test]
fn rejects_ids_that_would_overflow_the_id_counters() {
    let mut file = sample_file();
    file.balls[0].id = usize::MAX;
    assert!(error_text(&file).contains("above"));

    let mut file = sample_file();
    let mut sphere = sample_sphere();
    sphere.id = MAX_ID + 1;
    file.source_spheres.push(sphere);
    assert!(error_text(&file).contains("source sphere id"));

    let mut file = sample_file();
    file.balls[0].id = MAX_ID;
    assert!(file.validate().is_ok(), "the bound itself is allowed");
}

#[test]
fn file_without_config_runs_with_the_default_config() {
    let file = sample_file();
    assert_eq!(file.config, None);
    let mut expected = SimConfig::default();
    expected.integration.time_step = file.time_step;
    assert_eq!(file.sim_config(), expected);
}

#[test]
fn saved_config_is_validated() {
    let mut file = sample_file();
    let mut config = file.sim_config();
    config.potential.cutoff_factor = 0.5;
    file.config = Some(config);
    assert!(error_text(&file).contains("cutoff_factor"));
}

#[test]
fn saved_config_must_match_the_time_step() {
    let mut file = sample_file();
    let mut config = file.sim_config();
    config.integration.time_step = file.time_step * 2.0;
    file.config = Some(config);
    assert!(error_text(&file).contains("time_step"));
}

#[test]
fn unknown_config_keys_are_rejected() {
    let json = sample_file().to_json().unwrap().replacen(
        "\"gravity\"",
        "\"config\": {\"potential\": {\"max_forc\": 80.0}},\n  \"gravity\"",
        1,
    );
    let error = format!("{:?}", WorldFile::from_json(&json).unwrap_err());
    assert!(error.contains("max_forc"), "{error}");
    assert!(error.contains("newer MolDyAt build"), "{error}");
}

#[test]
fn rejects_time_step_outside_the_accepted_range() {
    for time_step in [MIN_TIME_STEP / 2.0, MAX_TIME_STEP * 2.0] {
        let mut file = sample_file();
        file.time_step = time_step;
        assert!(error_text(&file).contains("time_step"), "{time_step}");
    }
    for time_step in [MIN_TIME_STEP, MAX_TIME_STEP] {
        let mut file = sample_file();
        file.time_step = time_step;
        assert!(file.validate().is_ok(), "{time_step}");
    }
}
