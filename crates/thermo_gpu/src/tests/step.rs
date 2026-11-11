// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A run with the kernel in the forces phase against the same run on the
//! CPU kernel, on a real adapter. Skipped (passes with a note) without one.

use crate::GpuForces;
use thermo_physics::application::MainContent;
use thermo_physics::application::sim_stats::ForcePath;
use thermo_physics::objects2d::Rect;
use thermo_physics::test_support::new_main_content_seeded;

fn gas() -> MainContent {
    let mut mc = new_main_content_seeded(Rect::new(-20.0, 20.0, 40.0, -40.0), 7).unwrap();
    mc.seed_random_balls_with_speed(150, 2.0);
    mc
}

#[test]
fn test_gpu_run_follows_the_cpu_run() {
    let Some(backend) = GpuForces::headless() else {
        eprintln!("no GPU adapter: GPU run skipped");
        return;
    };
    let (mut cpu, mut gpu) = (gas(), gas());
    gpu.set_force_backend(Some(Box::new(backend)));
    for _ in 0..200 {
        cpu.simulation_step().unwrap();
        gpu.simulation_step().unwrap();
        assert_eq!(gpu.force_path(), Some(ForcePath::Gpu));
    }
    let (a, b) = (cpu.world().sim(), gpu.world().sim());
    assert!(a.get_kinetic_energy() > 1.0, "the gas moves");
    // f32 rounding of the kernels, grown over 200 steps.
    let close = |x: f64, y: f64| (x - y).abs() <= 1e-4 * (1.0 + x.abs());
    assert!(
        close(a.total_energy(), b.total_energy()),
        "energy {} vs {}",
        a.total_energy(),
        b.total_energy()
    );
    assert!(close(
        f64::from(a.get_potential_energy()),
        f64::from(b.get_potential_energy())
    ));
    assert_eq!(a.safety_nets_total, b.safety_nets_total);
    for (x, y) in cpu.world().balls().iter().zip(gpu.world().balls()) {
        let d = *x.get_position() - y.get_position();
        assert!(d.norm() < 1e-2, "ball {} moved apart by {d:?}", x.get_id());
    }
}
