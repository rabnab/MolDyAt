//! Nosé-Hoover thermostat operator — placement, control, and per-step application.

use super::{MainContent, StatsAccumulator};
use crate::application::thermo_field::LocalKineticStats;
use crate::objects2d::{
    THERMOSTAT_CHAIN_MAX, THERMOSTAT_CHAIN_MIN, THERMOSTAT_RADIUS_STEP,
    THERMOSTAT_TARGET_TEMP_STEP, Vec2D,
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

impl MainContent {
    /// Place a new thermostat at `position`.
    pub(crate) fn add_thermostat(&mut self, position: Vec2D) {
        self.thermostats.add(position);
    }

    /// Toggle the on/off state of an existing thermostat.
    pub(crate) fn toggle_thermostat(&mut self, thermostat_id: usize) {
        if let Some(thermostat) = self.thermostats.find_mut(thermostat_id) {
            thermostat.state.toggle();
        }
    }

    /// Move an existing thermostat to a new world-space position.
    pub(crate) fn move_thermostat(&mut self, thermostat_id: usize, new_pos: Vec2D) {
        if let Some(thermostat) = self.thermostats.find_mut(thermostat_id) {
            thermostat.position = new_pos;
        }
    }

    /// Adjust the target temperature of a thermostat.
    pub(crate) fn adjust_thermostat_target_temperature(
        &mut self,
        thermostat_id: usize,
        delta_steps: f32,
    ) {
        if let Some(thermostat) = self.thermostats.find_mut(thermostat_id) {
            thermostat.adjust_target_temperature(delta_steps * THERMOSTAT_TARGET_TEMP_STEP);
        }
    }

    /// Adjust the radius of a thermostat.
    pub(crate) fn adjust_thermostat_radius(&mut self, thermostat_id: usize, delta_steps: f32) {
        if let Some(thermostat) = self.thermostats.find_mut(thermostat_id) {
            thermostat.adjust_radius(delta_steps * THERMOSTAT_RADIUS_STEP);
        }
    }

    /// Apply all active thermostats after force/collision resolution.
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
            let local_temp = bucket.kinetic.temperature();
            let should_apply =
                if let Some(thermostat) = self.thermostats.find_mut(bucket.thermostat_id) {
                    thermostat.current_local_temp = local_temp.unwrap_or(f32::NAN);
                    local_temp.is_some()
                } else {
                    false
                };

            if !should_apply {
                continue;
            }

            if self.apply_thermostat_bucket(bucket, dt) {
                velocity_changed = true;
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

    fn apply_thermostat_bucket(&mut self, bucket: &ThermostatBucket, dt: f32) -> bool {
        let Some(local_temp) = bucket.kinetic.temperature() else {
            return false;
        };
        let Some(v_com) = bucket.kinetic.center_of_mass_velocity() else {
            return false;
        };

        let Some(thermostat) = self.thermostats.find_mut(bucket.thermostat_id) else {
            return false;
        };

        if thermostat.target_temperature <= f32::EPSILON {
            for &ball_idx in &bucket.ball_indices {
                self.balls[ball_idx].v = Vec2D::zero();
            }
            *thermostat.chain0_mut() = 0.0;
            return !bucket.ball_indices.is_empty();
        }

        let tau_sq = thermostat.tau * thermostat.tau;
        if tau_sq <= f32::EPSILON {
            return false;
        }

        let target_temperature = thermostat.target_temperature;
        let xi = thermostat.chain0_mut();
        *xi += dt * (local_temp / target_temperature - 1.0) / tau_sq;
        *xi = xi.clamp(THERMOSTAT_CHAIN_MIN, THERMOSTAT_CHAIN_MAX);
        let friction = dt * *xi;

        for &ball_idx in &bucket.ball_indices {
            // Temperature is measured in the bucket COM frame, so only the
            // peculiar velocity should be thermostatted. Preserving `v_com`
            // avoids spuriously slowing or accelerating a rigidly drifting
            // cluster that has zero local thermal motion.
            let peculiar_velocity = self.balls[ball_idx].v - &v_com;
            self.balls[ball_idx].v = v_com.add_scaled(&peculiar_velocity, 1.0 - friction);
        }

        !bucket.ball_indices.is_empty()
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
