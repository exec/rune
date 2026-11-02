#![cfg(feature = "terminal")]

use rune::{
    config::{load_config_file, Config},
    tabs::TabManager,
};

#[test]
fn explicit_configuration_is_loaded_clamped_and_saved_to_the_same_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("benchmark.toml");
    let config = Config {
        check_for_updates: false,
        tab_width: 0,
        ..Config::default()
    };
    std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
    let mut tabs = TabManager::new_with_config_file(path.clone()).unwrap();
    assert!(!tabs.config.check_for_updates);
    assert_eq!(tabs.config.tab_width, 1);
    tabs.config.tab_width = 8;
    tabs.save_config();
    assert_eq!(load_config_file(&path).unwrap().tab_width, 8);
}

#[test]
fn explicit_configuration_errors_do_not_fall_back_to_user_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid.toml");
    assert!(TabManager::new_with_config_file(path.clone()).is_err());
    std::fs::write(&path, "not a configuration").unwrap();
    assert!(TabManager::new_with_config_file(path).is_err());
}

#[test]
fn invalid_cli_configuration_fails_before_taking_over_the_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_rune"))
        .arg("--config")
        .arg(dir.path().join("missing.toml"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.windows(2).any(|bytes| bytes == b"\x1b["));
}
