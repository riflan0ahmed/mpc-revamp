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

/// View > Zoom, and Options > Playback "Default zoom" (MPC's `iZoomLevel`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Zoom {
    Half,
    Normal,
    Double,
    AutoFit,
    AutoFitLarger,
}

impl Zoom {
    pub const ALL: [Zoom; 5] = [Zoom::Half, Zoom::Normal, Zoom::Double, Zoom::AutoFit, Zoom::AutoFitLarger];
    pub fn label(self) -> &'static str {
        ["50%", "100%", "200%", "Auto Fit", "Auto Fit (Larger Only)"][self as usize]
    }
}

/// View > Video Frame (MPC's `iDefaultVideoSize`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum VideoFrame {
    Half,
    Normal,
    Double,
    Stretch,
    FromInside,
    Zoom1,
    Zoom2,
    FromOutside,
}

/// A pan & scan preset: name, position (0..1, 0.5 = centered) and zoom per axis.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct PanScanPreset {
    pub name: String,
    pub pos_x: f64,
    pub pos_y: f64,
    pub zoom_x: f64,
    pub zoom_y: f64,
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
    pub remember_dvd_pos: bool,
    pub remember_window_size: bool,
    pub window_size: (i32, i32),
    pub recent_files_number: usize,
    pub show_osd: bool,
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
    /// Rate change per step in %; 0 = double/halve (MPC's "auto").
    pub speed_step: u32,
    pub pitch_correction: bool,
    /// Resize the window to `zoom` when a file opens ("Auto-zoom"), fitting `autofit_factor` % of the screen.
    pub auto_zoom: bool,
    pub zoom: Zoom,
    pub autofit_factor: u32,
    pub video_frame: VideoFrame,
    pub pan_scan_presets: Vec<PanScanPreset>,
    // Output
    pub hwdec: bool,
    pub color_management: bool,
    /// ICC rendering intent: 0 perceptual, 1 relative colorimetric, 2 saturation, 3 absolute colorimetric.
    pub icc_intent: u32,
    /// Output levels: false = 0-255 (full), true = 16-235 (limited).
    pub output_limited: bool,
    // Sync
    pub sync_to_display: bool,
    pub interpolation: bool,
    // Shaders, by name (see `shaders::available`)
    pub shaders_pre: Vec<String>,
    pub shaders_post: Vec<String>,
    // Screenshots and thumbnails
    pub snapshot_dir: String,
    pub snapshot_ext: String,
    pub thumb_rows: u32,
    pub thumb_cols: u32,
    pub thumb_width: u32,
    // Audio (MPC's audio switcher page)
    pub audio_normalize: bool,
    pub audio_max_norm_factor: u32,
    /// Boost in % (0..300), on top of the volume.
    pub audio_boost: u32,
    pub audio_time_shift_ms: i32,
    pub downsample_44100: bool,
    pub downmix_stereo: bool,
    /// Headphone crossfeed (bs2b).
    pub crossfeed: bool,
    // Subtitles
    pub sub_override_placement: bool,
    /// Vertical position in % of the height (100 = bottom).
    pub sub_pos_v: u32,
    pub sub_delay_step_ms: u32,
    // Subtitle style (mpv's units: font size on a 720-line screen). Colors are "#RRGGBBAA".
    pub sub_override_ass: bool,
    pub sub_font: String,
    pub sub_font_size: u32,
    pub sub_bold: bool,
    pub sub_italic: bool,
    pub sub_color: String,
    pub sub_border_color: String,
    pub sub_border_size: f64,
    pub sub_shadow_offset: f64,
    pub sub_shadow_color: String,
    pub sub_opaque_box: bool,
    pub sub_back_color: String,
    pub sub_margin_y: u32,
    pub show_subresync: bool,
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
    pub osd_font: String,
    pub osd_font_size: u32,
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
            remember_dvd_pos: false,
            remember_window_size: false,
            window_size: (960, 620),
            recent_files_number: 20,
            show_osd: true,
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
            speed_step: 0,
            pitch_correction: true,
            auto_zoom: true,
            zoom: Zoom::Normal,
            autofit_factor: 75,
            video_frame: VideoFrame::FromInside,
            pan_scan_presets: {
                let p = |name: &str, zx: f64, zy: f64| PanScanPreset { name: name.into(), pos_x: 0.5, pos_y: 0.5, zoom_x: zx, zoom_y: zy };
                let (r43, r169, r235) = (4.0 / 3.0, 16.0 / 9.0, 2.35);
                vec![p("Scale to 16:9 TV", 1.0, r169 / r43), p("Zoom to Widescreen", r169 / r43, r169 / r43), p("Zoom to Ultra-Widescreen", r235 / r43, r235 / r43)]
            },
            hwdec: false,
            color_management: false,
            icc_intent: 1,
            output_limited: false,
            sync_to_display: false,
            interpolation: false,
            shaders_pre: Vec::new(),
            shaders_post: Vec::new(),
            snapshot_dir: glib::user_special_dir(glib::UserDirectory::Pictures).unwrap_or_else(glib::home_dir).to_string_lossy().into_owned(),
            snapshot_ext: "jpg".into(),
            thumb_rows: 4,
            thumb_cols: 4,
            thumb_width: 1024,
            audio_normalize: false,
            audio_max_norm_factor: 400,
            audio_boost: 0,
            audio_time_shift_ms: 0,
            downsample_44100: false,
            downmix_stereo: false,
            crossfeed: false,
            sub_override_placement: false,
            sub_pos_v: 90,
            sub_delay_step_ms: 500,
            sub_override_ass: false,
            sub_font: "sans-serif".into(),
            sub_font_size: 38,
            sub_bold: false,
            sub_italic: false,
            sub_color: "#ffffffff".into(),
            sub_border_color: "#000000ff".into(),
            sub_border_size: 3.0,
            sub_shadow_offset: 0.0,
            sub_shadow_color: "#000000f0".into(),
            sub_opaque_box: false,
            sub_back_color: "#00000080".into(),
            sub_margin_y: 22,
            show_subresync: false,
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
            osd_font: "sans-serif".into(),
            osd_font_size: 30,
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

/// A Favorites menu entry: a file, disc or device, optionally with a start position.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Favorite {
    pub name: String,
    pub path: String,
    pub position: Option<f64>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    /// Most recent first.
    pub recent: Vec<String>,
    /// Last position (seconds) of unfinished files, most recent first.
    pub positions: Vec<(String, f64)>,
    /// For discs: the title that was playing (with its position in `positions`).
    pub disc_titles: Vec<(String, i64)>,
    pub favorites: Vec<Favorite>,
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
