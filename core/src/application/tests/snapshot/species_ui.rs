// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Snapshot tests moved from `thermo_physics` `core/tests/species_ui.rs` (#308).

use crate::application::MainContent;
use crate::application::SnapshotPublisher;
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

#[test]
fn status_lists_the_new_ball_species_and_the_count_per_species() {
    let mut publisher = SnapshotPublisher::new();
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
    let snapshot_status = publisher.snapshot(&mc).status_text.expect("ball stats");
    assert!(
        snapshot_status.ends_with("\nNew: Ar\nN2: 3\nAr: 2"),
        "{snapshot_status}"
    );
    publisher.set_layers(&mut mc, crate::application::scene_layers::SceneLayers::BASE);
    let without_balls = publisher.snapshot(&mc).status_text.expect("ball stats");
    assert!(
        without_balls.ends_with("\nNew: Ar\nN2: 3\nAr: 2"),
        "{without_balls}"
    );
}
