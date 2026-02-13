#![feature(portable_simd)]
#![allow(dead_code)]

mod application;
mod graphics;
mod logging;
mod objects2d;
mod timing_monad;

#[cfg(feature = "renderer-wgpu")]
use crate::graphics::wgpu::wgpu_window::WgpuWindow;

use std::env;
use std::path;
use std::path::PathBuf;

/// Initialize Rayon thread pool for parallel physics computations.
///
/// Rayon is used for:
/// - Parallel force calculations (main performance bottleneck)
/// - Parallel position updates
/// - Parallel neighbor statistics gathering
///
/// # Thread Count
/// Uses (CPU cores - 2) to leave headroom for:
/// - Main rendering thread
/// - OS/background processes
///
/// This prevents CPU saturation and maintains UI responsiveness.
fn init_rayon() {
    use rayon::ThreadPoolBuilder;

    const RESERVED_THREADS: usize = 2; // Reserve for main thread + OS
    let num_cpus = num_cpus::get().saturating_sub(RESERVED_THREADS).max(1);

    ThreadPoolBuilder::new()
        .num_threads(num_cpus)
        .thread_name(|i| format!("rayon-worker-{}", i)) // Named threads for easier profiling
        .build_global()
        .expect("Failed to initialize Rayon thread pool");

    log::info!("Rayon thread pool: {} worker threads", num_cpus);
}

/// Print startup diagnostics about parallelization and SIMD features.
fn print_diagnostics() {
    logging::init();
    log::info!(
        "Parallel: enabled | SIMD: enabled | CPU cores: {}",
        num_cpus::get()
    );
    init_rayon();
}

/// Get the resource directory path
fn get_resource_dir() -> PathBuf {
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let mut path = path::PathBuf::from(manifest_dir);
        path.push("resources");
        path
    } else {
        path::PathBuf::from("./resources")
    }
}

#[cfg(feature = "renderer-wgpu")]
fn run_wgpu_renderer() {
    use crate::application::MainContent;
    use crate::graphics::window::{MARGIN_PX_X, MARGIN_PX_Y};
    use crate::objects2d::Rect;

    log::info!("Renderer: wgpu");

    // Compute initial world bounds based on default window size (same as ggez)
    // LEN_TO_PX = 0.1 means 1 world unit = 10 pixels
    const LEN_TO_PX: f32 = 0.1;
    let default_width: f32 = 1280.0;
    let default_height: f32 = 720.0;
    let region_width = default_width - 2.0 * MARGIN_PX_X;
    let region_height = default_height - 2.0 * MARGIN_PX_Y;
    let world_width = region_width * LEN_TO_PX;
    let world_height = region_height * LEN_TO_PX;

    // Create world rect centered at origin (matching ggez behavior)
    let world_rect = Rect::new(
        -world_width / 2.0,
        world_height / 2.0,
        world_width,
        -world_height,
    );

    let application = MainContent::new(world_rect).expect("Failed to create MainContent");

    let window = WgpuWindow::new();
    window.create_window(application);
}

#[cfg(feature = "renderer-wgpu")]
fn main() {
    print_diagnostics();
    run_wgpu_renderer();
}
