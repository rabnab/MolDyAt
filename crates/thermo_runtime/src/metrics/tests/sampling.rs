// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use crate::application::CreateSnapshot;
use crate::application::renderer_snapshot::RendererSnapshot;
use thermo_physics::application::observables::Obs;
use thermo_physics::test_support::create_main_content;

fn stepped_snapshot(steps: usize) -> RendererSnapshot {
    let (mut content, _) = create_main_content();
    content.add_random_candidates(10);
    for _ in 0..steps {
        content.step_recording_thermo_fields(false).unwrap();
    }
    content.create_snapshot()
}

#[test]
fn test_sample_records_the_values_of_the_snapshot() {
    let snapshot = stepped_snapshot(3);
    let mut registry = MetricsRegistry::default();
    registry.sample(&snapshot);

    let last = |name: &str| registry.get_series(name).unwrap().latest().copied();
    let values = &snapshot.frame.values;
    for (series, id) in [
        ("E_kin", Obs::KineticEnergy),
        ("E_pot", Obs::PotentialEnergy),
        ("E_tot", Obs::EnergyTotal),
        ("wall_momentum", Obs::WallMomentum),
    ] {
        assert_eq!(last(series), Some(values[id] as f32), "{series}");
    }

    let first_species = snapshot.balls.species.ids().next().unwrap();
    let name = format!(
        "{}:count",
        snapshot.balls.species.species(first_species).name
    );
    assert_eq!(last(&name), Some(snapshot.balls.len() as f32));
}

#[test]
fn test_sample_records_the_time_of_each_sample_next_to_its_step() {
    let mut registry = MetricsRegistry::default();
    let mut snapshot = stepped_snapshot(2);
    registry.sample(&snapshot);
    snapshot.frame.simulation_step += 1;
    snapshot.frame.time += 0.25;
    registry.sample(&snapshot);

    assert_eq!(registry.times().len(), registry.steps().len());
    assert_eq!(
        registry.times().latest().copied(),
        Some(snapshot.frame.time)
    );
    let times: Vec<f32> = registry.times().get().iter().copied().collect();
    assert_eq!(times[1] - times[0], 0.25);

    // A new world clears the times with the rest.
    snapshot.frame.world_generation = 1;
    registry.sample(&snapshot);
    assert_eq!(registry.times().len(), 1);
}

#[test]
fn test_sample_skips_a_step_already_sampled() {
    let snapshot = stepped_snapshot(2);
    let mut registry = MetricsRegistry::default();
    registry.sample(&snapshot);
    let version = registry.version();
    registry.sample(&snapshot);
    assert_eq!(registry.get_series("E_kin").unwrap().len(), 1);
    assert_eq!(registry.version(), version);
}

#[test]
fn test_sample_of_a_new_generation_starts_a_new_history() {
    let mut registry = MetricsRegistry::default();
    let mut snapshot = stepped_snapshot(2);
    registry.sample(&snapshot);
    snapshot.frame.simulation_step += 1;
    registry.sample(&snapshot);
    assert_eq!(registry.get_series("E_kin").unwrap().len(), 2);

    // A loaded world may sit at the same step; it still starts afresh.
    snapshot.frame.world_generation = 1;
    registry.sample(&snapshot);
    assert_eq!(registry.get_series("E_kin").unwrap().len(), 1);
}

#[test]
fn test_species_series_are_registered_once_per_table() {
    let snapshot = stepped_snapshot(1);
    let mut registry = MetricsRegistry::default();
    registry.sample(&snapshot);
    let series = registry.all_series().count();
    let mut next = snapshot.clone();
    next.frame.simulation_step += 1;
    registry.sample(&next);
    assert_eq!(registry.all_series().count(), series);
    assert!(
        registry
            .get_metadata("N2:count")
            .is_some_and(|m| m.is_per_species)
    );
}

/// The `net_*` series hold the safety-net interventions since the previous
/// sample (#422): exact in f32 even when the totals are far past 2^24.
#[test]
fn test_safety_net_series_record_the_increase_since_the_last_sample() {
    let mut registry = MetricsRegistry::default();
    let mut snapshot = stepped_snapshot(1);
    let base = 50_000_000.0;
    snapshot.frame.values[Obs::CappedPairs] = base;
    registry.sample(&snapshot);
    snapshot.frame.simulation_step += 1;
    snapshot.frame.values[Obs::CappedPairs] = base + 3.0;
    snapshot.frame.values[Obs::Braked] = 1.0;
    registry.sample(&snapshot);

    let last = |name: &str| registry.get_series(name).unwrap().latest().copied();
    assert_eq!(last("net_capped_pairs"), Some(3.0));
    assert_eq!(last("net_braked"), Some(1.0));
    assert_eq!(last("net_backstop_hits"), Some(0.0));
}
