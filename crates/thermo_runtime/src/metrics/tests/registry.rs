// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

fn meta(per_species: bool) -> SeriesMetadata {
    SeriesMetadata {
        unit: None,
        description: "test",
        is_per_species: per_species,
    }
}

#[test]
fn test_new_registry_has_builtin_series() {
    let registry = MetricsRegistry::new(100);
    for name in ["E_kin", "E_pot", "E_tot", "wall_momentum"] {
        assert!(registry.get_series(name).is_some(), "{name}");
    }
    // Never recorded, so never registered.
    assert!(registry.get_series("step_time").is_none());
    assert!(registry.get_series("neighbour_count").is_none());
}

#[test]
fn test_register_series() {
    let mut registry = MetricsRegistry::new(100);
    let original_len = registry.all_series().count();
    registry.register_series("custom_metric".to_string(), meta(false));
    assert_eq!(registry.all_series().count(), original_len + 1);
    assert!(registry.get_series("custom_metric").is_some());
}

#[test]
fn test_record_and_retrieve() {
    let mut registry = MetricsRegistry::new(100);
    for v in [10.5, 11.2, 9.8] {
        registry.record("E_kin", v);
    }
    let buffer = registry.get_series("E_kin").unwrap();
    assert_eq!(buffer.len(), 3);
    assert_eq!(buffer.latest(), Some(&9.8));
}

#[test]
fn test_record_unknown_series_is_ignored_and_keeps_version() {
    let mut registry = MetricsRegistry::new(100);
    let version = registry.version();
    registry.record("nope", 1.0);
    assert_eq!(registry.version(), version);
}

#[test]
fn test_version_changes_with_every_record() {
    let mut registry = MetricsRegistry::new(100);
    let v0 = registry.version();
    registry.record("E_kin", 1.0);
    assert_ne!(registry.version(), v0);
}

#[test]
fn test_clear_empties_series_and_drops_per_species_ones() {
    let mut registry = MetricsRegistry::new(100);
    registry.register_series("N2:count".to_string(), meta(true));
    registry.record("E_kin", 10.0);
    registry.record("N2:count", 3.0);
    registry.clear();
    assert!(registry.get_series("E_kin").unwrap().is_empty());
    assert!(registry.get_series("N2:count").is_none());
    assert!(registry.is_empty());
}

#[test]
fn test_is_empty_depends_on_samples_not_on_registered_series() {
    let mut registry = MetricsRegistry::new(100);
    assert!(registry.is_empty());
    registry.record("E_kin", 1.0);
    assert!(!registry.is_empty());
}

#[test]
fn test_default_capacity() {
    assert_eq!(MetricsRegistry::default().capacity(), 1000);
}

#[test]
fn test_all_series_is_sorted_by_name() {
    let registry = MetricsRegistry::new(10);
    let names: Vec<_> = registry.all_series().map(|(n, _)| n.clone()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}
