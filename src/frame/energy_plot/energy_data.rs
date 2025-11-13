use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Shared energy data structure for communication between main simulation and energy plot window
#[derive(Clone)]
pub struct EnergyData {
    pub v_kin: VecDeque<f32>,
    pub v_pot: VecDeque<f32>,
    pub time: f32,
}

impl EnergyData {
    pub fn new() -> Self {
        Self {
            v_kin: VecDeque::with_capacity(1000),
            v_pot: VecDeque::with_capacity(1000),
            time: 0.0,
        }
    }

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
