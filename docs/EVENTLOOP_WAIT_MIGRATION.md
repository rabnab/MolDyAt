# Event Loop Migration: Poll → Wait with EventLoopProxy

## Problem Statement

**Issue**: Energy window stops rendering when the simulation window has focus.

**Root Cause**: Windows (and some other platforms) suppress `RedrawRequested` events for unfocused windows when using `ControlFlow::Poll`. The energy window calls `window.request_redraw()` after data updates, but the OS never delivers the corresponding `WindowEvent::RedrawRequested` to the event loop.

**Evidence from logs**:
```
[TRACE] Energy window redraw requested on Some(WindowId(2360232)) after energy update
# ↑ request_redraw() is called...
# ↓ ...but RedrawRequested event never arrives for WindowId(2360232)
[TRACE] Handling redraw for window with id WindowId(2622858) and should_render: true
# ↑ Only main window (2622858) receives RedrawRequested events
```

## Current Architecture Issues

### ControlFlow::Poll Problems
1. **CPU Inefficiency**: Event loop continuously polls, burning CPU cycles even when nothing changes
2. **OS Redraw Suppression**: Windows suppresses redraws for unfocused windows as optimization
3. **Energy Inefficiency**: High power consumption from constant polling
4. **Unreliable Multi-Window**: Secondary windows don't render properly when not focused

### Current Data Flow
```
Physics Thread (loop)
  ├─ simulation_step()
  ├─ publish_snapshot() → ArcSwapOption
  └─ (no wake-up signal)

Render Thread (continuous polling)
  ├─ about_to_wait() called every loop iteration
  ├─ begin_frame() pulls snapshot from ArcSwapOption
  ├─ window.request_redraw() for main window
  └─ (energy window redraw requests are suppressed by OS)
```

## Proposed Solution: ControlFlow::Wait + EventLoopProxy

### Benefits
- ✅ **CPU Efficient**: Event loop sleeps until woken by events
- ✅ **No OS Suppression**: `ControlFlow::Wait` ensures all RedrawRequested events are delivered
- ✅ **Event-Driven**: Renders only when new simulation data arrives
- ✅ **Multi-Window Support**: Energy window receives proper RedrawRequested events regardless of focus
- ✅ **Lower Power Consumption**: CPU can enter low-power states between frames

### New Data Flow
```
Physics Thread (loop)
  ├─ simulation_step()
  ├─ publish_snapshot() → ArcSwapOption
  └─ proxy.send_event(UserEvent::NewSnapshot) → wakes event loop

Render Thread (waiting for events)
  ├─ user_event(UserEvent::NewSnapshot)
  │   ├─ begin_frame() pulls snapshot
  │   ├─ main_window.request_redraw()
  │   └─ energy_window.request_redraw()
  └─ window_event(RedrawRequested)
      └─ Both windows render properly (OS delivers all events in Wait mode)
```

## Implementation Steps

### 1. Define UserEvent Type
**File**: `wgpu_renderer/src/wgpu/wgpu_window.rs`

Add after imports, before `WgpuWindow` struct:
```rust
use winit::event_loop::EventLoopProxy;

/// User events sent to wake up the event loop
#[derive(Debug, Clone, Copy)]
enum UserEvent {
    /// Physics thread published a new snapshot
    NewSnapshot,
}
```

### 2. Create EventLoop with User Events
**File**: `wgpu_renderer/src/wgpu/wgpu_window.rs`

Update `WgpuWindow::create_window`:
```rust
pub fn create_window(&self, application: MainContent) {
    log::info!("Creating WGPU window");

    // Create event loop that supports user events
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");

    // Get proxy for sending wake-up events from physics thread
    let proxy = event_loop.create_proxy();

    let mut app = WgpuApp::new(application, proxy);
    event_loop
        .run_app(&mut app)
        .expect("Winit event loop terminated with an error");
}
```

### 3. Store EventLoopProxy in WgpuApp
**File**: `wgpu_renderer/src/wgpu/wgpu_window.rs`

Update `WgpuApp` struct and constructor:
```rust
struct WgpuApp {
    application: Option<MainContent>,
    proxy: EventLoopProxy<UserEvent>,  // ADD THIS
    window: Option<Arc<Window>>,
    ctx: Option<WgpuContext>,
    pipelines: Option<WgpuPipelines>,
    state: Option<UnifiedRenderState>,
}

impl WgpuApp {
    fn new(application: MainContent, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            application: Some(application),
            proxy,  // ADD THIS
            window: None,
            ctx: None,
            pipelines: None,
            state: None,
        }
    }
}
```

### 4. Pass Proxy to UnifiedRenderState
**File**: `wgpu_renderer/src/wgpu/wgpu_window.rs`

In `init_if_needed`, pass proxy when creating state:
```rust
fn init_if_needed(&mut self, event_loop: &ActiveEventLoop) {
    // ...existing window/context setup...

    // Create closure that sends wake-up event
    let proxy = self.proxy.clone();
    let wakeup_fn = move || {
        let _ = proxy.send_event(UserEvent::NewSnapshot);
    };

    let state = UnifiedRenderState::new_with_wakeup(
        application,
        initial_world_bounds,
        window_size,
        (MARGIN_PX_X, MARGIN_PX_Y),
        Box::new(wakeup_fn),
    );

    // ...rest unchanged...
}
```

### 5. Add Wakeup Constructor to UnifiedRenderState
**File**: `core/src/graphics/renderer/event_loop/unified_render_state.rs`

Add new constructor (keep existing `new()` for backwards compatibility):
```rust
/// Create new render state with event loop wakeup callback.
///
/// The wakeup callback is called by the physics thread after publishing
/// each snapshot, allowing the event loop to use ControlFlow::Wait efficiently.
pub fn new_with_wakeup(
    application: MainContent,
    initial_world_bounds: Rect,
    window_size: (f32, f32),
    margins: (f32, f32),
    wakeup_callback: Box<dyn Fn() + Send + 'static>,
) -> Self {
    let (viewport, view_dirty) =
        ViewportTransform::new(initial_world_bounds, window_size, margins);

    let physics = PhysicsCoordinator::new_with_wakeup(application, wakeup_callback);

    Self {
        config: Default::default(),
        physics,
        viewport,
        view_dirty,
        energy_manager: EnergyWindowManager::new(physics.get_energy_data()),
        last_snapshot: None,
        cursor_world_pos: Vec2D::zero(),
        current_ball_size_idx: 2,
        metrics: MetricsCollector::new(),
    }
}
```

### 6. Pass Wakeup Callback Through PhysicsCoordinator
**File**: `core/src/graphics/renderer/event_loop/physics_coordinator.rs`

Add new constructor:
```rust
/// Create new coordinator with event loop wakeup callback
pub fn new_with_wakeup(
    application: MainContent,
    wakeup_callback: Box<dyn Fn() + Send + 'static>,
) -> Self {
    let snapshot_slot = application.get_snapshot_slot();

    // Spawn physics thread with wakeup callback
    let physics_thread = PhysicsThread::spawn_with_wakeup(
        application,
        wakeup_callback,
    );
    let command_sender = physics_thread.get_command_sender();

    Self {
        physics_thread: Some(physics_thread),
        command_sender: Some(command_sender),
        snapshot_slot: Some(snapshot_slot),
        last_processed_step: 0,
        last_displayed_step: 0,
        steps_per_frame: 0.0,
        last_step_time: std::time::Instant::now(),
        simulation_fps: 0.0,
        render_fps: 0,
        last_reported_render_fps: 0.0,
        render_report_interval: std::time::Instant::now(),
    }
}
```

### 7. Store and Call Wakeup in PhysicsThread
**File**: `core/src/application/physics_thread.rs`

Add wakeup callback to struct:
```rust
pub struct PhysicsThread {
    thread_handle: Option<JoinHandle<()>>,
    command_sender: Sender<PhysicsCommand>,
    shutdown_signal: ShutdownSignal,
    wakeup_callback: Option<Arc<dyn Fn() + Send + Sync>>,  // ADD THIS
}
```

Add new spawn method:
```rust
/// Spawn physics thread with event loop wakeup callback
pub fn spawn_with_wakeup(
    mut main_content: MainContent,
    wakeup_callback: Box<dyn Fn() + Send + 'static>,
) -> Self {
    let (command_sender, command_receiver) = mpsc::channel();
    let shutdown_signal = ShutdownSignal::new();
    let thread_signal = shutdown_signal.clone_signal();

    // Wrap callback in Arc for sharing with thread
    let wakeup = Arc::new(wakeup_callback);
    let thread_wakeup = Arc::clone(&wakeup);

    let thread_handle = thread::Builder::new()
        .name("physics".to_string())
        .spawn(move || {
            let snapshot_interval = std::time::Duration::from_micros(16_667); // ~60 Hz
            let mut last_snapshot_time = std::time::Instant::now();
            let mut physics_step_count: u64 = 0;
            let mut last_perf_report = std::time::Instant::now();
            let mut snapshot_time_total = std::time::Duration::ZERO;
            let mut snapshot_count = 0u64;

            while !thread_signal.should_shutdown() {
                Self::process_commands(&mut main_content, &command_receiver);

                let wait_time_ms = main_content.get_wait_time();
                if wait_time_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(wait_time_ms));
                }

                let _ = main_content.simulation_step();
                physics_step_count += 1;

                let now = std::time::Instant::now();
                if now.duration_since(last_snapshot_time) >= snapshot_interval {
                    let snapshot_start = std::time::Instant::now();
                    main_content.publish_snapshot();

                    // KEY CHANGE: Wake up event loop after publishing snapshot
                    (thread_wakeup)();

                    snapshot_time_total += snapshot_start.elapsed();
                    snapshot_count += 1;
                    last_snapshot_time = now;
                }

                // ...rest of performance tracking unchanged...
            }
        })
        .expect("Failed to spawn physics thread");

    Self {
        thread_handle: Some(thread_handle),
        command_sender,
        shutdown_signal,
        wakeup_callback: Some(wakeup),
    }
}
```

### 8. Update ApplicationHandler Implementation
**File**: `wgpu_renderer/src/wgpu/wgpu_window.rs`

Change to `ControlFlow::Wait` and handle user events:
```rust
impl ApplicationHandler<UserEvent> for WgpuApp {  // ADD <UserEvent> generic
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // CHANGE: Use Wait instead of Poll for efficiency
        event_loop.set_control_flow(ControlFlow::Wait);
        self.init_if_needed(event_loop);
    }

    // ADD: Handle user events from physics thread
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::NewSnapshot => {
                // Pull latest snapshot and request redraws for both windows
                if let (Some(state), Some(window)) = (self.state.as_mut(), self.window.as_ref()) {
                    state.begin_frame();

                    // Request redraw for main window
                    window.request_redraw();

                    // Request redraw for energy window if it's open
                    state.energy_manager.request_redraw_if_open();
                }
            }
        }
    }

    fn window_event(/* ...unchanged... */) {
        // Existing window event handling remains the same
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // REMOVE: No longer needed - snapshot pulling and redraw requests
        // are now handled in user_event when physics thread wakes us up

        // This method can be removed or left empty
    }
}
```

### 9. Add Helper to EnergyWindowManager
**File**: `core/src/graphics/renderer/event_loop/energy_window_manager.rs`

Add public method:
```rust
/// Request redraw for energy window if it exists.
///
/// This should be called after pulling new snapshot data to ensure
/// the energy window renders even when not focused.
pub fn request_redraw_if_open(&self) {
    if let Some(window) = &self.window {
        window.request_redraw();
        log::trace!(
            "Energy window redraw requested from explicit call on {:?}",
            window.window_id()
        );
    }
}
```

## Testing Checklist

After implementing these changes:

- [ ] Cargo check passes for both `core` and `wgpu_renderer` crates
- [ ] Application builds and runs without errors
- [ ] Main simulation window renders smoothly
- [ ] Energy window renders continuously when open
- [ ] Energy window continues rendering when main window has focus (key test!)
- [ ] Energy window continues rendering when energy window has focus
- [ ] CPU usage is lower than with Poll mode (check task manager)
- [ ] No stuttering or frame drops
- [ ] All keyboard inputs still work (H for help, D for data toggle, etc.)
- [ ] Window resize works for both windows
- [ ] Closing windows works properly
- [ ] Physics thread metrics still logged correctly

## Verification

To verify the fix works, enable trace logging and watch for:

```bash
RUST_LOG=trace cargo +nightly run --release
```

Expected log pattern (energy window ID will vary):
```
[TRACE] Energy window redraw requested from explicit call on Some(WindowId(...))
[TRACE] Handling redraw for window with id WindowId(...) and should_render: true
[TRACE] EnergyWindowManager: handle_redraw called
```

The key improvement is that `handle_redraw` should be called consistently, even when the main window is focused.

## Backwards Compatibility

Keep existing constructors for other renderers (ggez):
- `PhysicsThread::spawn()` - spawns without wakeup callback
- `PhysicsCoordinator::new()` - creates coordinator without wakeup
- `UnifiedRenderState::new()` - creates state without wakeup

The ggez renderer continues to use ControlFlow::Poll and will work as before.

## Related Files

Core architecture files involved in this migration:
- `wgpu_renderer/src/wgpu/wgpu_window.rs` - Event loop and window management
- `core/src/graphics/renderer/event_loop/unified_render_state.rs` - High-level render state
- `core/src/graphics/renderer/event_loop/physics_coordinator.rs` - Physics thread coordination
- `core/src/application/physics_thread.rs` - Physics simulation thread
- `core/src/graphics/renderer/event_loop/energy_window_manager.rs` - Energy plot window

## Performance Impact

Expected improvements:
- **CPU Usage**: Should drop significantly when simulation is paused or idle
- **Power Consumption**: Lower due to CPU able to sleep between frames
- **Frame Consistency**: More consistent frame times due to event-driven rendering
- **Multi-Window**: Energy window renders properly regardless of which window has focus

No negative performance impact expected - the physics thread runs at the same rate, and rendering happens at the same ~60 Hz snapshot rate.
