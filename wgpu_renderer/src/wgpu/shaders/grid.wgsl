// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Cell grid shader: one instance covers the whole grid.
// Each cell gets one of six faint pastel colours ((col + row) % 6), and the
// grid is framed by a reddish border.
//
// u.params: unused

struct Inst {
    // Top-left corner of the grid (world units; rows grow downwards)
    @location(0) origin: vec2<f32>,
    @location(1) cell_size: vec2<f32>,
    // (cols, rows)
    @location(2) dims: vec2<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    // Position in cell units from the top-left corner: x right, y down.
    @location(0) cell_pos: vec2<f32>,
    @location(1) @interpolate(flat) dims: vec2<f32>,
    @location(2) @interpolate(flat) cell_size: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    let c = quad_corner(vi);
    let cell_pos = vec2<f32>(c.x, 1.0 - c.y) * inst.dims;
    let world = vec2<f32>(
        inst.origin.x + cell_pos.x * inst.cell_size.x,
        inst.origin.y - cell_pos.y * inst.cell_size.y,
    );

    var out: VOut;
    out.clip = u.view_proj * vec4<f32>(world, 0.0, 1.0);
    out.cell_pos = cell_pos;
    out.dims = inst.dims;
    out.cell_size = inst.cell_size;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // Border: 8 % of the smaller cell side, in world units, on every edge.
    let border_world = 0.08 * min(in.cell_size.x, in.cell_size.y);
    let border = vec2<f32>(border_world) / in.cell_size;
    let near_low = in.cell_pos < border;
    let near_high = (in.dims - in.cell_pos) < border;
    if any(near_low) || any(near_high) {
        return vec4<f32>(0.9, 0.25, 0.25, 0.8);
    }

    let cell = min(vec2<u32>(in.cell_pos), vec2<u32>(in.dims) - vec2<u32>(1u));
    switch (cell.x + cell.y) % 6u {
        case 0u: { return vec4<f32>(0.8, 0.8, 1.0, 0.15); }  // faint blue
        case 1u: { return vec4<f32>(0.8, 1.0, 0.8, 0.15); }  // faint green
        case 2u: { return vec4<f32>(1.0, 0.8, 0.8, 0.15); }  // faint red
        case 3u: { return vec4<f32>(1.0, 1.0, 0.8, 0.15); }  // faint yellow
        case 4u: { return vec4<f32>(0.8, 1.0, 1.0, 0.15); }  // faint cyan
        default: { return vec4<f32>(1.0, 0.8, 1.0, 0.15); }  // faint magenta
    }
}
