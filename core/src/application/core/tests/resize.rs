// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::create_main_content;
use crate::objects2d::Rect;
use crate::objects2d::Vec2D;
use crate::objects2d::ball_impl::{Ball2D, SPECIES_M};

#[test]
//#[ignore] // TODO: Fix resize after removing renderer from MainContent
fn test_resize_event() {
    let (mut main_content, _win_def) = create_main_content();

    let new_width = 300.0;
    let new_height = 200.0;

    let new_world_rect = Rect::new(0., 0., new_width, -new_height);

    main_content.resize(new_world_rect);
    main_content.unpause_after_resize();

    let new_world = main_content.world().cell_grid.world_rect();

    assert_eq!(new_world.w, new_width);
    assert_eq!(new_world.h, -new_height);

    // assert_eq!(win_def.region_px.0, 300.0 - 2.0 * win_def.margin_px.0);
    // assert_eq!(win_def.region_px.1, 200.0 - 2.0 * win_def.margin_px.1);
}

#[test]
fn test_resize_world_rect_matches_cell_grid() {
    let (mut main_content, _def) = create_main_content();

    let new_width = 400.0;
    let new_height = 350.0;

    // Resize window definition and main_content

    let new_world_rect = Rect::new(0., 0., new_width, -new_height);
    main_content.resize(new_world_rect);

    // Verify main_content's world_rect matches what we passed
    assert_eq!(main_content.world().world_rect.x, new_world_rect.x);
    assert_eq!(main_content.world().world_rect.y, new_world_rect.y);
    assert_eq!(main_content.world().world_rect.w, new_world_rect.w);
    assert_eq!(main_content.world().world_rect.h, new_world_rect.h);

    // Verify cell_grid world_rect matches main_content's world_rect
    let cell_grid_world = main_content.world().cell_grid.world_rect();
    assert_eq!(cell_grid_world.x, main_content.world().world_rect.x);
    assert_eq!(cell_grid_world.y, main_content.world().world_rect.y);
    assert_eq!(cell_grid_world.w, main_content.world().world_rect.w);
    assert_eq!(cell_grid_world.h, main_content.world().world_rect.h);
}

#[test]
fn test_resize_cell_grid_origin_correct() {
    let (mut main_content, _win_def) = create_main_content();

    let new_width = 500.0;
    let new_height = 400.0;

    // Resize and check cell grid origin matches world_rect top-left
    let new_world_rect = Rect::new(0., 0., new_width, -new_height);
    main_content.resize(new_world_rect);

    let cell_grid_world = main_content.world().cell_grid.world_rect();

    // Cell grid origin should be at world_rect top-left
    assert_eq!(
        cell_grid_world.x, new_world_rect.x,
        "Cell grid x origin should match world_rect x"
    );
    assert_eq!(
        cell_grid_world.y, new_world_rect.y,
        "Cell grid y origin should match world_rect y"
    );
}

#[test]
fn test_resize_multiple_times_maintains_consistency() {
    let (mut main_content, mut _win_def) = create_main_content();

    // Perform multiple resizes
    let sizes = [
        (300.0, 200.0),
        (600.0, 400.0),
        (450.0, 350.0),
        (800.0, 600.0),
    ];

    for (width, height) in &sizes {
        let world_rect = Rect::new(0., 0., *width, -*height);
        main_content.resize(world_rect);

        // Verify consistency after each resize
        let cell_grid_world = main_content.world().cell_grid.world_rect();

        assert_eq!(
            main_content.world().world_rect.x,
            cell_grid_world.x,
            "X mismatch after resize to {}x{}",
            width,
            height
        );
        assert_eq!(
            main_content.world().world_rect.y,
            cell_grid_world.y,
            "Y mismatch after resize to {}x{}",
            width,
            height
        );
        assert_eq!(
            main_content.world().world_rect.w,
            cell_grid_world.w,
            "Width mismatch after resize to {}x{}",
            width,
            height
        );
        assert_eq!(
            main_content.world().world_rect.h,
            cell_grid_world.h,
            "Height mismatch after resize to {}x{}",
            width,
            height
        );
    }
}

#[test]
fn test_resize_cell_count_changes_with_world_size() {
    let (mut main_content, _win_def) = create_main_content();

    // Get initial cell count
    let (initial_rows, initial_cols) = main_content.world().cell_grid.get_grid_size();

    // Resize to much larger dimensions

    let large_world_rect = Rect::new(0., 0., 1000.0, -800.0);
    main_content.resize(large_world_rect);

    let (large_rows, large_cols) = main_content.world().cell_grid.get_grid_size();

    // Larger world should have more cells
    assert!(
        large_rows > initial_rows || large_cols > initial_cols,
        "Larger world should have more cells. Initial: {}x{}, Large: {}x{}",
        initial_rows,
        initial_cols,
        large_rows,
        large_cols
    );

    // Resize to smaller dimensions
    let small_world_rect = Rect::new(0., 0., 200.0, -150.0);
    main_content.resize(small_world_rect);

    let (small_rows, small_cols) = main_content.world().cell_grid.get_grid_size();

    // Smaller world should have fewer cells than large
    assert!(
        small_rows < large_rows || small_cols < large_cols,
        "Smaller world should have fewer cells. Small: {}x{}, Large: {}x{}",
        small_rows,
        small_cols,
        large_rows,
        large_cols
    );
}

#[test]
fn test_unpause_after_resize_resumes_a_simulation_the_resize_paused() {
    let (mut main_content, _win_def) = create_main_content();
    if main_content.world().sim.is_paused() {
        main_content.toggle_pause();
    }

    main_content.resize(Rect::new(0., 0., 300.0, -200.0));
    assert!(main_content.world().sim.is_paused());

    main_content.unpause_after_resize();
    assert!(!main_content.world().sim.is_paused());
    assert!(!main_content.runtime().paused_for_resize);
}

#[test]
fn test_unpause_after_resize_leaves_every_ball_in_its_cell() {
    let (mut main_content, _win_def) = create_main_content();
    for i in 0..20 {
        let pos = Vec2D::new(-5.0 + (i % 5) as f32 * 18.0, 5.0 - (i / 5) as f32 * 18.0);
        main_content.add_candidate_to_vec(Ball2D::new(i, pos, SPECIES_M));
    }

    main_content.resize(Rect::new(0., 0., 300.0, -200.0));
    main_content.unpause_after_resize();

    let world = main_content.world();
    let (rows, cols) = world.cell_grid.get_grid_size();
    let in_cells: usize = (0..rows)
        .flat_map(|row| (0..cols).map(move |col| (row, col)))
        .map(|(row, col)| {
            world
                .cell_grid
                .get_cell(row, col)
                .unwrap()
                .get_ball_indices()
                .len()
        })
        .sum();
    assert_eq!(
        in_cells,
        world.balls.len(),
        "each ball is in exactly one cell"
    );

    for (idx, ball) in world.balls.iter().enumerate() {
        let cell = world
            .cell_grid
            .get_cell_indices(ball.get_position())
            .expect("ball is inside the grid");
        assert_eq!(ball.current_cell, Some(cell));
        let (row, col) = cell;
        assert!(
            world
                .cell_grid
                .get_cell(row, col)
                .unwrap()
                .get_ball_indices()
                .contains(&idx),
            "ball {idx} is not in the cell its position maps to"
        );
    }
}

#[test]
fn test_resize_remaps_ball_positions_fractionally() {
    let (mut main_content, _win_def) = create_main_content();

    // World from helper: Rect::new(-10, 10, 100, -100).
    // This point is exactly at 50%/50% in that world.
    let ball = Ball2D::new(0, Vec2D::new(40.0, -40.0), SPECIES_M);
    main_content.add_candidate_to_vec(ball);

    let new_world_rect = Rect::new(0.0, 0.0, 200.0, -200.0);
    main_content.resize(new_world_rect);

    let pos = *main_content.world().balls[0].get_position();
    assert!((pos.x - 100.0).abs() < 1e-5);
    assert!((pos.y + 100.0).abs() < 1e-5);
}
