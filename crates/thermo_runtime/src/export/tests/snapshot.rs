// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

#[test]
fn test_snapshot_csv_lists_every_ball_sorted_by_id() {
    let mut snapshot = snapshot_with_balls();
    snapshot.balls.data[0].velocity.x = f32::NAN;
    let rows = parse_csv(&snapshot_to_string(&snapshot, ExportFormat::Csv));
    assert_eq!(
        rows[0],
        [
            "id",
            "species",
            "x",
            "y",
            "vx",
            "vy",
            "mass",
            "contact_radius"
        ]
    );
    assert_eq!(rows.len(), snapshot.balls.len() + 1);
    let ids: Vec<usize> = rows[1..].iter().map(|r| r[0].parse().unwrap()).collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
    let nan_id = snapshot.balls[0].id.to_string();
    let row = rows.iter().find(|r| r[0] == nan_id).unwrap();
    assert_eq!(row[4], "");
}

#[test]
fn test_snapshot_jsonl_round_trips_ball_state() {
    let snapshot = snapshot_with_balls();
    let text = snapshot_to_string(&snapshot, ExportFormat::Jsonl);
    let records: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(records[0]["record_kind"], "snapshot_header");
    assert_eq!(records[0]["ball_count"], snapshot.balls.len());
    assert_eq!(records.len(), snapshot.balls.len() + 1);
    let ball = snapshot.balls.iter().min_by_key(|b| b.id).unwrap();
    assert_eq!(records[1]["id"], ball.id);
    assert_eq!(records[1]["position"]["x"], ball.position.x);
    assert_eq!(records[1]["mass"], ball.mass);
}
