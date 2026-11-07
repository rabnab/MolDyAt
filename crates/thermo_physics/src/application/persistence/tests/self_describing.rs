// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Self-describing saves (#426): a file holds its species table and states
//! its units, so neither the built-in table nor the unit species decide how
//! it loads.

use super::fixture;
use crate::application::SimConfig;
use crate::application::persistence::WorldFile;
use crate::application::persistence::migrate::upgrade;
use crate::application::units::UnitSystem;
use crate::objects2d::ball_impl::SpeciesTable;
use serde_json::{Value, json};

fn load(file: &Value) -> WorldFile {
    WorldFile::from_json(&file.to_string()).unwrap()
}

/// The built-in table of v2 to v8, frozen for the v8 → v9 migration, in Ar
/// units: what files from before v9 that name built-in species ran with.
#[test]
fn files_before_v9_get_the_frozen_v8_table() {
    // (name, f32 bits of mass, sigma, epsilon, display_radius), in table
    // order (#405, format v6; #409, v7: TraPPE CH4, NH3, the former CH4 as
    // CH4_HCB; #349: MB water H2O; #406: N2_2S; #407: H2_2S; #408: CO2_3S).
    const PINNED: [(&str, u32, u32, u32, u32); 16] = [
        ("N2", 0x3f3384e6, 0x3f8b03af, 0x3f4b1ca2, 0x3f0b03af), // 0.7012466, 1.08605, 0.7934057
        ("XS", 0x3f3384e6, 0x3fb02b31, 0x3f4b1ca2, 0x3f302b31), // 0.7012466, 1.376318, 0.7934057
        ("S", 0x3f576c48, 0x3fd552b2, 0x3f4b1ca2, 0x3f5552b2),  // 0.841496, 1.666586, 0.7934057
        ("M", 0x3f9cd96e, 0x400fd0db, 0x3f4b1ca2, 0x3f8fd0db),  // 1.225385, 2.247123, 0.7934057
        ("L", 0x40099aea, 0x405a1fde, 0x3f4b1ca2, 0x3fda1fde),  // 2.15008, 3.408195, 0.7934057
        ("XL", 0x408a25a6, 0x40b75ef2, 0x3f4b1ca2, 0x40375ef2), // 4.317096, 5.73034, 0.7934057
        ("Ar", 0x3f800000, 0x3f800000, 0x3f800000, 0x3f000000), // 1, 1, 1
        ("Ne", 0x3f0151f3, 0x3f4eadfa, 0x3e98259c, 0x3eceadfa), // 0.5051567, 0.8073422, 0.2971619
        ("CH4", 0x3ecd9e22, 0x3f8c37a3, 0x3f9e2156, 0x3f0c37a3), // 0.4015971, 1.095448, 1.235392
        ("O2", 0x3f4d0f68, 0x3f86941d, 0x3f7b15cc, 0x3f06941d), // 0.8010163, 1.051395, 0.9808013
        ("NH3", 0x3eda47d3, 0x3f5a0845, 0x409520e8, 0x3eda0845), // 0.4263292, 0.8516887, 4.660267
        ("CH4_HCB", 0x3ecd9e22, 0x3f8f7ce1, 0x3f9e580a, 0x3f0f7ce1), // 0.4015971, 1.120999, 1.237062
        ("H2O", 0x3ee6e465, 0x3f135c1a, 0x3f93fadf, 0x3ed28392), // 0.4509613, 0.5756241, 1.156093
        ("N2_2S", 0x3f3385e2, 0x3fa5c79d, 0x00000000, 0x3ef8db89), // 0.7012616, 1.295154, 0
        ("H2_2S", 0x3d4eb506, 0x3f82114b, 0x00000000, 0x3ecc7fd0), // 0.05046561, 1.016153, 0
        ("CO2_3S", 0x3f8d031a, 0x3fc9de2b, 0x00000000, 0x3ee54f52), // 1.101657, 1.577093, 0
    ];
    let mut file = fixture("v8.json");
    assert!(file["config"].get("species").is_none());
    upgrade(&mut file).unwrap();
    let config = SimConfig {
        species: serde_json::from_value(file["config"]["species"].clone()).unwrap(),
        ..SimConfig::default()
    };
    let actual: Vec<_> = config
        .species_table()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s.name.clone(),
                s.mass.to_bits(),
                s.sigma.to_bits(),
                s.epsilon.to_bits(),
                s.display_radius.to_bits(),
            )
        })
        .collect();
    let pinned: Vec<_> = PINNED
        .iter()
        .map(|&(name, m, s, e, d)| (name.to_string(), m, s, e, d))
        .collect();
    assert_eq!(actual, pinned);
    assert_eq!(file["units"], json!(UnitSystem::AR));
}

/// A file written in other units holds the same physical state once loaded:
/// here the reference save, declared in N₂ units.
#[test]
fn file_in_other_units_is_converted_on_load() {
    let ar_file = fixture("v9.json");
    let mut n2_file = ar_file.clone();
    n2_file["units"] = json!(UnitSystem::N2);
    let (ar, n2) = (load(&ar_file), load(&n2_file));
    let s = UnitSystem::N2.scale_to(&UnitSystem::REDUCED);
    let close = |converted: f32, value: f32, factor: f64| {
        let expected = f64::from(value) * factor;
        assert!(
            (f64::from(converted) - expected).abs() <= 1e-5 * expected.abs().max(1e-3),
            "{converted} vs {expected}"
        );
    };
    assert_eq!(n2.units, UnitSystem::REDUCED);
    for (a, b) in ar.balls.iter().zip(&n2.balls) {
        close(b.pos.x, a.pos.x, s.length);
        close(b.v.y, a.v.y, s.speed());
        close(b.accel.x, a.accel.x, s.acceleration());
    }
    close(n2.world_rect.w, ar.world_rect.w, s.length);
    close(n2.gravity, ar.gravity, s.acceleration());
    let (pa, pb) = (&ar.config.potential, &n2.config.potential);
    close(pb.max_force, pa.max_force, s.energy / s.length);
    close(n2.config.walls.epsilon, ar.config.walls.epsilon, s.energy);
    for (a, b) in ar.thermostats().zip(n2.thermostats()) {
        close(b.target_temperature, a.target_temperature, s.energy);
    }
    assert!(ar.thermostats().count() > 0);
    // In literature units already; numerical settings keep their number.
    assert_eq!(n2.config.species, ar.config.species);
    assert_eq!(n2.time_step, ar.time_step);
}

#[test]
fn file_with_invalid_units_is_rejected() {
    let mut file = fixture("v9.json");
    file["units"]["mass_u"] = json!(-1.0);
    let error = WorldFile::from_json(&file.to_string()).unwrap_err();
    assert!(format!("{error:?}").contains("units"), "{error:?}");
}

/// The file's table decides, not the built-in one: a changed `N2` loads as
/// written.
#[test]
fn file_table_wins_over_the_builtin_table() {
    let mut file = fixture("v9.json");
    let species = file["config"]["species"].as_array_mut().unwrap();
    let n2 = species.iter_mut().find(|s| s["name"] == "N2").unwrap();
    n2["sigma_angstrom"] = json!(4.0);
    let config = load(&file).config;
    let table = config.species_table().unwrap();
    let id = table.by_name("N2").unwrap();
    assert_eq!(table.species(id).sigma, (4.0 / 3.405) as f32);
    assert_ne!(table.species(id), SpeciesTable::builtin().species(id));
}
