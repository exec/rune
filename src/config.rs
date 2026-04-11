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
