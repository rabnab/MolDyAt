use rust_thermo_core::application::renderer_snapshot::RendererSnapshot;
use serde_json::json;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

pub struct HeadlessWriter {
    stdout: Option<BufWriter<io::Stdout>>,
    file: Option<BufWriter<File>>,
}

impl HeadlessWriter {
    pub fn new(stdout_enabled: bool, output: Option<&Path>) -> io::Result<Self> {
        let stdout = if stdout_enabled {
            Some(BufWriter::new(io::stdout()))
        } else {
            None
        };
        let file = match output {
            Some(path) => Some(BufWriter::new(File::create(path)?)),
            None => None,
        };
        Ok(Self { stdout, file })
    }

    pub fn write_snapshot(&mut self, snapshot: &RendererSnapshot) -> io::Result<()> {
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

fn snapshot_to_jsonl(snapshot: &RendererSnapshot) -> String {
    json!({
        "simulation_step": snapshot.simulation_step,
        "time": snapshot.time,
        "kinetic_energy": snapshot.kinetic_energy,
        "potential_energy": snapshot.potential_energy,
        "ball_count": snapshot.balls.len(),
        "gravity_force_multiple": snapshot.gravity_force_multiple,
        "physics_fps": snapshot.physics_fps,
    })
    .to_string()
}
