// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use crate::application::CreateSnapshot;
use crate::application::renderer_snapshot::RendererSnapshot;
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
    assert_eq!(last("E_kin"), Some(snapshot.kinetic_energy));
    assert_eq!(last("E_pot"), Some(snapshot.potential_energy));
    assert_eq!(
        last("E_tot"),
        Some(snapshot.kinetic_energy + snapshot.potential_energy)
    );
    assert_eq!(last("wall_momentum"), Some(snapshot.wall_momentum));

    let first_species = snapshot.species.ids().next().unwrap();
    let name = format!("{}:count", snapshot.species.species(first_species).name);
    assert_eq!(last(&name), Some(snapshot.balls.len() as f32));
}

#[test]
fn test_sample_records_the_time_of_each_sample_next_to_its_step() {
    let mut registry = MetricsRegistry::default();
    let mut snapshot = stepped_snapshot(2);
    registry.sample(&snapshot);
    snapshot.simulation_step += 1;
    snapshot.time += 0.25;
    registry.sample(&snapshot);

    assert_eq!(registry.times().len(), registry.steps().len());
    assert_eq!(registry.times().latest().copied(), Some(snapshot.time));
    let times: Vec<f32> = registry.times().get().iter().copied().collect();
    assert_eq!(times[1] - times[0], 0.25);

    // A new world clears the times with the rest.
    snapshot.world_generation = 1;
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
    snapshot.simulation_step += 1;
    registry.sample(&snapshot);
    assert_eq!(registry.get_series("E_kin").unwrap().len(), 2);

    // A loaded world may sit at the same step; it still starts afresh.
    snapshot.world_generation = 1;
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
    next.simulation_step += 1;
    registry.sample(&next);
    assert_eq!(registry.all_series().count(), series);
    assert!(
        registry
            .get_metadata("N2:count")
            .is_some_and(|m| m.is_per_species)
    );
}
