// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Metrics registry for time-series data.

use crate::application::renderer_snapshot::RendererSnapshot;
use crate::objects2d::ball_impl::SpeciesTable;
use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::series::{RingBuffer, SeriesMetadata, SeriesName};

/// Unit label of the energy series: reduced energy, multiples of ε*.
pub const ENERGY_UNIT: &str = "eps*";

/// Unit label of the wall momentum: reduced momentum, m*·σ*/τ*.
pub const MOMENTUM_UNIT: &str = "m*sigma*/tau*";

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
/// All series are sampled together: from its registration on, a series gets
/// a value at every sample, so the newest value of every series belongs to
/// the newest step and a shorter series (registered later) is right-aligned
/// with `steps()`. The export relies on this; `record` outside `sample` is
/// for tests and for series a caller feeds in lockstep with `sample`.
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
    /// The step of every sample. All series are sampled together, so the
    /// newest entry of a series belongs to the newest step here.
    steps: RingBuffer<u64>,
    /// Simulation time of every sample, next to `steps` (the plot's x axis).
    times: RingBuffer<f32>,
    /// `(world generation, step)` of the last sample, to skip a repeat.
    last_sampled: Option<(u64, u64)>,
    /// World generation the stored history belongs to.
    generation: u64,
    /// Counts every change of the stored series, so a consumer can tell
    /// whether it has to copy them again.
    version: u64,
    /// Maximum number of samples to retain per series.
    capacity: usize,
    /// The species table the per-species series below belong to.
    species_table: Option<Arc<SpeciesTable>>,
    /// Name of the count series of each species, by species index, so a
    /// sample allocates nothing.
    count_series: Vec<SeriesName>,
}

impl MetricsRegistry {
    /// Create a new MetricsRegistry with the specified capacity.
    ///
    /// Also pre-registers all built-in series.
    pub fn new(capacity: usize) -> Self {
        let mut registry = Self {
            series_metadata: BTreeMap::new(),
            buffers: BTreeMap::new(),
            steps: RingBuffer::new(capacity),
            times: RingBuffer::new(capacity),
            last_sampled: None,
            generation: 0,
            version: 0,
            capacity,
            species_table: None,
            count_series: Vec::new(),
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
    ///
    /// The values are in the simulation's reduced Lennard-Jones units
    /// (`docs/units.md`), and so are the unit labels, in ASCII so every
    /// output can show them: energies in `eps*` (ε*, 1.312E-21 J for N₂),
    /// the wall momentum in `m*sigma*/tau*` (m*·σ*/τ*).
    fn register_builtin_series(&mut self) {
        for (name, description) in [
            ("E_kin", "Kinetic Energy"),
            ("E_pot", "Potential Energy"),
            ("E_tot", "Total Energy"),
        ] {
            self.register_series(
                name.to_string(),
                SeriesMetadata {
                    unit: Some(ENERGY_UNIT),
                    description,
                    is_per_species: false,
                },
            );
        }
        self.register_series(
            "wall_momentum".to_string(),
            SeriesMetadata {
                unit: Some(MOMENTUM_UNIT),
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
        self.steps.clear();
        self.times.clear();
        self.species_table = None;
        self.count_series.clear();
        self.last_sampled = None;
        self.version = self.version.wrapping_add(1);
    }

    /// Sample all metrics from `snapshot`, the values the renderer and the
    /// headless frames show.
    ///
    /// This is called at snapshot cadence (not every physics step) to
    /// maintain performance and determinism. A sample is taken only for a
    /// step not sampled yet, so a paused world adds no duplicate points. A
    /// new world generation (a loaded world) starts a new history.
    pub fn sample(&mut self, snapshot: &RendererSnapshot) {
        let generation = snapshot.frame.world_generation;
        let step = snapshot.frame.simulation_step;
        if self.last_sampled == Some((generation, step)) {
            return;
        }
        if generation != self.generation {
            self.generation = generation;
            self.clear();
        }
        self.last_sampled = Some((generation, step));
        self.steps.push(step);
        self.times.push(snapshot.frame.time);

        let (e_kin, e_pot) = (
            snapshot.frame.kinetic_energy,
            snapshot.frame.potential_energy,
        );
        self.record("E_kin", e_kin);
        self.record("E_pot", e_pot);
        self.record("E_tot", e_kin + e_pot);
        self.record("wall_momentum", snapshot.frame.wall_momentum);

        self.sample_species_counts(snapshot);
    }

    /// Record the ball count of every species, counted by the snapshot.
    fn sample_species_counts(&mut self, snapshot: &RendererSnapshot) {
        let known = self
            .species_table
            .as_ref()
            .is_some_and(|table| Arc::ptr_eq(table, &snapshot.balls.species));
        if !known {
            self.species_table = Some(Arc::clone(&snapshot.balls.species));
            self.count_series = snapshot
                .balls
                .species
                .ids()
                .map(|id| Self::per_species_name(&snapshot.balls.species.species(id).name, "count"))
                .collect();
            // Another table: the count series of species it lacks stop for
            // good. Drop them, so every series that is left receives a value
            // at every sample (the alignment of the export relies on it).
            let (buffers, metadata, current) = (
                &mut self.buffers,
                &mut self.series_metadata,
                &self.count_series,
            );
            metadata.retain(|name, meta| !meta.is_per_species || current.contains(name));
            buffers.retain(|name, _| metadata.contains_key(name));
            for name in self.count_series.clone() {
                if !self.buffers.contains_key(&name) {
                    self.register_series(
                        name,
                        SeriesMetadata {
                            unit: None,
                            description: "Ball count of the species",
                            is_per_species: true,
                        },
                    );
                }
            }
        }
        for (name, &count) in self.count_series.iter().zip(&snapshot.frame.species_counts) {
            if let Some(buffer) = self.buffers.get_mut(name) {
                buffer.push(count as f32);
            }
        }
        self.version = self.version.wrapping_add(1);
    }

    /// Add a sample step without sampling a world (tests); its time is a
    /// tenth of the step.
    #[cfg(test)]
    pub(crate) fn push_step_for_test(&mut self, step: u64) {
        self.steps.push(step);
        self.times.push(step as f32 / 10.0);
    }

    /// The step of every sample, oldest first; the newest entry of each
    /// series was recorded at the newest step.
    pub fn steps(&self) -> &RingBuffer<u64> {
        &self.steps
    }

    /// The simulation time of every sample, oldest first, one per entry of
    /// [`Self::steps`]. Plots use it as their x axis, so they stay
    /// comparable when the time step changes.
    pub fn times(&self) -> &RingBuffer<f32> {
        &self.times
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
