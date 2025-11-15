# Plan: Fix Velocity Verlet Integration with Wall Bounces

## Problem Summary

Wall bounces modify velocities during the **position update step**, then the Velocity Verlet algorithm modifies them again during the **velocity update step**. This violates the Verlet assumption that velocities remain constant between these two phases, causing:
- **5.4× momentum drift** over 100 steps (system momentum grows from 83 → 456 units)
- Potential energy drift in long simulations
- Physically incorrect dynamics

## Chosen Solution: Option 1 - Velocity Reflection

Apply wall bounces as **impulses during the velocity update step**, not during position update.

### Why Option 1?

✅ **Pros:**
- Minimal code changes (primarily in `ball_core.rs` and `sim.rs`)
- Preserves Verlet integration structure
- Maintains symplectic properties (energy conservation)
- Clean separation: position update only affects positions
- Performance impact negligible (same number of operations)

❌ **Cons of other options:**
- **Option 2 (Split Timestep)**: Complex, requires collision time calculation, multiple integration steps per timestep
- **Option 3 (RATTLE)**: Large refactoring, overkill for simple reflective boundaries

## Implementation Plan

### Step 1: Modify Position Update (ball_core.rs)

**Current behavior:**
```rust
fn update_y_and_handle_top_bottom_boundary() -> (f32, f32, f32) {
    // ...
    res_vy = -self.v.y;  // ← Velocity modified here!
    wall_momentum_y = 2.0 * self.mass * self.v.y;
    // ...
}

fn execute_update_pos() -> (f32, f32) {
    // ...
    self.v = Vec2D::new(vx, vy);  // ← Velocity updated during position step!
}
```

**New behavior:**
```rust
fn update_y_and_handle_top_bottom_boundary() -> (f32, bool) {
    // Only reflect position, DON'T modify velocity
    // Return: (new_y, collision_flag)
    if newy <= bound_min_y {
        let overy = bound_min_y - newy;
        return (bound_min_y + overy, true);  // Collision detected
    }
    // ...
}

fn execute_update_pos() -> BounceInfo {
    // Track which walls were hit, but don't modify velocity
    // Return collision flags for x and y directions
}
```

**Key changes:**
- Position reflection: `x_reflected = boundary ± (boundary - x_new)`
- Velocity: **Unchanged** during position update
- Return collision detection flags instead of wall momentum

### Step 2: Add Bounce Info Structure

```rust
pub struct BounceInfo {
    pub wall_px: f32,  // Will be calculated during velocity update
    pub wall_py: f32,
    pub bounced_x: bool,  // Did we hit left/right wall?
    pub bounced_y: bool,  // Did we hit top/bottom wall?
    pub v_x_before: f32,  // Velocity before bounce (for momentum calc)
    pub v_y_before: f32,
}
```

### Step 3: Apply Bounces During Velocity Update (sim.rs)

**Current flow:**
```
1. update_positions() → modifies position AND velocity
2. compute_forces()
3. apply_forces_and_collect_stats() → Verlet velocity update
```

**New flow:**
```
1. update_positions() → modifies position ONLY, returns bounce flags
2. compute_forces()
3. apply_forces_and_collect_stats() → 
   a. Apply wall bounces as impulses (if flagged)
   b. Calculate wall momentum: 2*m*v_before_bounce
   c. Apply Verlet velocity update to post-bounce velocity
```

**Pseudocode:**
```rust
fn apply_single_force_and_collect_stats(..., bounce_info: &BounceInfo) {
    let ball = &mut self.balls[ball_idx];
    
    // Step 1: Apply wall bounce impulses FIRST
    let mut v_after_bounce = ball.v;
    let mut wall_px = 0.0;
    let mut wall_py = 0.0;
    
    if bounce_info.bounced_x {
        wall_px = 2.0 * ball.mass * v_after_bounce.x;
        v_after_bounce.x = -v_after_bounce.x;
    }
    if bounce_info.bounced_y {
        wall_py = 2.0 * ball.mass * v_after_bounce.y;
        v_after_bounce.y = -v_after_bounce.y;
    }
    
    // Step 2: Compute new acceleration
    let accel = ball.accel_with_speed_limit(...);
    
    // Step 3: Apply Verlet update to post-bounce velocity
    v_after_bounce.x += 0.5 * (accel.x + ball.accel.x) * dt;
    v_after_bounce.y += 0.5 * (accel.y + ball.accel.y) * dt;
    
    ball.v = v_after_bounce;
    ball.accel = accel;
    
    return (wall_px, wall_py);  // For accumulation
}
```

### Step 4: Wall Momentum Accumulation

**Current:** Collected during `update_positions()` in parallel

**New:** Collected during `apply_forces_and_collect_stats()` sequentially
- Each ball's bounce info is processed
- Wall momentum accumulated per ball
- Total summed and stored in `SimStats`

### Step 5: Update Data Flow

**Modified functions:**

1. **`ball_core.rs`:**
   - `update_y_and_handle_top_bottom_boundary()`: Return `(f32, bool)` instead of `(f32, f32, f32)`
   - `update_x_and_handle_left_right_boundary()`: Return `(f32, bool)` instead of `(f32, f32, f32)`
   - `execute_update_pos()`: Return `BounceInfo` instead of `(f32, f32)`

2. **`ball_physics.rs`:**
   - `update_positions()`: Return `BounceInfo` instead of `(f32, f32)`

3. **`sim.rs`:**
   - `update_positions()`: Return `Vec<BounceInfo>` instead of `(f32, f32)`
   - `apply_single_force_and_collect_stats()`: Accept `BounceInfo`, return `(f32, f32)`
   - `apply_forces_and_collect_stats()`: Process bounces and accumulate wall momentum
   - `update_sim()`: Pass bounce info to force application

## Testing Strategy

### Validation Tests

1. **Momentum Conservation (Primary Goal)**
   - Run `test_energy_conservation_1000_balls`
   - **Expected:** System momentum drift drops from ~450 to <1 unit
   - **Target:** System momentum conserved to machine precision (no wall collisions) or <<1% (with walls)

2. **Energy Conservation**
   - Same test, check energy drift
   - **Expected:** Similar or better than current (~21% for 1000 balls over 100 steps)
   - Should improve since velocity modifications now respect Verlet structure

3. **2-Ball Test**
   - `test_cutoff_energy_loss` already shows perfect conservation without walls
   - Add variant with walls to verify bounce behavior

4. **Position Correctness**
   - Balls should still reflect correctly off boundaries
   - No balls should escape the simulation domain
   - Visual inspection during runtime

### Regression Tests

- All existing tests must pass
- No change to energy conservation in non-wall scenarios
- No change to force calculations or neighbor lists

## Expected Outcomes

### Before Fix (Current Master)
```
1000-ball test, 100 steps:
  Initial system momentum: (21.02, -80.69), |p| = 83.4
  Final system momentum:   (-420.5, -176.7), |p| = 456
  System drift:            451.7 units (5.4× growth) ✗
  Energy drift:            ~21%
```

### After Fix (Target)
```
1000-ball test, 100 steps:
  Initial system momentum: (21.02, -80.69), |p| = 83.4
  Final system momentum:   (~21, ~-81), |p| ≈ 83.4
  System drift:            <1 unit (<1% of initial) ✓
  Energy drift:            ≤21% (same or better)
```

## Implementation Order

1. ✅ Create branch `feature/master/fix-verlet-wall-bounce`
2. ✅ Write this implementation plan
3. Add `BounceInfo` struct to `ball_core.rs`
4. Modify `update_x/y_and_handle_boundary()` to return `(f32, bool)`
5. Modify `execute_update_pos()` to return `BounceInfo`
6. Update `ball_physics.rs` to propagate `BounceInfo`
7. Modify `sim.rs::update_positions()` to collect `Vec<BounceInfo>`
8. Modify `apply_single_force_and_collect_stats()` to apply bounces + return wall momentum
9. Update `apply_forces_and_collect_stats()` to accumulate wall momentum
10. Update `update_sim()` to pass bounce info through pipeline
11. Run tests and verify momentum conservation
12. Compare before/after results
13. Update PR #37 description or create new PR

## Physics Verification

### Velocity Verlet Algorithm (Correct Implementation)

```
Given: x(t), v(t), a(t)

1. Position update:  x(t+dt) = x(t) + v(t)*dt + 0.5*a(t)*dt²
2. Compute forces:   F(t+dt) from x(t+dt)
3. Acceleration:     a(t+dt) = F(t+dt)/m
4. Velocity update:  v(t+dt) = v(t) + 0.5*(a(t) + a(t+dt))*dt

Properties:
- Time-reversible
- Symplectic (conserves phase space volume)
- 2nd order accurate: O(dt²)
- Velocity at step n uses accelerations from steps n and n+1
```

### Wall Bounce as Impulse (New Approach)

When position update detects collision:
1. Reflect position only (geometric correction)
2. Set collision flag
3. **During velocity update:**
   - Apply impulse: `v_post = -v_pre` (before Verlet step)
   - Wall momentum: `Δp_wall = 2*m*v_pre`
   - Then apply Verlet: `v_final = v_post + 0.5*(a_old + a_new)*dt`

This maintains the integrity of the Verlet integration:
- Position update: Pure kinematics, no velocity modification
- Velocity update: All velocity modifications (bounce + forces) happen together
- Bounce is instantaneous impulse, doesn't interfere with continuous force integration

## Potential Issues & Solutions

### Issue 1: Velocity after position update isn't used for anything
**Solution:** Already true - velocity isn't read between position and velocity updates

### Issue 2: Multiple bounces per timestep (corner collisions)
**Solution:** Both `bounced_x` and `bounced_y` can be true simultaneously, handled correctly

### Issue 3: Performance impact of sequential wall momentum accumulation
**Solution:** Negligible - same number of operations, just moved to different location

### Issue 4: Wall momentum might be calculated differently
**Solution:** Calculation is identical `2*m*v`, just timing changes (before vs during velocity update)

## Success Criteria

✅ **All tests pass** (95/95)
✅ **Momentum drift** drops from 5.4× to <1%
✅ **Energy conservation** maintained or improved
✅ **No visual artifacts** during simulation
✅ **Clippy clean** with `-D warnings`
✅ **Formatted** with `cargo fmt`

## References

- Issue #36: Wall boundary collisions break Velocity Verlet integration
- PR #37: Add wall momentum tracking (merged)
- Velocity Verlet: https://en.wikipedia.org/wiki/Verlet_integration#Velocity_Verlet
- Symplectic integrators: Hairer et al. "Geometric Numerical Integration" (2006)
