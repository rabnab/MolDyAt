pub(crate) mod renderer;
pub(crate) mod window;

#[cfg(feature = "renderer-ggez")]
pub(crate) mod ggez;

// Energy plot window is renderer-agnostic (uses softbuffer)
#[cfg(any(feature = "renderer-ggez", feature = "renderer-wgpu"))]
mod energy_plot;

#[cfg(feature = "renderer-wgpu")]
pub(crate) mod wgpu;

// Compile-time check that at least one renderer is selected
#[cfg(not(any(feature = "renderer-ggez", feature = "renderer-wgpu")))]
compile_error!("At least one renderer feature must be enabled: 'renderer-ggez' or 'renderer-wgpu'");
