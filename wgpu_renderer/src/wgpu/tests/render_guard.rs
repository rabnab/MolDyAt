// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The renderer uses no winit (#372): `wgpu/render/` draws, `wgpu/shell/`
//! talks to the OS, and only the shell may name winit types.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read the render module") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

/// Lines of `source` that use winit, outside `//` comments.
fn winit_uses(source: &str) -> Vec<(usize, String)> {
    source
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let code = line.split("//").next().unwrap_or_default();
            code.contains("winit")
                .then(|| (i + 1, line.trim().to_string()))
        })
        .collect()
}

#[test]
fn the_renderer_module_does_not_use_winit() {
    let render = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/wgpu/render");
    let mut files = Vec::new();
    rust_files(&render, &mut files);
    assert!(files.len() > 20, "found the render module: {files:?}");
    let offenders: Vec<String> = files
        .iter()
        .flat_map(|file| {
            let source = std::fs::read_to_string(file).expect("read a source file");
            winit_uses(&source)
                .into_iter()
                .map(move |(line, text)| format!("{}:{line}: {text}", file.display()))
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "wgpu/render/ must not use winit; the shell (wgpu/shell/) talks to the \
         OS and hands the renderer a `RenderWindow` (#372):\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_guard_finds_winit_in_code_but_not_in_comments() {
    let source =
        "use winit::window::Window;\n// winit is fine here\nlet x = 1; // and winit here\n";
    assert_eq!(
        winit_uses(source),
        [(1, "use winit::window::Window;".to_string())]
    );
}
