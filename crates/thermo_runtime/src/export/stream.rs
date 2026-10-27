// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The sample stream (#366): every metrics sample appended to a file as it
//! is taken, so a long run keeps its whole history although the registry
//! holds only the newest samples.
//!
//! The owner calls [`SampleStream::record`] right after each
//! `MetricsRegistry::sample`. The rows go through a buffered writer that is
//! flushed at most [`FLUSH_INTERVAL`] after a row was written (the owner
//! calls [`SampleStream::flush_if_due`] while no samples come, e.g. while
//! paused) and when the stream ends, so a crash loses at most about a
//! second of samples.
//!
//! A file holds one world: a new world generation, or another set of series
//! (another species table), starts a new file with its own header. The first
//! file is named `<stem>.<ext>`, later ones get the `_01`, `_02`, … suffix
//! of [`write_atomic_unique`]. The header is written atomically, the rows
//! are appended.
//!
//! Formats, both `export.v1` like the series export: CSV with the header
//! `step,time,<name>[ [<unit>]],…` and one row per sample; JSONL with a
//! `stream_header` record (the series' names, units, descriptions), then a
//! `series_row` record per sample that also carries `time`. Non-finite
//! values are empty cells (CSV) or `null` (JSONL).

use super::{ExportFormat, SCHEMA_VERSION, csv_cell, csv_f32, json_f32};
use crate::metrics::MetricsRegistry;
use serde_json::json;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use thermo_physics::application::persistence::write_atomic_unique;

/// Longest time a written row stays in the buffer.
pub const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

/// Where and how a [`SampleStream`] writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamSettings {
    pub dir: PathBuf,
    /// File stem of the first file; later files add a `_01`, … suffix.
    pub stem: String,
    pub format: ExportFormat,
    /// Replace an existing `<stem>.<ext>` (headless runs, which want a fixed
    /// name) instead of adding a suffix (interactive streams).
    pub replace_existing: bool,
}

/// The file being written.
struct Section {
    writer: BufWriter<File>,
    /// Epoch of the registry's steps the file belongs to (a new world clears
    /// the registry and with it starts a new file).
    epoch: u64,
    /// The series of the header, in order.
    columns: Vec<String>,
}

/// Appends the registry's samples to a file as they are taken. After an
/// error the stream is broken: drop it (dropping also flushes, ignoring
/// errors; [`Self::finish`] reports them).
pub struct SampleStream {
    settings: StreamSettings,
    section: Option<Section>,
    /// `(epoch, total_pushed)` of the registry's steps at the last record:
    /// the samples taken since are the new ones.
    seen: (u64, u64),
    files: Vec<PathBuf>,
    rows: u64,
    last_flush: Instant,
    /// Whether rows were written since the last flush.
    dirty: bool,
}

impl SampleStream {
    /// A stream of the samples `registry` takes from now on (the samples it
    /// holds already are not written). No file is created before the first
    /// sample.
    pub fn new(settings: StreamSettings, registry: &MetricsRegistry) -> Self {
        let steps = registry.steps();
        Self {
            settings,
            section: None,
            seen: (steps.epoch(), steps.total_pushed()),
            files: Vec::new(),
            rows: 0,
            last_flush: Instant::now(),
            dirty: false,
        }
    }

    /// The files written so far, oldest first.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// The rows written so far, all files together.
    pub fn rows(&self) -> u64 {
        self.rows
    }

    /// Write the samples `registry` took since the last call. Returns the
    /// path of a file this call started (the first sample, a new world or
    /// another set of series).
    ///
    /// The caller feeds the stream at least once per registry capacity of
    /// samples (the physics loop and the headless run feed it after every
    /// sample). Samples the registry evicted before they were written cannot
    /// be written any more: that is an error, so the file never has a silent
    /// gap.
    pub fn record(&mut self, registry: &MetricsRegistry) -> io::Result<Option<PathBuf>> {
        let steps = registry.steps();
        let (epoch, pushed) = (steps.epoch(), steps.total_pushed());
        let taken = if epoch == self.seen.0 {
            pushed.saturating_sub(self.seen.1)
        } else {
            // A new world: every sample since the registry was cleared, the
            // evicted ones included.
            steps.len() as u64 + steps.evicted()
        };
        self.seen = (epoch, pushed);
        let missing = taken.saturating_sub(steps.len() as u64);
        if missing > 0 {
            return Err(io::Error::other(format!(
                "{missing} samples were evicted from the metrics registry before \
                 the stream could write them"
            )));
        }
        let new = taken as usize;
        if new == 0 {
            return Ok(None);
        }

        let columns: Vec<&String> = registry.all_series().map(|(name, _)| name).collect();
        let current = self
            .section
            .as_ref()
            .is_some_and(|s| s.epoch == epoch && s.columns.iter().eq(columns.iter().copied()));
        let started = if current {
            None
        } else {
            self.finish_section()?;
            Some(self.start_section(registry, epoch)?)
        };

        let section = self.section.as_mut().expect("a section was started above");
        let mut text = String::new();
        let len = steps.len();
        for row in len - new..len {
            let from_end = len - row;
            let value = |buffer: &crate::metrics::RingBuffer<f32>| {
                buffer
                    .len()
                    .checked_sub(from_end)
                    .and_then(|i| buffer.get().get(i).copied())
            };
            let step = steps.get()[row];
            let time = registry.times().get().get(row).copied();
            match self.settings.format {
                ExportFormat::Csv => {
                    text.push_str(&step.to_string());
                    text.push(',');
                    text.push_str(&csv_f32(time));
                    for (_, buffer) in registry.all_series() {
                        text.push(',');
                        text.push_str(&csv_f32(value(buffer)));
                    }
                }
                ExportFormat::Jsonl => {
                    let values: serde_json::Map<_, _> = registry
                        .all_series()
                        .map(|(name, buffer)| (name.clone(), json_f32(value(buffer))))
                        .collect();
                    let record = json!({
                        "schema_version": SCHEMA_VERSION,
                        "record_kind": "series_row",
                        "step": step,
                        "time": json_f32(time),
                        "values": values,
                    });
                    text.push_str(&record.to_string());
                }
            }
            text.push('\n');
        }
        section.writer.write_all(text.as_bytes())?;
        self.rows += new as u64;
        self.dirty = true;
        self.flush_if_due()?;
        Ok(started)
    }

    /// Flush the buffered rows if the oldest of them may be older than
    /// [`FLUSH_INTERVAL`].
    pub fn flush_if_due(&mut self) -> io::Result<()> {
        if self.dirty && self.last_flush.elapsed() >= FLUSH_INTERVAL {
            self.flush()?;
        }
        Ok(())
    }

    /// Flush the buffered rows to the file.
    pub fn flush(&mut self) -> io::Result<()> {
        if let Some(section) = &mut self.section {
            section.writer.flush()?;
        }
        self.last_flush = Instant::now();
        self.dirty = false;
        Ok(())
    }

    /// End the stream: flush and close the file. Returns the files written.
    pub fn finish(mut self) -> io::Result<Vec<PathBuf>> {
        self.finish_section()?;
        Ok(std::mem::take(&mut self.files))
    }

    fn finish_section(&mut self) -> io::Result<()> {
        self.flush()?;
        self.section = None;
        Ok(())
    }

    /// Write the header of a new file and open it for appending.
    fn start_section(&mut self, registry: &MetricsRegistry, epoch: u64) -> io::Result<PathBuf> {
        let header = header(registry, self.settings.format);
        let keep_existing = !(self.settings.replace_existing && self.files.is_empty());
        let path = write_atomic_unique(
            &self.settings.dir,
            &self.settings.stem,
            self.settings.format.extension(),
            header.as_bytes(),
            keep_existing,
        )?;
        let file = open_append(&path)?;
        self.files.push(path.clone());
        self.section = Some(Section {
            writer: BufWriter::new(file),
            epoch,
            columns: registry
                .all_series()
                .map(|(name, _)| name.clone())
                .collect(),
        });
        self.last_flush = Instant::now();
        Ok(path)
    }
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().append(true).open(path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("cannot open {} for appending: {e}", path.display()),
        )
    })
}

/// The header of a stream file: the CSV header row, or the JSONL
/// `stream_header` record.
fn header(registry: &MetricsRegistry, format: ExportFormat) -> String {
    let mut out = String::new();
    match format {
        ExportFormat::Csv => {
            out.push_str("step,time");
            for (name, _) in registry.all_series() {
                out.push(',');
                match registry.get_metadata(name).and_then(|m| m.unit) {
                    Some(unit) => out.push_str(&csv_cell(&format!("{name} [{unit}]"))),
                    None => out.push_str(&csv_cell(name)),
                }
            }
        }
        ExportFormat::Jsonl => {
            let series: Vec<_> = registry
                .all_series()
                .map(|(name, _)| {
                    let metadata = registry.get_metadata(name);
                    json!({
                        "name": name,
                        "unit": metadata.and_then(|m| m.unit),
                        "description": metadata.map_or("", |m| m.description),
                        "per_species": metadata.is_some_and(|m| m.is_per_species),
                    })
                })
                .collect();
            let record = json!({
                "schema_version": SCHEMA_VERSION,
                "record_kind": "stream_header",
                "series": series,
            });
            out.push_str(&record.to_string());
        }
    }
    out.push('\n');
    out
}
