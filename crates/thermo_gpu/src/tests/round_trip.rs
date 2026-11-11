// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Upload points and a pair table, copy them into the results with a test
//! kernel and read them back, on a real adapter. Skipped (passes with a
//! note) without one, e.g. in CI.

use crate::GpuForces;
use thermo_physics::objects2d::ball_impl::{KernelPoint, PairForceBackend, PairForces, PairParams};

/// Copies every binding into the results: the offset times the cell size,
/// the column, `σ` of the pair (own species, 0), and `row_species`.
const COPY: &str = "
@compute @workgroup_size(WORKGROUP_SIZE)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= uniforms.balls {
        return;
    }
    let p = points[i];
    let pair = pairs[(p.row_species & SPECIES_MASK) * uniforms.species];
    results[i] = PairForces(vec2(p.x, p.y) * uniforms.cell, f32(p.col), pair.sigma, p.row_species);
}
";

const SPECIES: u32 = 3;
const CELL: [f32; 2] = [2.0, 3.0];

fn points(n: u32) -> Vec<KernelPoint> {
    (0..n)
        .map(|i| KernelPoint {
            x: i as f32 * 0.5,
            y: -(i as f32),
            col: 1000 + i,
            row_species: (i << 8) | (i % SPECIES),
        })
        .collect()
}

/// Pair `(a, b)` has `σ = 10 a + b`.
fn pairs() -> Vec<PairParams> {
    (0..SPECIES * SPECIES)
        .map(|k| PairParams {
            sigma: (10 * (k / SPECIES) + k % SPECIES) as f32,
            ..Default::default()
        })
        .collect()
}

fn check(gpu: &mut GpuForces, pipeline: &wgpu::ComputePipeline, n: u32) {
    let points = points(n);
    gpu.upload_points(&points, CELL);
    let mut results = vec![PairForces::default(); 3];
    gpu.run(pipeline, &mut results);
    assert_eq!(results.len(), n as usize);
    for (p, r) in points.iter().zip(&results) {
        assert_eq!(r.force, [p.x * CELL[0], p.y * CELL[1]]);
        assert_eq!(r.potential, p.col as f32);
        assert_eq!(r.virial, (10 * (p.row_species & 0xff)) as f32);
        assert_eq!(r.capped, p.row_species);
    }
}

#[test]
fn test_points_and_pairs_round_trip_through_the_gpu() {
    let Some(mut gpu) = GpuForces::headless() else {
        eprintln!("no GPU adapter: round trip skipped");
        return;
    };
    let pipeline = crate::pipeline(&gpu.device, &gpu.layout, COPY);
    gpu.set_pairs(&pairs());
    // Not a multiple of the workgroup size, then growing the buffers, then
    // shrinking the count, one ball and none.
    for n in [100, 1000, 70, 1, 0] {
        check(&mut gpu, &pipeline, n);
    }
}
