use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use ggez::GameResult;
use ggez::graphics::{Color, DrawMode, MeshBuilder, Rect};
use ggez::input::keyboard::KeyCode;
use ggez::mint::Point2;
use std::cell::RefCell;
use std::collections::vec_deque::Iter;
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

const PLT_SIZE: usize = 1000;
const PLT_STEP: usize = 100;

const _U_KG: f32 = 1.66E-27;
const _MASS_SCALE_KG: f32 = 39.948 * _U_KG; //6.6E-26kg mass of Ar
const ENERGY_SCALE_J: f32 = 122. * KBOLTZMANN_J_P_K; //energy scale for Ar at 122K
const LEN_SCALE_M: f32 = 3.33729598E-9 / 6.; // about 0.5nm reason -- f32::cbrt(6.631E-26/1.784)/6; //density 1.784 kg/m³
const TIME_SCALE: f32 = 3.48235E-11; //=f32::sqrt(MASS_SCALE_KG/ ENERGY_SCALE_J)*LEN_SCALE_M; //0.35ns
const TIME_SCALE_NS: f32 = 3.48235E-2; //=f32::sqrt(MASS_SCALE_KG/ ENERGY_SCALE_J)*LEN_SCALE_M; //0.35ns
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
    speedup: u32,
    draw_cursor: bool,
    frames_since_draw: u32,
    v_pot: VecDeque<f32>,
    v_kin: VecDeque<f32>,
    max_neighbours: usize,
    update_hood_cnt: usize,
    pressed_key: HashSet<KeyCode>,
    min_dist_initial: f32,
}

impl SimStats {
    pub(crate) fn toggle_pause(&mut self) {
        self.set_pause(!self.is_paused())
    }

    pub fn set_pause(&mut self, pause: bool) {
        self.pause = pause;
    }

    pub fn is_paused(&self) -> bool {
        self.pause
    }

    pub fn set_draw_cursor(&mut self, draw: bool) {
        self.draw_cursor = draw;
    }

    pub(crate) fn reset_frame_since_draw(&mut self) {
        self.frames_since_draw = 0;
    }
    pub(crate) fn inc_frame_since_draw(&mut self) {
        self.frames_since_draw += 1
    }

    pub fn draw_cursor(&self) -> bool {
        self.draw_cursor
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

    pub(crate) fn register_key_pressed(&mut self, key: KeyCode) {
        self.pressed_key.insert(key);
    }

    pub(crate) fn register_key_released(&mut self, key: KeyCode) {
        self.pressed_key.remove(&key);
    }

    pub(crate) fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.pressed_key.contains(&key)
    }

    pub(crate) fn set_update_hood_cnt(&mut self, update_hood_cnt: usize) {
        self.update_hood_cnt = update_hood_cnt;
    }

    pub fn add_vkin_to_line<F>(
        &self,
        mesh_builder: &mut MeshBuilder,
        to_screen_coords: F,
    ) -> GameResult
    where
        F: Fn(usize, f32) -> Point2<f32>,
    {
        Self::add_data_to_line(mesh_builder, to_screen_coords, Color::RED, &self.v_kin)
    }

    pub fn add_vpot_to_line<F>(
        &self,
        mesh_builder: &mut MeshBuilder,
        to_screen_coords: F,
    ) -> GameResult
    where
        F: Fn(usize, f32) -> Point2<f32>,
    {
        Self::add_data_to_line(mesh_builder, to_screen_coords, Color::BLUE, &self.v_pot)
    }

    pub fn add_vtot_to_line<F>(
        &self,
        mesh_builder: &mut MeshBuilder,
        to_screen_coords: F,
    ) -> GameResult
    where
        F: Fn(usize, f32) -> Point2<f32>,
    {
        let data = self
            .v_kin
            .iter()
            .zip(self.v_pot.iter())
            .map(|(k, p)| -> f32 { k + p })
            .collect();
        Self::add_data_to_line(mesh_builder, to_screen_coords, Color::YELLOW, &data)
    }

    fn add_data_to_line<F>(
        mesh_builder: &mut MeshBuilder,
        to_screen_coords: F,
        color: Color,
        data: &VecDeque<f32>,
    ) -> GameResult
    where
        F: Fn(usize, f32) -> Point2<f32>,
    {
        let mut points: Vec<Point2<f32>> = Vec::with_capacity(data.len());
        for (i, &value) in data.iter().enumerate() {
            points.push(to_screen_coords(i, value));
        }
        mesh_builder.polyline(DrawMode::stroke(2.0), &points, color)?;
        Ok(())
    }

    pub(crate) fn get_speedup(&self) -> u32 {
        self.speedup
    }
    pub(crate) fn increase_speedup(&mut self) {
        self.speedup = self.speedup + 1;
    }

    pub(crate) fn decrease_speedup(&mut self) {
        self.speedup = if self.speedup > 1 {
            self.speedup - 1
        } else {
            1
        };
    }

    pub fn reset_min_dist(&mut self, bound_rect: &Rect) {
        self.min_dist_initial = bound_rect.w + bound_rect.h;
        self.min_dist = 0.;
    }

    pub fn update_min_dist(&mut self, min_dist: f32) {
        if self.min_dist_initial > min_dist && min_dist > 0. {
            self.min_dist_initial = min_dist;
            self.min_dist = min_dist;
        }
    }

    pub fn get_status_text_real(&self, fps: u32, num_balls: usize) -> String {
        format!(
            "fps {} time: {:.1}[ns] ({})\n#Balls: {}\nmaxNeig: {}\n\
                TotalE: {:.3E}[J]\nTemp: {:.2}[K]\nmaxV: {:.2}[m/s]\nvCOM: {}[m/s]",
            fps,
            self.time * TIME_SCALE_NS,
            self.speedup,
            num_balls,
            //self.sim.frames_since_draw,
            self.max_neighbours,
            (self.energy_pot + self.energy_kin) * ENERGY_SCALE_J,
            self.temperature * TEMP_SCALE,
            self.max_v * LEN_SCALE_M / TIME_SCALE,
            //self.sim.min_dist * LEN_SCALE_M,
            self.v_com.scaled(LEN_SCALE_M / TIME_SCALE)
        )
    }

    pub(crate) fn get_status_text(&self, fps: u32, num_balls: usize, hood_cnt: usize) -> String {
        format!(
            "fps {} time: {:.1} ({})\n#Balls: {} #hoodCnt: {}/{}\nmaxNeig: {}\nTotalE: {:.3E}\nTemp: {:.3}\nmaxV: {:.2}\nvCOM: {}",
            fps,
            self.time,
            self.speedup,
            num_balls,
            self.update_hood_cnt,
            hood_cnt,
            self.max_neighbours,
            self.energy_pot + self.energy_kin,
            self.temperature,
            self.max_v,
            //self.sim.min_dist,
            self.v_com,
        )
    }

    pub fn update(&mut self, kin: f32, pot: f32, max_id: usize, max_v: f32, dt: f32, v_com: Vec2D) {
        self.energy_kin = kin;
        self.energy_pot = pot;
        if (self.step % PLT_STEP) == 0 {
            if self.v_pot.len() >= PLT_SIZE {
                self.v_pot.pop_front();
            }
            self.v_pot.push_back(pot);
            if self.v_kin.len() >= PLT_SIZE {
                self.v_kin.pop_front();
            }
            self.v_kin.push_back(kin);
        }

        self.max_v = max_v;
        self.max_id = max_id;
        self.time += dt;
        self.step += 1;
        self.v_com = v_com;
    }

    pub(crate) fn calculate_temperature(&mut self, v_balls_rc: &Vec<Rc<RefCell<Ball2D>>>) {
        let mut sum_rel_kin_doubled: f32 = 0.;
        let mut max_neighbours: usize = 0;

        for ball_rc in v_balls_rc {
            let cur = ball_rc.borrow();
            let v_abs = &cur.v;
            let v_rel_x = v_abs.x - self.v_com.x;
            let v_rel_y = v_abs.y - self.v_com.y;
            sum_rel_kin_doubled += cur.mass * (v_rel_x * v_rel_x + v_rel_y * v_rel_y);

            let cur_num_neighbour = cur.hood.get_number_of_neighbors();
            max_neighbours = max_neighbours.max(cur_num_neighbour);
        }
        self.max_neighbours = max_neighbours;
        //self.sim.temperature = 0.5 / (/*KB*/ 1. * (3. * self.v_points.len() as f32 - 3.)) * sum_rel_kin;
        self.temperature = sum_rel_kin_doubled / 3. / (v_balls_rc.len() as f32) /* /KB == /1*/
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
            speedup: 1,
            draw_cursor: false,
            frames_since_draw: 0,
            v_pot: VecDeque::with_capacity(100),
            v_kin: VecDeque::with_capacity(100),
            max_neighbours: 0,
            update_hood_cnt: 0,
            pressed_key: HashSet::new(),
            min_dist_initial: f32::MAX,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    //use float_cmp::approx_eq;

    impl SimStats {
        pub(crate) fn get_kinetic_energy(&self) -> f32 {
            self.energy_kin
        }
    }

    #[test]
    fn test_defaults() {
        let default_sim_stats = SimStats::default();
        assert_eq!(default_sim_stats.energy_pot, 0.);
        assert_eq!(default_sim_stats.energy_kin, 0.);
    }
}
