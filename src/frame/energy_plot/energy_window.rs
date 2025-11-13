use super::energy_data::SharedEnergyData;
use std::rc::Rc;
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::{Window, WindowBuilder, WindowId};

const DEFAULT_WIDTH: u32 = 640;
const DEFAULT_HEIGHT: u32 = 360;

/// Energy plot window - a separate OS window for displaying energy plots
pub struct EnergyPlotWindow {
    pub window: Option<Rc<Window>>,
    energy_data: SharedEnergyData,
}

impl EnergyPlotWindow {
    pub fn new(energy_data: SharedEnergyData) -> Self {
        Self {
            window: None,
            energy_data,
        }
    }

    pub fn create(
        &mut self,
        window_target: &EventLoopWindowTarget<()>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let window = Rc::new(
            WindowBuilder::new()
                .with_title("Energy Plot 🐾")
                .with_inner_size(LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT))
                .with_resizable(true)
                .with_min_inner_size(LogicalSize::new(320, 180))
                .build(window_target)?,
        );

        self.window = Some(window);
        Ok(())
    }

    pub fn is_open(&self) -> bool {
        self.window.is_some()
    }

    pub fn close(&mut self) {
        self.window = None;
    }

    pub fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|w| w.id())
    }

    pub fn focus(&self) {
        if let Some(window) = &self.window {
            window.focus_window();
        }
    }

    pub fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub fn handle_resize(&mut self, _width: u32, _height: u32) {
        self.request_redraw();
    }

    pub fn get_dimensions(&self) -> (u32, u32) {
        if let Some(window) = &self.window {
            let size = window.inner_size();
            (size.width, size.height)
        } else {
            (DEFAULT_WIDTH, DEFAULT_HEIGHT)
        }
    }

    pub fn get_energy_data(&self) -> &SharedEnergyData {
        &self.energy_data
    }
}
