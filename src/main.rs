#![feature(portable_simd)]
#![allow(dead_code)]

//#[cfg(all(feature = "renderer-ggez", feature = "renderer-wgpu"))]
//compile_error!("Enable only one renderer feature: either `renderer-ggez` or `renderer-wgpu`.");

#[cfg(not(any(feature = "renderer-ggez", feature = "renderer-wgpu")))]
compile_error!("No renderer selected. Enable one of: `renderer-ggez` or `renderer-wgpu`.");

mod application;
mod graphics;
mod objects2d;
mod timing_monad;

#[cfg(feature = "renderer-ggez")]
use crate::graphics::ggez::ggez_window::GgezWindow;
#[cfg(feature = "renderer-ggez")]
use crate::graphics::window::Window;
#[cfg(feature = "renderer-ggez")]
use ggez::GameResult;

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

    println!(
        "✓ Rayon thread pool initialized with {} worker threads",
        num_cpus
    );
}

/// Print startup diagnostics about parallelization and SIMD features
fn print_diagnostics() {
    println!("✓ Parallel feature: ENABLED (always on)");
    init_rayon();

    println!("✓ SIMD feature: ENABLED (always on)");
    println!("System has {} logical CPU cores", num_cpus::get());
    println!("Note: Add many balls (F5 repeatedly) to see parallel performance benefits\n");
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

#[cfg(feature = "renderer-ggez")]
fn run_ggez_renderer() -> GameResult {
    println!("Using ggez renderer");
    let resource_dir = get_resource_dir();
    let window = GgezWindow::new(resource_dir, 0.5, 0.5);
    window.create_window();
    Ok(())
}

#[cfg(feature = "renderer-wgpu")]
fn run_wgpu_renderer() {
    use crate::application::MainContent;
    use crate::graphics::window::{MARGIN_PX_X, MARGIN_PX_Y};
    use crate::objects2d::Rect;

    println!("Using wgpu renderer");
    let resource_dir = get_resource_dir();

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

    let window = WgpuWindow::new(resource_dir, 0.5, 0.5);
    window.create_window(application);
}

#[cfg(feature = "renderer-ggez")]
fn main() -> GameResult {
    print_diagnostics();
    run_ggez_renderer()
}

#[cfg(feature = "renderer-wgpu")]
fn main() {
    print_diagnostics();
    run_wgpu_renderer();
}
