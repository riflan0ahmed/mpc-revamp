//! Settings and history: MPC's defaults, the TOML file format, and older or partial files.

use mpc_rs::config::{AfterPlayback, Config, Favorite, History, RepeatMode, TitleBar, VideoFrame, Zoom, to_text};

#[test]
fn defaults_match_mpc() {
    let c = Config::default();
    assert_eq!((c.volume, c.volume_step, c.loops), (100, 5, 1));
    assert_eq!((c.jump_small_ms, c.jump_medium_ms, c.jump_large_ms), (1000, 5000, 20000));
    assert_eq!((c.recent_files_number, c.keep_history, c.remember_playlist), (20, true, true));
    assert!(!c.remember_file_pos && !c.remember_dvd_pos && !c.repeat_forever && !c.allow_multiple_instances);
    assert_eq!((c.repeat_mode, c.after_playback, c.title_bar), (RepeatMode::Playlist, AfterPlayback::DoNothing, TitleBar::FileName));
    assert_eq!((c.zoom, c.autofit_factor, c.video_frame), (Zoom::Normal, 75, VideoFrame::FromInside));
    assert_eq!((c.sub_delay_step_ms, c.audio_max_norm_factor, c.audio_boost), (500, 400, 0));
    assert_eq!(c.pan_scan_presets.len(), 3);
    assert!(c.keys.is_empty(), "no key is changed by default");
}

#[test]
fn file_roundtrip() {
    let c = Config {
        volume: 42,
        repeat_mode: RepeatMode::File,
        after_playback: AfterPlayback::Exit,
        shaders_pre: vec!["Invert".into()],
        ..Default::default()
    };
    let text = to_text(&c);
    let back: Config = toml::from_str(&text).unwrap();
    assert!(back == c, "settings change in a save/load round trip:\n{text}");
}

#[test]
fn older_and_partial_files_still_load() {
    // A file from an earlier version: only a few settings, the rest come from the defaults.
    let c: Config = toml::from_str("volume = 40\nshuffle = true\n").unwrap();
    assert_eq!((c.volume, c.shuffle), (40, true));
    assert_eq!(c.jump_medium_ms, Config::default().jump_medium_ms);
    // A setting that no longer exists is ignored rather than breaking the whole file.
    let c: Config = toml::from_str("volume = 30\nweb_port = 13579\n").unwrap();
    assert_eq!(c.volume, 30);
}

#[test]
fn recent_files() {
    let mut h = History::default();
    for f in ["a", "b", "c", "a"] {
        h.add_recent(f, 3);
    }
    assert_eq!(h.recent, ["a", "c", "b"], "most recent first, no duplicates");
    h.add_recent("d", 3);
    assert_eq!(h.recent, ["d", "a", "c"], "the oldest drops off past the limit");
}

#[test]
fn positions_and_favorites() {
    let mut h = History::default();
    h.set_position("/m/a.mkv", Some(61.5), 20);
    h.set_position("/m/b.mkv", Some(10.0), 20);
    assert_eq!(h.position("/m/a.mkv"), Some(61.5));
    h.set_position("/m/a.mkv", None, 20); // played to the end: forgotten
    assert_eq!(h.position("/m/a.mkv"), None);
    h.favorites.push(Favorite { name: "Film".into(), path: "/m/b.mkv".into(), position: Some(90.0) });
    let back: History = toml::from_str(&toml::to_string(&h).unwrap()).unwrap();
    assert_eq!(back.favorites, h.favorites);
    assert_eq!(back.position("/m/b.mkv"), Some(10.0));
}
