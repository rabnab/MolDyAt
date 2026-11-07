// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The backstop behind soft walls (#424): the only hard mechanism of the
//! soft step, a counted safety net for balls the wall force cannot hold.

use super::new_main_content_seeded;
use crate::application::MainContent;
use crate::objects2d::ball_impl::molecule::axis;
use crate::objects2d::ball_impl::{Ball2D, SPECIES_AR, SpeciesId, SpeciesTable};
use crate::objects2d::{Rect, Vec2D};

fn world() -> MainContent {
    let mut mc = new_main_content_seeded(Rect::new(-30.0, 30.0, 60.0, -60.0), 3).unwrap();
    mc.world_mut().speed_limit_override = Some(f32::INFINITY);
    mc
}

fn add(mc: &mut MainContent, pos: Vec2D, v: Vec2D, theta: f32, species: SpeciesId) {
    let table = mc.world().species().clone();
    let mut ball = Ball2D::of_species(1, pos, species, table.species(species));
    ball.v = v;
    ball.theta = theta;
    mc.world_mut().add_ball_and_register(ball);
    mc.collect_neighbours_by_cell(mc.active_time_step());
}

fn backstop_hits(mc: &MainContent) -> u64 {
    mc.world().sim.safety_nets_total.backstop_hits
}

/// A ball fast enough to cross a thin wall completely in one step is pushed
/// back to the side it came from, the hit is counted, the wall takes the
/// momentum and the ball keeps its speed.
#[test]
fn a_ball_crossing_a_thin_wall_in_one_step_is_sent_back() {
    let mut mc = world();
    mc.add_line_segment(Vec2D::new(-20.0, 0.0), Vec2D::new(20.0, 0.0));
    // 2.5 per step: from 1.0 above the centre line to 1.5 below it, out of
    // the backstop's reach on the far side.
    let speed = 2.5 / mc.active_time_step();
    add(
        &mut mc,
        Vec2D::new(0.0, 1.0),
        Vec2D::new(0.0, -speed),
        0.0,
        SPECIES_AR,
    );
    let wall0 = mc.world().sim.wall_momentum_y;
    mc.simulation_step().unwrap();
    let ball = &mc.world().balls[0];
    assert!(
        ball.get_position().y > 0.5,
        "went through: {}",
        ball.get_position()
    );
    assert!(ball.v.y > 0.0, "not reflected: {}", ball.v);
    assert!(
        (ball.v.norm() - speed).abs() < 0.02 * speed,
        "speed {} vs {speed}",
        ball.v.norm()
    );
    assert_eq!(backstop_hits(&mc), 1);
    assert_eq!(mc.world().sim.safety_nets_step.overlap_corrections, 0);
    let wall = mc.world().sim.wall_momentum_y - wall0;
    assert!(wall < 0.0, "the wall takes the impulse: {wall}");
}

/// The box sides hold a ball the soft force cannot stop, from any side.
#[test]
fn the_box_backstop_holds_a_ball_fired_at_each_side() {
    for direction in [
        Vec2D::new(1.0, 0.0),
        Vec2D::new(-1.0, 0.0),
        Vec2D::new(0.0, 1.0),
        Vec2D::new(0.0, -1.0),
    ] {
        let mut mc = world();
        let speed = 1.5 / mc.active_time_step();
        add(
            &mut mc,
            Vec2D::zero(),
            direction.scaled(speed),
            0.0,
            SPECIES_AR,
        );
        for _ in 0..200 {
            mc.simulation_step().unwrap();
            let p = *mc.world().balls[0].get_position();
            let region = mc.world().world_region();
            let (left, top) = region.get_tl();
            let (right, bottom) = region.get_br();
            assert!(
                p.x > left && p.x < right && p.y > bottom && p.y < top,
                "{direction}: left the box at {p}"
            );
        }
        assert!(backstop_hits(&mc) > 0, "{direction}: backstop never hit");
    }
}

/// A molecule fired end-on at the floor is stopped at its site: every site
/// stays above the floor, and the impulse at the end site turns it.
#[test]
fn the_backstop_acts_on_the_sites_of_a_molecule() {
    let mut mc = world();
    let co2 = SpeciesTable::builtin().by_name("CO2_3S").unwrap();
    let speed = 1.0 / mc.active_time_step();
    let theta = -1.0; // tilted, so the end site's impulse has a lever
    add(
        &mut mc,
        Vec2D::new(0.0, -55.0),
        Vec2D::new(0.0, -speed),
        theta,
        co2,
    );
    let half_length = mc.world().species().species(co2).sites()[2].offset.x;
    let floor = {
        let region = mc.world().world_region();
        region.get_tl().1.min(region.get_br().1)
    };
    for _ in 0..50 {
        mc.simulation_step().unwrap();
        let ball = &mc.world().balls[0];
        let lowest = ball.get_position().y - (axis(ball.theta).y * half_length).abs();
        assert!(lowest > floor, "a site went through the floor: {lowest}");
    }
    assert!(backstop_hits(&mc) > 0, "backstop never hit");
    assert!(
        mc.world().balls[0].omega != 0.0,
        "the site impulse turns it"
    );
}

/// Behind soft walls, a thermal gas never needs the backstop.
#[test]
fn a_thermal_gas_never_reaches_the_backstop() {
    let mut mc = new_main_content_seeded(Rect::new(-30.0, 30.0, 60.0, -60.0), 5).unwrap();
    mc.add_line_segment(Vec2D::new(-20.0, 0.0), Vec2D::new(20.0, 0.0));
    mc.seed_random_balls(400);
    for _ in 0..1_000 {
        mc.simulation_step().unwrap();
    }
    let nets = mc.world().sim.safety_nets_total;
    assert_eq!(nets.backstop_hits, 0);
    assert_eq!(nets.overlap_corrections, 0);
}
