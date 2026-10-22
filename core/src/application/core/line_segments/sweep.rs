// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::Obstacle;
use crate::application::core::SimulationWorld;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::Ball2D;
use crate::objects2d::ball_impl::wall_contact::{WallContact, resolve_normal_position};

impl SimulationWorld {
    /// Resolve a ball's contact with the walls during its position update,
    /// the same way box walls are resolved (see `wall_contact`): the contact
    /// time and point come from the step's path, the ball is reflected there
    /// along the surface normal, and the velocity half of the step uses the
    /// returned contact. Forces are then computed at the resolved position.
    ///
    /// `start` is the ball's position before this step's drift; the ball's
    /// current position is the free (unreflected) end of the drift. `contact`
    /// is the ball's wall contact distance (`BallPhysics::wall_contact_distance`). A ball
    /// flagged `leaving_wall`, or one that starts the step inside a wall, is
    /// left to the overlap pass (`resolve_wall_overlaps_with`). Distance checks are added to
    /// `checks` (see `SimStats::wall_distance_checks`).
    pub(crate) fn sweep_wall_contact(
        obstacles: &[Obstacle],
        ball: &mut Ball2D,
        contact: f32,
        start: Vec2D,
        dt: f32,
        restitution: f32,
        checks: &mut u64,
    ) -> Option<WallContact> {
        if ball.leaving_wall {
            return None;
        }
        let end = *ball.get_position();
        let mut local_checks = 0;
        let mut first: Option<(f32, &Obstacle)> = None;
        for obstacle in obstacles {
            if let Some(t) = obstacle.first_touch(&start, &end, contact, &mut local_checks)
                && first.is_none_or(|(best, _)| t < best)
            {
                first = Some((t, obstacle));
            }
        }
        *checks += local_checks;
        let (touch_fraction, obstacle) = first?;

        // Flat wall through the touching point, normal to the surface there.
        let touch = start.add_scaled(&start.dist(&end), touch_fraction);
        let normal = obstacle.outward_normal(&touch, &start);
        let resolved = resolve_normal_position(
            touch.dist(&start).dot(&normal),
            ball.v.dot(&normal),
            ball.accel.dot(&normal),
            dt,
            restitution,
        )?;
        let free_distance = touch.dist(&end).dot(&normal);
        ball.set_position_and_update_tail(
            end.add_scaled(&normal, resolved.distance - free_distance),
        );

        Some(WallContact {
            normal,
            contact_fraction: resolved.contact_fraction,
            free_fraction: resolved.free_fraction,
            counts_for_momentum: !obstacle.ghost,
        })
    }
}
