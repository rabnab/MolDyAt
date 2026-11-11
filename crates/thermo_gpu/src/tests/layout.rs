// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The kernel validates, and the WGSL structs of `layout.wgsl` match the
//! Rust types they mirror: the same size (so the same array stride) and
//! field offsets.

use crate::{ALL_PAIRS, Uniforms, shader_source};
use std::mem::offset_of;
use thermo_physics::objects2d::ball_impl::{KernelPoint, PairForces, PairParams};
use wgpu::naga;

fn module(kernel: &str) -> naga::Module {
    let source = shader_source(kernel);
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    module
}

/// The WGSL struct `name` has size `size` and the field offsets `fields`.
fn check(module: &naga::Module, name: &str, size: usize, fields: &[(&str, usize)]) {
    let (members, span) = module
        .types
        .iter()
        .find_map(|(_, ty)| match &ty.inner {
            naga::TypeInner::Struct { members, span } if ty.name.as_deref() == Some(name) => {
                Some((members, *span))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no struct {name} in layout.wgsl"));
    assert_eq!(span as usize, size, "{name}: size");
    for &(field, offset) in fields {
        let member = members
            .iter()
            .find(|m| m.name.as_deref() == Some(field))
            .unwrap_or_else(|| panic!("{name}: no field {field}"));
        assert_eq!(member.offset as usize, offset, "{name}.{field}: offset");
    }
}

#[test]
fn test_wgsl_structs_match_the_rust_layouts() {
    let module = module(ALL_PAIRS);
    assert_eq!(module.entry_points.len(), 1, "the kernel's `main`");
    check(
        &module,
        "Uniforms",
        size_of::<Uniforms>(),
        &[
            ("balls", offset_of!(Uniforms, balls)),
            ("species", offset_of!(Uniforms, species)),
            ("cell", offset_of!(Uniforms, cell)),
        ],
    );
    check(
        &module,
        "KernelPoint",
        size_of::<KernelPoint>(),
        &[
            ("x", offset_of!(KernelPoint, x)),
            ("y", offset_of!(KernelPoint, y)),
            ("col", offset_of!(KernelPoint, col)),
            ("row_species", offset_of!(KernelPoint, row_species)),
        ],
    );
    check(
        &module,
        "PairParams",
        size_of::<PairParams>(),
        &[
            ("sigma", offset_of!(PairParams, sigma)),
            ("epsilon", offset_of!(PairParams, epsilon)),
            ("r_cap", offset_of!(PairParams, r_cap)),
            ("cutoff", offset_of!(PairParams, cutoff)),
            ("pot_at_cap", offset_of!(PairParams, pot_at_cap)),
            ("pot_at_cutoff", offset_of!(PairParams, pot_at_cutoff)),
            ("max_force", offset_of!(PairParams, max_force)),
            ("core_stiffness", offset_of!(PairParams, core_stiffness)),
        ],
    );
    check(
        &module,
        "PairForces",
        size_of::<PairForces>(),
        &[
            ("force", offset_of!(PairForces, force)),
            ("potential", offset_of!(PairForces, potential)),
            ("virial", offset_of!(PairForces, virial)),
            ("capped", offset_of!(PairForces, capped)),
        ],
    );
}

#[test]
fn test_layouts_have_the_issue_sizes() {
    assert_eq!(size_of::<KernelPoint>(), 16);
    assert_eq!(size_of::<PairParams>(), 32);
    assert_eq!(size_of::<PairForces>(), 24);
    assert_eq!(size_of::<Uniforms>(), 16);
}
