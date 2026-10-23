// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::wgpu::pipelines::source_label_pipeline::label_texts;
use thermo_physics::objects2d::ball_impl::{SpeciesId, SpeciesTable};
use thermo_physics::objects2d::{SimSourceSphere, Vec2D};

/// A sphere is labelled with the name of the species it emits, from the
/// world's table (#238).
#[test]
fn label_names_the_species_the_sphere_emits() {
    let table = SpeciesTable::builtin();
    let mut sphere = SimSourceSphere::new(0, Vec2D::zero());
    sphere.ball_species = table.by_name("Ar").unwrap();
    let (species, speed, rate) = label_texts(&sphere, table);
    assert_eq!(species, "Ar");
    assert!(speed.starts_with("S:") && rate.starts_with("R:"));

    sphere.ball_species = SpeciesId::new(table.len() as u16);
    assert_eq!(label_texts(&sphere, table).0, "?", "unknown species");
}
