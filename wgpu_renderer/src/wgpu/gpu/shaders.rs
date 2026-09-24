//! Every WGSL source, with `common.wgsl` prepended at compile time.
//!
//! WGSL has no `#include`, so the shared header (uniforms `u`, `quad_corner`,
//! `aa`, `ring_band`) is concatenated in front of each shader here. Pipelines
//! take their shader from this module only.

macro_rules! shaders {
    ($($name:ident => $file:literal),* $(,)?) => {
        $(pub const $name: &str = concat!(
            include_str!("../shaders/common.wgsl"),
            "\n",
            include_str!(concat!("../shaders/", $file)),
        );)*

        /// All shaders by file name (for validation tests).
        #[cfg(test)]
        pub const ALL: &[(&str, &str)] = &[$(($file, $name)),*];
    };
}

shaders! {
    BALL => "ball.wgsl",
    ENDPOINT => "endpoint.wgsl",
    FRAME => "frame.wgsl",
    GRID => "grid.wgsl",
    LABEL_CAPSULE => "label_capsule.wgsl",
    LINE => "line.wgsl",
    PLACEMENT_RINGS => "placement_rings.wgsl",
    SIDEBAR => "sidebar.wgsl",
    SOLID_RECT => "solid_rect.wgsl",
    SOURCE_SPHERE => "source_sphere.wgsl",
    TEXT => "text.wgsl",
    THERMO_HEATMAP => "thermo_heatmap.wgsl",
    THERMOSTAT_ICON => "thermostat_icon.wgsl",
    THERMOSTAT_ZONE => "thermostat_zone.wgsl",
    VALVE => "valve.wgsl",
    WALL => "wall.wgsl",
}
