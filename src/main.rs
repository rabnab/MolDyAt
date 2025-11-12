#![allow(dead_code)]

#[cfg(all(feature = "renderer-ggez", feature = "renderer-wgpu"))]
compile_error!("Enable only one renderer feature: either `renderer-ggez` or `renderer-wgpu`.");

#[cfg(not(any(feature = "renderer-ggez", feature = "renderer-wgpu")))]
compile_error!("No renderer selected. Enable one of: `renderer-ggez` or `renderer-wgpu`.");

mod frame;
mod objects2d;
mod timing_monad;

#[cfg(feature = "renderer-ggez")]
use crate::frame::ggez::ggez_window::GgezWindow;
#[cfg(feature = "renderer-ggez")]
use ggez::GameResult;

use crate::frame::AppResult;
use crate::frame::window::Window;
use std::env;
use std::path;
use std::path::PathBuf;

#[cfg(feature = "renderer-wgpu")]
fn init() {
    println!("Using wgpu renderer");
}

#[cfg(feature = "renderer-ggez")]
fn init(resource_dir: PathBuf, reg_x: f32, reg_y: f32) -> AppResult<GgezWindow> {
    println!("Using ggez renderer");
    Ok(GgezWindow::new(resource_dir, reg_x, reg_y))
}

/// Initialize Rayon thread pool with explicit configuration
fn init_rayon() {
    use rayon::ThreadPoolBuilder;

    let num_cpus = num_cpus::get();
    ThreadPoolBuilder::new()
        .num_threads(num_cpus)
        .thread_name(|i| format!("rayon-worker-{}", i))
        .build_global()
        .expect("Failed to initialize Rayon thread pool");

    println!(
        "✓ Rayon thread pool initialized with {} worker threads",
        num_cpus
    );
}

fn main() -> GameResult {
    // Initialize parallelization and print diagnostics
    println!("✓ Parallel feature: ENABLED (always on)");
    init_rayon();

    println!("✓ SIMD feature: ENABLED (always on)");
    println!("System has {} logical CPU cores", num_cpus::get());
    println!("Note: Add many balls (F5 repeatedly) to see parallel performance benefits\n");

    let resource_dir = if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let mut path = path::PathBuf::from(manifest_dir);
        path.push("resources");
        path
    } else {
        path::PathBuf::from("./resources")
    };

    let window = init(resource_dir, 0.5, 0.5)?;
    window.create_window();
    Ok(())
}
