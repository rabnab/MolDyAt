// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The `--help` text, built from option tables.
//!
//! Each parser owns the table for the flags it parses (`GENERAL_OPTIONS`,
//! `COMMON_OPTIONS` in `cli.rs`, `HEADLESS_OPTIONS` in `headless_cli.rs`). To
//! add a flag, parse it and add one [`OptionHelp`] to its parser's table; the
//! test `every_parsed_flag_is_documented` fails if a flag is parsed but missing.

/// One option in the help text.
pub struct OptionHelp {
    /// Spellings, e.g. `&["-h", "--help"]`.
    pub names: &'static [&'static str],
    /// Placeholder for the value, e.g. `Some("<N>")`; `None` for a switch.
    pub value: Option<&'static str>,
    /// One line per entry; later lines are indented under the first.
    pub help: &'static [&'static str],
}

/// A titled block of options.
pub struct OptionGroup {
    pub title: &'static str,
    pub options: &'static [OptionHelp],
}

/// Help lines start in this column; longer option columns put the help on the
/// next line.
const HELP_COLUMN: usize = 30;

/// The full usage text for `program`, with the version line on top.
pub fn usage(program: &str, version: &str, groups: &[OptionGroup]) -> String {
    let mut out =
        format!("{program} {version}\nMolDyAt: molecular dynamics (Lennard-Jones) simulation.\n\n");
    out.push_str("Usage:\n");
    out.push_str(&format!(
        "  {program} [OPTIONS]               open the simulation window\n"
    ));
    out.push_str(&format!(
        "  {program} --headless [OPTIONS]    run without a window\n"
    ));
    for group in groups {
        out.push_str(&format!("\n{}:\n", group.title));
        for option in group.options {
            push_option(&mut out, option);
        }
    }
    out
}

fn push_option(out: &mut String, option: &OptionHelp) {
    let mut column = format!("  {}", option.names.join(", "));
    if let Some(value) = option.value {
        column.push(' ');
        column.push_str(value);
    }
    let indent = " ".repeat(HELP_COLUMN);
    let mut lines = option.help.iter();
    match lines.next() {
        Some(first) if column.len() < HELP_COLUMN => {
            out.push_str(&format!("{column:<HELP_COLUMN$}{first}\n"));
        }
        Some(first) => out.push_str(&format!("{column}\n{indent}{first}\n")),
        None => out.push_str(&format!("{column}\n")),
    }
    for line in lines {
        out.push_str(&format!("{indent}{line}\n"));
    }
}
