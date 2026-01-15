//! wgpu window implementation (placeholder)

use std::path::PathBuf;

pub struct WgpuWindow {
    _resource_dir: PathBuf,
    _region: (f32, f32),
}

impl WgpuWindow {
    pub fn new(resource_dir: PathBuf, reg_x: f32, reg_y: f32) -> Self {
        Self {
            _resource_dir: resource_dir,
            _region: (reg_x, reg_y),
        }
    }

    pub fn create_window(&self) {
        todo!("wgpu window not yet implemented");
    }
}
