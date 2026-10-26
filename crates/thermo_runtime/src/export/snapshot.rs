// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The per-ball snapshot dump.

use super::{ExportFormat, SCHEMA_VERSION, csv_cell, csv_f32, json_f32};
use crate::application::renderer_snapshot::RendererSnapshot;
use serde_json::json;

/// The balls of `snapshot`, sorted by id.
///
/// CSV: `id,species,x,y,vx,vy,mass,contact_radius`, one row per ball.
/// JSONL: a `snapshot_header` record (step, time, ball count), then one
/// `ball` record per ball with the fields of the headless `balls` detail.
pub fn snapshot_to_string(snapshot: &RendererSnapshot, format: ExportFormat) -> String {
    let mut balls: Vec<_> = snapshot.balls.iter().collect();
    balls.sort_unstable_by_key(|ball| ball.id);
    let species = |ball: &crate::application::renderer_snapshot::BallRenderData| {
        snapshot.species.get(ball.species).map(|s| s.name.as_str())
    };
    let mut out = String::new();
    match format {
        ExportFormat::Csv => {
            out.push_str("id,species,x,y,vx,vy,mass,contact_radius\n");
            for ball in balls {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{},{}\n",
                    ball.id,
                    csv_cell(species(ball).unwrap_or("")),
                    csv_f32(Some(ball.position.x)),
                    csv_f32(Some(ball.position.y)),
                    csv_f32(Some(ball.velocity.x)),
                    csv_f32(Some(ball.velocity.y)),
                    csv_f32(Some(ball.mass)),
                    csv_f32(Some(ball.contact_radius)),
                ));
            }
        }
        ExportFormat::Jsonl => {
            let header = json!({
                "schema_version": SCHEMA_VERSION,
                "record_kind": "snapshot_header",
                "step": snapshot.simulation_step,
                "time": json_f32(Some(snapshot.time)),
                "ball_count": balls.len(),
            });
            out.push_str(&header.to_string());
            out.push('\n');
            for ball in balls {
                let record = json!({
                    "schema_version": SCHEMA_VERSION,
                    "record_kind": "ball",
                    "id": ball.id,
                    "species": species(ball),
                    "position": {
                        "x": json_f32(Some(ball.position.x)),
                        "y": json_f32(Some(ball.position.y)),
                    },
                    "velocity": {
                        "vx": json_f32(Some(ball.velocity.x)),
                        "vy": json_f32(Some(ball.velocity.y)),
                    },
                    "contact_radius": json_f32(Some(ball.contact_radius)),
                    "mass": json_f32(Some(ball.mass)),
                });
                out.push_str(&record.to_string());
                out.push('\n');
            }
        }
    }
    out
}
