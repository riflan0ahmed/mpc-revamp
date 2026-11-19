//! Settings file: `$XDG_CONFIG_HOME/mpc-rs/config.toml`, with MPC's defaults (`AppSettings.cpp`).
//! History (recent files, positions) lives next to it in `history.toml`, and the last playlist in
//! `default.mpcpl` (same name as MPC's).

use crate::commands::Binding;
use gtk4::glib;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RepeatMode {
    File,
    Playlist,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Changed command bindings, keyed by action name (see `commands::COMMANDS`).
    pub keys: BTreeMap<String, Binding>,
    pub repeat_forever: bool,
    pub repeat_mode: RepeatMode,
    pub shuffle: bool,
    pub show_playlist: bool,
    pub hide_playlist_fullscreen: bool,
    pub remember_playlist: bool,
    pub remember_file_pos: bool,
    pub recent_files_number: usize,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            keys: BTreeMap::new(),
            repeat_forever: false,
            repeat_mode: RepeatMode::Playlist,
            shuffle: false,
            show_playlist: false,
            hide_playlist_fullscreen: false,
            remember_playlist: true,
            remember_file_pos: false,
            recent_files_number: 20,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    /// Most recent first.
    pub recent: Vec<String>,
    /// Last position (seconds) of unfinished files, most recent first.
    pub positions: Vec<(String, f64)>,
}

impl History {
    pub fn add_recent(&mut self, path: &str, max: usize) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(max);
    }

    pub fn position(&self, path: &str) -> Option<f64> {
        self.positions.iter().find(|(p, _)| p == path).map(|(_, t)| *t)
    }

    /// `None` forgets the file (it was played to the end).
    pub fn set_position(&mut self, path: &str, pos: Option<f64>, max: usize) {
        self.positions.retain(|(p, _)| p != path);
        if let Some(t) = pos {
            self.positions.insert(0, (path.to_string(), t));
            self.positions.truncate(max);
        }
    }
}

pub fn dir() -> PathBuf {
    glib::user_config_dir().join("mpc-rs")
}

fn read<T: DeserializeOwned + Default>(name: &str) -> T {
    let path = dir().join(name);
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
            eprintln!("{}: {e}", path.display());
            T::default()
        }),
        Err(_) => T::default(),
    }
}

fn write<T: Serialize>(name: &str, value: &T) {
    let path = dir().join(name);
    let result = std::fs::create_dir_all(dir()).and_then(|_| std::fs::write(&path, toml::to_string(value).expect("serializes")));
    if let Err(e) = result {
        eprintln!("{}: {e}", path.display());
    }
}

pub fn load() -> Config {
    read("config.toml")
}

pub fn save(config: &Config) {
    write("config.toml", config);
}

pub fn load_history() -> History {
    read("history.toml")
}

pub fn save_history(history: &History) {
    write("history.toml", history);
}
