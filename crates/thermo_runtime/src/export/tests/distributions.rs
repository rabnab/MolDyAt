// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use crate::application::renderer_snapshot::RendererSnapshot;
use crate::distributions::{RDF_BINS, SPEED_BINS};
use thermo_physics::objects2d::Vec2D;

/// The balls of `snapshot_with_balls`, moving.
fn moving_balls() -> RendererSnapshot {
    let mut snapshot = snapshot_with_balls();
    for (i, ball) in snapshot.balls.data.iter_mut().enumerate() {
        ball.velocity = Vec2D::new(i as f32 - 2.0, 0.5 * i as f32);
    }
    snapshot
}

#[test]
fn test_distributions_csv_has_a_speed_row_per_bin_and_rdf_rows_per_pair() {
    let snapshot = moving_balls();
    let rows = parse_csv(&distributions_to_string(&snapshot, ExportFormat::Csv));
    assert_eq!(
        rows[0],
        ["distribution", "species", "x", "value", "expected"]
    );
    let speed: Vec<_> = rows[1..].iter().filter(|r| r[0] == "speed").collect();
    let rdf: Vec<_> = rows[1..].iter().filter(|r| r[0] == "rdf").collect();
    assert_eq!(speed.len(), SPEED_BINS);
    assert_eq!(rdf.len(), RDF_BINS, "one species: one pair");
    assert!(rdf.iter().all(|r| r[1] == "N2-N2" && r[4].is_empty()));
    let counted: u32 = speed.iter().map(|r| r[3].parse::<u32>().unwrap()).sum();
    assert_eq!(counted as usize, snapshot.balls.len());
    // Without neighbours g is NaN: exported as empty cells.
    assert!(
        rdf.iter()
            .all(|r| r[3].is_empty() || r[3].parse::<f32>().is_ok())
    );
}

#[test]
fn test_distributions_jsonl_records() {
    let snapshot = moving_balls();
    let text = distributions_to_string(&snapshot, ExportFormat::Jsonl);
    let records: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(records[0]["record_kind"], "distributions_header");
    assert_eq!(records[0]["ball_count"], snapshot.balls.len());
    assert_eq!(records[1]["record_kind"], "speed_histogram");
    assert_eq!(records[1]["counts"].as_array().unwrap().len(), SPEED_BINS);
    assert_eq!(records[2]["record_kind"], "rdf");
    assert_eq!(records[2]["species"], serde_json::json!(["N2", "N2"]));
    assert!(
        records
            .iter()
            .all(|r| r["schema_version"] == SCHEMA_VERSION)
    );
}
