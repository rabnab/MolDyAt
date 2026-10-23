// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::OptionHelp;

/// `--help` entries for every flag `parse_headless_args` takes. Keep in sync:
/// `every_parsed_flag_is_documented` checks it.
pub const HEADLESS_OPTIONS: &[OptionHelp] = &[
    OptionHelp {
        names: &["--headless"],
        value: None,
        help: &["Run without a window; enables the options below"],
    },
    OptionHelp {
        names: &["--steps"],
        value: Some("<N>"),
        help: &["Physics steps to run (default 1)"],
    },
    OptionHelp {
        names: &["--snapshot-every"],
        value: Some("<N>"),
        help: &["Write a frame every N steps (default 0: first and last only)"],
    },
    OptionHelp {
        names: &["--output"],
        value: Some("<PATH>"),
        help: &["Write JSONL records to this file"],
    },
    OptionHelp {
        names: &["--format"],
        value: Some("<jsonl>"),
        help: &["Output format (default jsonl)"],
    },
    OptionHelp {
        names: &["--detail"],
        value: Some("<LEVEL>"),
        help: &["summary (default), balls, thermo or balls+thermo"],
    },
    OptionHelp {
        names: &["--stdout"],
        value: None,
        help: &["Print a summary line per frame (default)"],
    },
    OptionHelp {
        names: &["--no-stdout"],
        value: None,
        help: &["No console output; needs --output"],
    },
    OptionHelp {
        names: &["--rng-seed"],
        value: Some("<N>"),
        help: &["Seed for reproducible runs"],
    },
    OptionHelp {
        names: &["--seed-balls"],
        value: Some("<N>"),
        help: &["Place up to N random balls before the first step"],
    },
    OptionHelp {
        names: &["--seed-speed"],
        value: Some("<V>"),
        help: &["Seeded balls get speeds in 0..V (default 0: at rest)"],
    },
    OptionHelp {
        names: &["--seed-species"],
        value: Some("<NAME[:W],...>"),
        help: &[
            "Seed a mixture: species of the run's table with weights W",
            "(default 1), e.g. N2:3,Ar:1; needs --seed-balls",
        ],
    },
    OptionHelp {
        names: &["--dt"],
        value: Some("<SECONDS>"),
        help: &["Time step, 0.0003125..=0.08 (default: the simulation default)"],
    },
    OptionHelp {
        names: &["--line-segment"],
        value: Some("<x1,y1,x2,y2[,t]>"),
        help: &[
            "Add a wall in world coordinates; thickness t 1..20 (default 1)",
            "Repeatable; walls are placed before --seed-balls",
        ],
    },
    OptionHelp {
        names: &["--valve"],
        value: Some("<wall,t0,t1[,open|closed]>"),
        help: &[
            "Cut a valve into --line-segment number `wall` (0-based) over",
            "fractions t0..t1 of its length; one per wall, default open",
        ],
    },
    OptionHelp {
        names: &["--save-setup"],
        value: None,
        help: &["Save the prepared world to --save-dir before the first step"],
    },
    OptionHelp {
        names: &["--timings"],
        value: Some("<PATH>"),
        help: &["Write ms/step per phase as JSON (see docs/perf-baseline.md)"],
    },
    OptionHelp {
        names: &["--timing-warmup"],
        value: Some("<N>"),
        help: &["Steps before --timings starts measuring (default 0)"],
    },
];
