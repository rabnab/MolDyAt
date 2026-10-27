// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The renderer uses no winit (#372): `wgpu/render/` draws, `wgpu/shell/`
//! talks to the OS, and only the shell may name winit types. The check
//! looks for the word `winit` in code, after blanking comments and string
//! literals (see [`code_only`]).

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

/// `source` with every comment (`//` and `/* */`, nested) and the contents
/// of every string literal (`"…"` with escapes, raw `r"…"` / `r#"…"#`, and
/// their byte forms) and character literals replaced by spaces, line breaks
/// kept, so a word found in it is code.
fn code_only(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(chars[i]));
                    i += 1;
                }
            }
        } else if c == 'r'
            && matches!(next, Some('"' | '#'))
            && !is_ident(chars.get(i.wrapping_sub(1)))
        {
            // A raw string: r, then n hashes, a quote, and the closing quote
            // followed by n hashes.
            let mut j = i + 1;
            while chars.get(j) == Some(&'#') {
                j += 1;
            }
            if chars.get(j) != Some(&'"') {
                out.push(c);
                i += 1;
                continue;
            }
            let hashes = j - i - 1;
            out.extend(&chars[i..=j]);
            i = j + 1;
            while i < chars.len() {
                let closes =
                    chars[i] == '"' && (0..hashes).all(|k| chars.get(i + 1 + k) == Some(&'#'));
                if closes {
                    out.extend(&chars[i..=i + hashes]);
                    i += 1 + hashes;
                    break;
                }
                out.push(blank(chars[i]));
                i += 1;
            }
        } else if let Some(len) = char_literal_len(&chars[i..]) {
            // A character literal such as '"' or '\'': not a string start.
            out.extend(std::iter::repeat_n(' ', len));
            i += len;
        } else if c == '"' {
            out.push('"');
            i += 1;
            while i < chars.len() {
                match chars[i] {
                    '\\' => {
                        out.push(' ');
                        if let Some(&escaped) = chars.get(i + 1) {
                            out.push(blank(escaped));
                        }
                        i += 2;
                    }
                    '"' => {
                        out.push('"');
                        i += 1;
                        break;
                    }
                    other => {
                        out.push(blank(other));
                        i += 1;
                    }
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// The length of the character literal at the start of `chars` (`'x'`,
/// `'\n'`, `'\u{…}'`), or `None` (a lifetime, or not a quote).
fn char_literal_len(chars: &[char]) -> Option<usize> {
    if chars.first() != Some(&'\'') {
        return None;
    }
    match chars.get(1)? {
        '\\' => chars
            .iter()
            .skip(2)
            .take(10)
            .position(|&c| c == '\'')
            .map(|end| end + 3),
        _ if chars.get(2) == Some(&'\'') => Some(3),
        _ => None,
    }
}

fn is_ident(c: Option<&char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || *c == '_')
}

/// Lines of `source` that use winit in code (not in comments or strings).
fn winit_uses(source: &str) -> Vec<(usize, String)> {
    let code = code_only(source);
    code.lines()
        .zip(source.lines())
        .enumerate()
        .filter(|(_, (code, _))| code.contains("winit"))
        .map(|(i, (_, line))| (i + 1, line.trim().to_string()))
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
fn the_guard_finds_winit_in_code_but_not_in_comments_or_strings() {
    let source = r##"use winit::window::Window;
// winit is fine here
let x = 1; // and winit here
/* a block comment
   naming winit::window, /* nested */ winit */
let s = "winit in a string";
let r = r#"winit in a "raw" string"#;
let b = b"winit";
let e = "an \" escaped quote, then winit";
let y = winit::dpi::LogicalSize::new(1.0, 1.0);
let q = '"'; let z: &'static str = winit::NAME;
"##;
    assert_eq!(
        winit_uses(source),
        [
            (1, "use winit::window::Window;".to_string()),
            (
                10,
                "let y = winit::dpi::LogicalSize::new(1.0, 1.0);".to_string()
            ),
            (
                11,
                "let q = '\"'; let z: &'static str = winit::NAME;".to_string()
            )
        ]
    );
}

#[test]
fn the_guard_keeps_lines_and_code_around_comments() {
    let code = code_only("a /* x */ b\n\"s\" // c\nd");
    assert_eq!(code.lines().count(), 3);
    assert!(code.starts_with("a ") && code.contains(" b"));
    assert!(code.ends_with('d'));
}
