use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

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

/// Shared energy data structure for communication between main simulation and energy plot window
#[derive(Clone)]
pub struct EnergyData {
    pub v_kin: VecDeque<f32>,
    pub v_pot: VecDeque<f32>,
    pub time: f32,
    pub status_text: Option<String>,
    /// Monotonically increasing counter that increments each time energy samples
    /// are pushed. Renderers can compare this against a cached version to avoid
    /// rebuilding the plot when nothing has changed.
    pub version: u64,
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
        }
    }

    /// Push a kinetic and potential energy sample, incrementing the version.
    pub fn push_sample(&mut self, kinetic: f32, potential: f32, time: f32) {
        self.v_kin.push_back(kinetic);
        self.v_pot.push_back(potential);
        self.time = time;
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
    fn test_push_sample_increments_version() {
        let mut data = EnergyData::new();
        assert_eq!(data.version, 0);
        data.push_sample(1.0, 2.0, 0.1);
        assert_eq!(data.version, 1);
        data.push_sample(3.0, 4.0, 0.2);
        assert_eq!(data.version, 2);
    }

    #[test]
    fn test_push_sample_stores_values() {
        let mut data = EnergyData::new();
        data.push_sample(7.0, 3.0, 5.0);
        assert_eq!(data.v_kin.len(), 1);
        assert_eq!(data.v_pot.len(), 1);
        assert_eq!(data.v_kin[0], 7.0);
        assert_eq!(data.v_pot[0], 3.0);
        assert_eq!(data.time, 5.0);
    }

    #[test]
    fn test_push_sample_version_wraps_without_panic() {
        let mut data = EnergyData::new();
        data.version = u64::MAX;
        data.push_sample(0.0, 0.0, 0.0);
        // wrapping_add(1) on u64::MAX yields 0
        assert_eq!(data.version, 0);
    }
}
