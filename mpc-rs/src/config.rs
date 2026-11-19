//! Settings file: `$XDG_CONFIG_HOME/mpc-rs/config.toml`. Only values that differ from MPC's defaults are stored.

use crate::commands::Binding;
use gtk4::glib;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    /// Changed command bindings, keyed by action name (see `commands::COMMANDS`).
    #[serde(default)]
    pub keys: BTreeMap<String, Binding>,
}

fn path() -> PathBuf {
    glib::user_config_dir().join("mpc-rs").join("config.toml")
}

pub fn load() -> Config {
    let path = path();
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
            eprintln!("{}: {e}", path.display());
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

pub fn save(config: &Config) {
    let path = path();
    let result = std::fs::create_dir_all(path.parent().unwrap())
        .and_then(|_| std::fs::write(&path, toml::to_string(config).expect("config serializes")));
    if let Err(e) = result {
        eprintln!("{}: {e}", path.display());
    }
}
