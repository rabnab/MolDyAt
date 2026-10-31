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

use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::{Ball2D, SPECIES_M};

/// A 30 x 30 grid of 3 x 3 cells of size 10, its top-left at (0, 30).
fn grid() -> CellGrid {
    CellGrid::new(Rect::new(0.0, 30.0, 30.0, -30.0), 10.0).unwrap()
}

fn ball_at(id: usize, x: f32, y: f32) -> Ball2D {
    Ball2D::new(id, Vec2D::new(x, y), SPECIES_M)
}

fn neighbours(grid: &CellGrid, row: usize, col: usize) -> Vec<usize> {
    let mut out = Vec::new();
    grid.collect_ball_indices_into(row, col, &mut out);
    out.sort_unstable();
    out
}

#[test]
fn test_rebuild_sorts_balls_by_cell_in_ball_order() {
    let mut grid = grid();
    // Cells (row, col): (2, 2), (0, 0), (2, 2), (1, 1), outside.
    let balls = [
        ball_at(0, 25.0, 5.0),
        ball_at(1, 5.0, 25.0),
        ball_at(2, 21.0, 1.0),
        ball_at(3, 15.0, 15.0),
        ball_at(4, 45.0, 15.0),
    ];
    grid.rebuild(&balls);
    assert_eq!(grid.cell_of_ball(0), Some((2, 2)));
    assert_eq!(grid.cell_of_ball(1), Some((0, 0)));
    assert_eq!(grid.cell_of_ball(3), Some((1, 1)));
    assert_eq!(grid.cell_of_ball(4), None, "outside the grid");
    assert_eq!(grid.sorted_balls(), &[1, 3, 0, 2]);
    assert_eq!(grid.balls_in_cell(2, 2).collect::<Vec<_>>(), vec![0, 2]);
    assert_eq!(neighbours(&grid, 0, 0), vec![1, 3]);
    assert_eq!(neighbours(&grid, 1, 1), vec![0, 1, 2, 3]);
    assert_eq!(neighbours(&grid, 2, 0), vec![3]);
}

#[test]
fn test_added_balls_are_found_before_and_after_the_next_rebuild() {
    let mut grid = grid();
    let mut balls = vec![ball_at(0, 5.0, 25.0)];
    grid.rebuild(&balls);

    balls.push(ball_at(1, 15.0, 25.0));
    grid.add_ball(1, balls[1].get_position());
    balls.push(ball_at(2, 99.0, 99.0));
    grid.add_ball(2, balls[2].get_position());
    assert_eq!(grid.cell_of_ball(1), Some((0, 1)));
    assert_eq!(grid.cell_of_ball(2), None);
    assert_eq!(neighbours(&grid, 0, 0), vec![0, 1]);
    assert_eq!(grid.balls_in_cell(0, 1).collect::<Vec<_>>(), vec![1]);

    grid.rebuild(&balls);
    assert_eq!(grid.sorted_balls(), &[0, 1]);
    assert_eq!(neighbours(&grid, 0, 0), vec![0, 1], "no ball twice");
}
