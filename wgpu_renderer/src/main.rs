#![allow(dead_code)]

mod wgpu;

use crate::wgpu::{DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MARGINS};

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
    rust_thermo_core::logging::init();
    log::info!(
        "Parallel: enabled | SIMD: enabled | CPU cores: {}",
        num_cpus::get()
    );
    init_rayon();
}

fn run_wgpu_renderer() {
    use rust_thermo_core::application::MainContent;
    use wgpu::WgpuWindow;

    log::info!("Renderer: wgpu");

    let world_rect = MainContent::bootstrap_world_rect(
        DEFAULT_WINDOW_WIDTH as u32,
        DEFAULT_WINDOW_HEIGHT as u32,
        MARGINS,
    );

    let application = MainContent::new(world_rect).expect("Failed to create MainContent");

    let window = WgpuWindow::new();
    window.create_window(application);
}

fn main() {
    print_diagnostics();
    run_wgpu_renderer();
}
