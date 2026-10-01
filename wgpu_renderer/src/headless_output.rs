use crate::headless_cli::{DetailLevel, HeadlessConfig, OutputFormat};
use rust_thermo_core::application::renderer_snapshot::{
    OperatorSnapshot, RendererSnapshot, ThermoGridSpec,
};
use rust_thermo_core::application::thermo_field::ThermoGrid;
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
        let line = run_start_to_jsonl(&self.config, snapshot);
        self.write_machine_record(&line)
    }

    fn write_frame(&mut self, snapshot: &RendererSnapshot) -> io::Result<()> {
        if self.config.stdout {
            let console_line = frame_to_console_summary(snapshot, self.initial_total_energy);
            self.write_stdout_line(&console_line)?;
        }
        if self.config.output.is_some() {
            let jsonl_line =
                frame_to_jsonl_with_config(&self.config, snapshot, self.initial_total_energy);
            self.write_file_line(&jsonl_line)?;
        }
        Ok(())
    }

    fn write_run_end(
        &mut self,
        final_snapshot: &RendererSnapshot,
        snapshots_emitted: u32,
    ) -> io::Result<()> {
        let line = run_end_to_jsonl(final_snapshot, snapshots_emitted);
        self.write_machine_record(&line)
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

    fn write_stdout_line(&mut self, line: &str) -> io::Result<()> {
        if let Some(stdout) = self.stdout.as_mut() {
            writeln!(stdout, "{line}")?;
        }
        Ok(())
    }

    fn write_file_line(&mut self, line: &str) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            writeln!(file, "{line}")?;
        }
        Ok(())
    }

    fn write_machine_record(&mut self, line: &str) -> io::Result<()> {
        if self.config.stdout {
            self.write_stdout_line(line)?;
        }
        self.write_file_line(line)?;
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
            "seed_speed": config.seed_speed.map_or(serde_json::Value::Null, f32_value),
            "world_scale": config.world_scale.map_or(serde_json::Value::Null, f32_value),
            "seeded_constructor": config.rng_seed.is_some(),
            "stdout": config.stdout,
            "has_output_file": config.output.is_some(),
            "dt": config.dt.map_or(serde_json::Value::Null, f32_value),
            "line_segments": config
                .line_segments
                .iter()
                .map(|segment| {
                    json!({
                        "start": [f32_value(segment.start.0), f32_value(segment.start.1)],
                        "end": [f32_value(segment.end.0), f32_value(segment.end.1)],
                        "thickness": f32_value(segment.thickness),
                    })
                })
                .collect::<Vec<_>>(),
        },
        "initial": {
            "step": snapshot.simulation_step,
            "time": f32_value(snapshot.time),
            "ball_count": snapshot.balls.len(),
            "gravity_force_multiple": snapshot.gravity_force_multiple,
            "operator_counts": operator_counts(&snapshot.operators),
            "world_rect": world_rect_to_json(&snapshot.cell_grid_bounds),
        },
    })
    .to_string()
}

/// World bounds, so `--line-segment` coordinates can be chosen from a run.
fn world_rect_to_json(rect: &rust_thermo_core::objects2d::Rect) -> serde_json::Value {
    let (left, top) = rect.get_tl();
    let (right, bottom) = rect.get_br();
    json!({
        "left": f32_value(left),
        "top": f32_value(top),
        "right": f32_value(right),
        "bottom": f32_value(bottom),
    })
}

fn frame_to_jsonl(snapshot: &RendererSnapshot, initial_total_energy: Option<f32>) -> String {
    frame_to_jsonl_with_config(&HeadlessConfig::default(), snapshot, initial_total_energy)
}

fn frame_to_jsonl_with_config(
    config: &HeadlessConfig,
    snapshot: &RendererSnapshot,
    initial_total_energy: Option<f32>,
) -> String {
    let total_energy = snapshot.kinetic_energy + snapshot.potential_energy;
    let delta_energy = initial_total_energy.map(|initial| total_energy - initial);
    let mut frame = json!({
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
    });

    if config.detail.includes_balls() {
        let mut balls: Vec<_> = snapshot.balls.iter().collect();
        balls.sort_unstable_by_key(|ball| ball.id);
        let ball_entries = balls
            .into_iter()
            .map(|ball| {
                json!({
                    "id": ball.id,
                    "size_idx": ball.size_idx,
                    "size_class": ball.size_idx,
                    "position": {
                        "x": f32_value(ball.position.x),
                        "y": f32_value(ball.position.y),
                    },
                    "velocity": {
                        "vx": f32_value(ball.velocity.x),
                        "vy": f32_value(ball.velocity.y),
                    },
                    "radius": f32_value(ball.radius),
                    "mass": f32_value(ball.mass),
                })
            })
            .collect::<Vec<_>>();
        frame["balls"] = serde_json::Value::Array(ball_entries);
    }

    if config.detail.includes_thermo() {
        frame["thermo"] = thermo_to_json(snapshot);
    }

    frame.to_string()
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
        operator_counts.valve_spans.len(),
        operator_counts.source_spheres.len(),
        operator_counts.thermostats.len(),
    )
}

fn operator_counts(operators: &OperatorSnapshot) -> serde_json::Value {
    json!({
        "line_segments": operators.line_segments.len(),
        "valve_segments": operators.valve_spans.len(),
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
        DetailLevel::Thermo => "thermo",
        DetailLevel::BallsAndThermo => "balls+thermo",
    }
}

fn thermo_to_json(snapshot: &RendererSnapshot) -> serde_json::Value {
    let mut thermo = json!({
        "temperature": thermo_grid_to_json(&snapshot.cell_temperature),
        "pressure": thermo_grid_to_json(&snapshot.cell_pressure),
    });

    if let Some(spec) = &snapshot.thermo_grid_spec {
        thermo["grid_spec"] = thermo_grid_spec_to_json(spec);
    }

    thermo
}

fn thermo_grid_to_json(grid: &ThermoGrid) -> serde_json::Value {
    let (rows, cols) = grid.shape();
    let values = grid
        .as_slice()
        .iter()
        .map(|value| f32_value(*value))
        .collect::<Vec<_>>();

    json!({
        "rows": rows,
        "cols": cols,
        "values": values,
    })
}

fn thermo_grid_spec_to_json(spec: &ThermoGridSpec) -> serde_json::Value {
    json!({
        "rows": spec.rows,
        "cols": spec.cols,
        "cell_width": f32_value(spec.cell_width),
        "cell_height": f32_value(spec.cell_height),
        "origin_x": f32_value(spec.origin_x),
        "origin_y": f32_value(spec.origin_y),
    })
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
