//! Per-phase step timings of a headless run (`--timings`, #221).
//!
//! One JSON object per run. All times are milliseconds per measured step; the
//! warmup steps are not counted.
use rust_thermo_core::application::phase_timings::PhaseTimings;
use rust_thermo_core::application::renderer_snapshot::RendererSnapshot;
use serde_json::json;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// Time the runner spends outside the physics step, from the first measured step.
pub struct RunClock {
    pub started: Instant,
    pub warmup_steps: u32,
    /// Runner loop iterations since the start, each one `simulation_step` call.
    pub steps: u64,
    /// `create_snapshot`.
    pub snapshot: Duration,
    /// Writing the snapshot records.
    pub output: Duration,
    pub snapshots: u32,
}

impl RunClock {
    pub fn start(warmup_steps: u32) -> Self {
        Self {
            started: Instant::now(),
            warmup_steps,
            steps: 0,
            snapshot: Duration::ZERO,
            output: Duration::ZERO,
            snapshots: 0,
        }
    }
}

pub fn timings_to_json(
    clock: &RunClock,
    wall: Duration,
    phases: &PhaseTimings,
    snapshot: &RendererSnapshot,
) -> serde_json::Value {
    let steps = clock.steps.max(1) as f64;
    let ms = |d: Duration| d.as_secs_f64() * 1e3 / steps;
    // Whatever the phases do not cover: cell bookkeeping between phases,
    // the runner loop itself.
    let covered = phases.total() + clock.snapshot + clock.output;
    json!({
        "schema_version": "headless-timings.v1",
        "commit": env!("GIT_COMMIT"),
        "balls": snapshot.balls.len(),
        "wall_pieces": snapshot.operators.line_segments.len(),
        "valves": snapshot.operators.valve_spans.len(),
        "threads": rayon::current_num_threads(),
        "warmup_steps": clock.warmup_steps,
        "steps": clock.steps,
        "snapshots": clock.snapshots,
        "wall_ms": wall.as_secs_f64() * 1e3,
        "ms_per_step": {
            "total": ms(wall),
            "neighbours": ms(phases.neighbours),
            "positions": ms(phases.positions),
            "forces": ms(phases.forces),
            "apply": ms(phases.apply),
            "cells": ms(phases.cells),
            "thermo": ms(phases.thermo),
            "snapshot": ms(clock.snapshot),
            "output": ms(clock.output),
            "other": ms(wall.saturating_sub(covered)),
        },
    })
}

pub fn write_timings(
    path: &Path,
    clock: &RunClock,
    wall: Duration,
    phases: &PhaseTimings,
    snapshot: &RendererSnapshot,
) -> io::Result<()> {
    // A paused step runs no phase: per-step times would mix measured and
    // empty steps, so such a run is no benchmark.
    if phases.steps != clock.steps {
        return Err(io::Error::other(format!(
            "--timings: only {} of {} measured steps ran (simulation paused)",
            phases.steps, clock.steps
        )));
    }
    let value = timings_to_json(clock, wall, phases, snapshot);
    std::fs::write(path, format!("{value}\n"))
}
