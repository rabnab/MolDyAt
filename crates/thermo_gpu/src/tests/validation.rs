// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The GPU force pass against the CPU kernel in running worlds (#361), on
//! a real adapter; skipped (passes with a note) without one. The CPU side
//! is the step's own kernel with its Verlet lists, not the all-pairs
//! reference of `all_pairs.rs`.

use crate::GpuForces;
use thermo_physics::application::MainContent;
use thermo_physics::application::observables::{Obs, Values};
use thermo_physics::application::sim_config::PairOverrideConfig;
use thermo_physics::application::sim_stats::ForcePath;
use thermo_physics::objects2d::Rect;
use thermo_physics::test_support::test_config;

/// Per ball, relative to 1 plus its |virial| and |potential|: f32 rounding
/// of the two kernels.
const TOLERANCE: f32 = 1e-5;

fn gpu() -> Option<GpuForces> {
    let gpu = GpuForces::headless();
    if gpu.is_none() {
        eprintln!("no GPU adapter: validation skipped");
    }
    gpu
}

/// A seeded world of `side` x `side` with `count` balls of `species` (equal
/// weights) at speeds up to `speed`.
fn world(
    side: f32,
    count: usize,
    speed: f32,
    species: &[&str],
    overrides: Vec<PairOverrideConfig>,
) -> MainContent {
    let mut config = test_config();
    config.potential.pair_overrides = overrides;
    let rect = Rect::new(-side / 2.0, side / 2.0, side, -side);
    let mut mc = MainContent::with_config(rect, &config, Some(7)).unwrap();
    let mixture: Vec<_> = species.iter().map(|s| (s.to_string(), 1)).collect();
    mc.seed_random_mixture(count, speed, &mixture).unwrap();
    mc
}

fn on_gpu(mut mc: MainContent, gpu: GpuForces) -> MainContent {
    mc.set_force_backend(Some(Box::new(gpu)));
    mc
}

/// Two equal worlds run `warmup` steps on the CPU; then one takes a step
/// on the GPU, the other on the CPU, from the same state. Every ball's
/// forces of that step must agree. Returns the capped pairs of the step.
fn check_step(make: impl Fn() -> MainContent, warmup: usize) -> u64 {
    let Some(gpu) = gpu() else { return 1 };
    let (mut cpu, mut other) = (make(), make());
    for _ in 0..warmup {
        cpu.simulation_step().unwrap();
        other.simulation_step().unwrap();
    }
    let mut gpu = on_gpu(other, gpu);
    cpu.simulation_step().unwrap();
    gpu.simulation_step().unwrap();
    assert_eq!(gpu.force_path(), Some(ForcePath::Gpu));

    let (a, b) = (
        cpu.world().cached_force_results(),
        gpu.world().cached_force_results(),
    );
    assert_eq!(a.len(), b.len());
    for (i, (c, g)) in a.iter().zip(b).enumerate() {
        let eps = TOLERANCE * (1.0 + c.virial.abs() + c.potential_energy.abs());
        let close = |x: f32, y: f32| (x - y).abs() <= eps;
        assert!(
            close(g.force.x, c.force.x)
                && close(g.force.y, c.force.y)
                && close(g.potential_energy, c.potential_energy)
                && close(g.virial, c.virial),
            "ball {i}: GPU {g:?} vs CPU {c:?}"
        );
        assert_eq!(g.capped_pairs, c.capped_pairs, "ball {i}: capped pairs");
    }
    a.iter().map(|r| u64::from(r.capped_pairs)).sum()
}

#[test]
fn test_step_matches_the_cpu_for_one_species() {
    check_step(|| world(40.0, 400, 2.0, &["N2"], Vec::new()), 50);
}

#[test]
fn test_step_matches_the_cpu_for_mixed_species() {
    check_step(
        || world(40.0, 300, 2.0, &["N2", "Ar", "XL"], Vec::new()),
        50,
    );
}

#[test]
fn test_step_matches_the_cpu_with_a_pair_override() {
    let overridden = || {
        let overrides = vec![PairOverrideConfig {
            a: "N2".into(),
            b: "Ar".into(),
            sigma: Some(1.3),
            epsilon: Some(2.0),
        }];
        world(40.0, 300, 2.0, &["N2", "Ar"], overrides)
    };
    check_step(overridden, 50);
}

#[test]
fn test_step_matches_the_cpu_in_a_dense_hot_gas_with_capped_pairs() {
    // Fast balls in a small box drive pairs into the force cap's core.
    let capped = check_step(|| world(22.0, 300, 12.0, &["N2", "Ar"], Vec::new()), 60);
    assert!(capped > 0, "no pair inside the force cap");
}

/// Means over a run: temperature, potential energy and the 2D virial
/// pressure times the area, `N T + ¼ Σ_i virial_i` (each pair is in two
/// balls' virials); and the capped pairs and braked balls at its end.
fn run_statistics(mc: &mut MainContent, steps: usize) -> ([f64; 3], [f64; 2]) {
    let mut sums = [0.0; 3];
    for _ in 0..steps {
        mc.simulation_step().unwrap();
        let values = Values::read(mc);
        let results = mc.world().cached_force_results();
        let virial: f64 = results.iter().map(|r| f64::from(r.virial)).sum();
        sums[0] += values[Obs::Temperature];
        sums[1] += values[Obs::PotentialEnergy];
        sums[2] += results.len() as f64 * values[Obs::Temperature] + 0.25 * virial;
    }
    let values = Values::read(mc);
    (
        sums.map(|sum| sum / steps as f64),
        [values[Obs::CappedPairs], values[Obs::Braked]],
    )
}

/// Long runs diverge (chaos), so their statistics are compared.
#[test]
fn test_long_run_has_the_statistics_of_the_cpu_run() {
    let Some(gpu) = gpu() else { return };
    // Cool enough that no ball is braked, which would not conserve energy.
    let make = || world(30.0, 300, 2.0, &["N2", "Ar"], Vec::new());
    let (mut cpu, mut gpu) = (make(), on_gpu(make(), gpu));
    let (cpu_means, cpu_nets) = run_statistics(&mut cpu, 3000);
    let (gpu_means, gpu_nets) = run_statistics(&mut gpu, 3000);
    eprintln!("GPU_STATISTICS cpu={cpu_means:?} {cpu_nets:?} gpu={gpu_means:?} {gpu_nets:?}");

    // Two realisations of the same ensemble: time averages agree to a few
    // percent, counts of rare events more loosely.
    let within = |a: f64, b: f64, rel: f64| (a - b).abs() <= rel * a.abs().max(b.abs());
    assert!(within(cpu_means[0], gpu_means[0], 0.03), "temperature");
    assert!(within(cpu_means[1], gpu_means[1], 0.05), "potential energy");
    assert!(within(cpu_means[2], gpu_means[2], 0.05), "virial pressure");
    assert!(cpu_nets[0] > 0.0, "the run reaches the force cap");
    assert!(within(cpu_nets[0], gpu_nets[0], 0.3), "capped pairs");
    assert_eq!((cpu_nets[1], gpu_nets[1]), (0.0, 0.0), "braked balls");
    // From the same start both end at the same total energy, within 1 % of
    // the kinetic energy (the total itself is near zero here).
    let energy = |mc: &MainContent| Values::read(mc)[Obs::EnergyTotal];
    let kinetic = Values::read(&cpu)[Obs::KineticEnergy];
    let gap = (energy(&cpu) - energy(&gpu)).abs();
    assert!(gap < 0.01 * kinetic, "total energies {gap} apart");
}

#[test]
fn test_two_gpu_runs_of_one_seed_are_identical() {
    let (Some(first), Some(second)) = (gpu(), gpu()) else {
        return;
    };
    let make = || world(30.0, 300, 6.0, &["N2", "Ar", "XL"], Vec::new());
    let (mut a, mut b) = (on_gpu(make(), first), on_gpu(make(), second));
    for _ in 0..300 {
        a.simulation_step().unwrap();
        b.simulation_step().unwrap();
    }
    assert_eq!(a.force_path(), Some(ForcePath::Gpu));
    assert_eq!(Values::read(&a), Values::read(&b));
    for (x, y) in a.world().balls().iter().zip(b.world().balls()) {
        assert_eq!(x.get_position(), y.get_position());
        assert_eq!(x.v, y.v);
    }
}
