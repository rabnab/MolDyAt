// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use thermo_runtime::metrics::{RingBuffer, SharedMetricsRegistry, read_metrics};

/// Bring `cache` up to date with `series`: append the values pushed since the
/// last sync (`cursor`) when the series was not cleared and nothing it
/// pushed has been evicted unseen, copy it whole otherwise.
fn sync_series<T: Copy>(
    cache: &mut VecDeque<T>,
    cursor: &mut Option<(u64, u64)>,
    series: Option<&RingBuffer<T>>,
) {
    let Some(series) = series else {
        cache.clear();
        *cursor = None;
        return;
    };
    let now = (series.epoch(), series.total_pushed());
    let new = match *cursor {
        Some((epoch, pushed)) if epoch == now.0 && pushed <= now.1 => (now.1 - pushed) as usize,
        _ => usize::MAX,
    };
    if new <= series.len() {
        cache.extend(series.get().iter().skip(series.len() - new).copied());
        while cache.len() > series.capacity() {
            cache.pop_front();
        }
    } else {
        cache.clear();
        cache.extend(series.get().iter().copied());
    }
    *cursor = Some(now);
}

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
    /// Simulation step of each sample (synced from MetricsRegistry). The
    /// series end at the newest step: the last value of `v_kin` and `v_pot`
    /// belongs to the last step here.
    pub steps: VecDeque<u64>,
    pub time: f32,
    pub status_text: Option<String>,
    /// Monotonically increasing counter that increments each time energy samples
    /// are synced. Renderers can compare this against a cached version to avoid
    /// rebuilding the plot when nothing has changed.
    pub version: u64,
    /// Shared reference to the metrics registry
    metrics: Option<SharedMetricsRegistry>,
    /// Registry version of the last sync.
    synced_version: Option<u64>,
    /// `(epoch, total pushed)` of the registry series at the last sync, to
    /// append only what is new.
    kin_cursor: Option<(u64, u64)>,
    pot_cursor: Option<(u64, u64)>,
    steps_cursor: Option<(u64, u64)>,
}

impl EnergyData {
    /// Create new energy data with pre-allocated capacity of 1000 samples.
    pub fn new() -> Self {
        Self {
            v_kin: VecDeque::with_capacity(1000),
            v_pot: VecDeque::with_capacity(1000),
            steps: VecDeque::with_capacity(1000),
            time: 0.0,
            status_text: None,
            version: 0,
            metrics: None,
            synced_version: None,
            kin_cursor: None,
            pot_cursor: None,
            steps_cursor: None,
        }
    }

    /// Create new energy data with a reference to MetricsRegistry.
    pub fn with_metrics(metrics: SharedMetricsRegistry) -> Self {
        Self {
            v_kin: VecDeque::with_capacity(1000),
            v_pot: VecDeque::with_capacity(1000),
            steps: VecDeque::with_capacity(1000),
            time: 0.0,
            status_text: None,
            version: 0,
            metrics: Some(metrics),
            synced_version: None,
            kin_cursor: None,
            pot_cursor: None,
            steps_cursor: None,
        }
    }

    /// Take the energy series from the MetricsRegistry if it changed since the
    /// last sync (appending only the new values when it can), and bump
    /// `version` then. Called before rendering; without
    /// a registry, or without news, it does nothing, so the plot is not
    /// rebuilt for nothing.
    pub fn sync_from_metrics(&mut self) {
        let Some(metrics) = &self.metrics else {
            return;
        };
        let registry = read_metrics(metrics);
        if self.synced_version == Some(registry.version()) {
            return;
        }
        self.synced_version = Some(registry.version());
        sync_series(
            &mut self.v_kin,
            &mut self.kin_cursor,
            registry.get_series("E_kin"),
        );
        sync_series(
            &mut self.v_pot,
            &mut self.pot_cursor,
            registry.get_series("E_pot"),
        );
        sync_series(
            &mut self.steps,
            &mut self.steps_cursor,
            Some(registry.steps()),
        );
        self.version = self.version.wrapping_add(1);
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
    use thermo_runtime::metrics::MetricsRegistry;

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
    fn test_sync_from_metrics_without_metrics() {
        let mut data = EnergyData::new();
        data.sync_from_metrics();
        assert_eq!(data.version, 0);
        assert!(data.v_kin.is_empty());
    }

    #[test]
    fn test_sync_from_metrics_copies_series_and_bumps_version_once() {
        let metrics = MetricsRegistry::shared();
        {
            let mut registry = metrics.write().unwrap();
            registry.record("E_kin", 7.0);
            registry.record("E_kin", 8.0);
            registry.record("E_pot", 3.0);
            registry.record("E_pot", 4.0);
        }
        let mut data = EnergyData::with_metrics(Arc::clone(&metrics));
        data.sync_from_metrics();
        assert_eq!(data.v_kin.iter().copied().collect::<Vec<_>>(), [7.0, 8.0]);
        assert_eq!(data.v_pot.iter().copied().collect::<Vec<_>>(), [3.0, 4.0]);
        assert_eq!(data.version, 1);

        // Nothing new in the registry: no copy, no new version.
        data.sync_from_metrics();
        assert_eq!(data.version, 1);

        metrics.write().unwrap().record("E_kin", 9.0);
        data.sync_from_metrics();
        assert_eq!(data.v_kin.len(), 3);
        assert_eq!(data.version, 2);
    }

    #[test]
    fn test_sync_from_metrics_follows_a_cleared_registry() {
        let metrics = MetricsRegistry::shared();
        metrics.write().unwrap().record("E_kin", 1.0);
        let mut data = EnergyData::with_metrics(Arc::clone(&metrics));
        data.sync_from_metrics();
        metrics.write().unwrap().clear();
        data.sync_from_metrics();
        assert!(data.v_kin.is_empty());
    }

    #[test]
    fn test_sync_appends_new_values_and_survives_ring_wraparound() {
        let metrics = Arc::new(std::sync::RwLock::new(MetricsRegistry::new(3)));
        let mut data = EnergyData::with_metrics(Arc::clone(&metrics));
        for round in 0..7 {
            {
                let mut registry = metrics.write().unwrap();
                registry.record("E_kin", round as f32);
                registry.record("E_pot", -(round as f32));
                if round == 4 {
                    // Several values between two syncs, more than fit.
                    for extra in 0..5 {
                        registry.record("E_kin", 100.0 + extra as f32);
                    }
                }
            }
            data.sync_from_metrics();
            let registry = metrics.read().unwrap();
            let expected = |name: &str| -> Vec<f32> {
                registry
                    .get_series(name)
                    .unwrap()
                    .get()
                    .iter()
                    .copied()
                    .collect()
            };
            assert_eq!(
                data.v_kin.iter().copied().collect::<Vec<_>>(),
                expected("E_kin")
            );
            assert_eq!(
                data.v_pot.iter().copied().collect::<Vec<_>>(),
                expected("E_pot")
            );
        }
    }

    #[test]
    fn test_sync_version_wraps_without_panic() {
        let metrics = MetricsRegistry::shared();
        let mut data = EnergyData::with_metrics(metrics);
        data.version = u64::MAX;
        data.sync_from_metrics();
        assert_eq!(data.version, 0);
    }
}
