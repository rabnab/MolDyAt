// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![allow(dead_code)]

mod cli;
mod headless_cli;
mod headless_output;
mod headless_runner;
mod headless_timings;
mod wgpu;

use crate::wgpu::{DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MARGINS};
use rust_thermo_core::application::MainContent;
use rust_thermo_core::application::persistence::{LoadSource, SaveStore};

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
/// `RAYON_NUM_THREADS` (a positive integer) overrides the count, e.g. to pin
/// it for perf runs (`scripts/headless_perf.sh`).
fn init_rayon() {
    use rayon::ThreadPoolBuilder;

    const RESERVED_THREADS: usize = 2; // Reserve for main thread + OS
    let num_cpus = std::env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&threads| threads > 0)
        .unwrap_or_else(|| num_cpus::get().saturating_sub(RESERVED_THREADS).max(1));

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
    log::info!("{} {}", cli::PROGRAM, cli::VERSION);
    log::info!(
        "Parallel: enabled | SIMD: enabled | CPU cores: {}",
        num_cpus::get()
    );
    init_rayon();
}

fn run_wgpu_renderer(common: cli::CommonArgs) {
    use wgpu::WgpuWindow;

    log::info!("Renderer: wgpu");

    let world_rect = MainContent::bootstrap_world_rect(
        DEFAULT_WINDOW_WIDTH as u32,
        DEFAULT_WINDOW_HEIGHT as u32,
        MARGINS,
    );

    let store = SaveStore::new(&common.save_dir);
    let mut application = MainContent::new(world_rect).expect("Failed to create MainContent");
    if let Some(path) = common.load {
        // Same entry point as Ctrl+O; the window then shows the loaded world.
        match application.load_world(&store, &LoadSource::Path(path.clone()), true) {
            Ok(loaded) => log::info!("Loaded world from {} (paused)", loaded.path.display()),
            Err(e) => {
                eprintln!("cannot load {}: {e:?}", path.display());
                std::process::exit(1);
            }
        }
    }
    let application = application.with_save_store(store);

    let window = WgpuWindow::new();
    window.create_window(application);
}

fn run_headless_renderer(
    config: headless_cli::HeadlessConfig,
    common: cli::CommonArgs,
) -> std::io::Result<()> {
    log::info!("Renderer: headless");

    let world_rect = MainContent::bootstrap_world_rect(
        DEFAULT_WINDOW_WIDTH as u32,
        DEFAULT_WINDOW_HEIGHT as u32,
        MARGINS,
    );
    let world_rect = match config.world_scale {
        Some(scale) => headless_runner::scale_world_rect(world_rect, scale),
        None => world_rect,
    };
    let store = SaveStore::new(&common.save_dir);
    let application =
        headless_runner::prepare_application(&config, world_rect, &store, common.load)?;
    if config.save_setup {
        let path = application
            .save_world(&store)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        eprintln!("saved world: {}", path.display());
    }
    headless_runner::HeadlessRunner::from_config(config, application)?.run()?;
    Ok(())
}

/// Exit status for an invalid command line (as in sysexits' EX_USAGE family).
const EXIT_USAGE: i32 = 2;

/// Report an invalid command line with the usage, and exit.
fn usage_error(error: &headless_cli::CliError) -> ! {
    eprintln!("{error}\n");
    eprint!("{}", cli::usage_text());
    std::process::exit(EXIT_USAGE);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if cli::wants_help(&args) {
        print!("{}", cli::usage_text());
        return;
    }
    if cli::wants_version(&args) {
        println!("{} {}", cli::PROGRAM, cli::VERSION);
        return;
    }
    let (common, args) = cli::split_common_args(&args).unwrap_or_else(|e| usage_error(&e));
    match headless_cli::parse_headless_args(&args) {
        Ok(Some(config)) => {
            print_diagnostics();
            log::info!("Running in headless mode");
            if let Err(error) = run_headless_renderer(config, common) {
                eprintln!("headless renderer failed: {error}");
                std::process::exit(1);
            }
        }
        Ok(None) => {
            cli::reject_window_args(&args).unwrap_or_else(|e| usage_error(&e));
            print_diagnostics();
            run_wgpu_renderer(common);
        }
        Err(e) => usage_error(&e),
    }
}
