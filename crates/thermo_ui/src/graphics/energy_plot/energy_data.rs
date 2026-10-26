// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use thermo_runtime::metrics::SharedMetricsRegistry;

/// Energy display mode - controls which energy traces are shown
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode {
    #[default]
    Pot = 0, // Potential energy only
    Kin = 1,   // Kinetic energy only
    Total = 2, // Total energy only
    All = 3,   // All three traces
}

impl DisplayMode {
    /// Cycle to next display mode in sequence: Pot → Kin → Total → All → Pot.
    pub fn next(self) -> Self {
        match self {
            DisplayMode::Pot => DisplayMode::Kin,
            DisplayMode::Kin => DisplayMode::Total,
            DisplayMode::Total => DisplayMode::All,
            DisplayMode::All => DisplayMode::Pot,
        }
    }

    /// Get short string representation for display in window title.
    pub fn as_str(self) -> &'static str {
        match self {
            DisplayMode::Pot => "pot",
            DisplayMode::Kin => "kin",
            DisplayMode::Total => "total",
            DisplayMode::All => "all",
        }
    }
}

/// Shared energy data structure for communication between main simulation and energy plot window.
///
/// This is now a thin wrapper around MetricsRegistry (decision 8-B). It syncs
/// its cached values from the registry when requested.
#[derive(Clone)]
pub struct EnergyData {
    /// Cached kinetic energy samples (synced from MetricsRegistry)
    pub v_kin: VecDeque<f32>,
    /// Cached potential energy samples (synced from MetricsRegistry)
    pub v_pot: VecDeque<f32>,
    pub time: f32,
    pub status_text: Option<String>,
    /// Monotonically increasing counter that increments each time energy samples
    /// are synced. Renderers can compare this against a cached version to avoid
    /// rebuilding the plot when nothing has changed.
    pub version: u64,
    /// Shared reference to the metrics registry
    metrics: Option<SharedMetricsRegistry>,
}

impl EnergyData {
    /// Create new energy data with pre-allocated capacity of 1000 samples.
    pub fn new() -> Self {
        Self {
            v_kin: VecDeque::with_capacity(1000),
            v_pot: VecDeque::with_capacity(1000),
            time: 0.0,
            status_text: None,
            version: 0,
            metrics: None,
        }
    }

    /// Create new energy data with a reference to MetricsRegistry.
    pub fn with_metrics(metrics: SharedMetricsRegistry) -> Self {
        Self {
            v_kin: VecDeque::with_capacity(1000),
            v_pot: VecDeque::with_capacity(1000),
            time: 0.0,
            status_text: None,
            version: 0,
            metrics: Some(metrics),
        }
    }

    /// Push a kinetic and potential energy sample, incrementing the version.
    ///
    /// This method is kept for backward compatibility, but now it's a no-op
    /// since metrics are recorded directly in the MetricsRegistry by the physics thread.
    /// To update the cached values, call `sync_from_metrics()` instead.
    pub fn push_sample(&mut self, _kinetic: f32, _potential: f32, _time: f32) {
        // No-op: metrics are now recorded in MetricsRegistry by physics thread
    }

    /// Sync cached values from MetricsRegistry.
    ///
    /// This should be called before rendering to ensure the cached values
    /// are up-to-date with the latest metrics from the registry.
    pub fn sync_from_metrics(&mut self) {
        if let Some(ref metrics) = self.metrics {
            let metrics_lock = metrics.read().unwrap();

            // Sync energy values from registry
            if let Some(kin_series) = metrics_lock.get_series("E_kin") {
                self.v_kin = kin_series.get().clone();
            }
            if let Some(pot_series) = metrics_lock.get_series("E_pot") {
                self.v_pot = pot_series.get().clone();
            }
            if let Some(time_value) = metrics_lock.get_current("time") {
                self.time = time_value;
            }

            // Increment version to signal that data has been updated
            self.version = self.version.wrapping_add(1);
        }
    }

    /// Calculate total energy (kinetic + potential) for each time point.
    /// Returns new VecDeque with element-wise sum.
    pub fn v_tot(&self) -> VecDeque<f32> {
        self.v_kin
            .iter()
            .zip(self.v_pot.iter())
            .map(|(k, p)| k + p)
            .collect()
    }
}

impl Default for EnergyData {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedEnergyData = Arc<Mutex<EnergyData>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_energy_data_new() {
        let data = EnergyData::new();
        assert_eq!(data.v_kin.len(), 0);
        assert_eq!(data.v_pot.len(), 0);
        assert_eq!(data.time, 0.0);
        assert_eq!(data.v_kin.capacity(), 1000);
        assert_eq!(data.v_pot.capacity(), 1000);
    }

    #[test]
    fn test_energy_data_default() {
        let data = EnergyData::default();
        assert_eq!(data.v_kin.len(), 0);
        assert_eq!(data.v_pot.len(), 0);
        assert_eq!(data.time, 0.0);
    }

    #[test]
    fn test_v_tot_calculation() {
        let mut data = EnergyData::new();
        data.v_kin.push_back(10.0);
        data.v_kin.push_back(20.0);
        data.v_kin.push_back(30.0);
        data.v_pot.push_back(5.0);
        data.v_pot.push_back(15.0);
        data.v_pot.push_back(25.0);

        let v_tot = data.v_tot();
        assert_eq!(v_tot.len(), 3);
        assert_eq!(v_tot[0], 15.0);
        assert_eq!(v_tot[1], 35.0);
        assert_eq!(v_tot[2], 55.0);
    }

    #[test]
    fn test_v_tot_empty() {
        let data = EnergyData::new();
        let v_tot = data.v_tot();
        assert_eq!(v_tot.len(), 0);
    }

    #[test]
    fn test_v_tot_mismatched_lengths() {
        let mut data = EnergyData::new();
        data.v_kin.push_back(10.0);
        data.v_kin.push_back(20.0);
        data.v_pot.push_back(5.0);

        let v_tot = data.v_tot();
        // zip stops at shortest sequence
        assert_eq!(v_tot.len(), 1);
        assert_eq!(v_tot[0], 15.0);
    }

    #[test]
    fn test_shared_energy_data_thread_safety() {
        let shared_data = Arc::new(Mutex::new(EnergyData::new()));

        // Add data through the mutex
        {
            let mut data = shared_data.lock().unwrap();
            data.v_kin.push_back(100.0);
            data.v_pot.push_back(50.0);
            data.time = 1.5;
        }

        // Read data through the mutex
        {
            let data = shared_data.lock().unwrap();
            assert_eq!(data.v_kin.len(), 1);
            assert_eq!(data.v_pot.len(), 1);
            assert_eq!(data.time, 1.5);
        }
    }

    #[test]
    fn test_energy_data_clone() {
        let mut data1 = EnergyData::new();
        data1.v_kin.push_back(10.0);
        data1.v_pot.push_back(5.0);
        data1.time = 2.0;

        let data2 = data1.clone();
        assert_eq!(data2.v_kin.len(), 1);
        assert_eq!(data2.v_pot.len(), 1);
        assert_eq!(data2.time, 2.0);
        assert_eq!(data2.v_kin[0], 10.0);
        assert_eq!(data2.v_pot[0], 5.0);
    }

    #[test]
    fn test_push_sample_is_noop() {
        // push_sample is now a no-op since metrics are recorded in MetricsRegistry
        let mut data = EnergyData::new();
        assert_eq!(data.version, 0);
        data.push_sample(1.0, 2.0, 0.1);
        // push_sample is a no-op, so version doesn't increment
        assert_eq!(data.version, 0);
        assert_eq!(data.v_kin.len(), 0);
        assert_eq!(data.v_pot.len(), 0);
    }

    #[test]
    fn test_sync_from_metrics_without_metrics() {
        // When no metrics are available, sync_from_metrics does nothing
        let mut data = EnergyData::new();
        data.sync_from_metrics();
        assert_eq!(data.v_kin.len(), 0);
        assert_eq!(data.v_pot.len(), 0);
    }

    #[test]
    fn test_sync_from_metrics_with_metrics() {
        use std::sync::RwLock;
        use thermo_runtime::metrics::MetricsRegistry;

        // Create a metrics registry with some data
        let metrics = Arc::new(RwLock::new(MetricsRegistry::default()));
        {
            let mut metrics_lock = metrics.write().unwrap();
            metrics_lock.record("E_kin", 7.0);
            metrics_lock.record("E_kin", 8.0);
            metrics_lock.record("E_pot", 3.0);
            metrics_lock.record("E_pot", 4.0);
        }

        // Create EnergyData with metrics
        let mut data = EnergyData::with_metrics(Arc::clone(&metrics));

        // Sync from metrics
        data.sync_from_metrics();

        // Should have synced the data
        assert_eq!(data.v_kin.len(), 2);
        assert_eq!(data.v_pot.len(), 2);
        assert_eq!(data.v_kin[0], 7.0);
        assert_eq!(data.v_pot[0], 3.0);
        assert_eq!(data.version, 1); // Version increments on sync
    }
}
