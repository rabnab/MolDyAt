// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn defaults_without_shared_options() {
    let (common, rest) = split_common_args(&args(&["program", "--headless"])).unwrap();
    assert_eq!(common, CommonArgs::default());
    assert_eq!(common.save_dir, PathBuf::from(DEFAULT_SAVE_DIR));
    assert_eq!(rest, args(&["program", "--headless"]));
}

#[test]
fn takes_load_and_save_dir_and_keeps_the_rest_in_order() {
    let (common, rest) = split_common_args(&args(&[
        "program",
        "--headless",
        "--load",
        "saves/a.json",
        "--steps",
        "10",
        "--save-dir",
        "/tmp/worlds",
    ]))
    .unwrap();
    assert_eq!(common.load, Some(PathBuf::from("saves/a.json")));
    assert_eq!(common.save_dir, PathBuf::from("/tmp/worlds"));
    assert_eq!(rest, args(&["program", "--headless", "--steps", "10"]));
}

#[test]
fn takes_config_in_both_modes() {
    let (common, rest) =
        split_common_args(&args(&["program", "--config", "run.toml", "--headless"])).unwrap();
    assert_eq!(common.config, Some(PathBuf::from("run.toml")));
    assert_eq!(rest, args(&["program", "--headless"]));
}

#[test]
fn takes_world_scale_in_both_modes() {
    let (common, rest) =
        split_common_args(&args(&["program", "--world-scale", "2", "--headless"])).unwrap();
    assert_eq!(common.world_scale, Some(2.0));
    assert_eq!(rest, args(&["program", "--headless"]));
    let (common, rest) = split_common_args(&args(&["program", "--world-scale", "0.5"])).unwrap();
    assert_eq!(common.world_scale, Some(0.5));
    assert_eq!(rest, args(&["program"]));
}

#[test]
fn world_scale_rejects_bad_values() {
    for bad in ["0", "-2", "0.01", "101", "nan", "inf", "x"] {
        let result = split_common_args(&args(&["program", "--world-scale", bad]));
        assert!(result.is_err(), "{bad} must be rejected");
    }
    assert!(split_common_args(&args(&["program", "--world-scale"])).is_err());
}

#[test]
fn config_and_load_cannot_be_combined() {
    let error = split_common_args(&args(&[
        "program",
        "--config",
        "run.toml",
        "--load",
        "saves/a.json",
    ]))
    .unwrap_err();
    assert!(error.message.contains("--config"), "{}", error.message);
    assert!(error.message.contains("--load"), "{}", error.message);
}

#[test]
fn world_scale_and_load_cannot_be_combined() {
    let error = split_common_args(&args(&[
        "program",
        "--world-scale",
        "2",
        "--load",
        "saves/a.json",
    ]))
    .unwrap_err();
    assert!(error.message.contains("--world-scale"), "{}", error.message);
    assert!(error.message.contains("--load"), "{}", error.message);
}

#[test]
fn world_scale_sets_the_config() {
    let (common, _) = split_common_args(&args(&["program", "--world-scale", "2"])).unwrap();
    let config = common.sim_config().unwrap();
    assert_eq!(config.world.scale, 2.0);
    assert_eq!(config.world.rect().w, 224.0);
}

#[test]
fn world_scale_overrides_the_config_file() {
    let dir = std::env::temp_dir().join(format!("moldyat_cli_world_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("world.toml");
    std::fs::write(&path, "[world]\nscale = 4.0\n").unwrap();
    let from_file = CommonArgs {
        config: Some(path.clone()),
        ..CommonArgs::default()
    };
    assert_eq!(from_file.sim_config().unwrap().world.scale, 4.0);
    let overridden = CommonArgs {
        world_scale: Some(0.5),
        ..from_file
    };
    assert_eq!(overridden.sim_config().unwrap().world.scale, 0.5);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sim_config_defaults_without_a_file() {
    assert_eq!(
        CommonArgs::default().sim_config().unwrap(),
        SimConfig::default()
    );
}

#[test]
fn sim_config_reads_the_file_and_reports_errors_with_its_path() {
    let dir = std::env::temp_dir().join(format!("moldyat_cli_config_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let good = dir.join("good.toml");
    std::fs::write(&good, "[integration]\ntime_step = 0.0025\n").unwrap();
    let bad = dir.join("bad.toml");
    std::fs::write(&bad, "[integration]\ntime_stp = 0.005\n").unwrap();

    let common = |path: &PathBuf| CommonArgs {
        config: Some(path.clone()),
        ..CommonArgs::default()
    };
    let config = common(&good).sim_config().unwrap();
    assert_eq!(config.integration.time_step, 0.0025);
    let error = common(&bad).sim_config().unwrap_err();
    assert!(error.message.contains("bad.toml"), "{}", error.message);
    assert!(error.message.contains("time_stp"), "{}", error.message);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shared_options_need_a_value() {
    assert!(split_common_args(&args(&["program", "--load"])).is_err());
    assert!(split_common_args(&args(&["program", "--save-dir"])).is_err());
    assert!(split_common_args(&args(&["program", "--config"])).is_err());
    assert!(split_common_args(&args(&["program", "--load", "--headless"])).is_err());
}

#[test]
fn version_flag_is_recognised_in_any_position() {
    assert!(wants_version(&args(&["program", "--version"])));
    assert!(wants_version(&args(&["program", "-V"])));
    assert!(wants_version(&args(&[
        "program",
        "--headless",
        "--version"
    ])));
    assert!(!wants_version(&args(&["program", "--headless"])));
    // The program name itself is never taken as the flag.
    assert!(!wants_version(&args(&["--version"])));
}

#[test]
fn version_string_starts_with_the_package_version() {
    let prefix = format!("{} (", env!("CARGO_PKG_VERSION"));
    assert!(VERSION.starts_with(&prefix), "{VERSION}");
    assert!(VERSION.ends_with(')'), "{VERSION}");
}

#[test]
fn help_flag_is_recognised_in_any_position() {
    assert!(wants_help(&args(&["program", "--help"])));
    assert!(wants_help(&args(&["program", "--headless", "-h"])));
    assert!(!wants_help(&args(&["program", "--headless"])));
    assert!(!wants_help(&args(&["--help"])));
}

#[test]
fn window_mode_rejects_leftover_arguments() {
    assert_eq!(
        parse_window_args(&args(&["program"])).unwrap(),
        WindowArgs::default()
    );
    let error = parse_window_args(&args(&["program", "--steps", "5"])).unwrap_err();
    assert!(error.message.contains("--steps"), "{}", error.message);
    assert!(error.message.contains("--headless"), "{}", error.message);
}

#[test]
fn window_mode_takes_a_ui_config() {
    let window = parse_window_args(&args(&["program", "--ui-config", "keys.toml"])).unwrap();
    assert_eq!(window.ui_config, Some(PathBuf::from("keys.toml")));
    assert!(parse_window_args(&args(&["program", "--ui-config"])).is_err());
}

#[test]
fn usage_lists_every_option_group() {
    let text = usage_text();
    assert!(
        text.starts_with(&format!("{PROGRAM} {VERSION}\n")),
        "{text}"
    );
    for option in GENERAL_OPTIONS
        .iter()
        .chain(COMMON_OPTIONS)
        .chain(WINDOW_OPTIONS)
        .chain(HEADLESS_OPTIONS)
    {
        for name in option.names {
            assert!(text.contains(name), "{name} missing from usage:\n{text}");
        }
    }
    // A long option column moves its help to the next line, indented.
    assert!(
        text.contains("  --valve <wall,t0,t1[,open|closed]>\n"),
        "{text}"
    );
}

/// Every flag `cli.rs` matches on (`"--flag" =>` arms) must have a help entry.
/// The headless flags are checked in `thermo_runtime` (`headless_cli/tests.rs`).
#[test]
fn every_parsed_flag_is_documented() {
    let sources = [include_str!("../cli.rs")];
    let documented: Vec<&str> = GENERAL_OPTIONS
        .iter()
        .chain(COMMON_OPTIONS)
        .chain(WINDOW_OPTIONS)
        .flat_map(|option| option.names.iter().copied())
        .collect();
    let mut parsed = 0;
    for source in sources {
        for line in source.lines() {
            let line = line.trim_start();
            let Some(rest) = line.strip_prefix("\"--") else {
                continue;
            };
            let Some((flag, tail)) = rest.split_once('"') else {
                continue;
            };
            if !tail.trim_start().starts_with("=>") {
                continue;
            }
            let flag = format!("--{flag}");
            parsed += 1;
            assert!(
                documented.contains(&flag.as_str()),
                "{flag} is parsed but has no OptionHelp entry"
            );
        }
    }
    assert!(
        parsed >= 3,
        "found only {parsed} parsed flags; did the parser layout change?"
    );
}
