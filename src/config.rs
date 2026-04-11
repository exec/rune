use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Configuration settings for the editor
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub mouse_enabled: bool,
    pub show_line_numbers: bool,
    pub tab_width: usize,
    pub word_wrap: bool,
    pub auto_indent: bool,
    pub show_whitespace: bool,
    pub constant_cursor_position: bool,
    #[serde(default)]
    pub backup_on_save: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mouse_enabled: true,
            show_line_numbers: false,
            tab_width: super::constants::DEFAULT_TAB_WIDTH,
            word_wrap: false,
            auto_indent: true,
            show_whitespace: false,
            constant_cursor_position: false,
            backup_on_save: false,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rune").join("config.toml"))
}

pub fn load_config() -> Config {
    if let Some(path) = config_path() {
        if let Ok(config_str) = fs::read_to_string(path) {
            if let Ok(mut config) = toml::from_str::<Config>(&config_str) {
                config.tab_width = config.tab_width.max(1);
                return config;
            }
        }
    }
    Config::default()
}

pub fn save_config(config: &Config) -> Result<()> {
    if let Some(path) = config_path() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let config_str = toml::to_string(config)?;
        fs::write(path, config_str)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_values() {
        let config = Config::default();
        assert!(config.mouse_enabled);
        assert!(!config.show_line_numbers);
        assert_eq!(config.tab_width, super::super::constants::DEFAULT_TAB_WIDTH);
        assert!(!config.word_wrap);
        assert!(config.auto_indent);
        assert!(!config.show_whitespace);
        assert!(!config.constant_cursor_position);
        assert!(!config.backup_on_save);
    }

    #[test]
    fn test_config_round_trip() {
        let mut config = Config::default();
        config.tab_width = 8;
        config.word_wrap = true;
        config.show_line_numbers = true;
        config.backup_on_save = true;

        let serialized = toml::to_string(&config).expect("serialize");
        let deserialized: Config = toml::from_str(&serialized).expect("deserialize");

        assert_eq!(deserialized.tab_width, 8);
        assert!(deserialized.word_wrap);
        assert!(deserialized.show_line_numbers);
        assert!(deserialized.backup_on_save);
        assert_eq!(deserialized.mouse_enabled, config.mouse_enabled);
        assert_eq!(deserialized.auto_indent, config.auto_indent);
    }
}
