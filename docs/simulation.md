# Coordinate Systems

The simulation uses a small set of coordinate systems that intentionally overlap in data flow but serve different purposes. The most important distinction is between simulation world space, pixel space in the renderer, and grid/index space for derived fields.

## World Space

World space is the canonical simulation coordinate system.

- Ball positions, velocities, forces, line-segment endpoints, source-sphere centers, and thermostat positions are stored in world coordinates.
- The simulation is treated as Cartesian: positive `x` points right and positive `y` points up.
- The default visible world is centered around the origin, so `(0, 0)` is the center of the simulation box.

## Rect Representation

`Rect` is a generic container used to describe world bounds and viewport bounds. It stores:

- `x`, `y`: the top-left corner in the rectangle's own convention
- `w`, `h`: width and height

For simulation world bounds, production code commonly uses a negative `h` to represent a Cartesian box while still preserving a top-left representation. For example:

```text
Rect::new(-50.0, 35.0, 100.0, -70.0)
```

describes a world that spans:

- `x` from `-50` to `50`
- `y` from `35` at the top to `-35` at the bottom

This lets code recover the true bounds with `min`/`max` operations while keeping the stored rectangle compatible with top-left based rendering logic.

## Pixel Space

Renderer input and output operate in window pixel coordinates.

- Pixel coordinates use the usual screen convention: origin at the top-left of the window, `x` increasing right, `y` increasing down.
- `ViewportTransform` converts between pixel space and world space with `px_to_world()` and `world_to_px()`.
- At zoom `1.0`, the full world bounds fit inside the viewport rectangle after margins are applied.
- The world origin maps to the center of the viewport when the view is reset.

Because world `y` points up while pixel `y` points down, the viewport transform flips the vertical axis as part of the mapping.

## Viewport Space

The full window is not always available for simulation rendering. The renderer reserves margins for UI, and the simulation is drawn only inside the viewport rectangle.

- Window size is measured in raw pixels.
- Margins shrink that window into a drawable viewport.
- Pan offsets are stored in world units, not pixels.
- Zoom changes the visible world rectangle, not the underlying simulation coordinates.

The visible part of the world is the viewport's world-space view (`visible_world_rect`); it equals the world bounds only at zoom `1.0` without panning.

## Operator-Local Coordinates

Most operators are fully world-space objects, but valve segments add one local parameterization.

- Line segments store `start` and `end` directly in world coordinates.
- Source spheres and thermostats store their centers in world coordinates.
- Valve segments are defined relative to a parent line segment using parametric coordinates `t_start` and `t_end` in `[0, 1]`.

That means a valve's geometry is not stored as two independent world points. Instead, it is derived from its parent segment plus normalized coordinates along that segment.

## Thermodynamic Grid Space

Derived temperature and pressure fields are stored in row-major grid space.

- `ThermoGrid` stores `(rows, cols)` plus flat row-major data.
- Grid cell `(0, 0)` corresponds to the top-left cell of the world rectangle.
- Grid indexing is computed from the minimum world-space `x` and `y` corner, so both positive-height and negative-height `Rect` values map consistently.
- The thermo grid is its own, not the physics `CellGrid` (#290): `thermo_grid_dims` fits as many cells of at least `THERMO_CELL_SIZE` (10, in `σ` of `N2`) into the world rectangle as there is room for, at least one per axis. The default 112 x 62 world has 11 x 6 cells, whatever species it holds. Only a resize or a load changes the shape; `ThermoFieldHistory` restarts its rolling average then, never when species come or go. The physics cells, by contrast, are at least the largest neighbour-list radius among the species present (4.8 with only `N2`, 21.9 with `XL` balls), which the 3x3 neighbour search needs; `ThermoGridSpec` in the snapshot describes the thermo grid, `CellGridSpec` the physics grid (the `Grid` overlay).
- The T/P field is sampled every `THERMO_SAMPLE_STRIDE` (10) steps, and right away when the history is empty, and only while the heatmap is shown. The rolling average keeps `THERMO_WINDOW` (100) samples, so it still spans `THERMO_WINDOW_STEPS` (1000) steps. The field does not feed back into the physics.

This grid space is for visualization and analysis only. It does not replace world-space coordinates for simulation objects.

## Summary

- All persistent simulation state is in world space.
- Pixel space appears only in input handling, window geometry and renderer output.
- `Rect` is a storage format and does not fix the axis direction; bounds come from `min`/`max`.
- Operator data is in world space; valves are the only operator with a local parameterization (`t` along the parent wall).
