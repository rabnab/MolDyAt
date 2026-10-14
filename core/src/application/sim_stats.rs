// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::core::StatsAccumulator;
use crate::application::thermo_field::LocalKineticStats;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use std::collections::VecDeque;

const PLT_SIZE: usize = 1000;
const PLT_STEP: usize = 100;

const _U_KG: f32 = 1.66E-27;
const _MASS_SCALE_KG: f32 = 39.948 * _U_KG; //6.6E-26kg mass of Ar
const ENERGY_SCALE_J: f32 = 122. * KBOLTZMANN_J_P_K; //energy scale for Ar at 122K
const LEN_SCALE_M: f32 = 3.405E-10; // σ for Argon (standard LJ parameter)
const TIME_SCALE: f32 = 2.137E-12; // LEN_SCALE_M * sqrt(MASS_SCALE_KG / ENERGY_SCALE_J)
const TIME_SCALE_NS: f32 = TIME_SCALE * 1E9;
const KBOLTZMANN_J_P_K: f32 = 1.380649E-23;
const TEMP_SCALE: f32 = 122.; //=ENERGY_SCALE_J / KBOLTZMANN_J_P_K; //exact 122K
pub(super) const NORMAL_GRAVITY_MPS2: f32 = 9.81;
pub(super) const HIGH_GRAVITY_MPS2: f32 = NORMAL_GRAVITY_MPS2 * 5.0;

const fn sim_acceleration_from_mps2(accel_mps2: f32) -> f32 {
    accel_mps2 * TIME_SCALE * TIME_SCALE / LEN_SCALE_M
}

pub(super) const NORMAL_GRAVITY: f32 = sim_acceleration_from_mps2(NORMAL_GRAVITY_MPS2);
pub(super) const HIGH_GRAVITY: f32 = sim_acceleration_from_mps2(HIGH_GRAVITY_MPS2);

/// Simulation statistics updated once per physics step.
///
/// Owns energy, temperature, velocity, and timing metrics that are
/// accumulated during each step and read by snapshot creation.
/// Does **not** own frame-render bookkeeping or neighbor-update
/// scheduling — those responsibilities belong to their respective
/// orchestrators.
pub struct SimStats {
    energy_pot: f32,
    energy_kin: f32,
    temperature: f32,
    max_v: f32,
    max_id: usize,
    v_com: Vec2D,
    time: f32,
    step: usize,
    pause: bool,

    v_pot: VecDeque<f32>,
    v_kin: VecDeque<f32>,
    max_neighbours: usize,

    // Wall momentum tracking
    pub wall_momentum_x: f32,
    pub wall_momentum_y: f32,

    /// Wall contacts resolved exactly during the position update (box walls
    /// and line segments, see `wall_contact`), cumulative.
    pub wall_contacts: u64,
    /// Balls the overlap fallback pushed out of a line wall (they started a
    /// step inside a wall), cumulative. Should stay rare: the fallback
    /// projects the ball, which does not conserve energy exactly.
    pub wall_overlap_corrections: u64,
    /// Ball-wall distance computations in the contact sweep and the overlap
    /// pass, cumulative: pairs that passed the bounding-box reject. A
    /// deterministic measure of wall-collision work (no timing), guarded by
    /// `test_wall_distance_checks_stay_local`.
    pub wall_distance_checks: u64,
}

impl SimStats {
    /// Toggle simulation pause state.
    pub(crate) fn toggle_pause(&mut self) {
        self.set_pause(!self.is_paused())
    }

    /// Set simulation pause state.
    ///
    /// # Arguments
    /// * `pause` - If true, pause simulation; if false, resume
    pub fn set_pause(&mut self, pause: bool) {
        self.pause = pause;
    }

    pub fn is_paused(&self) -> bool {
        self.pause
    }

    #[cfg(test)]
    pub fn len_v_energy(&self) -> usize {
        self.v_kin.len().max(self.v_pot.len())
    }

    pub fn clear_energy(&mut self) {
        self.v_kin.clear();
        self.v_pot.clear();
    }

    pub fn get_status_text_real(&self, fps: u32, num_balls: usize, dt: f32) -> String {
        format!(
            "fps {} | time: {:.1}ns | dt: {:.4}ns\n\
             #Balls: {} | maxNeig: {}\n\
             TotalE: {:.3E}J | Ekin: {:.3E}J\n\
             Epot: {:.3E}J\n\
             Temp: {:.2}K | maxV: {:.2}m/s\n\
             vCOM: ({:.2e}, {:.2e})m/s",
            fps,
            self.time * TIME_SCALE_NS,
            dt * TIME_SCALE_NS,
            num_balls,
            self.max_neighbours,
            (self.energy_pot + self.energy_kin) * ENERGY_SCALE_J,
            self.energy_kin * ENERGY_SCALE_J,
            self.energy_pot * ENERGY_SCALE_J,
            self.temperature * TEMP_SCALE,
            self.max_v * LEN_SCALE_M / TIME_SCALE,
            self.v_com.x * LEN_SCALE_M / TIME_SCALE,
            self.v_com.y * LEN_SCALE_M / TIME_SCALE
        )
    }

    pub(crate) fn get_status_text(
        &self,
        fps: u32,
        num_balls: usize,
        hood_cnt: usize,
        dt: f32,
    ) -> String {
        format!(
            "fps {} time: {:.1} (dt:{:.4})\n#Balls: {} #hood: {}\nmaxNeig: {}\n\
                TotalE: {:.3E}\nEkin: {:.3E}\nEpot: {:.3E}\nTemp: {:.3}\nmaxV: {:.2}\nvCOM: {}",
            fps,
            self.time,
            dt,
            num_balls,
            hood_cnt,
            self.max_neighbours,
            self.energy_pot + self.energy_kin,
            self.energy_kin,
            self.energy_pot,
            self.temperature,
            self.max_v,
            self.v_com,
        )
    }

    /// Update statistics from accumulated physics data.
    /// Stores energy values in deques every PLT_STEP iterations for plotting.
    /// Maintains fixed deque size of PLT_SIZE by removing oldest values.
    ///
    /// # Arguments
    /// * `dt` - Time step
    /// * `stats_acumulator` - Accumulated physics statistics (kinetic, potential energy, velocities)
    pub fn update(&mut self, dt: f32, stats_acumulator: StatsAccumulator) {
        self.energy_kin = stats_acumulator.kin;
        self.energy_pot = stats_acumulator.pot;
        if self.step.is_multiple_of(PLT_STEP) {
            if self.v_pot.len() >= PLT_SIZE {
                self.v_pot.pop_front();
            }
            self.v_pot.push_back(stats_acumulator.pot);
            if self.v_kin.len() >= PLT_SIZE {
                self.v_kin.pop_front();
            }
            self.v_kin.push_back(stats_acumulator.kin);
        }

        self.max_v = stats_acumulator.max_v;
        self.max_id = stats_acumulator.max_id;
        self.time += dt;
        self.step += 1;
        self.v_com = stats_acumulator.v_com;
    }

    /// Calculate temperature from kinetic energy in center-of-mass frame.
    /// Temperature is proportional to average kinetic energy per particle relative to COM velocity.
    /// Also updates max_neighbours count.
    ///
    /// # Arguments
    /// * `v_balls` - Slice of all balls in simulation
    #[inline]
    pub(crate) fn calculate_temperature_from_balls(&mut self, v_balls: &[Ball2D]) {
        let mut kinetic = LocalKineticStats::new();
        let mut max_neighbours = 0;

        for cur in v_balls {
            kinetic.add_ball(cur);
            max_neighbours = max_neighbours.max(cur.hood.get_number_of_neighbors());
        }

        self.max_neighbours = max_neighbours;
        // Keep the global temperature definition consistent with thermostats and
        // thermo-field overlays by using the shared COM-subtracted accumulator.
        self.temperature = kinetic.temperature().unwrap_or(0.0);
    }

    pub(crate) fn get_kinetic_energy(&self) -> f32 {
        self.energy_kin
    }

    pub(crate) fn get_potential_energy(&self) -> f32 {
        self.energy_pot
    }

    pub(crate) fn get_time(&self) -> f32 {
        self.time
    }
}

impl Default for SimStats {
    fn default() -> SimStats {
        SimStats {
            energy_pot: 0.,
            energy_kin: 0.,
            temperature: 0.,
            max_v: 0.,
            max_id: 0,
            v_com: Vec2D::new(0., 0.),
            time: 0.,
            step: 0,
            pause: false,
            v_pot: VecDeque::with_capacity(100),
            v_kin: VecDeque::with_capacity(100),
            max_neighbours: 0,
            wall_momentum_x: 0.,
            wall_momentum_y: 0.,
            wall_contacts: 0,
            wall_overlap_corrections: 0,
            wall_distance_checks: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects2d::ball_impl::Ball2D;

    fn make_ball(id: usize, velocity: Vec2D) -> Ball2D {
        let mut ball = Ball2D::new(id, Vec2D::zero(), (0, 1.0));
        ball.v = velocity;
        ball
    }
    //use float_cmp::approx_eq;

    #[test]
    fn test_defaults() {
        let default_sim_stats = SimStats::default();
        assert_eq!(default_sim_stats.energy_pot, 0.);
        assert_eq!(default_sim_stats.energy_kin, 0.);
    }

    #[test]
    fn test_get_kinetic_energy() {
        let mut stats = SimStats::default();
        assert_eq!(stats.get_kinetic_energy(), 0.0);

        stats.energy_kin = 42.5;
        assert_eq!(stats.get_kinetic_energy(), 42.5);
    }

    #[test]
    fn test_get_potential_energy() {
        let mut stats = SimStats::default();
        assert_eq!(stats.get_potential_energy(), 0.0);

        stats.energy_pot = 17.3;
        assert_eq!(stats.get_potential_energy(), 17.3);
    }

    #[test]
    fn test_get_time() {
        let mut stats = SimStats::default();
        assert_eq!(stats.get_time(), 0.0);

        stats.time = 123.456;
        assert_eq!(stats.get_time(), 123.456);
    }

    #[test]
    fn test_energy_getters_independence() {
        let stats = SimStats {
            energy_kin: 10.0,
            energy_pot: 20.0,
            time: 5.0,
            ..Default::default()
        };

        assert_eq!(stats.get_kinetic_energy(), 10.0);
        assert_eq!(stats.get_potential_energy(), 20.0);
        assert_eq!(stats.get_time(), 5.0);

        // Verify getters don't modify state
        assert_eq!(stats.get_kinetic_energy(), 10.0);
        assert_eq!(stats.get_potential_energy(), 20.0);
    }

    #[test]
    fn test_calculate_temperature_matches_shared_com_definition() {
        let mut stats = SimStats::default();
        let balls = vec![
            make_ball(0, Vec2D::new(1.0, 0.0)),
            make_ball(1, Vec2D::new(-1.0, 0.0)),
        ];

        stats.calculate_temperature_from_balls(&balls);

        assert!((stats.temperature - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_status_text_shows_hood_cnt_parameter() {
        let stats = SimStats::default();
        let text = stats.get_status_text(60, 10, 42, 0.01);
        assert!(
            text.contains("#hood: 42"),
            "status text should include the '#hood: 42' label/value pair for the passed-in hood_cnt, got: {}",
            text
        );
    }

    #[test]
    fn test_clear_energy_empties_deques() {
        let mut stats = SimStats::default();
        // Manually push some values into the energy deques
        stats.v_kin.push_back(1.0);
        stats.v_kin.push_back(2.0);
        stats.v_pot.push_back(3.0);

        assert_eq!(stats.len_v_energy(), 2);

        stats.clear_energy();

        assert_eq!(
            stats.len_v_energy(),
            0,
            "energy deques should be empty after clear_energy"
        );
    }

    #[test]
    fn test_wall_momentum_accumulates_via_wall_bounce() {
        // Place a single ball just inside the right wall moving toward it.
        // After a few simulation steps the ball bounces, and SimStats must
        // record a non-zero wall momentum transferred during the impact.
        use crate::application::MainContent;
        use crate::objects2d::ball_impl::BallFactory;
        use crate::objects2d::{Rect, Vec2D};

        // Tiny world: x in [0, 10], y in [0, -10]
        let world_rect = Rect::new(0.0, 0.0, 10.0, -10.0);
        let mut mc = MainContent::new(world_rect).expect("failed to create simulation");
        mc.world_mut().speed_limit_override = Some(f32::INFINITY);

        let factory = BallFactory::new();
        let ball_radius = factory.get_new_ball_radius();
        // Start close to the right wall (x = world_rect.x + world_rect.w = 10.0)
        // and move rightward so a bounce is guaranteed within a few steps.
        let ball = factory.create_massive_with_velocity(
            Vec2D::new(10.0 - ball_radius - 0.05, -5.0),
            Vec2D::new(10.0, 0.0),
        );
        let ball_mass = ball.mass;
        mc.world_mut().balls.push(ball);
        mc.world_mut().rebuild_cell_grid();

        let bound = mc.world().world_region();

        for _ in 0..10 {
            mc.update_sim(0.01, &bound);
        }

        // After the bounce the ball reverses direction, so ball_momentum < 0,
        // while wall_momentum_x > 0 (wall absorbed rightward momentum).
        // Together they should approximately conserve the initial momentum.
        let p_ball = ball_mass * mc.world().balls[0].v.x;
        let p_wall = mc.world().sim.wall_momentum_x;

        assert!(
            p_wall > 0.0,
            "wall_momentum_x must be positive after rightward wall bounce, got {p_wall}"
        );
        // Momentum conservation: p_ball + p_wall ≈ initial momentum (ball_mass * 10.0)
        let initial = ball_mass * 10.0;
        let error = ((p_ball + p_wall) - initial).abs();
        assert!(
            error < 1e-3,
            "momentum not conserved: p_ball={p_ball:.6}, p_wall={p_wall:.6}, initial={initial:.6}, error={error:.2e}"
        );
    }

    #[test]
    fn test_time_scale_matches_argon_units() {
        let expected = LEN_SCALE_M * (_MASS_SCALE_KG / ENERGY_SCALE_J).sqrt();
        assert!(
            (TIME_SCALE / expected - 1.0).abs() < 1e-3,
            "TIME_SCALE {TIME_SCALE:e} vs derived {expected:e}"
        );
        assert!((TIME_SCALE_NS / (TIME_SCALE * 1e9) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_gravity_in_reduced_units() {
        // a* = g tau^2 / sigma ~ 1.3e-13 for 9.81 m/s^2 in argon units
        let expected = NORMAL_GRAVITY_MPS2 * TIME_SCALE * TIME_SCALE / LEN_SCALE_M;
        assert_eq!(NORMAL_GRAVITY, expected);
        assert!((NORMAL_GRAVITY / 1.3157e-13 - 1.0).abs() < 1e-3);
        assert!((HIGH_GRAVITY / NORMAL_GRAVITY - 5.0).abs() < 1e-5);
    }
}
