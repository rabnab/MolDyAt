// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The status lines formatted from a snapshot's numbers (#341).

use crate::graphics::energy_plot::status_text::{status_text, write_species_lines};
use thermo_physics::application::sim_stats::StatusNumbers;
use thermo_physics::objects2d::Vec2D;
use thermo_runtime::application::renderer_snapshot::RendererSnapshot;

fn numbers(real_units: bool) -> StatusNumbers {
    StatusNumbers {
        real_units,
        dt: 0.01,
        hood_updates: 42,
        max_neighbours: 6,
        temperature: 1.0,
        max_v: 2.0,
        v_com: Vec2D::new(0.0, 0.0),
    }
}

fn snapshot(status: Option<StatusNumbers>) -> RendererSnapshot {
    let mut snapshot = RendererSnapshot::new();
    snapshot.frame.time = 100.0;
    snapshot.frame.status = status;
    snapshot
}

#[test]
fn no_status_while_ball_stats_are_hidden() {
    assert_eq!(status_text(&snapshot(None), "fps 60"), None);
}

#[test]
fn reduced_status_shows_the_neighbour_updates_and_starts_with_the_rates() {
    let text = status_text(&snapshot(Some(numbers(false))), "fps 60").unwrap();
    assert!(text.starts_with("fps 60 time: 100.0 (dt:0.0100)"), "{text}");
    assert!(text.contains("#hood: 42"), "{text}");
    assert!(text.contains("maxNeig: 6"), "{text}");
}

#[test]
fn real_status_shows_kelvin_and_picoseconds() {
    let text = status_text(&snapshot(Some(numbers(true))), "fps 60").unwrap();
    assert!(text.contains("time: 220.2ps"), "{text}");
    assert!(text.contains("dt: 22.0fs"), "{text}");
    assert!(text.contains("Temp: 95.05K"), "{text}");
}

#[test]
fn species_lines_name_the_new_ball_species_and_count_per_species() {
    let mut snapshot = snapshot(Some(numbers(false)));
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
