use super::*;
use thermo_physics::test_support::create_main_content;

fn stepped(steps: usize) -> thermo_physics::application::MainContent {
    let (mut content, _) = create_main_content();
    content.add_random_candidates(10);
    for _ in 0..steps {
        content.step_recording_thermo_fields(false).unwrap();
    }
    content
}

#[test]
fn test_sample_records_energies_and_species_counts() {
    let content = stepped(3);
    let mut registry = MetricsRegistry::default();
    registry.sample(&content, 0);

    let e_kin = registry.get_current("E_kin").unwrap();
    let e_pot = registry.get_current("E_pot").unwrap();
    assert_eq!(registry.get_current("E_tot"), Some(e_kin + e_pot));
    assert!(registry.get_current("wall_momentum").is_some());

    let default_species = content
        .world()
        .species()
        .species(content.world().species().ids().next().unwrap());
    let count = registry
        .get_current(&format!("{}:count", default_species.name))
        .expect("per-species count is registered and recorded");
    assert_eq!(count, content.world().balls().len() as f32);
    assert_eq!(registry.current_step(), content.world().step_count() as u64);
}

#[test]
fn test_sample_skips_a_step_already_sampled() {
    let content = stepped(2);
    let mut registry = MetricsRegistry::default();
    registry.sample(&content, 0);
    let version = registry.version();
    registry.sample(&content, 0);
    assert_eq!(registry.get_series("E_kin").unwrap().len(), 1);
    assert_eq!(registry.version(), version);
}

#[test]
fn test_sample_of_a_new_generation_starts_a_new_history() {
    let content = stepped(2);
    let mut registry = MetricsRegistry::default();
    registry.sample(&content, 0);
    let (mut next, _) = create_main_content();
    next.step_recording_thermo_fields(false).unwrap();
    registry.sample(&content, 1);
    registry.sample(&next, 1);
    assert_eq!(registry.get_series("E_kin").unwrap().len(), 2);
    // Same step, new generation: still sampled.
    let mut other = MetricsRegistry::default();
    other.sample(&content, 0);
    other.sample(&content, 1);
    assert_eq!(other.get_series("E_kin").unwrap().len(), 1);
}

#[test]
fn test_disabled_sampling_records_nothing() {
    let content = stepped(1);
    let mut registry = MetricsRegistry::default();
    registry.set_sampling_enabled(false);
    registry.sample(&content, 0);
    assert!(registry.is_empty());
}
