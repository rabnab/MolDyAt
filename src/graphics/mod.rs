pub(crate) mod renderer;
pub(crate) mod window;

// Energy plot window is renderer-agnostic (uses softbuffer)
#[cfg(feature = "renderer-wgpu")]
mod energy_plot;

#[cfg(feature = "renderer-wgpu")]
pub(crate) mod wgpu;
