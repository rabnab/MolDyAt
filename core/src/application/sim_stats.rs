use crate::application::core::StatsAccumulator;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::{Rect, Vec2D};
use std::collections::VecDeque;
use std::collections::vec_deque::Iter;

const PLT_SIZE: usize = 1000;
const PLT_STEP: usize = 100;

const _U_KG: f32 = 1.66E-27;
const _MASS_SCALE_KG: f32 = 39.948 * _U_KG; //6.6E-26kg mass of Ar
const ENERGY_SCALE_J: f32 = 122. * KBOLTZMANN_J_P_K; //energy scale for Ar at 122K
const LEN_SCALE_M: f32 = 3.405E-10; // σ for Argon (standard LJ parameter)
const TIME_SCALE: f32 = 2.156E-12; // sqrt(MASS_SCALE * LEN_SCALE^2 / ENERGY_SCALE)
const TIME_SCALE_NS: f32 = 2.156E-3; // TIME_SCALE * 1E9
const KBOLTZMANN_J_P_K: f32 = 1.380649E-23;
const TEMP_SCALE: f32 = 122.; //=ENERGY_SCALE_J / KBOLTZMANN_J_P_K; //exact 122K

pub struct SimStats {
    energy_pot: f32,
    energy_kin: f32,
    temperature: f32,
    max_v: f32,
    max_id: usize,
    min_dist: f32,
    v_com: Vec2D,
    time: f32,
    step: usize,
    pause: bool,

    frames_since_draw: u32,
    v_pot: VecDeque<f32>,
    v_kin: VecDeque<f32>,
    max_neighbours: usize,
    update_hood_cnt: usize,

    min_dist_initial: f32,

    // Wall momentum tracking
    pub wall_momentum_x: f32,
    pub wall_momentum_y: f32,
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

    pub(crate) fn reset_frame_since_draw(&mut self) {
        self.frames_since_draw = 0;
    }
    pub(crate) fn inc_frame_since_draw(&mut self) {
        self.frames_since_draw += 1
    }

    pub fn iter_vkin(&self) -> Iter<'_, f32> {
        self.v_kin.iter()
    }

    pub fn iter_vpot(&self) -> Iter<'_, f32> {
        self.v_pot.iter()
    }

    pub fn len_v_energy(&self) -> usize {
        self.v_kin.len().max(self.v_pot.len())
    }

    pub fn is_max_id(&self, id: usize) -> bool {
        self.max_id == id
    }

    pub fn clear_energy(&mut self) {
        self.v_kin.clear();
        self.v_pot.clear();
    }

    /// Reset minimum distance tracking to boundary dimensions.
    /// Initial value is set to sum of boundary width and height.
    ///
    /// # Arguments
    /// * `bound_rect` - Simulation boundary rectangle
    pub fn reset_min_dist(&mut self, bound_rect: &Rect) {
        self.min_dist_initial = bound_rect.w + bound_rect.h;
        self.min_dist = 0.;
    }

    /// Update minimum distance if new value is smaller than current minimum.
    /// Only updates if distance is positive and less than previously recorded minimum.
    ///
    /// # Arguments
    /// * `min_dist` - New minimum distance candidate
    pub fn update_min_dist(&mut self, min_dist: f32) {
        if self.min_dist_initial > min_dist && min_dist > 0. {
            self.min_dist_initial = min_dist;
            self.min_dist = min_dist;
        }
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
            "fps {} time: {:.1} (dt:{:.4})\n#Balls: {} #hoodCnt: {}/{}\nmaxNeig: {}\n\
                TotalE: {:.3E}\nEkin: {:.3E}\nEpot: {:.3E}\nTemp: {:.3}\nmaxV: {:.2}\nvCOM: {}",
            fps,
            self.time,
            dt,
            num_balls,
            self.update_hood_cnt,
            hood_cnt,
            self.max_neighbours,
            self.energy_pot + self.energy_kin,
            self.energy_kin,
            self.energy_pot,
            self.temperature,
            self.max_v,
            //self.sim.min_dist,
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
        let mut sum_rel_kin_doubled: f32 = 0.;
        let mut max_neighbours: usize = 0;
        let vcom_x = self.v_com.x;
        let vcom_y = self.v_com.y;

        for cur in v_balls {
            let v_abs = &cur.v;
            let v_rel_x = v_abs.x - vcom_x;
            let v_rel_y = v_abs.y - vcom_y;
            sum_rel_kin_doubled += cur.mass * (v_rel_x * v_rel_x + v_rel_y * v_rel_y);
            max_neighbours = max_neighbours.max(cur.hood.get_number_of_neighbors());
        }
        self.max_neighbours = max_neighbours;
        self.temperature = sum_rel_kin_doubled / 3.0 / (v_balls.len() as f32);
    }

    // alternative faster temperature calculation that avoids Rc<RefCell<...>>
    // Use only if callers can provide a slice of Ball2D (borrowed) to avoid runtime borrow checks.
    #[inline]
    pub(crate) fn calculate_temperature_fast(&mut self, v_balls: &[&Ball2D]) {
        let mut sum_rel_kin_doubled: f32 = 0.;
        let mut max_neighbours: usize = 0;
        let vcom_x = self.v_com.x;
        let vcom_y = self.v_com.y;

        for cur in v_balls {
            let v_abs = &cur.v;
            let v_rel_x = v_abs.x - vcom_x;
            let v_rel_y = v_abs.y - vcom_y;
            sum_rel_kin_doubled += cur.mass * (v_rel_x * v_rel_x + v_rel_y * v_rel_y);
            max_neighbours = max_neighbours.max(cur.hood.get_number_of_neighbors());
        }
        self.max_neighbours = max_neighbours;
        // For 2D: (1/2) * m * v² per degree of freedom
        // Total kinetic energy in COM application = (1/2) * sum(m * v_rel²)
        // Temperature = (2 * KE) / (2 * N * k_B) = KE / (N * k_B)
        //self.temperature = sum_rel_kin_doubled / 3.0 / (v_balls.len() as f32);
        self.temperature = sum_rel_kin_doubled / 2.0 / (v_balls.len() as f32);
    }

    pub(crate) fn get_update_hood_cnt(&self) -> usize {
        self.update_hood_cnt
    }

    pub(crate) fn update_hood_cnt(&mut self, update_hood_at_frame_start: usize) {
        self.update_hood_cnt = self.get_update_hood_cnt() - update_hood_at_frame_start;
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
            min_dist: 0.,
            v_com: Vec2D::new(0., 0.),
            time: 0.,
            step: 0,
            pause: false,
            frames_since_draw: 0,
            v_pot: VecDeque::with_capacity(100),
            v_kin: VecDeque::with_capacity(100),
            max_neighbours: 0,
            update_hood_cnt: 0,
            min_dist_initial: f32::MAX,
            wall_momentum_x: 0.,
            wall_momentum_y: 0.,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let mut stats = SimStats::default();
        stats.energy_kin = 10.0;
        stats.energy_pot = 20.0;
        stats.time = 5.0;

        assert_eq!(stats.get_kinetic_energy(), 10.0);
        assert_eq!(stats.get_potential_energy(), 20.0);
        assert_eq!(stats.get_time(), 5.0);

        // Verify getters don't modify state
        assert_eq!(stats.get_kinetic_energy(), 10.0);
        assert_eq!(stats.get_potential_energy(), 20.0);
    }
}
