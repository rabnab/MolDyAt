// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Metrics registry for time-series data.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thermo_physics::objects2d::ball_impl::SpeciesId;

use super::series::{RingBuffer, SeriesMetadata, SeriesName};

/// Shared, thread-safe reference to the metrics registry.
pub type SharedMetricsRegistry = Arc<RwLock<MetricsRegistry>>;

/// Central registry for all time-series metrics.
///
/// Stores named series (E_kin, E_pot, T, P, etc.) in ring buffers,
/// sampled at snapshot cadence. Both UI plots and headless output
/// read from this registry.
pub struct MetricsRegistry {
    /// Metadata for all registered series.
    series_metadata: HashMap<SeriesName, SeriesMetadata>,
    /// Ring buffers storing the actual data for each series.
    buffers: HashMap<SeriesName, RingBuffer<f32>>,
    /// Current simulation step when last sampled.
    current_step: u64,
    /// Maximum number of samples to retain per series.
    capacity: usize,
    /// Whether metrics are currently being sampled.
    sampling_enabled: bool,
}

impl MetricsRegistry {
    /// Create a new MetricsRegistry with the specified capacity.
    ///
    /// Also pre-registers all built-in series.
    pub fn new(capacity: usize) -> Self {
        let mut registry = Self {
            series_metadata: HashMap::new(),
            buffers: HashMap::new(),
            current_step: 0,
            capacity,
            sampling_enabled: true,
        };
        registry.register_builtin_series();
        registry
    }

    /// Register all built-in series.
    fn register_builtin_series(&mut self) {
        // Energy series
        self.register_series(
            "E_kin".to_string(),
            SeriesMetadata {
                unit: Some("J"),
                description: "Kinetic Energy",
                is_per_species: false,
            },
        );
        self.register_series(
            "E_pot".to_string(),
            SeriesMetadata {
                unit: Some("J"),
                description: "Potential Energy",
                is_per_species: false,
            },
        );
        self.register_series(
            "E_tot".to_string(),
            SeriesMetadata {
                unit: Some("J"),
                description: "Total Energy",
                is_per_species: false,
            },
        );

        // Thermodynamic series - Note: Temperature is computed as a per-cell
        // field in thermo_field.rs and not available as a global scalar via
        // SimStats public API. To add: either add getter to SimStats or compute
        // globally from balls.
        // self.register_series("T".to_string(), SeriesMetadata { .. });

        // Mechanical series
        self.register_series(
            "wall_momentum".to_string(),
            SeriesMetadata {
                unit: Some("kg·m/s"),
                description: "Wall Momentum",
                is_per_species: false,
            },
        );

        // System metrics
        self.register_series(
            "neighbour_count".to_string(),
            SeriesMetadata {
                unit: None,
                description: "Total Neighbor Count",
                is_per_species: false,
            },
        );
        self.register_series(
            "step_time".to_string(),
            SeriesMetadata {
                unit: Some("s"),
                description: "Physics Step Time",
                is_per_species: false,
            },
        );
    }

    /// Register a new series.
    ///
    /// # Arguments
    /// * `name` - Unique name for the series
    /// * `metadata` - Metadata describing the series
    pub fn register_series(&mut self, name: SeriesName, metadata: SeriesMetadata) {
        self.series_metadata.insert(name.clone(), metadata);
        self.buffers
            .insert(name, RingBuffer::new(self.capacity));
    }

    /// Register a per-species series.
    ///
    /// Creates a series with name `{species_name}:{base_name}`.
    ///
    /// # Arguments
    /// * `base_name` - The metric name (e.g., "count", "temperature")
    /// * `species_id` - The species identifier
    /// * `metadata` - Metadata for the series
    pub fn register_per_species_series(
        &mut self,
        base_name: &str,
        species_id: SpeciesId,
        metadata: SeriesMetadata,
    ) {
        let species_name = format!("species_{}", species_id.index());
        let series_name = format!("{}:{}", species_name, base_name);
        self.register_series(series_name, metadata);
    }

    /// Record a value for a series.
    ///
    /// If the series doesn't exist, this is a no-op.
    pub fn record(&mut self, name: &str, value: f32) {
        if let Some(buffer) = self.buffers.get_mut(name) {
            buffer.push(value);
        }
    }

    /// Get the ring buffer for a series.
    pub fn get_series(&self, name: &str) -> Option<&RingBuffer<f32>> {
        self.buffers.get(name)
    }

    /// Get the current (latest) value for a series.
    pub fn get_current(&self, name: &str) -> Option<f32> {
        self.buffers
            .get(name)
            .and_then(|b| b.latest().copied())
    }

    /// Get the metadata for a series.
    pub fn get_metadata(&self, name: &str) -> Option<&SeriesMetadata> {
        self.series_metadata.get(name)
    }

    /// Clear all data for all series.
    ///
    /// Called when a new world is loaded.
    pub fn clear(&mut self) {
        for buffer in self.buffers.values_mut() {
            buffer.clear();
        }
        self.current_step = 0;
    }

    /// Clear all data for a specific species.
    ///
    /// Called when a species is removed from the simulation.
    pub fn clear_species(&mut self, species_id: SpeciesId) {
        let species_prefix = format!("species_{}:", species_id.index());
        self.buffers
            .retain(|name, _| !name.starts_with(&species_prefix));
        self.series_metadata
            .retain(|name, _| !name.starts_with(&species_prefix));
    }

    /// Enable or disable sampling.
    ///
    /// When disabled, `sample_from_world` is a no-op.
    pub fn set_sampling_enabled(&mut self, enabled: bool) {
        self.sampling_enabled = enabled;
    }

    /// Sample all metrics from the simulation world.
    ///
    /// This is called at snapshot cadence (not every physics step)
    /// to maintain performance and determinism.
    pub fn sample_from_world(
        &mut self,
        world: &thermo_physics::application::core::SimulationWorld,
    ) {
        if !self.sampling_enabled {
            return;
        }

        let sim = world.sim();

        // Energy metrics
        let e_kin = sim.get_kinetic_energy();
        let e_pot = sim.get_potential_energy();
        self.record("E_kin", e_kin);
        self.record("E_pot", e_pot);
        self.record("E_tot", e_kin + e_pot);

        // Wall momentum (accessing public fields wall_momentum_x/y)
        let wall_momentum = (sim.wall_momentum_x.powi(2) + sim.wall_momentum_y.powi(2)).sqrt();
        self.record("wall_momentum", wall_momentum);
        
        // Note: Temperature and Pressure are computed as per-cell fields (see thermo_field.rs)
        // and are not available as global scalars via SimStats public API.
        // To add them: either add getters to SimStats or compute globally from balls.

        // Per-species metrics
        self.sample_per_species(world);

        // Update current step
        self.current_step = world.step_count() as u64;
    }

    /// Sample per-species metrics from the simulation world.
    fn sample_per_species(
        &mut self,
        world: &thermo_physics::application::core::SimulationWorld,
    ) {
        for species_id in world.species().ids() {
            // Use species name if available, otherwise fall back to indexed name
            let species_name = world
                .species()
                .get(species_id)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| format!("species_{}", species_id.index()));

            // Count
            let count_series_name = format!("{}:count", species_name);
            let count = world
                .balls()
                .iter()
                .filter(|b| b.species() == species_id)
                .count() as f32;
            self.record(&count_series_name, count);

            // Temperature (per-species) - if available
            // Note: Per-species temperature computation would need to be added
            // to SimulationWorld or computed here from ball velocities
        }
    }

    /// Get the current simulation step.
    pub fn current_step(&self) -> u64 {
        self.current_step
    }

    /// Check if any series have data.
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Get all series as an iterator.
    pub fn all_series(&self) -> impl Iterator<Item = (&SeriesName, &RingBuffer<f32>)> {
        self.buffers.iter()
    }

    /// Get the capacity of the ring buffers.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thermo_physics::objects2d::ball_impl::SpeciesId;

    #[test]
    fn test_new_registry_has_builtin_series() {
        let registry = MetricsRegistry::new(100);
        
        // Check energy series exist
        assert!(registry.get_series("E_kin").is_some());
        assert!(registry.get_series("E_pot").is_some());
        assert!(registry.get_series("E_tot").is_some());
        
        // Note: T (temperature) not registered as it's a per-cell field, not a global scalar
        
        // Check mechanical series exist
        assert!(registry.get_series("wall_momentum").is_some());
        
        // Check system series exist
        assert!(registry.get_series("neighbour_count").is_some());
        assert!(registry.get_series("step_time").is_some());
    }

    #[test]
    fn test_register_series() {
        let mut registry = MetricsRegistry::new(100);
        let original_len = registry.all_series().count();
        
        registry.register_series(
            "custom_metric".to_string(),
            SeriesMetadata {
                unit: Some("m"),
                description: "Custom metric",
                is_per_species: false,
            },
        );
        
        assert_eq!(registry.all_series().count(), original_len + 1);
        assert!(registry.get_series("custom_metric").is_some());
    }

    #[test]
    fn test_record_and_retrieve() {
        let mut registry = MetricsRegistry::new(100);
        
        registry.record("E_kin", 10.5);
        registry.record("E_kin", 11.2);
        registry.record("E_kin", 9.8);
        
        let buffer = registry.get_series("E_kin").unwrap();
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.latest(), Some(&9.8));
    }

    #[test]
    fn test_clear() {
        let mut registry = MetricsRegistry::new(100);
        
        registry.record("E_kin", 10.0);
        registry.record("E_kin", 20.0);
        registry.record("E_pot", 5.0);
        
        registry.clear();
        
        assert!(registry.get_series("E_kin").unwrap().is_empty());
        assert!(registry.get_series("E_pot").unwrap().is_empty());
    }

    #[test]
    fn test_default_capacity() {
        let registry = MetricsRegistry::default();
        assert_eq!(registry.capacity(), 1000);
    }

    #[test]
    fn test_per_species_registration() {
        let mut registry = MetricsRegistry::new(100);
        
        let species_id = SpeciesId::new(0);
        registry.register_per_species_series(
            "count",
            species_id,
            SeriesMetadata {
                unit: None,
                description: "Species count",
                is_per_species: true,
            },
        );
        
        assert!(registry.get_series("species_0:count").is_some());
    }

    #[test]
    fn test_clear_species() {
        let mut registry = MetricsRegistry::new(100);
        
        let species_id = SpeciesId::new(0);
        registry.register_per_species_series(
            "count",
            species_id,
            SeriesMetadata {
                unit: None,
                description: "Species count",
                is_per_species: true,
            },
        );
        registry.register_per_species_series(
            "temperature",
            species_id,
            SeriesMetadata {
                unit: Some("K"),
                description: "Species temperature",
                is_per_species: true,
            },
        );
        
        registry.record("species_0:count", 10.0);
        registry.record("species_0:temperature", 300.0);
        
        registry.clear_species(species_id);
        
        assert!(registry.get_series("species_0:count").is_none());
        assert!(registry.get_series("species_0:temperature").is_none());
    }
}
