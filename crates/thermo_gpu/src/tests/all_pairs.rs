// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The all-pairs kernel against the CPU kernel (`force_simd`) on the same
//! kernel points, on a real adapter. Skipped (passes with a note) without
//! one, e.g. in CI.

use crate::GpuForces;
use thermo_physics::application::MainContent;
use thermo_physics::application::sim_config::PairOverrideConfig;
use thermo_physics::objects2d::Rect;
use thermo_physics::objects2d::ball_impl::{
    KernelPoint, PairForceBackend, PairForces, SPECIES_BITS, SpeciesTable, pair_forces_reference,
};
use thermo_physics::test_support::test_config;

const CELL: [f32; 2] = [8.0, 8.0];
/// GPU and CPU differ by f32 rounding only (reciprocals, `sqrt`, summation
/// order): relative to 1 plus the ball's |virial| and |potential|, which do
/// not cancel like the force sum.
const TOLERANCE: f32 = 1e-5;
/// Cell indices that wrap `u32` columns and reach the last (24-bit) rows.
const FAR: [u32; 2] = [u32::MAX, (1 << 24) - 8];

fn gpu() -> Option<GpuForces> {
    let gpu = GpuForces::headless();
    if gpu.is_none() {
        eprintln!("no GPU adapter: all-pairs kernel test skipped");
    }
    gpu
}

/// A world of the built-in species with the pair `overrides`, for its
/// physics.
fn world(overrides: Vec<PairOverrideConfig>) -> MainContent {
    let mut config = test_config();
    config.potential.pair_overrides = overrides;
    MainContent::with_config(Rect::new(-10.0, 10.0, 20.0, -20.0), &config, None).unwrap()
}

fn species(name: &str) -> u32 {
    SpeciesTable::builtin().by_name(name).unwrap().index() as u32
}

/// The kernel point of `species` at `(x, -y)`, `x, y ≥ 0`, in a frame
/// whose cell `frame` (column, row) has its top left corner at the origin.
fn point(x: f32, y: f32, species: u32, frame: [u32; 2]) -> KernelPoint {
    let (col, row) = ((x / CELL[0]).floor(), (y / CELL[1]).floor());
    KernelPoint {
        x: x - col * CELL[0],
        y: row * CELL[1] - y,
        col: frame[0].wrapping_add(col as u32),
        row_species: ((frame[1] + row as u32) << SPECIES_BITS) | species,
    }
}

/// `n` balls of `species` in turn on a jittered square lattice, dense
/// enough for pairs inside `r_cap`.
fn gas(n: usize, species_names: &[&str], frame: [u32; 2]) -> Vec<KernelPoint> {
    let side = n.isqrt() + 1;
    let mut seed = 1u32;
    let mut jitter = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f32 / (1 << 24) as f32 * 0.4
    };
    (0..n)
        .map(|i| {
            let x = (i % side) as f32 * 1.1 + jitter();
            let y = (i / side) as f32 * 1.1 + jitter();
            point(x, y, species(species_names[i % species_names.len()]), frame)
        })
        .collect()
}

/// A ball's force (x, y), potential, virial and capped count: the fields
/// the kernel writes (comparing `PairForces` would read its padding).
type Fields = [f32; 5];
const CAPPED: usize = 4;

/// The GPU forces of `points` with the physics of `world`, checked against
/// the CPU's.
fn check(gpu: &mut GpuForces, world: &MainContent, points: &[KernelPoint]) -> Vec<Fields> {
    gpu.set_pairs(&world.world().physics().pair_params());
    let mut results = Vec::new();
    gpu.compute(points, CELL, &mut results).unwrap();
    let reference = pair_forces_reference(world.world().physics(), points, CELL);
    assert_eq!(results.len(), reference.len());
    for (i, (g, c)) in results.iter().zip(&reference).enumerate() {
        let eps = TOLERANCE * (1.0 + c.virial.abs() + c.potential.abs());
        let close = |a: f32, b: f32| (a - b).abs() <= eps;
        assert!(
            close(g.force[0], c.force[0])
                && close(g.force[1], c.force[1])
                && close(g.potential, c.potential)
                && close(g.virial, c.virial),
            "ball {i} of {}: GPU {g:?} vs CPU {c:?}",
            points.len()
        );
        assert_eq!(g.capped, c.capped, "ball {i}: capped pairs");
    }
    let fields = |r: &PairForces| {
        [
            r.force[0],
            r.force[1],
            r.potential,
            r.virial,
            r.capped as f32,
        ]
    };
    results.iter().map(fields).collect()
}

#[test]
fn test_one_species_matches_the_cpu_for_any_ball_count() {
    let Some(mut gpu) = gpu() else { return };
    let world = world(Vec::new());
    // Around the workgroup size, and several tiles.
    for n in [1, 63, 64, 65, 300] {
        let results = check(&mut gpu, &world, &gas(n, &["N2"], [0, 0]));
        assert!(n == 1 || results.iter().any(|r| r[CAPPED] > 0.0 && r[2] != 0.0));
    }
    let alone = check(&mut gpu, &world, &gas(1, &["N2"], [0, 0]));
    assert_eq!(alone, [Fields::default()]);
}

#[test]
fn test_mixed_species_match_the_cpu() {
    let Some(mut gpu) = gpu() else { return };
    check(
        &mut gpu,
        &world(Vec::new()),
        &gas(130, &["N2", "Ar", "XL"], [0, 0]),
    );
}

#[test]
fn test_pair_override_matches_the_cpu() {
    let Some(mut gpu) = gpu() else { return };
    let points = gas(130, &["N2", "Ar", "XL"], [0, 0]);
    let overridden = world(vec![PairOverrideConfig {
        a: "N2".into(),
        b: "Ar".into(),
        sigma: Some(1.3),
        epsilon: Some(2.0),
    }]);
    let mixed = check(&mut gpu, &world(Vec::new()), &points);
    assert_ne!(check(&mut gpu, &overridden, &points), mixed);
}

#[test]
fn test_cap_and_cutoff_edges_match_the_cpu() {
    let Some(mut gpu) = gpu() else { return };
    let world = world(Vec::new());
    let ar = species("Ar");
    let pairs = world.world().physics().pair_params();
    let pair = pairs[(ar as usize) * (pairs.len().isqrt() + 1)];
    let cases = [
        (0.5 * pair.r_cap, 1.0),
        (0.999 * pair.r_cap, 1.0),
        (1.001 * pair.r_cap, 0.0),
        (0.999 * pair.cutoff, 0.0),
        (1.001 * pair.cutoff, 0.0),
    ];
    for (r, capped) in cases {
        // Across a cell edge.
        let points = [point(6.0, 1.0, ar, [0, 0]), point(6.0 + r, 1.0, ar, [0, 0])];
        let results = check(&mut gpu, &world, &points);
        assert_eq!(results[0][CAPPED], capped, "r = {r}");
        assert_eq!(results[0] == Fields::default(), r > pair.cutoff, "r = {r}");
    }
}

#[test]
fn test_far_from_the_origin_gives_the_same_forces() {
    let Some(mut gpu) = gpu() else { return };
    let world = world(Vec::new());
    let species = ["N2", "Ar", "XL"];
    let far = check(&mut gpu, &world, &gas(100, &species, FAR));
    assert_eq!(far, check(&mut gpu, &world, &gas(100, &species, [0, 0])));
}

/// `cargo test -p thermo_gpu --profile perf -- --ignored --nocapture timing`
#[test]
#[ignore = "timing, not a check"]
fn timing_of_the_all_pairs_kernel() {
    let Some(mut gpu) = gpu() else { return };
    gpu.set_pairs(&world(Vec::new()).world().physics().pair_params());
    let mut results = Vec::new();
    for n in [1_000, 10_000, 30_000] {
        let points = gas(n, &["N2", "Ar", "XL"], [0, 0]);
        gpu.compute(&points, CELL, &mut results).unwrap();
        let start = std::time::Instant::now();
        for _ in 0..5 {
            gpu.compute(&points, CELL, &mut results).unwrap();
        }
        let ms = start.elapsed().as_secs_f64() * 200.0;
        eprintln!(
            "n = {n}, workgroup {}: {ms:.2} ms/step",
            crate::WORKGROUP_SIZE
        );
    }
}
