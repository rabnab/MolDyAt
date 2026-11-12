// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Run-time options of the physics thread that are not part of the world
//! (#305, plan item M2): how the run is shown and stepped, and whether the
//! GPU computes the pair forces. They live in `MainContent`, so a load,
//! which replaces the world, keeps them, and they are never saved.

use super::SimulationWorld;
use crate::application::sim_stats::ForcePath;
use crate::objects2d::ball_impl::{PairForceBackend, PairForces, PairParams};

const NO_GPU: ForcePath = ForcePath::Cpu("no GPU");

/// The opt-in GPU force pass (`--gpu-forces`).
pub(crate) struct GpuPass {
    /// `None` without an adapter and after a GPU error.
    backend: Option<Box<dyn PairForceBackend>>,
    /// The pair table the backend holds.
    pairs: Vec<PairParams>,
    results: Vec<PairForces>,
    pub(crate) path: ForcePath,
}

impl GpuPass {
    pub(crate) fn new(backend: Option<Box<dyn PairForceBackend>>) -> Self {
        Self {
            // Before the first step: what it will try.
            path: if backend.is_some() {
                ForcePath::Gpu
            } else {
                NO_GPU
            },
            backend,
            pairs: Vec::new(),
            results: Vec::new(),
        }
    }

    /// The pair-table forces of every ball of `world` from the backend, or
    /// `None` where this step stays on the CPU kernel: without a backend,
    /// with `interacting` species (their pairs are not in the pair table)
    /// and while a ball shows its neighbour lines.
    pub(crate) fn compute(
        &mut self,
        world: &SimulationWorld,
        interacting: bool,
    ) -> Option<&[PairForces]> {
        self.path = match &mut self.backend {
            None => NO_GPU,
            Some(_) if interacting => ForcePath::Cpu("molecules or water present"),
            Some(_) if world.balls.iter().any(|ball| ball.verbose.lines()) => {
                ForcePath::Cpu("neighbour lines shown")
            }
            Some(backend) => {
                let pairs = world.physics.pair_params();
                if pairs != self.pairs {
                    backend.set_pairs(&pairs);
                    self.pairs = pairs;
                }
                let points = &world.cache.kernel_points;
                match backend.compute(&points.points, points.cell, &mut self.results) {
                    Ok(()) => ForcePath::Gpu,
                    Err(error) => {
                        log::warn!("GPU forces failed, the CPU kernel takes over: {error}");
                        self.backend = None;
                        NO_GPU
                    }
                }
            }
        };
        (self.path == ForcePath::Gpu).then_some(&self.results)
    }
}

/// Display and stepping options, toggled by keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeOptions {
    /// Status text in SI units instead of reduced units (key `D`).
    pub(crate) real_data: bool,
    /// Pause after every step (key `S`).
    pub(crate) step_by_step: bool,
    /// Log the time of every step phase (key `F4`, with the `measure-time`
    /// feature).
    pub show_timings: bool,
    /// Build the status text of the ball statistics overlay (key `B`).
    pub(crate) show_ball_stats: bool,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            real_data: false,
            step_by_step: false,
            show_timings: false,
            show_ball_stats: true,
        }
    }
}
