//! Nosé-Hoover thermostat operator — placement, control, and per-step application.

use super::{MainContent, SimulationWorld, StatsAccumulator};
use crate::application::thermo_field::LocalKineticStats;
use crate::objects2d::{
    DEFAULT_THERMOSTAT_CHAIN_LENGTH, THERMOSTAT_CHAIN_MAX, THERMOSTAT_CHAIN_MIN,
    THERMOSTAT_RADIUS_STEP, THERMOSTAT_TARGET_TEMP_STEP, Vec2D,
};

/// Runtime bucket for one thermostat's owned balls during a simulation step.
struct ThermostatBucket {
    thermostat_id: usize,
    ball_indices: Vec<usize>,
    kinetic: LocalKineticStats,
}

impl ThermostatBucket {
    fn new(thermostat_id: usize) -> Self {
        Self {
            thermostat_id,
            ball_indices: Vec::new(),
            kinetic: LocalKineticStats::new(),
        }
    }
}

/// Use a few smaller reversible sub-steps for better local thermostat stability.
const NHC_SUBSTEPS: usize = 4;
const NHC_SUBSTEPS_F32: f32 = 4.0;
/// Lower bound for Nosé-Hoover masses to avoid division-by-zero.
const MIN_CHAIN_MASS: f32 = 1.0e-6;
/// Lower bound for user-facing time constant when deriving internal masses.
const MIN_EFFECTIVE_TAU: f32 = 1.0e-4;

fn clamp_chain_variable(value: f32) -> f32 {
    value.clamp(THERMOSTAT_CHAIN_MIN, THERMOSTAT_CHAIN_MAX)
}

fn compute_thermal_dof(count: usize) -> Option<f32> {
    if count < 2 {
        return None;
    }
    Some(2.0 * (count as f32 - 1.0))
}

/// Derive Nosé–Hoover chain masses from user-level thermostat controls.
///
/// `tau` is the only exposed coupling knob, so internal masses are derived as:
/// - `Q0 = g * T_target * tau^2`, where `g` is local thermal DOF (`2*(N-1)`)
/// - `Q1..Q4 = T_target * tau^2`
///
/// This keeps the primary thermostat coupled to the bucket size while using
/// uniform trailing masses for the remaining chain links.
fn derive_nhc_masses(
    tau: f32,
    target_temperature: f32,
    dof: f32,
) -> [f32; DEFAULT_THERMOSTAT_CHAIN_LENGTH] {
    let effective_tau = tau.max(MIN_EFFECTIVE_TAU);
    let tau_sq = effective_tau * effective_tau;
    let q0 = (dof * target_temperature * tau_sq).max(MIN_CHAIN_MASS);
    let q_trailing = (target_temperature * tau_sq).max(MIN_CHAIN_MASS);
    [q0, q_trailing, q_trailing, q_trailing, q_trailing]
}

fn chain_drive(
    index: usize,
    chain: &[f32],
    masses: &[f32; DEFAULT_THERMOSTAT_CHAIN_LENGTH],
    thermal_kinetic: f32,
    target_temperature: f32,
    thermal_dof: f32,
) -> f32 {
    if index == 0 {
        (2.0 * thermal_kinetic - thermal_dof * target_temperature) / masses[0]
    } else {
        let prev_chain_sq = chain[index - 1] * chain[index - 1];
        (masses[index - 1] * prev_chain_sq - target_temperature) / masses[index]
    }
}

fn integrate_nhc_chain(
    chain: &mut [f32],
    dt: f32,
    thermal_kinetic: f32,
    target_temperature: f32,
    thermal_dof: f32,
    tau: f32,
) -> f32 {
    let mut total_scale = 1.0_f32;
    let mut current_kinetic = thermal_kinetic.max(0.0);
    let masses = derive_nhc_masses(tau, target_temperature, thermal_dof);
    let sub_dt = dt / NHC_SUBSTEPS_F32;
    let quarter_dt = 0.25 * sub_dt;
    let eighth_dt = 0.125 * sub_dt;

    for _ in 0..NHC_SUBSTEPS {
        for idx in (1..DEFAULT_THERMOSTAT_CHAIN_LENGTH).rev() {
            let drive = chain_drive(
                idx,
                chain,
                &masses,
                current_kinetic,
                target_temperature,
                thermal_dof,
            );
            chain[idx] = clamp_chain_variable(chain[idx] + quarter_dt * drive);
            let coupling = (-eighth_dt * chain[idx]).exp();
            chain[idx - 1] = clamp_chain_variable(chain[idx - 1] * coupling);
        }

        let drive0 = chain_drive(
            0,
            chain,
            &masses,
            current_kinetic,
            target_temperature,
            thermal_dof,
        );
        chain[0] = clamp_chain_variable(chain[0] + 0.5 * sub_dt * drive0);

        let scale = (-sub_dt * chain[0]).exp();
        total_scale *= scale;
        current_kinetic *= scale * scale;

        for idx in 1..DEFAULT_THERMOSTAT_CHAIN_LENGTH {
            let coupling = (-eighth_dt * chain[idx]).exp();
            chain[idx - 1] = clamp_chain_variable(chain[idx - 1] * coupling);
            let drive = chain_drive(
                idx,
                chain,
                &masses,
                current_kinetic,
                target_temperature,
                thermal_dof,
            );
            chain[idx] = clamp_chain_variable(chain[idx] + quarter_dt * drive);
        }
    }

    total_scale
}

impl MainContent {
    /// Place a new thermostat at `position`.
    pub(crate) fn add_thermostat(&mut self, position: Vec2D) {
        self.world.add_thermostat(position);
    }

    /// Toggle the on/off state of an existing thermostat.
    pub(crate) fn toggle_thermostat(&mut self, thermostat_id: usize) {
        self.world.toggle_thermostat(thermostat_id);
    }

    /// Move an existing thermostat to a new world-space position.
    pub(crate) fn move_thermostat(&mut self, thermostat_id: usize, new_pos: Vec2D) {
        self.world.move_thermostat(thermostat_id, new_pos);
    }

    /// Adjust the target temperature of a thermostat.
    pub(crate) fn adjust_thermostat_target_temperature(
        &mut self,
        thermostat_id: usize,
        delta_steps: f32,
    ) {
        self.world
            .adjust_thermostat_target_temperature(thermostat_id, delta_steps);
    }

    /// Adjust the radius of a thermostat.
    pub(crate) fn adjust_thermostat_radius(&mut self, thermostat_id: usize, delta_steps: f32) {
        self.world
            .adjust_thermostat_radius(thermostat_id, delta_steps);
    }

    /// Apply all active thermostats after force/collision resolution.
    pub(crate) fn apply_thermostats(&mut self, dt: f32, stats_accumulator: &mut StatsAccumulator) {
        self.world.apply_thermostats(dt, stats_accumulator);
    }
}

impl SimulationWorld {
    pub(crate) fn add_thermostat(&mut self, position: Vec2D) {
        self.thermostats.add(position);
    }

    pub(crate) fn toggle_thermostat(&mut self, thermostat_id: usize) {
        if let Some(thermostat) = self.thermostats.find_mut(thermostat_id) {
            thermostat.state.toggle();
        }
    }

    pub(crate) fn move_thermostat(&mut self, thermostat_id: usize, new_pos: Vec2D) {
        if let Some(thermostat) = self.thermostats.find_mut(thermostat_id) {
            thermostat.position = new_pos;
        }
    }

    pub(crate) fn adjust_thermostat_target_temperature(
        &mut self,
        thermostat_id: usize,
        delta_steps: f32,
    ) {
        if let Some(thermostat) = self.thermostats.find_mut(thermostat_id) {
            thermostat.adjust_target_temperature(delta_steps * THERMOSTAT_TARGET_TEMP_STEP);
        }
    }

    pub(crate) fn adjust_thermostat_radius(&mut self, thermostat_id: usize, delta_steps: f32) {
        if let Some(thermostat) = self.thermostats.find_mut(thermostat_id) {
            thermostat.adjust_radius(delta_steps * THERMOSTAT_RADIUS_STEP);
        }
    }

    pub(crate) fn apply_thermostats(&mut self, dt: f32, stats_accumulator: &mut StatsAccumulator) {
        let active_ids: Vec<usize> = self
            .thermostats
            .thermostats()
            .iter()
            .filter(|thermostat| thermostat.is_active())
            .map(|thermostat| thermostat.id)
            .collect();

        if active_ids.is_empty() {
            return;
        }

        for thermostat in self.thermostats.thermostats().iter() {
            if !thermostat.is_active() {
                continue;
            }
            debug_assert!(thermostat.radius.is_finite());
        }

        let mut buckets: Vec<ThermostatBucket> = active_ids
            .iter()
            .map(|&thermostat_id| ThermostatBucket::new(thermostat_id))
            .collect();

        for (ball_idx, ball) in self.balls.iter().enumerate() {
            let Some(owner_bucket_idx) =
                self.find_nearest_active_thermostat_bucket(*ball.get_position(), &buckets)
            else {
                continue;
            };

            buckets[owner_bucket_idx].ball_indices.push(ball_idx);
            buckets[owner_bucket_idx].kinetic.add_ball(ball);
        }

        let mut velocity_changed = false;

        for bucket in &buckets {
            let should_apply = bucket.kinetic.temperature().is_some();

            if let Some(thermostat) = self.thermostats.find_mut(bucket.thermostat_id) {
                thermostat.current_local_temp = f32::NAN;
            }

            if !should_apply {
                continue;
            }

            if self.apply_thermostat_bucket(bucket, dt) {
                velocity_changed = true;
            }

            let measured_temp = self
                .measure_bucket_temperature(&bucket.ball_indices)
                .unwrap_or(f32::NAN);
            if let Some(thermostat) = self.thermostats.find_mut(bucket.thermostat_id) {
                thermostat.current_local_temp = measured_temp;
            }
        }

        if velocity_changed {
            self.refresh_velocity_stats(stats_accumulator);
        }
    }

    fn find_nearest_active_thermostat_bucket(
        &self,
        ball_pos: Vec2D,
        buckets: &[ThermostatBucket],
    ) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;

        for (bucket_idx, bucket) in buckets.iter().enumerate() {
            let Some(thermostat) = self.thermostats.find(bucket.thermostat_id) else {
                continue;
            };

            let distance_sq = ball_pos.distance_squared(&thermostat.position);
            if distance_sq > thermostat.radius * thermostat.radius {
                continue;
            }

            match best {
                Some((_, best_distance_sq)) if distance_sq >= best_distance_sq => {}
                _ => best = Some((bucket_idx, distance_sq)),
            }
        }

        best.map(|(bucket_idx, _)| bucket_idx)
    }

    /// Apply one thermostat bucket update in local COM coordinates.
    ///
    /// Steps:
    /// 1. Measure local COM-frame thermal state (`T_local`, `v_com`, DOF).
    /// 2. Handle explicit edge cases (`target_temperature == 0.0` => freeze owned balls).
    /// 3. Integrate the coupled 5-chain Nosé–Hoover variables.
    /// 4. Scale only peculiar velocities `u_i = v_i - v_com`, then reconstruct
    ///    `v_i = v_com + u_i'` so bucket drift is preserved.
    ///
    /// Returns `true` only when owned velocities were actually modified.
    fn apply_thermostat_bucket(&mut self, bucket: &ThermostatBucket, dt: f32) -> bool {
        let Some(local_temp) = bucket.kinetic.temperature() else {
            return false;
        };
        let Some(v_com) = bucket.kinetic.center_of_mass_velocity() else {
            return false;
        };
        let Some(thermal_dof) = compute_thermal_dof(bucket.kinetic.count()) else {
            return false;
        };

        let Some(thermostat) = self.thermostats.find_mut(bucket.thermostat_id) else {
            return false;
        };

        let target_temperature = thermostat.target_temperature;
        thermostat.ensure_chain_len(DEFAULT_THERMOSTAT_CHAIN_LENGTH);

        if target_temperature <= f32::EPSILON {
            for &ball_idx in &bucket.ball_indices {
                self.balls[ball_idx].v = Vec2D::zero();
            }
            thermostat.chain.fill(0.0);
            return !bucket.ball_indices.is_empty();
        }

        let thermal_kinetic = 0.5 * thermal_dof * local_temp;
        if !thermal_kinetic.is_finite() {
            return false;
        }

        let damping_scale = integrate_nhc_chain(
            thermostat.chain.as_mut_slice(),
            dt,
            thermal_kinetic,
            target_temperature,
            thermal_dof,
            thermostat.tau,
        );
        if !damping_scale.is_finite() || damping_scale <= 0.0 {
            return false;
        }

        for &ball_idx in &bucket.ball_indices {
            // Thermostat only the peculiar velocity in the local COM frame.
            let peculiar_velocity = self.balls[ball_idx].v - &v_com;
            self.balls[ball_idx].v = v_com.add_scaled(&peculiar_velocity, damping_scale);
        }

        !bucket.ball_indices.is_empty()
    }

    fn measure_bucket_temperature(&self, ball_indices: &[usize]) -> Option<f32> {
        let mut kinetic = LocalKineticStats::new();
        for &ball_idx in ball_indices {
            kinetic.add_ball(&self.balls[ball_idx]);
        }
        kinetic.temperature()
    }

    fn refresh_velocity_stats(&self, stats_accumulator: &mut StatsAccumulator) {
        stats_accumulator.kin = 0.0;
        stats_accumulator.v_com = Vec2D::zero();
        stats_accumulator.sum_mass = 0.0;
        stats_accumulator.max_v = 0.0;
        stats_accumulator.max_id = 0;

        for ball in &self.balls {
            stats_accumulator.v_com += ball.moment();
            stats_accumulator.sum_mass += ball.mass;

            let speed_sq = ball.v.norm_sq();
            stats_accumulator.kin += speed_sq * ball.mass * 0.5;

            if stats_accumulator.max_v * stats_accumulator.max_v < speed_sq {
                stats_accumulator.max_v = speed_sq.sqrt();
                stats_accumulator.max_id = ball.get_id();
            }
        }
    }
}
