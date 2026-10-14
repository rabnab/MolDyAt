// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::ShutdownSignal;
use super::command_handler::DomainCommandHandler;
use crate::application::DomainCommand;
use crate::application::MainContent;
use crate::application::PhysicsEvent;
use crate::application::core::SimulationEngine;
use crate::application::scene_layers::SceneLayers;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

pub(crate) trait CommandHandler<C> {
    fn handle_command(&mut self, command: C);

    /// Called once the queue is drained, to finish work held back while
    /// handling commands.
    fn flush(&mut self) {}
}

pub(crate) struct PhysicsRuntime {
    snapshot_interval: Duration,
}

impl PhysicsRuntime {
    pub(crate) fn new(snapshot_interval: Duration) -> Self {
        Self { snapshot_interval }
    }

    pub(crate) fn drain_commands<C, H>(&self, receiver: &Receiver<C>, handler: &mut H)
    where
        H: CommandHandler<C>,
    {
        while let Ok(command) = receiver.try_recv() {
            handler.handle_command(command);
        }
        handler.flush();
    }

    pub(crate) fn run(
        &self,
        simulation: &mut MainContent,
        receiver: &Receiver<DomainCommand>,
        events: &Sender<PhysicsEvent>,
        shutdown_signal: &ShutdownSignal,
        wakeup: &Arc<dyn Fn() + Send + Sync>,
    ) {
        log::info!(
            "Physics thread ID: {:?} - Started!",
            std::thread::current().id()
        );
        log::info!("Entering physics loop (running as fast as possible)...");

        let mut last_snapshot_time = Instant::now();
        let mut physics_step_count: u64 = 0;
        let mut last_perf_report = Instant::now();
        let mut snapshot_time_total = Duration::ZERO;
        let mut snapshot_count = 0u64;
        // Whether the world changed since the last published snapshot. A
        // snapshot is published only then, so a paused, untouched world
        // publishes nothing and the renderer stays idle.
        let mut changed = true;

        while !shutdown_signal.should_shutdown() {
            let world_resized = {
                let mut handler = DomainCommandHandler::new(simulation, events);
                self.drain_commands(receiver, &mut handler);
                changed |= handler.commands_handled() > 0;
                handler.resizes_applied() > 0
            };

            let wait_time_ms = simulation.runtime().wait_time();
            if wait_time_ms > 0 {
                std::thread::sleep(Duration::from_millis(wait_time_ms));
            }

            let was_paused = simulation.world().sim.is_paused();
            {
                let (world, step_config) = simulation.world_and_step_config();
                let _ = SimulationEngine::simulation_step(world, step_config);
            }
            if !was_paused {
                physics_step_count += 1;
                changed = true;
                // The heatmap's fields, only while the heatmap is shown, and
                // sampled every few steps (see `record_thermo_fields_if_due`).
                if simulation
                    .runtime()
                    .snapshot_layers()
                    .contains(SceneLayers::THERMO)
                {
                    SimulationEngine::record_thermo_fields_if_due(simulation.world_mut());
                }
            }

            let now = Instant::now();
            // The renderer already draws the new window size, around the old
            // world until it gets a snapshot of the resized one: don't make it
            // wait for the next interval.
            if changed
                && (world_resized
                    || now.duration_since(last_snapshot_time) >= self.snapshot_interval)
            {
                let snapshot_start = Instant::now();
                let snapshot = simulation.create_snapshot();
                simulation.runtime().publish_snapshot(snapshot);
                wakeup();

                snapshot_time_total += snapshot_start.elapsed();
                snapshot_count += 1;
                last_snapshot_time = now;
                changed = false;
            }

            if now.duration_since(last_perf_report).as_secs() >= 1 {
                let elapsed = now.duration_since(last_perf_report).as_secs_f32();
                let physics_fps = physics_step_count as f32 / elapsed;

                // The rate is shown in the stats text: publish when it moves
                // (it drops to zero once after a pause).
                changed |= physics_fps != simulation.runtime().physics_fps();
                simulation.runtime_mut().set_physics_fps(physics_fps);

                if snapshot_count > 0 {
                    let avg_snapshot_us = snapshot_time_total.as_micros() / snapshot_count as u128;
                    let snapshot_hz = snapshot_count as f32 / elapsed;
                    let total_overhead_pct = (snapshot_time_total.as_secs_f32() / elapsed) * 100.0;
                    log::debug!(
                        "Physics thread: {:.0} steps/sec | Snapshots: {:.1} Hz, {:.0} µs avg ({:.1}% overhead)",
                        physics_fps,
                        snapshot_hz,
                        avg_snapshot_us,
                        total_overhead_pct
                    );
                } else {
                    log::debug!("Physics thread: {:.0} steps/sec", physics_fps);
                }

                physics_step_count = 0;
                snapshot_time_total = Duration::ZERO;
                snapshot_count = 0;
                last_perf_report = now;
            }

            std::thread::yield_now();
        }
    }
}

impl Default for PhysicsRuntime {
    fn default() -> Self {
        Self::new(Duration::from_micros(16667))
    }
}
