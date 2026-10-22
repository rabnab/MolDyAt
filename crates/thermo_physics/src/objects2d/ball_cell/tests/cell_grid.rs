// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::objects2d::Rect;
use crate::objects2d::ball_cell::cell_grid::CellGrid;

#[test]
fn test_cells_are_at_least_interaction_range_wide() {
    // 3x3 cells only cover every pair in range if no cell is narrower than
    // the list radius (cutoff + skin). Widths that are not a multiple of it
    // used to round the cell count up and shrink the cells.
    let required = 10.0;
    for (w, h) in [(112.0, 62.0), (95.0, 31.0), (10.0, 10.0), (5.0, 3.0)] {
        let grid = CellGrid::new(Rect::new(0.0, 0.0, w, h), required).unwrap();
        let (rows, cols) = grid.get_grid_size();
        let (cell_w, cell_h) = grid.get_cell_size();
        assert!(rows >= 1 && cols >= 1);
        let smaller_world_side = |side: f32| side < required;
        assert!(
            cell_w >= required || smaller_world_side(w),
            "{w}x{h}: cell width {cell_w} < {required}"
        );
        assert!(
            cell_h >= required || smaller_world_side(h),
            "{w}x{h}: cell height {cell_h} < {required}"
        );
    }
}
