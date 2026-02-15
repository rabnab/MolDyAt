//! Energy window manager - shared logic for both ggez and wgpu renderers
//!
//! Handles energy plot window creation, event handling, and data updates.

use crate::application::renderer_snapshot::RendererSnapshot;
use crate::graphics::energy_plot::{DisplayMode, EnergyPlotWindow, SharedEnergyData};
use crate::graphics::renderer::event_loop::metrics::FrameMetrics;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowId;

/// Manages the energy plot window lifecycle and data updates.
///
/// This struct encapsulates all the energy window logic that was previously
/// duplicated between ggez and wgpu renderers.
pub struct EnergyWindowManager {
    /// The energy plot window (if open)
    window: Option<EnergyPlotWindow>,
    /// Shared energy data (shared with window)
    energy_data: SharedEnergyData,
    /// Last processed simulation step (to avoid duplicate updates)
    last_processed_step: u64,
}

/// Maximum number of energy samples to keep in buffer
const MAX_ENERGY_SAMPLES: usize = 1000;

impl EnergyWindowManager {
    /// Create a new energy window manager.
    ///
    /// # Arguments
    /// * `energy_data` - Shared energy data for the energy plot
    pub fn new(energy_data: SharedEnergyData) -> Self {
        Self {
            window: None,
            energy_data,
            last_processed_step: 0,
        }
    }

    /// Get the window ID if the energy window is open.
    pub fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().and_then(|w| w.window_id())
    }

    /// Check if this window event is for the energy window.
    pub fn is_energy_window_event(&self, window_id: WindowId) -> bool {
        log::trace!(
            "Checking if event for window {:?} matches energy window {:?}",
            window_id,
            self.window_id()
        );
        self.window_id() == Some(window_id)
    }

    /// Toggle or cycle the energy window.
    ///
    /// If no window exists, creates one with default mode.
    /// If window exists and is open, cycles display mode.
    /// If window was closed, reopens with last mode.
    pub fn toggle(&mut self, window_target: &ActiveEventLoop) {
        if self.window.is_none() {
            // Create and open energy window with default mode
            let mut window =
                EnergyPlotWindow::new(self.energy_data.clone(), DisplayMode::default());
            if let Err(e) = window.create(window_target) {
                eprintln!("Failed to create energy window: {}", e);
                return;
            }
            self.window = Some(window);
        } else if let Some(window) = &mut self.window {
            if window.is_open() {
                // Cycle display mode instead of closing
                window.cycle_display_mode();
                window.request_redraw();
            } else {
                // Reopen if closed with last mode
                let last_mode = window.get_display_mode();
                let mut new_window = EnergyPlotWindow::new(self.energy_data.clone(), last_mode);
                if let Err(e) = new_window.create(window_target) {
                    eprintln!("Failed to create energy window: {}", e);
                    return;
                }
                self.window = Some(new_window);
            }
        }
    }

    /// Close the energy window.
    pub fn close(&mut self) {
        self.window = None;
    }

    /// Handle a window event for the energy window.
    ///
    /// # Returns
    /// `true` if the event was handled, `false` if not an energy window event.
    pub fn handle_event(&mut self, event: WindowEvent) -> bool {
        match event {
            WindowEvent::CloseRequested => {
                self.window = None;
                true
            }
            WindowEvent::Resized(size) => {
                if let Some(window) = &mut self.window {
                    window.handle_resize(size.width, size.height);
                }
                true
            }
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: PhysicalKey::Code(keycode),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => match keycode {
                KeyCode::Escape => {
                    self.window = None;
                    true
                }
                KeyCode::Tab | KeyCode::KeyP => {
                    if let Some(window) = &mut self.window {
                        window.cycle_display_mode();
                        window.request_redraw();
                    }
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Handle a redraw request for the energy window.
    ///
    /// # Returns
    /// `true` if render was performed, `false` if not energy window's redraw.
    pub fn handle_redraw(&mut self) -> bool {
        log::trace!("EnergyWindowManager: handle_redraw called");
        if let Some(window) = &mut self.window {
            let _ = window.render();
            true
        } else {
            false
        }
    }

    /// Update energy data from a snapshot.
    ///
    /// Only updates if this is a new simulation step (avoids duplicates).
    /// Updates kinetic/potential energy, time, and status text.
    ///
    /// # Arguments
    /// * `snapshot` - The renderer snapshot with energy data
    /// * `render_fps` - Smoothed render FPS for status text
    /// * `steps_per_frame` - Physics steps per render frame for status text
    /// * `frame_metrics` - Per-frame metrics (draw calls, frame time, etc.)
    pub fn update_from_snapshot(
        &mut self,
        snapshot: &RendererSnapshot,
        render_fps: f32,
        steps_per_frame: f32,
        frame_metrics: &FrameMetrics,
    ) {
        // Only update if this is a new simulation step
        if snapshot.simulation_step <= self.last_processed_step {
            return;
        }

        self.last_processed_step = snapshot.simulation_step;

        if let Ok(mut data) = self.energy_data.lock() {
            data.v_kin.push_back(snapshot.kinetic_energy);
            data.v_pot.push_back(snapshot.potential_energy);
            data.time = snapshot.time;

            // Update status text with FPS and per-frame metrics
            if let Some(ref status) = snapshot.status_text {
                let fps_line = format_metrics_line(
                    render_fps,
                    snapshot.physics_fps,
                    steps_per_frame,
                    frame_metrics,
                );
                let updated_status = status.replace("fps 0", &fps_line);
                data.status_text = Some(updated_status);
            } else {
                data.status_text = None;
            }

            // Limit buffer size to prevent unbounded growth
            if data.v_kin.len() > MAX_ENERGY_SAMPLES {
                data.v_kin.pop_front();
            }
            if data.v_pot.len() > MAX_ENERGY_SAMPLES {
                data.v_pot.pop_front();
            }

            // Release lock before requesting redraw
            drop(data);
        }
    }

    /// Get a reference to the shared energy data.
    pub fn energy_data(&self) -> &SharedEnergyData {
        &self.energy_data
    }
}

/// Format the metrics replacement for the `"fps 0"` placeholder.
///
/// Produces two lines:
/// 1. FPS & throughput:  `render: 60 fps (16.7ms) | sim: 1200 fps | steps/frame: 20.0`
/// 2. Draw-call stats:   `draws: 3 | balls: 100`
fn format_metrics_line(
    render_fps: f32,
    physics_fps: f32,
    steps_per_frame: f32,
    m: &FrameMetrics,
) -> String {
    format!(
        "render: {} fps ({:.1}ms) | sim: {:.0} fps | steps/frame: {:.1}\n\
         draws: {} | balls: {}",
        render_fps as u32,
        m.frame_time_ms,
        physics_fps,
        steps_per_frame,
        m.draw_calls,
        m.balls_rendered,
    )
}
