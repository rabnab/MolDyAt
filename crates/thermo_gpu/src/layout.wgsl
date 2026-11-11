// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Buffers of the pair-force kernel, prepended to its source. Each struct
// mirrors the Rust type of the same name (sizes and offsets checked in
// tests/layout.rs).

struct Uniforms {
    balls: u32,
    species: u32,
    cell: vec2<f32>,
}

// Rows grow downwards, y upwards: Δ = (Δcol · cell.x + Δx, −Δrow · cell.y + Δy).
struct KernelPoint {
    x: f32,
    y: f32,
    col: u32,
    // Row (high 24 bits) and species index (low 8 bits).
    row_species: u32,
}

// Pair (a, b) at a * species + b.
struct PairParams {
    sigma: f32,
    epsilon: f32,
    r_cap: f32,
    cutoff: f32,
    pot_at_cap: f32,
    pot_at_cutoff: f32,
    max_force: f32,
    core_stiffness: f32,
}

struct PairForces {
    force: vec2<f32>,
    potential: f32,
    virial: f32,
    capped: u32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> points: array<KernelPoint>;
@group(0) @binding(2) var<storage, read> pairs: array<PairParams>;
@group(0) @binding(3) var<storage, read_write> results: array<PairForces>;
