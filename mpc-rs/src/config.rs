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

/// MPC's `eAfterPlayback` (Options > Playback), minus "turn off the monitor".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AfterPlayback {
    DoNothing,
    PlayNext,
    Rewind,
    Close,
    Exit,
}

impl AfterPlayback {
    pub const ALL: [AfterPlayback; 5] = [AfterPlayback::DoNothing, AfterPlayback::PlayNext, AfterPlayback::Rewind, AfterPlayback::Close, AfterPlayback::Exit];
    pub fn label(self) -> &'static str {
        ["Do nothing", "Play next file in the folder", "Rewind current file", "Close", "Exit"][self as usize]
    }
}

/// Window title (MPC's `iTitleBarTextStyle`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TitleBar {
    FullPath,
    FileName,
    NoPrefix,
}

/// Every setting, with MPC's default (`AppSettings.cpp`). Grouped like the Options pages.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // Player
    pub allow_multiple_instances: bool,
    pub title_bar: TitleBar,
    pub title_bar_media_title: bool,
    pub keep_history: bool,
    pub remember_playlist: bool,
    pub remember_file_pos: bool,
    pub remember_window_size: bool,
    pub window_size: (i32, i32),
    pub recent_files_number: usize,
    // Keys: changed command bindings, keyed by action name (see `commands::COMMANDS`).
    pub keys: BTreeMap<String, Binding>,
    // Logo: an image file instead of the built-in logo (empty = built-in).
    pub logo_file: String,
    // Playback
    pub volume: u32,
    pub volume_step: u32,
    pub loops: u32,
    pub repeat_forever: bool,
    pub repeat_mode: RepeatMode,
    pub after_playback: AfterPlayback,
    /// Preferred languages, comma separated (mpv `alang`/`slang`, e.g. "jpn,eng").
    pub audio_langs: String,
    pub subtitle_langs: String,
    pub autoload_audio: bool,
    pub autoload_subtitles: bool,
    // Fullscreen
    pub launch_fullscreen: bool,
    pub hide_fullscreen_controls: bool,
    pub hide_fullscreen_controls_delay_ms: u32,
    pub hide_fullscreen_docked_panels: bool,
    pub exit_fullscreen_at_end: bool,
    // Tweaks
    pub jump_small_ms: u32,
    pub jump_medium_ms: u32,
    pub jump_large_ms: u32,
    pub fast_seek: bool,
    pub show_chapter_marks: bool,
    pub search_in_folder: bool,
    pub time_tooltip: bool,
    pub hide_windowed_mouse_pointer: bool,
    // Misc: color controls
    pub brightness: i32,
    pub contrast: i32,
    pub hue: i32,
    pub saturation: i32,
    // Playlist panel
    pub shuffle: bool,
    pub show_playlist: bool,
    pub hide_playlist_fullscreen: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            allow_multiple_instances: false,
            title_bar: TitleBar::FileName,
            title_bar_media_title: false,
            keep_history: true,
            remember_playlist: true,
            remember_file_pos: false,
            remember_window_size: false,
            window_size: (960, 620),
            recent_files_number: 20,
            keys: BTreeMap::new(),
            logo_file: String::new(),
            volume: 100,
            volume_step: 5,
            loops: 1,
            repeat_forever: false,
            repeat_mode: RepeatMode::Playlist,
            after_playback: AfterPlayback::DoNothing,
            audio_langs: String::new(),
            subtitle_langs: String::new(),
            autoload_audio: true,
            autoload_subtitles: true,
            launch_fullscreen: false,
            hide_fullscreen_controls: true,
            hide_fullscreen_controls_delay_ms: 0,
            hide_fullscreen_docked_panels: true,
            exit_fullscreen_at_end: true,
            jump_small_ms: 1000,
            jump_medium_ms: 5000,
            jump_large_ms: 20000,
            fast_seek: true,
            show_chapter_marks: true,
            search_in_folder: true,
            time_tooltip: true,
            hide_windowed_mouse_pointer: true,
            brightness: 0,
            contrast: 0,
            hue: 0,
            saturation: 0,
            shuffle: false,
            show_playlist: false,
            hide_playlist_fullscreen: false,
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

/// The settings as TOML text (Options > Misc > Export).
pub fn to_text(config: &Config) -> String {
    toml::to_string(config).expect("serializes")
}

pub fn load_history() -> History {
    read("history.toml")
}

pub fn save_history(history: &History) {
    write("history.toml", history);
}
