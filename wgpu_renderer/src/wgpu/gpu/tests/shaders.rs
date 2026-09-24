//! Parse and validate every WGSL source (common header included) with naga,
//! so shader errors fail `cargo test` instead of panicking at pipeline creation.

use crate::wgpu::gpu::shaders::ALL;
use wgpu::naga;

fn validate(file: &str, source: &str) -> naga::Module {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("{file}: {}", e.emit_to_string(source)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{file}: {}", e.emit_to_string(source)));
    module
}

#[test]
fn all_shaders_parse_and_validate() {
    for (file, source) in ALL {
        validate(file, source);
    }
}

#[test]
fn all_shaders_use_the_shared_entry_points() {
    for (file, source) in ALL {
        let module = validate(file, source);
        let names: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(names.contains(&"vs_main"), "{file}: missing vs_main");
        assert!(names.contains(&"fs_main"), "{file}: missing fs_main");
    }
}

#[test]
fn every_wgsl_file_is_registered() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/wgpu/shaders");
    for entry in std::fs::read_dir(dir).expect("shader dir") {
        let name = entry.expect("dir entry").file_name();
        let name = name.to_string_lossy();
        if name == "common.wgsl" || !name.ends_with(".wgsl") {
            continue;
        }
        assert!(
            ALL.iter().any(|(file, _)| *file == name),
            "{name} is not registered in gpu/shaders.rs"
        );
    }
}
