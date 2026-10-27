// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The series export.

use super::{ExportFormat, SCHEMA_VERSION, csv_cell, csv_f32, json_f32};
use crate::metrics::MetricsRegistry;
use serde_json::json;

/// One selected series, aligned to the rows of the export.
struct Column<'a> {
    name: &'a str,
    unit: Option<&'static str>,
    description: &'static str,
    per_species: bool,
    values: Vec<Option<f32>>,
}

/// The step and simulation time of each row.
struct Rows {
    steps: Vec<Option<u64>>,
    times: Vec<Option<f32>>,
}

/// The selected series, sorted by name, and the step and time of each row.
/// Rows are the sampled steps; a series shorter than the others (registered
/// later) has no value for the first rows, since all series end at the
/// newest step. The times are parallel to the steps.
fn columns<'a>(
    registry: &'a MetricsRegistry,
    selection: Option<&[String]>,
) -> (Rows, Vec<Column<'a>>) {
    let series: Vec<_> = registry
        .all_series()
        .filter(|(name, _)| selection.is_none_or(|names| names.iter().any(|n| n == *name)))
        .collect();
    let rows = series
        .iter()
        .map(|(_, buffer)| buffer.len())
        .chain([registry.steps().len()])
        .max()
        .unwrap_or(0);
    let aligned = |len: usize, row: usize| row.checked_sub(rows - len);
    let steps = (0..rows)
        .map(|row| {
            aligned(registry.steps().len(), row)
                .and_then(|i| registry.steps().get().get(i).copied())
        })
        .collect();
    let times = (0..rows)
        .map(|row| {
            aligned(registry.times().len(), row)
                .and_then(|i| registry.times().get().get(i).copied())
        })
        .collect();
    let columns = series
        .into_iter()
        .map(|(name, buffer)| {
            let metadata = registry.get_metadata(name);
            Column {
                name,
                unit: metadata.and_then(|m| m.unit),
                description: metadata.map_or("", |m| m.description),
                per_species: metadata.is_some_and(|m| m.is_per_species),
                values: (0..rows)
                    .map(|row| {
                        aligned(buffer.len(), row).and_then(|i| buffer.get().get(i).copied())
                    })
                    .collect(),
            }
        })
        .collect();
    (Rows { steps, times }, columns)
}

/// The `selection` of the registry's series (`None` = all), sorted by name.
///
/// CSV: a header row `step,time,<name>[ [<unit>]],…` and one row per
/// sampled step. JSONL: a `series_header` record (names, units,
/// descriptions, row count, `sample_interval_steps`), then one `series_row`
/// record per step with its `time` and a `values` object. The time is the
/// simulation time of the sample, as in the sample stream, so files of one
/// run can be joined on it.
///
/// `sample_interval_steps` is the nominal distance in steps between two
/// samples when the run sampled only every k-th frame to fit the registry
/// (the last step is always sampled); `None` for a run that sampled every
/// frame. A CSV states it in a first line `# sample_interval_steps=N` only
/// when there is one.
pub fn series_to_string(
    registry: &MetricsRegistry,
    selection: Option<&[String]>,
    format: ExportFormat,
    sample_interval_steps: Option<u64>,
) -> String {
    let (rows, columns) = columns(registry, selection);
    let steps = &rows.steps;
    let mut out = String::new();
    match format {
        ExportFormat::Csv => {
            if let Some(interval) = sample_interval_steps {
                out.push_str(&format!("# sample_interval_steps={interval}\n"));
            }
            out.push_str("step,time");
            for column in &columns {
                out.push(',');
                match column.unit {
                    Some(unit) => out.push_str(&csv_cell(&format!("{} [{unit}]", column.name))),
                    None => out.push_str(&csv_cell(column.name)),
                }
            }
            out.push('\n');
            for (row, step) in steps.iter().enumerate() {
                out.push_str(&step.map(|s| s.to_string()).unwrap_or_default());
                out.push(',');
                out.push_str(&csv_f32(rows.times[row]));
                for column in &columns {
                    out.push(',');
                    out.push_str(&csv_f32(column.values[row]));
                }
                out.push('\n');
            }
        }
        ExportFormat::Jsonl => {
            let header = json!({
                "schema_version": SCHEMA_VERSION,
                "record_kind": "series_header",
                "rows": steps.len(),
                "sample_interval_steps": sample_interval_steps,
                "series": columns.iter().map(|c| json!({
                    "name": c.name,
                    "unit": c.unit,
                    "description": c.description,
                    "per_species": c.per_species,
                })).collect::<Vec<_>>(),
            });
            out.push_str(&header.to_string());
            out.push('\n');
            for (row, step) in steps.iter().enumerate() {
                let values: serde_json::Map<_, _> = columns
                    .iter()
                    .map(|c| (c.name.to_string(), json_f32(c.values[row])))
                    .collect();
                let record = json!({
                    "schema_version": SCHEMA_VERSION,
                    "record_kind": "series_row",
                    "step": step,
                    "time": json_f32(rows.times[row]),
                    "values": values,
                });
                out.push_str(&record.to_string());
                out.push('\n');
            }
        }
    }
    out
}
