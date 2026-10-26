// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Metrics registry for time-series data.

use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use thermo_physics::application::MainContent;

use super::series::{RingBuffer, SeriesMetadata, SeriesName};

/// Shared, thread-safe reference to the metrics registry.
pub type SharedMetricsRegistry = Arc<RwLock<MetricsRegistry>>;

/// Read the registry. A panic in another thread must not take readers down
/// with it: the series stay valid, so a poisoned lock is used as it is.
pub fn read_metrics(metrics: &SharedMetricsRegistry) -> RwLockReadGuard<'_, MetricsRegistry> {
    metrics.read().unwrap_or_else(PoisonError::into_inner)
}

/// Write the registry; poisoning is ignored as in [`read_metrics`].
pub fn write_metrics(metrics: &SharedMetricsRegistry) -> RwLockWriteGuard<'_, MetricsRegistry> {
    metrics.write().unwrap_or_else(PoisonError::into_inner)
}

/// Central registry for all time-series metrics.
///
/// Stores named series (E_kin, E_pot, E_tot, wall momentum, per-species
/// counts) in ring buffers, sampled at snapshot cadence. Both UI plots and
/// headless output read from this registry. Series live in sorted maps so
/// iteration, and with it the headless output, is deterministic.
pub struct MetricsRegistry {
    /// Metadata for all registered series.
    series_metadata: BTreeMap<SeriesName, SeriesMetadata>,
    /// Ring buffers storing the actual data for each series.
    buffers: BTreeMap<SeriesName, RingBuffer<f32>>,
    /// Simulation step of the last sample.
    current_step: u64,
    /// `(world generation, step)` of the last sample, to skip a repeat.
    last_sampled: Option<(u64, u64)>,
    /// World generation the stored history belongs to.
    generation: u64,
    /// Counts every change of the stored series, so a consumer can tell
    /// whether it has to copy them again.
    version: u64,
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
            series_metadata: BTreeMap::new(),
            buffers: BTreeMap::new(),
            current_step: 0,
            last_sampled: None,
            generation: 0,
            version: 0,
            capacity,
            sampling_enabled: true,
        };
        registry.register_builtin_series();
        registry
    }

    /// A new registry with the default capacity, ready to share.
    pub fn shared() -> SharedMetricsRegistry {
        Arc::new(RwLock::new(Self::default()))
    }

    /// Register all built-in series.
    ///
    /// Temperature and pressure are per-cell fields (see `thermo_field.rs`),
    /// not global scalars, and a step time or neighbour count would be
    /// wall-clock or costly to read, so none of them is a built-in series.
    fn register_builtin_series(&mut self) {
        for (name, description) in [
            ("E_kin", "Kinetic Energy"),
            ("E_pot", "Potential Energy"),
            ("E_tot", "Total Energy"),
        ] {
            self.register_series(
                name.to_string(),
                SeriesMetadata {
                    unit: Some("J"),
                    description,
                    is_per_species: false,
                },
            );
        }
        self.register_series(
            "wall_momentum".to_string(),
            SeriesMetadata {
                unit: Some("kg·m/s"),
                description: "Wall Momentum",
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
        self.buffers.insert(name, RingBuffer::new(self.capacity));
    }

    /// Name of a per-species series: `{species_name}:{base_name}`.
    pub fn per_species_name(species_name: &str, base_name: &str) -> SeriesName {
        format!("{species_name}:{base_name}")
    }

    /// Register a per-species series under [`Self::per_species_name`].
    pub fn register_per_species_series(
        &mut self,
        base_name: &str,
        species_name: &str,
        metadata: SeriesMetadata,
    ) {
        self.register_series(Self::per_species_name(species_name, base_name), metadata);
    }

    /// Record a value for a series.
    ///
    /// If the series doesn't exist, this is a no-op.
    pub fn record(&mut self, name: &str, value: f32) {
        if let Some(buffer) = self.buffers.get_mut(name) {
            buffer.push(value);
            self.version = self.version.wrapping_add(1);
        }
    }

    /// Get the ring buffer for a series.
    pub fn get_series(&self, name: &str) -> Option<&RingBuffer<f32>> {
        self.buffers.get(name)
    }

    /// Get the current (latest) value for a series.
    pub fn get_current(&self, name: &str) -> Option<f32> {
        self.buffers.get(name).and_then(|b| b.latest().copied())
    }

    /// Get the metadata for a series.
    pub fn get_metadata(&self, name: &str) -> Option<&SeriesMetadata> {
        self.series_metadata.get(name)
    }

    /// Clear all data for all series and drop the per-species ones (the new
    /// world may have other species; they register again when sampled).
    ///
    /// Called when a new world is loaded.
    pub fn clear(&mut self) {
        self.series_metadata.retain(|_, meta| !meta.is_per_species);
        let metadata = &self.series_metadata;
        self.buffers.retain(|name, _| metadata.contains_key(name));
        for buffer in self.buffers.values_mut() {
            buffer.clear();
        }
        self.current_step = 0;
        self.last_sampled = None;
        self.version = self.version.wrapping_add(1);
    }

    /// Remove all series of a species (by name).
    ///
    /// Called when a species is removed from the simulation.
    pub fn clear_species(&mut self, species_name: &str) {
        let prefix = format!("{species_name}:");
        self.buffers.retain(|name, _| !name.starts_with(&prefix));
        self.series_metadata
            .retain(|name, _| !name.starts_with(&prefix));
        self.version = self.version.wrapping_add(1);
    }

    /// Enable or disable sampling.
    ///
    /// When disabled, `sample` is a no-op.
    pub fn set_sampling_enabled(&mut self, enabled: bool) {
        self.sampling_enabled = enabled;
    }

    /// Sample all metrics from the simulation.
    ///
    /// This is called at snapshot cadence (not every physics step) to
    /// maintain performance and determinism. A sample is taken only for a
    /// step not sampled yet, so a paused world adds no duplicate points. A
    /// new `generation` (a loaded world) starts a new history.
    pub fn sample(&mut self, simulation: &MainContent, generation: u64) {
        if !self.sampling_enabled {
            return;
        }
        let world = simulation.world();
        let step = world.step_count() as u64;
        if self.last_sampled == Some((generation, step)) {
            return;
        }
        if generation != self.generation {
            self.generation = generation;
            self.clear();
        }
        self.last_sampled = Some((generation, step));
        self.current_step = step;

        let sim = world.sim();
        let e_kin = sim.get_kinetic_energy();
        let e_pot = sim.get_potential_energy();
        self.record("E_kin", e_kin);
        self.record("E_pot", e_pot);
        self.record("E_tot", e_kin + e_pot);

        let (px, py) = simulation.wall_momentum();
        self.record("wall_momentum", px.hypot(py));

        self.sample_per_species(simulation);
    }

    /// Record the ball count of every species, in one pass over the balls.
    fn sample_per_species(&mut self, simulation: &MainContent) {
        let world = simulation.world();
        let species = world.species();
        let mut counts = vec![0u32; species.len()];
        for ball in world.balls() {
            if let Some(count) = counts.get_mut(ball.species().index()) {
                *count += 1;
            }
        }
        for (id, count) in species.ids().zip(counts) {
            let name = Self::per_species_name(&species.species(id).name, "count");
            if !self.buffers.contains_key(&name) {
                self.register_series(
                    name.clone(),
                    SeriesMetadata {
                        unit: None,
                        description: "Ball count of the species",
                        is_per_species: true,
                    },
                );
            }
            self.record(&name, count as f32);
        }
    }

    /// Simulation step of the last sample.
    pub fn current_step(&self) -> u64 {
        self.current_step
    }

    /// Counter that changes whenever stored series change.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether no series holds a sample.
    pub fn is_empty(&self) -> bool {
        self.buffers.values().all(RingBuffer::is_empty)
    }

    /// Get all series as an iterator, sorted by name.
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
