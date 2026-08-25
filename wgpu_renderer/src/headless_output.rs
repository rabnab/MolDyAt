use rust_thermo_core::application::renderer_snapshot::RendererSnapshot;
use serde_json::json;
use std::fs::File;
use std::io::{self, BufWriter, Write};

pub trait HeadlessSnapshotSink {
    fn write_snapshot(&mut self, snapshot: &RendererSnapshot) -> io::Result<()>;
}

pub struct HeadlessWriter {
    stdout: Option<BufWriter<io::Stdout>>,
    file: Option<BufWriter<File>>,
}

impl HeadlessWriter {
    pub fn builder() -> HeadlessWriterBuilder {
        HeadlessWriterBuilder::default()
    }
}

impl HeadlessSnapshotSink for HeadlessWriter {
    fn write_snapshot(&mut self, snapshot: &RendererSnapshot) -> io::Result<()> {
        let line = snapshot_to_jsonl(snapshot);
        if let Some(stdout) = self.stdout.as_mut() {
            writeln!(stdout, "{line}")?;
        }
        if let Some(file) = self.file.as_mut() {
            writeln!(file, "{line}")?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct HeadlessWriterBuilder {
    stdout_enabled: bool,
    output: Option<std::path::PathBuf>,
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
        Ok(HeadlessWriter { stdout, file })
    }
}

fn snapshot_to_jsonl(snapshot: &RendererSnapshot) -> String {
    fn f32_value(value: f32) -> serde_json::Value {
        if value.is_finite() {
            json!(value)
        } else {
            serde_json::Value::Null
        }
    }

    json!({
        "simulation_step": snapshot.simulation_step,
        "time": f32_value(snapshot.time),
        "kinetic_energy": f32_value(snapshot.kinetic_energy),
        "potential_energy": f32_value(snapshot.potential_energy),
        "ball_count": snapshot.balls.len(),
        "gravity_force_multiple": snapshot.gravity_force_multiple,
        "physics_fps": f32_value(snapshot.physics_fps),
    })
    .to_string()
}
