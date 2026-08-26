use crate::headless_cli::{DetailLevel, HeadlessConfig, OutputFormat};
use rust_thermo_core::application::renderer_snapshot::{OperatorSnapshot, RendererSnapshot};
use serde_json::json;
use std::fs::File;
use std::io::{self, BufWriter, Write};

pub trait HeadlessSnapshotSink {
    fn write_run_start(&mut self, snapshot: &RendererSnapshot) -> io::Result<()>;
    fn write_frame(&mut self, snapshot: &RendererSnapshot) -> io::Result<()>;
    fn write_run_end(
        &mut self,
        final_snapshot: &RendererSnapshot,
        snapshots_emitted: u32,
    ) -> io::Result<()>;
}

pub struct HeadlessWriter {
    stdout: Option<BufWriter<io::Stdout>>,
    file: Option<BufWriter<File>>,
    config: HeadlessConfig,
    initial_total_energy: Option<f32>,
}

impl HeadlessWriter {
    pub fn builder() -> HeadlessWriterBuilder {
        HeadlessWriterBuilder::default()
    }
}

impl HeadlessSnapshotSink for HeadlessWriter {
    fn write_run_start(&mut self, snapshot: &RendererSnapshot) -> io::Result<()> {
        self.initial_total_energy = Some(snapshot.kinetic_energy + snapshot.potential_energy);
        self.write_line(run_start_to_jsonl(&self.config, snapshot))
    }

    fn write_frame(&mut self, snapshot: &RendererSnapshot) -> io::Result<()> {
        if self.config.stdout {
            self.write_line(frame_to_console_summary(
                snapshot,
                self.initial_total_energy,
            ))?;
        }
        if self.config.output.is_some() {
            self.write_line(frame_to_jsonl(snapshot, self.initial_total_energy))?;
        }
        Ok(())
    }

    fn write_run_end(
        &mut self,
        final_snapshot: &RendererSnapshot,
        snapshots_emitted: u32,
    ) -> io::Result<()> {
        self.write_line(run_end_to_jsonl(final_snapshot, snapshots_emitted))
    }
}

#[derive(Default)]
pub struct HeadlessWriterBuilder {
    stdout_enabled: bool,
    output: Option<std::path::PathBuf>,
    config: Option<HeadlessConfig>,
}

impl HeadlessWriterBuilder {
    pub fn stdout(mut self, enabled: bool) -> Self {
        self.stdout_enabled = enabled;
        self
    }

    pub fn output(mut self, output: Option<std::path::PathBuf>) -> Self {
        self.output = output;
        self
    }

    pub fn config(mut self, config: HeadlessConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn build(self) -> io::Result<HeadlessWriter> {
        let stdout = if self.stdout_enabled {
            Some(BufWriter::new(io::stdout()))
        } else {
            None
        };
        let file = match self.output {
            Some(path) => Some(BufWriter::new(File::create(path)?)),
            None => None,
        };
        let config = self.config.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "HeadlessWriterBuilder::config(...) must be provided to emit correct run_start metadata",
            )
        })?;
        Ok(HeadlessWriter {
            stdout,
            file,
            config,
            initial_total_energy: None,
        })
    }
}

impl HeadlessWriter {
    fn write_line(&mut self, line: String) -> io::Result<()> {
        if let Some(stdout) = self.stdout.as_mut() {
            writeln!(stdout, "{line}")?;
        }
        if let Some(file) = self.file.as_mut() {
            writeln!(file, "{line}")?;
        }
        Ok(())
    }
}

/// Versioned, machine-readable headless JSONL schema ("headless.v1").
///
/// Records are newline-delimited JSON objects with a `schema_version` and `record_kind`.
/// Consumers should treat unknown `record_kind` values and unknown fields as forward-compatible.
fn run_start_to_jsonl(config: &HeadlessConfig, snapshot: &RendererSnapshot) -> String {
    json!({
        "schema_version": "headless.v1",
        "record_kind": "run_start",
        "config": {
            "steps": config.steps,
            "snapshot_every": config.snapshot_every,
            "output_format": output_format_name(config.format),
            "detail_level": detail_level_name(config.detail),
            "rng_seed": config.rng_seed,
            "seed_balls": config.seed_balls,
            "stdout": config.stdout,
            "has_output_file": config.output.is_some(),
        },
        "initial": {
            "step": snapshot.simulation_step,
            "time": f32_value(snapshot.time),
            "ball_count": snapshot.balls.len(),
            "gravity_force_multiple": snapshot.gravity_force_multiple,
            "operator_counts": operator_counts(&snapshot.operators),
        },
    })
    .to_string()
}

fn frame_to_jsonl(snapshot: &RendererSnapshot, initial_total_energy: Option<f32>) -> String {
    fn f32_value(value: f32) -> serde_json::Value {
        if value.is_finite() {
            json!(value)
        } else {
            serde_json::Value::Null
        }
    }

    let total_energy = snapshot.kinetic_energy + snapshot.potential_energy;
    let delta_energy = initial_total_energy.map(|initial| total_energy - initial);

    json!({
        "schema_version": "headless.v1",
        "record_kind": "frame",
        "step": snapshot.simulation_step,
        "time": f32_value(snapshot.time),
        "kinetic_energy": f32_value(snapshot.kinetic_energy),
        "potential_energy": f32_value(snapshot.potential_energy),
        "delta_energy": delta_energy.map_or(serde_json::Value::Null, f32_value),
        "ball_count": snapshot.balls.len(),
        "gravity_force_multiple": snapshot.gravity_force_multiple,
        "physics_fps": f32_value(snapshot.physics_fps),
        "operator_counts": operator_counts(&snapshot.operators),
    })
    .to_string()
}

fn run_end_to_jsonl(snapshot: &RendererSnapshot, snapshots_emitted: u32) -> String {
    json!({
        "schema_version": "headless.v1",
        "record_kind": "run_end",
        "final_step": snapshot.simulation_step,
        "final_time": f32_value(snapshot.time),
        "snapshots_emitted": snapshots_emitted,
        "final_ball_count": snapshot.balls.len(),
        "operator_counts": operator_counts(&snapshot.operators),
    })
    .to_string()
}

fn frame_to_console_summary(
    snapshot: &RendererSnapshot,
    initial_total_energy: Option<f32>,
) -> String {
    let total_energy = snapshot.kinetic_energy + snapshot.potential_energy;
    let delta_energy = initial_total_energy
        .map(|initial| total_energy - initial)
        .unwrap_or(0.0);
    let operator_counts = &snapshot.operators;

    format!(
        "step={} time={:.5} balls={} kinetic={:.5} potential={:.5} total={:.5} dE={:.5} operators={}/{}/{}/{}",
        snapshot.simulation_step,
        snapshot.time,
        snapshot.balls.len(),
        snapshot.kinetic_energy,
        snapshot.potential_energy,
        total_energy,
        delta_energy,
        operator_counts.line_segments.len(),
        operator_counts.valve_segments.len(),
        operator_counts.source_spheres.len(),
        operator_counts.thermostats.len(),
    )
}

fn operator_counts(operators: &OperatorSnapshot) -> serde_json::Value {
    json!({
        "line_segments": operators.line_segments.len(),
        "valve_segments": operators.valve_segments.len(),
        "source_spheres": operators.source_spheres.len(),
        "thermostats": operators.thermostats.len(),
    })
}

fn output_format_name(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Jsonl => "jsonl",
    }
}

fn detail_level_name(detail: DetailLevel) -> &'static str {
    match detail {
        DetailLevel::Summary => "summary",
        DetailLevel::Balls => "balls",
    }
}

fn f32_value(value: f32) -> serde_json::Value {
    if value.is_finite() {
        json!(value)
    } else {
        serde_json::Value::Null
    }
}

#[cfg(test)]
mod tests;
