// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::application::core::StatsAccumulator;
use crate::application::thermo_field::LocalKineticStats;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::{Ball2D, SpeciesId};
use std::collections::VecDeque;

const PLT_SIZE: usize = 1000;
const PLT_STEP: usize = 100;

pub const NORMAL_GRAVITY_MPS2: f32 = 9.81;
pub const HIGH_GRAVITY_MPS2: f32 = NORMAL_GRAVITY_MPS2 * 5.0;

/// 9.81 m/s² in Ar units (`g·τ²/σ`, #405), as
/// `UnitSystem::AR.from_metres_per_second2` gives it; a constant because the
/// gravity presets are compared with `==` (`test_gravity_in_reduced_units`).
pub const NORMAL_GRAVITY: f32 = 1.339_646_4e-13;
pub(crate) const HIGH_GRAVITY: f32 = NORMAL_GRAVITY * 5.0;

/// Whether `gravity` is the 1 g preset: the nearest whole multiple of
/// `NORMAL_GRAVITY` is 1, as the gravity display shows it. Saves store the
/// reduced value, and older ones hold 1 g of their units (argon until #236:
/// 1.3157e-13, 2.3 % stronger), so an exact match would miss them.
pub(super) fn is_normal_gravity(gravity: f32) -> bool {
    (gravity / NORMAL_GRAVITY).round() == 1.0
}

/// What the status lines (`B` shows them, `D` switches to physical units)
/// need besides the snapshot's observable values: the UI formats them into
/// text (#341).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusInfo {
    /// Show physical units (Ar reference, #405) instead of reduced units.
    pub real_units: bool,
    /// The species whose time step limit holds `dt` below the set time
    /// step, if any (`MainContent::active_time_step`).
    pub dt_limited_by: Option<SpeciesId>,
}

/// How the last step computed the pair forces while the GPU force pass is
/// switched on (`MainContent::set_force_backend`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForcePath {
    Gpu,
    /// On the CPU kernel, and why the GPU could not take the step.
    Cpu(&'static str),
}

/// How often the safety nets that do not conserve energy stepped in (#422).
/// They hide instabilities (a time step too coarse for a species, balls
/// pushed into each other), so they are counted and shown instead of left
/// silent: in a healthy run they stay at or near zero.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SafetyNets {
    /// Balls slowed by the speed limit (`accel_with_speed_limit`).
    pub braked: u64,
    /// Pairs (and molecule site pairs) inside the force cap's sloped core
    /// (`CappedPotential`), each pair once.
    pub capped_pairs: u64,
    /// Hard contacts behind soft walls (the backstop); zero with hard walls,
    /// where the contact is the wall itself.
    pub backstop_hits: u64,
    /// Balls the overlap fallback pushed out of a line wall.
    pub overlap_corrections: u64,
}

impl std::ops::AddAssign for SafetyNets {
    fn add_assign(&mut self, other: Self) {
        self.braked += other.braked;
        self.capped_pairs += other.capped_pairs;
        self.backstop_hits += other.backstop_hits;
        self.overlap_corrections += other.overlap_corrections;
    }
}

/// Simulation statistics updated once per physics step.
///
/// Owns energy, temperature, velocity, and timing metrics that are
/// accumulated during each step and read by snapshot creation.
/// Does **not** own frame-render bookkeeping or neighbor-update
/// scheduling — those responsibilities belong to their respective
/// orchestrators.
pub struct SimStats {
    /// The last step's energies, summed in f64 over the balls (#421).
    energy_pot: f64,
    energy_kin: f64,
    pub(super) temperature: f32,
    pub(super) max_v: f32,
    max_id: usize,
    pub(super) v_com: Vec2D,
    /// Simulation time, summed in f64: in f32, `time += dt` stops advancing
    /// once `dt` is below half the spacing of f32 at `time` (after about
    /// 2^24 steps), which froze the energy plot's time axis.
    time: f64,
    step: usize,
    pause: bool,

    v_pot: VecDeque<f32>,
    v_kin: VecDeque<f32>,
    pub(super) max_neighbours: usize,

    /// Momentum the walls took since the start, summed in f64 (#421): in
    /// f32 a long run's total stops taking the small per-step amounts.
    pub wall_momentum_x: f64,
    pub wall_momentum_y: f64,

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
    /// Safety-net interventions of the last step (#422).
    pub safety_nets_step: SafetyNets,
    /// Safety-net interventions since the start (#422).
    pub safety_nets_total: SafetyNets,
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

    pub(crate) fn clear_energy(&mut self) {
        self.v_kin.clear();
        self.v_pot.clear();
    }

    /// Update statistics from accumulated physics data.
    /// Stores energy values in deques every PLT_STEP iterations for plotting.
    /// Maintains fixed deque size of PLT_SIZE by removing oldest values.
    ///
    /// # Arguments
    /// * `dt` - Time step
    /// * `stats_acumulator` - Accumulated physics statistics (kinetic, potential energy, velocities)
    pub(crate) fn update(&mut self, dt: f32, stats_acumulator: StatsAccumulator) {
        self.energy_kin = stats_acumulator.kin;
        self.energy_pot = stats_acumulator.pot;
        if self.step.is_multiple_of(PLT_STEP) {
            if self.v_pot.len() >= PLT_SIZE {
                self.v_pot.pop_front();
            }
            self.v_pot.push_back(stats_acumulator.pot as f32);
            if self.v_kin.len() >= PLT_SIZE {
                self.v_kin.pop_front();
            }
            self.v_kin.push_back(stats_acumulator.kin as f32);
        }

        self.max_v = stats_acumulator.max_v;
        self.max_id = stats_acumulator.max_id;
        self.advance_time(dt);
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
            max_neighbours = max_neighbours.max(cur.cold.hood.get_number_of_neighbors());
        }

        self.max_neighbours = max_neighbours;
        // Keep the global temperature definition consistent with thermostats and
        // thermo-field overlays by using the shared COM-subtracted accumulator.
        self.temperature = kinetic.temperature().unwrap_or(0.0);
    }

    pub fn get_kinetic_energy(&self) -> f32 {
        self.energy_kin as f32
    }

    pub fn get_potential_energy(&self) -> f32 {
        self.energy_pot as f32
    }

    /// Kinetic plus potential energy in f64 (#421): the sum and the drift
    /// from it keep the digits the f32 energies would cancel.
    pub fn total_energy(&self) -> f64 {
        self.energy_kin + self.energy_pot
    }

    pub fn get_time(&self) -> f32 {
        self.time as f32
    }

    fn advance_time(&mut self, dt: f32) {
        self.time += f64::from(dt);
    }

    #[cfg(test)]
    pub(crate) fn len_v_energy(&self) -> usize {
        self.v_kin.len().max(self.v_pot.len())
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
            safety_nets_step: SafetyNets::default(),
            safety_nets_total: SafetyNets::default(),
            wall_distance_checks: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::units::UnitSystem;
    use crate::objects2d::ball_impl::{Ball2D, SPECIES_AR};

    const UNITS: UnitSystem = UnitSystem::REDUCED;

    fn make_ball(id: usize, velocity: Vec2D) -> Ball2D {
        let mut ball = Ball2D::new(id, Vec2D::zero(), SPECIES_AR);
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

    /// Far into a run, every step still advances the time (an f32 sum
    /// froze at about 2^24 steps).
    #[test]
    fn test_time_keeps_advancing_in_long_runs() {
        let dt = 0.005_f32;
        let mut stats = SimStats {
            time: 200_000.0,
            ..Default::default()
        };
        let start = stats.get_time();
        for _ in 0..10_000 {
            stats.advance_time(dt);
        }
        let advanced = stats.get_time() - start;
        assert!((advanced - 50.0).abs() < 0.05, "advanced {advanced}");
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
        let p_wall = mc.world().sim.wall_momentum_x as f32;

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

    /// A long run's wall momentum still takes a single bounce (#421): with
    /// 2e7 already booked, f32 (spacing 2 there) would drop it.
    #[test]
    fn test_wall_momentum_keeps_adding_in_long_runs() {
        use crate::application::MainContent;
        use crate::objects2d::ball_impl::BallFactory;
        use crate::objects2d::{Rect, Vec2D};

        let world_rect = Rect::new(0.0, 0.0, 10.0, -10.0);
        let mut mc = MainContent::new(world_rect).expect("failed to create simulation");
        mc.world_mut().speed_limit_override = Some(f32::INFINITY);
        let factory = BallFactory::new();
        let ball = factory.create_massive_with_velocity(
            Vec2D::new(10.0 - factory.get_new_ball_radius() - 0.05, -5.0),
            Vec2D::new(10.0, 0.0),
        );
        let mass = f64::from(ball.mass);
        mc.world_mut().balls.push(ball);
        mc.world_mut().rebuild_cell_grid();
        const BOOKED: f64 = 2.0e7;
        mc.world_mut().sim.wall_momentum_x = BOOKED;

        let bound = mc.world().world_region();
        for _ in 0..10 {
            mc.update_sim(0.01, &bound);
        }
        // The wall took what the ball lost (it bounced: about 2 m v).
        let bounce = mass * (10.0 - f64::from(mc.world().balls[0].v.x));
        assert!(bounce > 10.0, "no bounce: {bounce}");
        let added = mc.world().sim.wall_momentum_x - BOOKED;
        assert!(
            (added - bounce).abs() < 1e-5 * bounce,
            "the wall took {added} of a {bounce} bounce"
        );
    }

    #[test]
    fn test_gravity_in_reduced_units() {
        // a* = g tau^2 / sigma ~ 1.34e-13 for 9.81 m/s^2 in Ar units
        let expected = UNITS.from_metres_per_second2(NORMAL_GRAVITY_MPS2 as f64);
        assert_eq!(NORMAL_GRAVITY, expected);
        let high = UNITS.from_metres_per_second2(HIGH_GRAVITY_MPS2 as f64);
        assert!((HIGH_GRAVITY / high - 1.0).abs() < 1e-6);
    }
}
