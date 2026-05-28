use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub global: GlobalConfig,
    pub states: HashMap<String, State>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GlobalConfig {
    pub anchor: Vec<String>, // "top", "bottom", "left", "right"
    pub margin_top: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub margin_right: i32,
    pub monitor: Option<String>,
    pub icon_path: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct State {
    pub width: i32,
    pub height: i32,
    pub opacity: f64,
    pub margin_top: i32,
    pub border_radius: i32,
    pub duration_ms: u64,
    pub wait_ms: u64,
    pub easing: String, // "linear", "ease-in", "ease-out", "ease-in-out"
    pub next_state: Option<String>,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}
