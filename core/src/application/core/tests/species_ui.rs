// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Mixed-species UI (#238): selecting the species of new balls, placing
//! balls and source spheres of it, and the per-species status lines.

use crate::application::MainContent;
use crate::application::core::SimulationWorld;
use crate::application::persistence::WorldFile;
use crate::application::sim_config::SimConfig;
use crate::objects2d::{Rect, Vec2D};

fn world_rect() -> Rect {
    Rect::new(-20.0, 20.0, 40.0, -40.0)
}

/// A world whose species table is N2, Ar (from a `[[species]]` list).
fn n2_ar() -> MainContent {
    let config = SimConfig::from_toml(
        "[[species]]\nname = \"N2\"\nsigma_angstrom = 3.698\nepsilon_kelvin = 95.05\n\
         mass_u = 28.0134\ncolour = [0.0, 1.0, 1.0]\n\
         [[species]]\nname = \"Ar\"\nsigma_angstrom = 3.405\nepsilon_kelvin = 119.8\n\
         mass_u = 39.948\ncolour = [0.6, 0.45, 1.0]\n",
    )
    .unwrap();
    MainContent::with_config(world_rect(), &config, Some(1)).unwrap()
}

fn cursor_name(mc: &MainContent) -> String {
    let snapshot = mc.create_snapshot();
    snapshot
        .species
        .species(snapshot.cursor_species)
        .name
        .clone()
}

#[test]
fn next_and_previous_species_wrap_within_the_worlds_table() {
    let mut mc = n2_ar();
    assert_eq!(cursor_name(&mc), "N2");
    mc.next_species();
    assert_eq!(cursor_name(&mc), "Ar");
    mc.next_species();
    assert_eq!(cursor_name(&mc), "N2", "wraps after the last species");
    mc.previous_species();
    assert_eq!(cursor_name(&mc), "Ar", "wraps before the first species");
}

#[test]
fn placed_balls_get_the_selected_species() {
    let mut mc = n2_ar();
    mc.next_species();
    mc.add_candidate_at_pos(Vec2D::new(0.0, 0.0)).unwrap();
    let world = mc.world();
    let ar = world.species().by_name("Ar").unwrap();
    let ball = &world.balls[0];
    assert_eq!(ball.species(), ar);
    assert_eq!(ball.get_mass(), world.species().species(ar).mass);
}

#[test]
fn random_balls_get_the_selected_species() {
    let mut mc = n2_ar();
    mc.next_species();
    mc.add_random_candidates(10);
    let ar = mc.world().species().by_name("Ar").unwrap();
    assert!(!mc.world().balls.is_empty());
    assert!(mc.world().balls.iter().all(|b| b.species() == ar));
}

/// A new source sphere emits the selected species; cycling it later does
/// not change the species of new balls.
#[test]
fn new_source_spheres_emit_the_selected_species() {
    let mut mc = n2_ar();
    mc.add_source_sphere(Vec2D::new(-5.0, 0.0));
    mc.next_species();
    mc.add_source_sphere(Vec2D::new(5.0, 0.0));
    let names = |mc: &MainContent| -> Vec<String> {
        let world = mc.world();
        world
            .source_spheres
            .spheres()
            .iter()
            .map(|s| world.species().species(s.ball_species).name.clone())
            .collect()
    };
    assert_eq!(names(&mc), ["N2", "Ar"]);
    mc.cycle_source_sphere_species(0);
    assert_eq!(names(&mc), ["Ar", "Ar"]);
    assert_eq!(cursor_name(&mc), "Ar");
}

/// A source sphere's species survives a save and load (built-in table).
#[test]
fn source_sphere_species_survives_save_and_load() {
    let mut mc = MainContent::new(world_rect()).unwrap();
    while cursor_name(&mc) != "CH4" {
        mc.next_species();
    }
    mc.add_source_sphere(Vec2D::new(0.0, 0.0));
    let json = WorldFile::capture(&mc).to_json().unwrap();
    assert!(json.contains("\"ball_species\": \"CH4\""), "{json}");
    let world = SimulationWorld::try_from(&WorldFile::from_json(&json).unwrap()).unwrap();
    let sphere = &world.source_spheres.spheres()[0];
    assert_eq!(world.species().species(sphere.ball_species).name, "CH4");
}

#[test]
fn status_lists_the_new_ball_species_and_the_count_per_species() {
    let mut mc = n2_ar();
    assert_eq!(mc.species_status(), "New: N2");
    for x in [-6.0, -3.0, 0.0] {
        mc.add_candidate_at_pos(Vec2D::new(x, 0.0)).unwrap();
    }
    mc.next_species();
    for x in [3.0, 6.0] {
        mc.add_candidate_at_pos(Vec2D::new(x, 0.0)).unwrap();
    }
    assert_eq!(mc.species_status(), "New: Ar\nN2: 3\nAr: 2");
    let status = mc.status_text(0, 0.01).expect("ball stats are shown");
    assert!(status.ends_with("\nNew: Ar\nN2: 3\nAr: 2"), "{status}");
    // The snapshot counts in its own pass over the balls (BALLS layer) and
    // without it; both give the same lines.
    let snapshot_status = mc.create_snapshot().status_text.expect("ball stats");
    assert!(
        snapshot_status.ends_with("\nNew: Ar\nN2: 3\nAr: 2"),
        "{snapshot_status}"
    );
    mc.set_snapshot_layers(crate::application::scene_layers::SceneLayers::BASE);
    let without_balls = mc.create_snapshot().status_text.expect("ball stats");
    assert!(
        without_balls.ends_with("\nNew: Ar\nN2: 3\nAr: 2"),
        "{without_balls}"
    );
}
