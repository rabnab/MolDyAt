---
applyTo: "crates/thermo_physics/src/objects2d/ball_impl/wall_law.rs,crates/thermo_physics/src/objects2d/ball_impl/wall_contact.rs,crates/thermo_physics/src/application/core/wall_forces.rs,crates/thermo_physics/src/application/core/line_segments/**,crates/thermo_physics/src/application/core/valves.rs"
---

# Wall rules

Adds to `physics.instructions.md`, which applies as well. Line walls and valves as operators: the `operator-framework` skill.

- Soft walls (`[walls]`, default `model = "soft"`) act only through their force (`wall_forces::soft_wall_force`): one `CappedPotential<LennardJones>` per site in the wall distance `z` (`SoftWalls`, Lorentz–Berthelot with the wall pseudo-species); every box side by its signed distance, of each chain only the nearest line wall (no energy bumps at joints), line walls scaled by the ball's ramp. Energy goes into `ForceResult::wall_potential` (code summing the potential itself must add it), the force minus the ghost wall's into `wall_force`, booked as wall momentum `−F·dt`.
- Ramp: a wall that appears within `wall_release_distance` of a ball flags it `leaving_wall` with `BallCold::wall_ramp` at 0, growing to 1 over `WALL_RAMP_TIME`. Ramp balls in; never exempt them.
- Hard contact at `WallSettings::contact_factor · σ` of each site: behind soft walls the counted backstop, with hard walls (pre-#280 saves; tests: `use_hard_walls()`) the wall. A single site reflects at the exact contact time on its path (`wall_contact`, `sweep_wall_contact`); species with more than one site get `Ball2D::wall_hit` per site after the drift (`molecule_backstop`). A `σ_ii` pair override sets a single site's wall `σ`. Never place a ball at the contact at the end of a step: the core's force makes that an energy error. The overlap fallback is for hard walls only.
