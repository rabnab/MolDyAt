// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The pair-table forces of each ball from all others, O(n²): the reference
// kernel. Same pair law as `CappedLanes` in pair_potential.rs.

var<workgroup> tile: array<KernelPoint, WORKGROUP_SIZE>;

@compute @workgroup_size(WORKGROUP_SIZE)
fn main(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) slot: u32,
) {
    let i = id.x;
    let n = uniforms.balls;
    // Invocations past `n` run along on the last point.
    let me = points[min(i, n - 1u)];
    let row = (me.row_species & SPECIES_MASK) * uniforms.species;
    // Most pairs are beyond every cutoff of the row: no storage read for them.
    var max_cutoff = 0.0;
    for (var s = 0u; s < uniforms.species; s++) {
        max_cutoff = max(max_cutoff, pairs[row + s].cutoff);
    }
    var out = PairForces();
    // Each invocation loads one point of the tile, then all read the tile.
    // Barriers need uniform control flow, so no invocation returns early.
    for (var start = 0u; start < n; start += WORKGROUP_SIZE) {
        tile[slot] = points[min(start + slot, n - 1u)];
        workgroupBarrier();
        for (var k = 0u; k < min(WORKGROUP_SIZE, n - start); k++) {
            let other = tile[k];
            // Wrapping differences, read as signed.
            let cells = vec2<i32>(vec2(
                me.col - other.col,
                (other.row_species >> SPECIES_BITS) - (me.row_species >> SPECIES_BITS),
            ));
            let d = vec2<f32>(cells) * uniforms.cell + vec2(me.x - other.x, me.y - other.y);
            let r2 = dot(d, d);
            if start + k != i && r2 < max_cutoff * max_cutoff {
                let pair = pairs[row + (other.row_species & SPECIES_MASK)];
                let r = sqrt(r2);
                if r < pair.cutoff {
                    // The law at max(r, r_cap) stays finite in the core.
                    let r_outer = max(r, pair.r_cap);
                    let inv = 1.0 / r_outer;
                    let q = pair.sigma * inv;
                    let q2 = q * q;
                    let q6 = q2 * q2 * q2;
                    let core = r_outer - r;
                    let force = 24.0 * pair.epsilon * q6 * (2.0 * q6 - 1.0) * inv
                        + pair.core_stiffness * core;
                    let force_to_dist = force / max(r, 1e-10);
                    out.force += d * force_to_dist;
                    out.virial += r2 * force_to_dist;
                    out.potential += select(
                        4.0 * pair.epsilon * q6 * (q6 - 1.0),
                        pair.pot_at_cap
                            + core * (pair.max_force + 0.5 * pair.core_stiffness * core),
                        r < pair.r_cap,
                    ) - pair.pot_at_cutoff;
                    out.capped += u32(r < pair.r_cap);
                }
            }
        }
        workgroupBarrier();
    }
    if i < n {
        results[i] = out;
    }
}
