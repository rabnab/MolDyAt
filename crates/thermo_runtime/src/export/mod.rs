// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Data export: selected metrics series and a per-ball snapshot dump, as CSV
//! or JSONL, for analysis outside the app (#337).
//!
//! The formats follow the headless contract: every JSONL record carries a
//! `schema_version` ([`SCHEMA_VERSION`]) and a `record_kind`, and every `f32`
//! that is not finite (`NaN`, `±Inf`) is exported as `null` (JSONL) or an
//! empty cell (CSV). A series without a value at some step (it was
//! registered later than the others) is exported the same way.
//!
//! The functions here only build text; [`write_export`] puts it in a file.

mod series;
mod snapshot;

#[cfg(test)]
mod tests;

pub use series::series_to_string;
pub use snapshot::snapshot_to_string;

use std::io;
use std::path::{Path, PathBuf};
use thermo_physics::application::persistence::write_atomic_unique;

/// Schema of every JSONL record of an export.
pub const SCHEMA_VERSION: &str = "export.v1";

/// What to export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// The metrics series, one row per sampled step.
    Series,
    /// The state of every ball at the latest snapshot.
    Snapshot,
}

impl ExportKind {
    fn file_stem(self) -> &'static str {
        match self {
            Self::Series => "series",
            Self::Snapshot => "snapshot",
        }
    }
}

/// File format of an export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    /// Comma-separated values with one header row.
    #[default]
    Csv,
    /// Newline-delimited JSON records.
    Jsonl,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Jsonl => "jsonl",
        }
    }

    /// Parse `csv` or `jsonl`.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "csv" => Some(Self::Csv),
            "jsonl" => Some(Self::Jsonl),
            _ => None,
        }
    }
}

/// The file stem `<prefix>_<kind>`; the prefix is a timestamp for interactive
/// exports and empty for headless runs, which want fixed names.
fn file_stem(prefix: &str, kind: ExportKind) -> String {
    let kind = kind.file_stem();
    if prefix.is_empty() {
        kind.to_string()
    } else {
        format!("{prefix}_{kind}")
    }
}

/// Write `contents` as `dir/<prefix>_<kind>.<ext>` (see [`file_stem`]) and
/// return the path. With `keep_existing` an existing file is never
/// overwritten: a `_01`, `_02`, … suffix is added to the stem.
pub fn write_export(
    dir: &Path,
    prefix: &str,
    kind: ExportKind,
    format: ExportFormat,
    contents: &str,
    keep_existing: bool,
) -> io::Result<PathBuf> {
    write_atomic_unique(
        dir,
        &file_stem(prefix, kind),
        format.extension(),
        contents.as_bytes(),
        keep_existing,
    )
}

/// One CSV cell, quoted when it holds a comma, quote or line break.
fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// A finite `f32` in its shortest round-trip form, anything else as an empty
/// cell.
fn csv_f32(value: Option<f32>) -> String {
    match value {
        Some(value) if value.is_finite() => value.to_string(),
        _ => String::new(),
    }
}

/// A finite `f32` as a JSON number, anything else as `null`.
fn json_f32(value: Option<f32>) -> serde_json::Value {
    match value {
        Some(value) if value.is_finite() => serde_json::json!(value),
        _ => serde_json::Value::Null,
    }
}
