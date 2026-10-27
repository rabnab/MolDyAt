// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod cli;
mod wgpu;

use thermo_physics::application::persistence::{LoadSource, SaveStore};
use thermo_physics::application::{MainContent, SimConfig};
use thermo_runtime::{headless_cli, headless_runner};

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
    thermo_ui::logging::init();
    log::info!("{} {}", cli::PROGRAM, cli::VERSION);
    log::info!(
        "Parallel: enabled | SIMD: enabled | CPU cores: {}",
        num_cpus::get()
    );
    init_rayon();
}

fn run_wgpu_renderer(
    common: cli::CommonArgs,
    sim_config: &SimConfig,
    key_map: thermo_ui::graphics::renderer::event_loop::KeyMap,
) {
    use wgpu::WgpuWindow;

    log::info!("Renderer: wgpu");

    let store = SaveStore::new(&common.save_dir);
    let mut application = MainContent::with_config(sim_config.world.rect(), sim_config, None)
        .expect("Failed to create MainContent");
    if let Some(path) = common.load {
        // Same entry point as Ctrl+O; the window then shows the loaded world.
        match application.load_world(&store, &LoadSource::Path(path.clone()), true) {
            Ok(loaded) => log::info!("{}", loaded.message(true)),
            Err(e) => {
                eprintln!("cannot load {}: {e:?}", path.display());
                std::process::exit(1);
            }
        }
    }
    let application = application.with_save_store(store);

    let window = WgpuWindow::new();
    window.create_window(application, key_map);
}

fn run_headless_renderer(
    config: headless_cli::HeadlessConfig,
    common: cli::CommonArgs,
    sim_config: &SimConfig,
) -> std::io::Result<()> {
    log::info!("Renderer: headless");

    let store = SaveStore::new(&common.save_dir);
    let application =
        headless_runner::prepare_application(&config, sim_config, &store, common.load)?;
    if config.save_setup {
        let path = application
            .save_world(&store)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        eprintln!("saved world: {}", path.display());
    }
    // `world.save` in a `--script` saves where Ctrl+S would.
    let application = application.with_save_store(store);
    let report =
        headless_runner::HeadlessRunner::from_config(config, application, env!("GIT_COMMIT"))?
            .run()?;
    for path in &report.exported {
        eprintln!("exported: {}", path.display());
    }
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
    let sim_config = common.sim_config().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    match headless_cli::parse_headless_args(&args) {
        Ok(Some(config)) => {
            print_diagnostics();
            log::info!("Running in headless mode");
            if let Err(error) = run_headless_renderer(config, common, &sim_config) {
                eprintln!("headless renderer failed: {error}");
                std::process::exit(1);
            }
        }
        Ok(None) => {
            let window_args = cli::parse_window_args(&args).unwrap_or_else(|e| usage_error(&e));
            print_diagnostics();
            let key_map = thermo_ui::ui_settings::load_key_map(window_args.ui_config.as_deref())
                .unwrap_or_else(|error| {
                    eprintln!("cannot read the UI settings: {error}");
                    std::process::exit(1);
                });
            run_wgpu_renderer(common, &sim_config, key_map);
        }
        Err(e) => usage_error(&e),
    }
}
