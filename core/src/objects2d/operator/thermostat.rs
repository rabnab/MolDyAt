//! Nosé-Hoover thermostat operator — data model, defaults, and collection.

use super::{
    OperatorCollection, OperatorIdCounter, OperatorState, Rect, Vec2D, is_degenerate, remap_point,
};

/// Default thermostat target temperature in LJ reduced units.
const DEFAULT_TARGET_TEMPERATURE: f32 = 35.0 / 122.0;
/// Default thermostat radius in world units.
pub const DEFAULT_THERMOSTAT_RADIUS: f32 = 5.0;
/// Default Nosé-Hoover coupling constant.
const DEFAULT_TAU: f32 = 0.5;
/// Lower bound for chain variables to avoid runaway values.
pub const THERMOSTAT_CHAIN_MIN: f32 = -10.0;
/// Upper bound for chain variables to avoid runaway values.
pub const THERMOSTAT_CHAIN_MAX: f32 = 10.0;
/// Scroll-wheel increment for target temperature changes.
pub const THERMOSTAT_TARGET_TEMP_STEP: f32 = 5.0 / 122.0;
/// Scroll-wheel increment for radius changes.
pub const THERMOSTAT_RADIUS_STEP: f32 = 0.5;
/// Minimum allowed thermostat radius.
const MIN_THERMOSTAT_RADIUS: f32 = 0.5;
/// Maximum allowed thermostat radius.
const MAX_THERMOSTAT_RADIUS: f32 = 50.0;
/// Minimum allowed target temperature.
const MIN_TARGET_TEMPERATURE: f32 = 0.0;
/// Maximum allowed target temperature.
const MAX_TARGET_TEMPERATURE: f32 = 10.0;

/// A finite spherical region that applies Nosé-Hoover velocity control.
#[derive(Debug, Clone)]
pub struct SimThermostat {
    /// Unique identifier assigned by [`ThermostatCollection`].
    pub id: usize,
    /// Center of the thermostat region in world coordinates.
    pub position: Vec2D,
    /// ON/OFF thermostat state.
    pub state: OperatorState,
    /// Target temperature in LJ reduced units.
    pub target_temperature: f32,
    /// Radius of the thermostat region in world units.
    pub radius: f32,
    /// Nosé-Hoover chain variables. The first element is used for now.
    pub chain: Vec<f32>,
    /// Coupling time constant.
    pub tau: f32,
    /// Current measured local temperature in LJ reduced units.
    pub current_local_temp: f32,
}

impl SimThermostat {
    /// Create a new thermostat starting in the `Off` state.
    pub fn new(id: usize, position: Vec2D) -> Self {
        Self {
            id,
            position,
            state: OperatorState::Off,
            target_temperature: DEFAULT_TARGET_TEMPERATURE,
            radius: DEFAULT_THERMOSTAT_RADIUS,
            chain: vec![0.0],
            tau: DEFAULT_TAU,
            current_local_temp: 0.0,
        }
    }

    /// Whether the thermostat is currently active.
    pub fn is_active(&self) -> bool {
        self.state.is_on()
    }

    /// Read the first chain variable, inserting it if absent.
    pub fn chain0_mut(&mut self) -> &mut f32 {
        if self.chain.is_empty() {
            self.chain.push(0.0);
        }
        &mut self.chain[0]
    }

    /// Adjust the target temperature by `delta`, clamped.
    pub fn adjust_target_temperature(&mut self, delta: f32) {
        self.target_temperature =
            (self.target_temperature + delta).clamp(MIN_TARGET_TEMPERATURE, MAX_TARGET_TEMPERATURE);
    }

    /// Adjust the thermostat radius by `delta`, clamped.
    pub fn adjust_radius(&mut self, delta: f32) {
        self.radius = (self.radius + delta).clamp(MIN_THERMOSTAT_RADIUS, MAX_THERMOSTAT_RADIUS);
    }
}

/// Owns the `Vec<SimThermostat>` and its ID counter.
#[derive(Debug, Clone, Default)]
pub struct ThermostatCollection {
    thermostats: Vec<SimThermostat>,
    next_id: OperatorIdCounter,
}

impl ThermostatCollection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new thermostat, returning its assigned ID.
    pub fn add(&mut self, position: Vec2D) -> usize {
        let id = self.next_id.next_id();
        self.thermostats.push(SimThermostat::new(id, position));
        id
    }

    /// Immutable access to all thermostats.
    pub fn thermostats(&self) -> &[SimThermostat] {
        &self.thermostats
    }

    /// Find a thermostat by ID (immutable).
    pub fn find(&self, id: usize) -> Option<&SimThermostat> {
        self.thermostats
            .iter()
            .find(|thermostat| thermostat.id == id)
    }

    /// Find a thermostat by ID (mutable).
    pub fn find_mut(&mut self, id: usize) -> Option<&mut SimThermostat> {
        self.thermostats
            .iter_mut()
            .find(|thermostat| thermostat.id == id)
    }
}

impl OperatorCollection for ThermostatCollection {
    fn clear(&mut self) {
        self.thermostats.clear();
        self.next_id.reset();
    }

    fn rescale(&mut self, old_rect: &Rect, new_rect: &Rect) {
        if is_degenerate(old_rect) {
            return;
        }

        let scale_x = (new_rect.w / old_rect.w).abs();
        let scale_y = (new_rect.h / old_rect.h).abs();
        let isotropic_scale = (scale_x * scale_y).sqrt();

        for thermostat in &mut self.thermostats {
            thermostat.position = remap_point(thermostat.position, old_rect, new_rect);
            thermostat.radius = (thermostat.radius * isotropic_scale)
                .clamp(MIN_THERMOSTAT_RADIUS, MAX_THERMOSTAT_RADIUS);
        }
    }
}
