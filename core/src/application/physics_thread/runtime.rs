use super::ShutdownSignal;
use super::command_handler::PhysicsCommandHandler;
use crate::application::GameLogic;
use crate::application::MainContent;
use crate::application::physics_thread::PhysicsCommand;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

pub(crate) trait CommandHandler<C> {
    fn handle_command(&mut self, command: C);
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
    }

    pub(crate) fn run(
        &self,
        simulation: &mut MainContent,
        receiver: &Receiver<PhysicsCommand>,
        shutdown_signal: &ShutdownSignal,
        wakeup: &Arc<dyn Fn() + Send + Sync>,
    ) {
        println!(
            "Physics thread ID: {:?} - Started!",
            std::thread::current().id()
        );
        println!("Entering physics loop (running as fast as possible)...");

        let mut last_snapshot_time = Instant::now();
        let mut physics_step_count: u64 = 0;
        let mut last_perf_report = Instant::now();
        let mut snapshot_time_total = Duration::ZERO;
        let mut snapshot_count = 0u64;

        while !shutdown_signal.should_shutdown() {
            {
                let mut handler = PhysicsCommandHandler::new(simulation);
                self.drain_commands(receiver, &mut handler);
            }

            let wait_time_ms = simulation.get_wait_time();
            if wait_time_ms > 0 {
                std::thread::sleep(Duration::from_millis(wait_time_ms));
            }

            let was_paused = simulation.sim.is_paused();
            let _ = simulation.simulation_step();
            if !was_paused {
                physics_step_count += 1;
            }

            let now = Instant::now();
            if now.duration_since(last_snapshot_time) >= self.snapshot_interval {
                let snapshot_start = Instant::now();
                simulation.publish_snapshot();
                wakeup();

                snapshot_time_total += snapshot_start.elapsed();
                snapshot_count += 1;
                last_snapshot_time = now;
            }

            if now.duration_since(last_perf_report).as_secs() >= 1 {
                let elapsed = now.duration_since(last_perf_report).as_secs_f32();
                let physics_fps = physics_step_count as f32 / elapsed;

                simulation.set_physics_fps(physics_fps);

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
