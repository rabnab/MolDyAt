#![allow(dead_code)]

mod headless_cli;
mod headless_output;
mod wgpu;

use crate::headless_cli::HeadlessConfig;
use crate::wgpu::{DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MARGINS};
use rust_thermo_core::application::{MainContent, VerletIntegration};
use std::io;
use std::time::Duration;

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

fn run_headless_renderer(config: HeadlessConfig) -> io::Result<()> {
    log::info!("Renderer: headless");

    let world_rect = MainContent::bootstrap_world_rect(
        DEFAULT_WINDOW_WIDTH as u32,
        DEFAULT_WINDOW_HEIGHT as u32,
        MARGINS,
    );
    let mut application = MainContent::new(world_rect).expect("Failed to create MainContent");
    let mut writer = headless_output::HeadlessWriter::new(config.stdout, config.output.as_deref())?;
    let bound = application.world().world_region();
    let snapshot_every = config.snapshot_every.max(1);

    writer.write_snapshot(&application.create_snapshot())?;
    for step in 0..config.steps {
        application.update_sim(Duration::from_millis(1).as_secs_f32(), &bound);
        if step + 1 == config.steps || (step + 1) % snapshot_every == 0 {
            writer.write_snapshot(&application.create_snapshot())?;
        }
    }

    Ok(())
}

fn main() {
    print_diagnostics();

    match headless_cli::parse_headless_args() {
        Ok(Some(config)) => {
            log::info!("Running in headless mode");
            if let Err(error) = run_headless_renderer(config) {
                eprintln!("headless renderer failed: {error}");
                std::process::exit(1);
            }
        }
        Ok(None) => {
            run_wgpu_renderer();
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
