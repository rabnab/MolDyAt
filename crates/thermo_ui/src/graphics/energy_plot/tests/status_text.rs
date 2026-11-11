// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The status lines formatted from a snapshot's numbers (#341).

use crate::graphics::energy_plot::status_text::{status_text, write_species_lines};
use thermo_physics::application::observables::Obs;
use thermo_physics::application::sim_stats::{ForcePath, StatusInfo};
use thermo_runtime::application::renderer_snapshot::RendererSnapshot;

fn info(real_units: bool) -> StatusInfo {
    StatusInfo {
        real_units,
        dt_limited_by: None,
    }
}

fn snapshot(status: Option<StatusInfo>) -> RendererSnapshot {
    let mut snapshot = RendererSnapshot::new();
    snapshot.frame.time = 100.0;
    snapshot.frame.status = status;
    let values = &mut snapshot.frame.values;
    values[Obs::Dt] = f64::from(0.01_f32);
    values[Obs::HoodUpdates] = 42.0;
    values[Obs::MaxNeighbours] = 6.0;
    values[Obs::Temperature] = 1.0;
    values[Obs::MaxV] = 2.0;
    snapshot
}

#[test]
fn no_status_while_ball_stats_are_hidden() {
    assert_eq!(status_text(&snapshot(None), "fps 60"), None);
}

#[test]
fn reduced_status_shows_the_neighbour_updates_and_starts_with_the_rates() {
    let text = status_text(&snapshot(Some(info(false))), "fps 60").unwrap();
    assert!(text.starts_with("fps 60 time: 100.0 (dt:0.0100)"), "{text}");
    assert!(text.contains("#hood: 42"), "{text}");
    assert!(text.contains("maxNeig: 6"), "{text}");
}

#[test]
fn real_status_shows_kelvin_and_picoseconds() {
    let text = status_text(&snapshot(Some(info(true))), "fps 60").unwrap();
    assert!(text.contains("time: 215.6ps"), "{text}");
    assert!(text.contains("dt: 21.6fs"), "{text}");
    assert!(text.contains("Temp: 119.80K"), "{text}");
}

#[test]
fn species_lines_name_the_new_ball_species_and_count_per_species() {
    let mut snapshot = snapshot(Some(info(false)));
    let table = snapshot.balls.species.clone();
    let ar = table
        .ids()
        .find(|&id| table.species(id).name == "Ar")
        .unwrap();
    snapshot.balls.cursor_species = ar;
    snapshot.frame.species_counts[0] = 3;
    snapshot.frame.species_counts[ar.index()] = 2;

    let mut text = String::new();
    write_species_lines(&mut text, &snapshot);
    assert_eq!(text, "New: Ar\nN2: 3\nAr: 2");
    let status = status_text(&snapshot, "fps 0").unwrap();
    assert!(status.contains("#Balls: 5 "), "{status}");
    assert!(status.ends_with("\nNew: Ar\nN2: 3\nAr: 2"), "{status}");
}

#[test]
fn status_names_the_species_that_limits_the_time_step() {
    let mut snapshot = snapshot(Some(info(false)));
    let text = status_text(&snapshot, "fps 60").unwrap();
    assert!(!text.contains("dt limited"), "{text}");

    let water = snapshot.balls.species.by_name("H2O").expect("built-in H2O");
    snapshot.frame.status = Some(StatusInfo {
        dt_limited_by: Some(water),
        ..info(false)
    });
    let text = status_text(&snapshot, "fps 60").unwrap();
    assert!(text.contains("\ndt limited by H2O\n"), "{text}");
}

#[test]
fn status_shows_the_force_backend_only_with_the_gpu_pass_on() {
    let mut snapshot = snapshot(Some(info(false)));
    let text = status_text(&snapshot, "fps 60").unwrap();
    assert!(!text.contains("Forces"), "{text}");

    snapshot.frame.forces = Some(ForcePath::Gpu);
    let text = status_text(&snapshot, "fps 60").unwrap();
    assert!(text.contains("\nForces: GPU\n"), "{text}");

    snapshot.frame.forces = Some(ForcePath::Cpu("no GPU"));
    let text = status_text(&snapshot, "fps 60").unwrap();
    assert!(text.contains("\nForces: CPU (no GPU)\n"), "{text}");
}

/// The safety-net counts since the start are one status line (#422).
#[test]
fn status_counts_the_safety_nets() {
    let mut snapshot = snapshot(Some(info(false)));
    let values = &mut snapshot.frame.values;
    values[Obs::Braked] = 1.0;
    values[Obs::CappedPairs] = 22.0;
    values[Obs::BackstopHits] = 3.0;
    values[Obs::OverlapCorrections] = 4.0;
    let text = status_text(&snapshot, "fps 60").unwrap();
    assert!(
        text.contains("\nNets: brake 1 cap 22 backstop 3 overlap 4\n"),
        "{text}"
    );
}
