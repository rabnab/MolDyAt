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
fn shared_options_need_a_value() {
    assert!(split_common_args(&args(&["program", "--load"])).is_err());
    assert!(split_common_args(&args(&["program", "--save-dir"])).is_err());
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
