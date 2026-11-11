// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The GPU force pass in the step, with the CPU kernel over all pairs as
//! its backend: which steps it takes, and that the run is the CPU's.

use super::new_main_content_seeded;
use crate::application::MainContent;
use crate::application::sim_stats::ForcePath;
use crate::objects2d::Rect;
use crate::objects2d::ball_impl::{
    BallPhysics, KernelPoint, PairForceBackend, PairForces, PairParams, SpeciesTable,
    pair_forces_reference,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Reference {
    physics: BallPhysics,
    /// Pair tables received.
    tables: Arc<AtomicUsize>,
    fail: bool,
}

impl PairForceBackend for Reference {
    fn set_pairs(&mut self, pairs: &[PairParams]) {
        assert_eq!(pairs, self.physics.pair_params());
        self.tables.fetch_add(1, Ordering::Relaxed);
    }

    fn compute(
        &mut self,
        points: &[KernelPoint],
        cell: [f32; 2],
        results: &mut Vec<PairForces>,
    ) -> Result<(), String> {
        if self.fail {
            return Err("lost".into());
        }
        *results = pair_forces_reference(&self.physics, points, cell);
        Ok(())
    }
}

/// 150 seeded balls with speeds; with `backend`, on the GPU force pass.
fn gas(backend: Option<bool>) -> (MainContent, Arc<AtomicUsize>) {
    let mut mc = new_main_content_seeded(Rect::new(-20.0, 20.0, 40.0, -40.0), 7).unwrap();
    mc.seed_random_balls_with_speed(150, 2.0);
    let tables = Arc::new(AtomicUsize::new(0));
    if let Some(fail) = backend {
        mc.set_force_backend(Some(Box::new(Reference {
            physics: mc.world().physics.clone(),
            tables: Arc::clone(&tables),
            fail,
        })));
    }
    (mc, tables)
}

fn step(mc: &mut MainContent) -> Option<ForcePath> {
    mc.simulation_step().unwrap();
    mc.force_path()
}

fn is_cpu(path: Option<ForcePath>) -> bool {
    matches!(path, Some(ForcePath::Cpu(_)))
}

#[test]
fn test_gpu_pass_gives_the_cpu_run_within_rounding() {
    let (mut cpu, _) = gas(None);
    let (mut gpu, tables) = gas(Some(false));
    for _ in 0..200 {
        assert_eq!(step(&mut cpu), None);
        assert_eq!(step(&mut gpu), Some(ForcePath::Gpu));
    }
    assert_eq!(tables.load(Ordering::Relaxed), 1, "one pair table upload");
    let (a, b) = (cpu.world(), gpu.world());
    assert!(a.sim.get_kinetic_energy() > 1.0, "the gas moves");
    // The same pairs in another order: f32 rounding only.
    let close = |x: f64, y: f64| (x - y).abs() <= 1e-4 * (1.0 + x.abs());
    assert!(close(a.sim.total_energy(), b.sim.total_energy()));
    assert!(close(
        f64::from(a.sim.get_potential_energy()),
        f64::from(b.sim.get_potential_energy())
    ));
    assert_eq!(a.sim.safety_nets_total, b.sim.safety_nets_total);
    for (x, y) in a.balls.iter().zip(&b.balls) {
        let d = *x.get_position() - y.get_position();
        assert!(d.norm() < 1e-2, "ball {} moved apart by {d:?}", x.get_id());
    }
}

#[test]
fn test_species_with_interactions_fall_back_until_they_are_gone() {
    let (mut mc, _) = gas(Some(false));
    assert_eq!(step(&mut mc), Some(ForcePath::Gpu));
    for name in ["H2O", "N2_2S"] {
        let species = SpeciesTable::builtin().by_name(name).unwrap();
        mc.step_config
            .get_ball_factory()
            .set_new_ball_species(species);
        mc.add_random_candidates(5);
        assert!(is_cpu(step(&mut mc)), "{name} present");
        let world = mc.world_mut();
        world.balls.retain(|ball| ball.species() != species);
        world.balls_changed();
        assert_eq!(step(&mut mc), Some(ForcePath::Gpu), "{name} removed");
    }
}

#[test]
fn test_neighbour_lines_fall_back_while_shown() {
    let (mut mc, _) = gas(Some(false));
    mc.world_mut().balls[3].verbose.toggle_main_neighbour();
    assert!(is_cpu(step(&mut mc)));
    assert!(
        !mc.world().cache.force_results[3].neighbor_forces.is_empty(),
        "the CPU kernel gives the lines their forces"
    );
    mc.world_mut().balls[3].verbose.toggle_main_neighbour();
    assert_eq!(step(&mut mc), Some(ForcePath::Gpu));
}

#[test]
fn test_backend_error_or_no_backend_stays_on_the_cpu() {
    let (mut cpu, _) = gas(None);
    let (mut failing, _) = gas(Some(true));
    let (mut none, _) = gas(None);
    none.set_force_backend(None);
    for _ in 0..20 {
        step(&mut cpu);
        assert!(is_cpu(step(&mut failing)));
        assert!(is_cpu(step(&mut none)));
    }
    // The failed step itself was computed on the CPU: the same run, bit
    // for bit.
    let energy = cpu.world().sim.total_energy();
    assert_eq!(failing.world().sim.total_energy(), energy);
    assert_eq!(none.world().sim.total_energy(), energy);
}
