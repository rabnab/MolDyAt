// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! WGPU window implementation
//!
//! Creates a winit window with WGPU rendering context and runs the event loop.
//! Renders a background grid and balls using instanced quads.
//! Integrates with the physics thread via PhysicsCoordinator to receive simulation snapshots.

use super::app_lifecycle::AppLifecycle;
use super::event_router::{EventRouter, GpuResources};
use super::frame_renderer::FrameRenderer;
use super::plot_surface::PlotTarget;
use super::wgpu_context::WgpuContext;
use super::wgpu_pipelines::WgpuPipelines;
use std::sync::Arc;
use thermo_physics::application::MainContent;
use thermo_ui::graphics::renderer::event_loop::{KeyMap, UnifiedRenderState};
use winit::event_loop::EventLoopProxy;

use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::Window,
};

/// WGPU-based window for rendering the molecular dynamics simulation.
///
/// This struct manages the winit window and WGPU context lifecycle.
pub struct WgpuWindow;

impl WgpuWindow {
    /// Create a new WGPU window configuration.
    pub fn new() -> Self {
        Self
    }

    /// Create the window and run the event loop.
    ///
    /// This method does not return until the window is closed.
    /// It initializes WGPU, creates the rendering surface, spawns the physics thread,
    /// and enters the main event loop receiving data from snapshots.
    ///
    /// # Arguments
    /// * `application` - MainContent to be moved to physics thread
    /// * `key_map` - the key bindings (defaults or a `ui.toml`)
    pub fn create_window(&self, application: MainContent, key_map: KeyMap) {
        log::info!("Creating WGPU window");

        let event_loop = EventLoop::<UserEvent>::with_user_event()
            .build()
            .expect("Failed to create event loop");
        let event_loop_proxy = event_loop.create_proxy();
        let mut app = WgpuApp::new(application, key_map, event_loop_proxy);
        event_loop
            .run_app(&mut app)
            .expect("Winit event loop terminated with an error");
    }
}

#[derive(Debug)]
pub(super) enum UserEvent {
    /// Triggered when new snapshot data is available from the physics thread.
    NewSnapshot,
}

struct WgpuApp {
    application: Option<MainContent>,
    /// Given to the render state when the window is created.
    key_map: KeyMap,
    window: Option<Arc<Window>>,
    proxy: EventLoopProxy<UserEvent>,
    ctx: Option<WgpuContext>,
    pipelines: Option<WgpuPipelines>,
    /// Surface of the energy plot window while it is open.
    plot: PlotTarget,
    state: Option<UnifiedRenderState>,
}

impl WgpuApp {
    fn new(application: MainContent, key_map: KeyMap, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            application: Some(application),
            key_map,
            proxy,
            window: None,
            ctx: None,
            pipelines: None,
            plot: PlotTarget::default(),
            state: None,
        }
    }

    fn init_if_needed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let application = self
            .application
            .take()
            .expect("Application state already consumed");
        let initialized = AppLifecycle::initialize(
            event_loop,
            application,
            self.key_map.clone(),
            self.proxy.clone(),
        );

        self.window = Some(initialized.window);
        self.ctx = Some(initialized.ctx);
        self.pipelines = Some(initialized.pipelines);
        self.state = Some(initialized.state);
    }
}

impl ApplicationHandler<UserEvent> for WgpuApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Sleep until an event arrives. The physics thread wakes the loop with
        // `UserEvent::NewSnapshot`, which is where we pull the latest snapshot
        // and request redraws.
        event_loop.set_control_flow(ControlFlow::Wait);
        self.init_if_needed(event_loop);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::NewSnapshot => {
                // Pull latest snapshot and request redraws for both windows
                if let (Some(state), Some(window)) = (self.state.as_mut(), self.window.as_ref()) {
                    FrameRenderer::handle_new_snapshot(state, window.as_ref());
                    if let Some(ctx) = self.ctx.as_mut() {
                        FrameRenderer::apply_window_size_request(state, window.as_ref(), ctx);
                    }
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        let gpu = GpuResources {
            ctx: self.ctx.as_mut(),
            pipelines: self.pipelines.as_mut(),
            plot: &mut self.plot,
        };
        let window = self.window.as_deref();

        EventRouter::route_window_event(window_id, event, gpu, state, event_loop, window);
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        EventRouter::route_device_event(event, state);
    }
}
