// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The GPU kernel as the force backend of this test build (`--cfg
//! gpu_drift`). `thermo_gpu` links the library build of this crate
//! (`thermo_physics::`), whose types are not this test build's (`crate::`)
//! but have the same plain-data layouts, so the slices are cast.

use crate::application::MainContent;
use crate::objects2d::ball_impl::{KernelPoint, PairForceBackend, PairForces, PairParams};
use thermo_gpu::GpuForces;
use thermo_physics::objects2d::ball_impl as lib;

struct Bridge {
    gpu: GpuForces,
    results: Vec<lib::PairForces>,
}

impl PairForceBackend for Bridge {
    fn set_pairs(&mut self, pairs: &[PairParams]) {
        lib::PairForceBackend::set_pairs(&mut self.gpu, bytemuck::cast_slice(pairs));
    }

    fn compute(
        &mut self,
        points: &[KernelPoint],
        cell: [f32; 2],
        results: &mut Vec<PairForces>,
    ) -> Result<(), String> {
        let points = bytemuck::cast_slice(points);
        lib::PairForceBackend::compute(&mut self.gpu, points, cell, &mut self.results)?;
        results.clear();
        results.extend_from_slice(bytemuck::cast_slice(&self.results));
        Ok(())
    }
}

/// Switch `content` to the GPU force pass, unless it is on already or
/// there is no adapter (then the run stays on the CPU kernel).
pub(super) fn switch_on(content: &mut MainContent) {
    if content.force_path().is_some() {
        return;
    }
    match GpuForces::headless() {
        Some(gpu) => content.set_force_backend(Some(Box::new(Bridge {
            gpu,
            results: Vec::new(),
        }))),
        None => println!("DRIFT_FORCES no GPU adapter: the CPU kernel runs"),
    }
}
