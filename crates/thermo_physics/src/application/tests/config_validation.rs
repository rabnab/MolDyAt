// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Property test of the config validation (#430): over random configs,
//! every accepted one builds a world that steps without NaN, and every
//! rejected one gives a message that names a key the generator broke.
//! Seeded, so a failure repeats; the message shows the seed.

use crate::application::MainContent;
use crate::application::sim_config::*;
use crate::application::units::UnitSystem;
use crate::error::AppError;
use crate::objects2d::ball_impl::SpeciesTable;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde_json::{Value, json};

const CASES: u64 = 1_000;
const STEPS: usize = 25;
const BALLS: usize = 30;

/// A species of the table: name, and whether it is a molecule.
type Entry = (String, bool);

/// The random choices of one config, and the keys it gave a value that must
/// be rejected.
struct Generator {
    rng: StdRng,
    /// Whether this config may break rules at all; half of them may not.
    breaking: bool,
    broken: Vec<String>,
}

impl Generator {
    fn chance(&mut self, one_in: u32) -> bool {
        self.rng.random_range(0..one_in) == 0
    }

    /// A value within `min..=max` (up to `hi`; log-uniform above 0), and, in
    /// a breaking config, one time in eight one outside it, recorded under
    /// `key`. Finite, so it survives the JSON the species and overrides
    /// travel in.
    fn value(&mut self, key: impl Into<String>, min: f32, max: f32, hi: f32) -> f32 {
        if self.breaking && self.chance(8) {
            self.broken.push(key.into());
            return if self.chance(2) || min <= 0.0 && self.chance(2) {
                max * 2.0 + 1.0
            } else if min > 0.0 {
                min * 0.5
            } else {
                -1.0
            };
        }
        if min > 0.0 {
            let (lo, hi) = (min * 1.01, hi * 0.99);
            self.rng.random_range(lo.ln()..=hi.ln()).exp().clamp(lo, hi)
        } else {
            self.rng.random_range(min..=hi * 0.99)
        }
    }

    /// `value` in literature units of `unit`.
    fn literature(&mut self, key: String, unit: f64, min: f32, max: f32, hi: f32) -> f64 {
        f64::from(self.value(key, min, max, hi)) * unit
    }

    /// `[[species]]` entries: atoms, some with an orientation.
    fn species(&mut self, count: usize) -> Vec<Value> {
        let units = UnitSystem::REDUCED;
        (0..count)
            .map(|i| {
                let key = format!("species[{i}]");
                let (sigma, epsilon, mass) = (
                    self.literature(
                        format!("{key}.sigma_angstrom"),
                        units.sigma_angstrom,
                        MIN_PAIR_SIGMA,
                        MAX_PAIR_SIGMA,
                        3.0,
                    ),
                    self.literature(
                        format!("{key}.epsilon_kelvin"),
                        units.epsilon_kelvin,
                        MIN_PAIR_EPSILON,
                        MAX_PAIR_EPSILON,
                        5.0,
                    ),
                    self.literature(
                        format!("{key}.mass_u"),
                        units.mass_u,
                        MIN_SPECIES_MASS,
                        MAX_SPECIES_MASS,
                        10.0,
                    ),
                );
                let mut entry = json!({
                    "name": format!("A{i}"),
                    "sigma_angstrom": sigma,
                    "epsilon_kelvin": epsilon,
                    "mass_u": mass,
                    "colour": [0.5, 0.5, 0.5],
                });
                if self.chance(3) {
                    entry["moment_of_inertia_u_angstrom2"] = json!(self.literature(
                        format!("{key}.moment_of_inertia_u_angstrom2"),
                        units.inertia_u_angstrom2(),
                        MIN_SPECIES_INERTIA,
                        MAX_SPECIES_INERTIA,
                        5.0,
                    ));
                }
                entry
            })
            .collect()
    }

    /// Up to two distinct overrides of `table`: pair overrides name two
    /// species, wall overrides one. An unknown name or a molecule breaks the
    /// config, as does an entry that sets neither `sigma` nor `epsilon`.
    fn overrides(&mut self, key: &str, table: &[Entry], pairs: bool) -> Vec<Value> {
        let mut used: Vec<Vec<String>> = Vec::new();
        let mut out = Vec::new();
        for i in 0..self.rng.random_range(0..=2) {
            let key = format!("{key}[{i}]");
            let mut broken = false;
            let mut names: Vec<String> = (0..if pairs { 2 } else { 1 })
                .map(|_| {
                    if self.breaking && self.chance(10) {
                        broken = true;
                        "Unknown".to_string()
                    } else {
                        let (name, molecule) = table[self.rng.random_range(0..table.len())].clone();
                        broken |= molecule && self.breaking;
                        if molecule && !self.breaking {
                            "Ar".to_string()
                        } else {
                            name
                        }
                    }
                })
                .collect();
            names.sort();
            if used.contains(&names) {
                continue;
            }
            if broken {
                self.broken.push(key.clone());
            }
            let mut entry = if pairs {
                json!({ "a": names[0], "b": names[1] })
            } else {
                json!({ "species": names[0] })
            };
            let (sigma, epsilon) = (self.chance(2), self.chance(2));
            let sigma = sigma || !(epsilon || self.breaking);
            if sigma {
                let min = if pairs {
                    MIN_PAIR_SIGMA
                } else {
                    MIN_WALL_PAIR_SIGMA
                };
                entry["sigma"] =
                    json!(self.value(format!("{key}.sigma"), min, MAX_PAIR_SIGMA, 3.0));
            }
            if epsilon {
                entry["epsilon"] = json!(self.value(
                    format!("{key}.epsilon"),
                    MIN_PAIR_EPSILON,
                    MAX_PAIR_EPSILON,
                    5.0
                ));
            }
            if !sigma && !epsilon {
                self.broken.push(key);
            }
            used.push(names);
            out.push(entry);
        }
        out
    }

    fn fields(&mut self) -> Vec<Value> {
        (0..self.rng.random_range(0..=2))
            .map(|i| {
                let stiffness = self.value(
                    format!("fields[{i}].stiffness"),
                    MIN_FIELD_STIFFNESS,
                    MAX_FIELD_STIFFNESS,
                    5.0,
                );
                json!({
                    "kind": "harmonic",
                    "center": [self.rng.random_range(-60.0..60.0), self.rng.random_range(-30.0..30.0)],
                    "stiffness": stiffness,
                })
            })
            .collect()
    }
}

fn random_config(g: &mut Generator) -> SimConfig {
    let builtin = SpeciesTable::builtin();
    let (table, species): (Vec<Entry>, Vec<Value>) = if g.chance(2) {
        let count = g.rng.random_range(1..=4);
        let species = g.species(count);
        (
            (0..count).map(|i| (format!("A{i}"), false)).collect(),
            species,
        )
    } else {
        let table = builtin
            .ids()
            .map(|id| {
                (
                    builtin.species(id).name.clone(),
                    builtin.sites(id).len() > 1,
                )
            })
            .collect();
        (table, Vec::new())
    };
    let mut value = serde_json::to_value(SimConfig::default()).unwrap();
    value["potential"]["pair_override"] =
        g.overrides("potential.pair_override", &table, true).into();
    value["walls"]["override"] = g.overrides("walls.override", &table, false).into();
    value["species"] = species.into();
    value["fields"] = g.fields().into();
    let mut config: SimConfig = serde_json::from_value(value).expect("a generated config parses");

    config.world.scale = g.value("world.scale", MIN_WORLD_SCALE, MAX_WORLD_SCALE, 3.0);
    let integration = &mut config.integration;
    integration.time_step = g.value(
        "integration.time_step",
        MIN_TIME_STEP,
        MAX_TIME_STEP,
        MAX_TIME_STEP,
    );
    integration.speed_limit_factor = g.value(
        "integration.speed_limit_factor",
        MIN_SPEED_LIMIT_FACTOR,
        MAX_SPEED_LIMIT_FACTOR,
        5.0,
    );
    let potential = &mut config.potential;
    potential.max_force = g.value(
        "potential.max_force",
        MIN_MAX_FORCE,
        MAX_MAX_FORCE,
        MAX_MAX_FORCE,
    );
    potential.core_stiffness = g.value(
        "potential.core_stiffness",
        MIN_CORE_STIFFNESS,
        MAX_CORE_STIFFNESS,
        MAX_CORE_STIFFNESS,
    );
    potential.cutoff_factor = g.value(
        "potential.cutoff_factor",
        MIN_CUTOFF_FACTOR,
        MAX_CUTOFF_FACTOR,
        MAX_CUTOFF_FACTOR,
    );
    let walls = &mut config.walls;
    walls.model = if g.chance(2) {
        WallModel::Soft
    } else {
        WallModel::Hard
    };
    walls.attractive = g.chance(2);
    walls.sigma = g.value("walls.sigma", MIN_WALL_SIGMA, MAX_WALL_SIGMA, 3.0);
    walls.epsilon = g.value("walls.epsilon", MIN_PAIR_EPSILON, MAX_PAIR_EPSILON, 5.0);
    walls.backstop = g.value(
        "walls.backstop",
        MIN_BACKSTOP_FACTOR,
        MAX_BACKSTOP_FACTOR,
        MAX_BACKSTOP_FACTOR,
    );
    config.neighbours.skin_factor = g.value(
        "neighbours.skin_factor",
        MIN_SKIN_FACTOR,
        MAX_SKIN_FACTOR,
        MAX_SKIN_FACTOR,
    );
    config.display.snapshot_interval_us = g.value(
        "display.snapshot_interval_us",
        MIN_SNAPSHOT_INTERVAL_US as f32,
        MAX_SNAPSHOT_INTERVAL_US as f32,
        1e5,
    ) as u64;
    config
}

#[test]
fn random_configs_are_accepted_and_run_or_rejected_naming_the_key() {
    let (mut accepted, mut rejected) = (0, 0);
    for seed in 0..CASES {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut g = Generator {
            breaking: rng.random_range(0..2) == 0,
            rng,
            broken: Vec::new(),
        };
        let config = random_config(&mut g);
        let context = format!("seed {seed}, broken {:?}: {config:?}", g.broken);
        match config.validate() {
            Err(AppError::CustomError(message)) => {
                rejected += 1;
                assert!(
                    g.broken.iter().any(|key| message.contains(key.as_str())),
                    "rejected without naming a broken key ({message}); {context}"
                );
            }
            Ok(()) => {
                accepted += 1;
                assert!(g.broken.is_empty(), "accepted a broken config; {context}");
                run(&config, seed, &context);
            }
        }
    }
    assert!(
        accepted >= CASES / 10 && rejected >= CASES / 10,
        "{accepted} accepted, {rejected} rejected"
    );
}

/// The world of an accepted config steps without NaN.
fn run(config: &SimConfig, seed: u64, context: &str) {
    let mut content = MainContent::with_config(config.world.rect(), config, Some(seed))
        .unwrap_or_else(|e| panic!("an accepted config builds a world ({e:?}); {context}"));
    let mixture: Vec<_> = content
        .world()
        .species()
        .iter()
        .map(|s| (s.name.clone(), 1))
        .collect();
    content.seed_random_mixture(BALLS, 1.0, &mixture).unwrap();
    for step in 0..STEPS {
        content
            .simulation_step()
            .unwrap_or_else(|e| panic!("step {step} failed ({e:?}); {context}"));
    }
    let world = content.world();
    let finite = world.balls().iter().all(|b| {
        let p = b.get_position();
        [p.x, p.y, b.v.x, b.v.y, b.omega]
            .iter()
            .all(|x| x.is_finite())
    });
    assert!(
        finite && world.sim().total_energy().is_finite(),
        "NaN after {STEPS} steps; {context}"
    );
}
