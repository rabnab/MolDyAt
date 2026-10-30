// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The distributions export (#343): the speed histogram with its
//! Maxwell–Boltzmann counts and g(r) per species pair.

use super::{ExportFormat, SCHEMA_VERSION, csv_cell, csv_f32, json_f32};
use crate::application::renderer_snapshot::RendererSnapshot;
use crate::distributions::{Distributions, distributions};
use serde_json::json;

/// The distributions of `snapshot`'s balls.
///
/// CSV: `distribution,species,x,value,expected`, one row per bin: `speed`
/// rows with the bin's centre speed, ball count and Maxwell–Boltzmann count,
/// then `rdf` rows per species pair (`N2-Ar`) with the bin's centre distance
/// and g. JSONL: a `distributions_header` record (step, time, temperature),
/// a `speed_histogram` record and one `rdf` record per species pair, with
/// the fields of the headless `distributions` object.
pub fn distributions_to_string(snapshot: &RendererSnapshot, format: ExportFormat) -> String {
    let data = distributions(snapshot);
    let mut out = String::new();
    match format {
        ExportFormat::Csv => {
            out.push_str("distribution,species,x,value,expected\n");
            if let Some(speed) = &data.speed {
                for (i, (count, expected)) in speed.counts.iter().zip(&speed.expected).enumerate() {
                    out.push_str(&format!(
                        "speed,,{},{count},{}\n",
                        csv_f32(Some(speed.bin_centre(i))),
                        csv_f32(Some(*expected)),
                    ));
                }
            }
            for (pair, names) in data.radial.pairs.iter().zip(pair_names(snapshot, &data)) {
                let names = csv_cell(&format!("{}-{}", names[0], names[1]));
                for (i, g) in pair.g.iter().enumerate() {
                    out.push_str(&format!(
                        "rdf,{names},{},{},\n",
                        csv_f32(Some(data.radial.bin_centre(i))),
                        csv_f32(Some(*g)),
                    ));
                }
            }
        }
        ExportFormat::Jsonl => {
            let header = json!({
                "schema_version": SCHEMA_VERSION,
                "record_kind": "distributions_header",
                "step": snapshot.frame.simulation_step,
                "time": json_f32(Some(snapshot.frame.time)),
                "ball_count": snapshot.balls.len(),
            });
            let mut records = vec![header];
            let value = distributions_json(snapshot, &data);
            if let Some(speed) = value.get("speed") {
                let mut record = speed.clone();
                record["schema_version"] = json!(SCHEMA_VERSION);
                record["record_kind"] = json!("speed_histogram");
                records.push(record);
            }
            let rdf = &value["rdf"];
            for pair in rdf["pairs"].as_array().into_iter().flatten() {
                let mut record = pair.clone();
                record["schema_version"] = json!(SCHEMA_VERSION);
                record["record_kind"] = json!("rdf");
                record["bin_width"] = rdf["bin_width"].clone();
                records.push(record);
            }
            for record in records {
                out.push_str(&record.to_string());
                out.push('\n');
            }
        }
    }
    out
}

/// The species names of every pair of `data`, `?` for one the table lacks.
fn pair_names<'a>(
    snapshot: &'a RendererSnapshot,
    data: &Distributions,
) -> impl Iterator<Item = [&'a str; 2]> {
    let table = &snapshot.balls.species;
    let name = |id| table.get(id).map_or("?", |s| s.name.as_str());
    data.radial
        .pairs
        .iter()
        .map(move |pair| [name(pair.species.0), name(pair.species.1)])
        .collect::<Vec<_>>()
        .into_iter()
}

/// `data` as the JSON object of a headless frame's `distributions` field:
/// `speed` (absent without a histogram) with `bin_width`, `temperature`,
/// `counts` and `maxwell_boltzmann`, and `rdf` with `bin_width` and
/// `pairs`, each with `species`, `reference_balls` and `g`. Non-finite
/// numbers are `null`.
pub fn distributions_json(snapshot: &RendererSnapshot, data: &Distributions) -> serde_json::Value {
    let floats = |values: &[f32]| {
        values
            .iter()
            .map(|v| json_f32(Some(*v)))
            .collect::<Vec<_>>()
    };
    let pairs: Vec<_> = data
        .radial
        .pairs
        .iter()
        .zip(pair_names(snapshot, data))
        .map(|(pair, names)| {
            json!({
                "species": names,
                "reference_balls": pair.reference_balls,
                "g": floats(&pair.g),
            })
        })
        .collect();
    let mut value = json!({
        "rdf": {
            "bin_width": json_f32(Some(data.radial.bin_width)),
            "pairs": pairs,
        },
    });
    if let Some(speed) = &data.speed {
        value["speed"] = json!({
            "bin_width": json_f32(Some(speed.bin_width)),
            "temperature": json_f32(Some(speed.temperature)),
            "counts": speed.counts,
            "maxwell_boltzmann": floats(&speed.expected),
        });
    }
    value
}
