//! Main window: menu bar, video, playlist, seek bar, toolbar, info/statistics/status bars.
//! Layout and default keys follow MPC-HC (`MainFrm.cpp`, `default_wmcmds` in `AppSettings.cpp`).

use crate::{
    cmdline::CmdLine,
    commands::{self, Binding, COMMANDS, Click, Media, Mouse},
    config::{self, AfterPlayback, Config, History, RepeatMode, TitleBar},
    mpris::Mpris,
    options,
    playlist::{self, Entry, Playlist},
    playlistbar::{self, Bar, Request},
    subresync::{self, Subresync},
    video,
};
use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use libmpv2::{GetData, Mpv};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};

const SUB_EXTS: &[&str] = &["srt", "ass", "ssa", "sub", "idx", "sup", "vtt", "smi", "usf"];

/// Bars that the View menu toggles, in top-to-bottom order: (action name, label, shown in Normal preset).
const BARS: &[(&str, &str, bool)] = &[
    ("view-captionmenu", "Caption&&Menu", true),
    ("view-information", "Information", false),
    ("view-statistics", "Statistics", false),
    ("view-seeker", "Seek Bar", true),
    ("view-controls", "Controls", true),
    ("view-status", "Status", true),
];

struct Ui {
    app: gtk::Application,
    win: gtk::ApplicationWindow,
    mpv: &'static Mpv,
    bindings: Rc<RefCell<Vec<Binding>>>,
    mpris: Mpris,
    video: gtk::GLArea,
    menu: gtk::PopoverMenu,
    pointer: Cell<(f64, f64)>, // last click on the video, where the context menu opens
    logo: gtk::Picture,
    bars: Vec<(&'static str, gtk::Widget)>,
    seek: gtk::Scale,
    volume: gtk::Scale,
    mute: gtk::ToggleButton,
    info: gtk::Label,
    stats: gtk::Label,
    state: gtk::Label,
    time: gtk::Label,
    stopped: Cell<bool>,
    fs_reveal: Cell<bool>,
    last_motion: Cell<Instant>,
    last_user_seek: Cell<Instant>,
    marks: Cell<(i64, i64)>, // (chapter count, duration ms) the seek bar marks were built for
    cfg: RefCell<Config>,
    history: RefCell<History>,
    pl: Rc<RefCell<Playlist>>,
    plbar: Rc<Bar>,
    recent: gio::Menu,
    /// The file mpv was last asked to load (for its saved position) and whether its end was handled.
    loaded: RefCell<Option<String>>,
    eof_handled: Cell<bool>,
    /// "Play N times": how often the current file / the playlist has been repeated so far.
    file_loops: Cell<u32>,
    playlist_loops: Cell<u32>,
    /// One-shot options for the next file, from the command line (/start, /open).
    start_at: Cell<Option<f64>>,
    start_paused: Cell<bool>,
    /// /close or /playnext override the After Playback setting for this run.
    after_override: Cell<Option<AfterPlayback>>,
    /// Fullscreen: when the pointer was last near the bottom edge (controls stay up for the hide delay).
    fs_zone_at: Cell<Option<Instant>>,
    subresync: Rc<Subresync>,
    /// Play > Audio / Subtitle / Video Track, rebuilt when the track list changes.
    track_menus: [gio::Menu; 3],
    tracks_sig: RefCell<String>,
}

const TRACK_KINDS: [(&str, &str, &str); 3] = [("audio", "aid", "audio-track"), ("sub", "sid", "sub-track"), ("video", "vid", "video-track")];

/// mpv's OSD line, as MPC shows rate and delay changes on its OSD.
fn osd(mpv: &Mpv, text: &str) {
    cmd(mpv, "show-text", &[text, "1500"]);
}

/// Hex "#RRGGBBAA" (our config, like CSS) to mpv's "#AARRGGBB".
fn mpv_color(c: &str) -> String {
    let h = c.trim_start_matches('#');
    if h.len() == 8 { format!("#{}{}", &h[6..], &h[..6]) } else { format!("#{h}") }
}

/// The audio filter chain and output options from Options > Audio.
fn apply_audio(ui: &Ui) {
    let cfg = ui.cfg.borrow();
    let mut af = Vec::new();
    if cfg.audio_normalize {
        af.push(format!("lavfi=[dynaudnorm=m={:.2}]", (cfg.audio_max_norm_factor as f64 / 100.0).clamp(1.0, 100.0)));
    }
    if cfg.crossfeed {
        af.push("lavfi=[bs2b]".to_string());
    }
    let gain = 20.0 * (1.0 + cfg.audio_boost as f64 / 100.0).log10();
    for (name, value) in [
        ("af", af.join(",")),
        ("volume-gain", format!("{gain:.2}")),
        ("audio-channels", if cfg.downmix_stereo { "stereo" } else { "auto-safe" }.into()),
        ("audio-samplerate", if cfg.downsample_44100 { "44100" } else { "0" }.into()),
        ("audio-pitch-correction", if cfg.pitch_correction { "yes" } else { "no" }.into()),
    ] {
        if let Err(e) = ui.mpv.set_property(name, value.as_str()) {
            eprintln!("mpv {name}={value}: {e}");
        }
    }
}

/// Options > Subtitles and Subtitle style, as mpv options.
fn apply_sub_style(ui: &Ui) {
    let c = ui.cfg.borrow();
    let yes = |b: bool| if b { "yes" } else { "no" }.to_string();
    for (name, value) in [
        ("sub-font", c.sub_font.clone()),
        ("sub-font-size", c.sub_font_size.to_string()),
        ("sub-bold", yes(c.sub_bold)),
        ("sub-italic", yes(c.sub_italic)),
        ("sub-color", mpv_color(&c.sub_color)),
        ("sub-outline-color", mpv_color(&c.sub_border_color)),
        ("sub-outline-size", c.sub_border_size.to_string()),
        ("sub-shadow-offset", c.sub_shadow_offset.to_string()),
        ("sub-shadow-color", mpv_color(&c.sub_shadow_color)),
        ("sub-border-style", if c.sub_opaque_box { "opaque-box" } else { "outline-and-shadow" }.into()),
        ("sub-back-color", mpv_color(&c.sub_back_color)),
        ("sub-margin-y", c.sub_margin_y.to_string()),
        ("sub-ass-override", if c.sub_override_ass { "force" } else { "scale" }.into()),
        ("sub-pos", if c.sub_override_placement { c.sub_pos_v.min(150) } else { 100 }.to_string()),
    ] {
        if let Err(e) = ui.mpv.set_property(name, value.as_str()) {
            eprintln!("mpv {name}={value}: {e}");
        }
    }
}

/// MPC's next/previous audio or subtitle stream: steps through the tracks of that type, wrapping around
/// (unlike mpv's `cycle`, never to "no track"; subtitles are turned off with On/Off instead).
fn cycle_track(mpv: &Mpv, kind: &str, property: &str, dir: i64) {
    let n = prop::<i64>(mpv, "track-list/count").unwrap_or(0);
    let ids: Vec<i64> = (0..n)
        .filter(|i| prop::<String>(mpv, &format!("track-list/{i}/type")).as_deref() == Some(kind))
        .filter_map(|i| prop::<i64>(mpv, &format!("track-list/{i}/id")))
        .collect();
    if ids.is_empty() {
        return;
    }
    let cur = prop::<String>(mpv, property).and_then(|s| s.parse::<i64>().ok());
    let pos = cur.and_then(|c| ids.iter().position(|&i| i == c)).map_or(0, |p| (p as i64 + dir).rem_euclid(ids.len() as i64) as usize);
    let _ = mpv.set_property(property, ids[pos]);
    if let Some(title) = prop::<String>(mpv, &format!("current-tracks/{kind}/title")).or_else(|| prop::<String>(mpv, &format!("current-tracks/{kind}/lang"))) {
        osd(mpv, &format!("{}: {title}", if kind == "audio" { "Audio" } else { "Subtitle" }));
    }
}

/// Rebuild the track submenus when mpv's track list changes, and keep their radio marks current.
fn update_tracks(ui: &Ui) {
    let n = prop::<i64>(ui.mpv, "track-list/count").unwrap_or(0);
    let mut tracks: Vec<(String, i64, String)> = Vec::new(); // (type, id, label)
    for i in 0..n {
        let get = |k: &str| prop::<String>(ui.mpv, &format!("track-list/{i}/{k}"));
        let (Some(kind), Some(id)) = (get("type"), prop::<i64>(ui.mpv, &format!("track-list/{i}/id"))) else { continue };
        let mut label = get("title").unwrap_or_else(|| format!("Track {id}"));
        if let Some(lang) = get("lang") {
            label += &format!(" [{lang}]");
        }
        if let Some(codec) = get("codec") {
            label += &format!(" ({codec})");
        }
        if prop::<bool>(ui.mpv, &format!("track-list/{i}/external")).unwrap_or(false) {
            label += " (external)";
        }
        tracks.push((kind, id, label));
    }
    let sig = format!("{tracks:?}");
    if *ui.tracks_sig.borrow() != sig {
        for ((kind, _, action), menu) in TRACK_KINDS.iter().zip(&ui.track_menus) {
            menu.remove_all();
            let list = gio::Menu::new();
            for (_, id, label) in tracks.iter().filter(|t| t.0 == *kind) {
                let item = gio::MenuItem::new(Some(&label.replace('_', "__")), None);
                item.set_action_and_target_value(Some(&format!("win.{action}")), Some(&id.to_string().to_variant()));
                list.append_item(&item);
            }
            menu.append_section(None, &list);
            if *kind == "sub" {
                let extra = gio::Menu::new();
                extra.append(Some("_Enabled"), Some("win.stream-sub-onoff"));
                extra.append(Some("_Reload"), Some("win.subtitles-subitem-2"));
                menu.append_section(None, &extra);
            }
        }
        *ui.tracks_sig.borrow_mut() = sig;
    }
    for (_, property, action) in TRACK_KINDS {
        let current = prop::<String>(ui.mpv, property).unwrap_or_default();
        if let Some(a) = ui.win.lookup_action(action).and_downcast::<gio::SimpleAction>()
            && a.state().and_then(|v| v.get::<String>()).as_deref() != Some(current.as_str())
        {
            a.set_state(&current.to_variant());
        }
    }
    let visible = prop::<bool>(ui.mpv, "sub-visibility").unwrap_or(true);
    if let Some(a) = ui.win.lookup_action("stream-sub-onoff").and_downcast::<gio::SimpleAction>()
        && a.state().and_then(|v| v.get::<bool>()) != Some(visible)
    {
        a.set_state(&visible.to_variant());
    }
}

thread_local! {
    // ponytail: one main window per process, so a global is the simplest way for `main` to reach it.
    static UI: RefCell<Option<Rc<Ui>>> = const { RefCell::new(None) };
}

fn save_config(ui: &Ui) {
    config::save(&ui.cfg.borrow());
}

fn act(win: &gtk::ApplicationWindow, name: &str) {
    gio::prelude::ActionGroupExt::activate_action(win, name, None);
}

/// Run the first implemented command whose binding matches (MPC: first match in table order).
fn run_bound(win: &gtk::ApplicationWindow, bindings: &[Binding], matches: impl Fn(&Binding) -> bool) -> bool {
    let hit = COMMANDS.iter().zip(bindings).find(|(c, b)| matches(b) && win.lookup_action(c.action).is_some());
    if let Some((c, _)) = hit {
        act(win, c.action);
    }
    hit.is_some()
}

fn run_mouse(ui: &Ui, m: Mouse) -> bool {
    let fs = ui.win.is_fullscreen();
    m != Mouse::None && run_bound(&ui.win, &ui.bindings.borrow(), |b| if fs { b.mouse_fs == m } else { b.mouse == m })
}

/// Key bindings go through GTK accelerators, so the menus show them too.
fn apply_keys(ui: &Ui) {
    for (c, b) in COMMANDS.iter().zip(ui.bindings.borrow().iter()) {
        if ui.win.lookup_action(c.action).is_some() {
            let keys: &[&str] = if b.key.is_empty() { &[] } else { &[&b.key] };
            ui.app.set_accels_for_action(&format!("win.{}", c.action), keys);
        }
    }
}

fn open_options(ui: &Rc<Ui>) {
    let implemented: Vec<bool> = COMMANDS.iter().map(|c| ui.win.lookup_action(c.action).is_some()).collect();
    let current = ui.cfg.borrow().clone();
    let ui = ui.clone();
    options::show(&ui.win.clone(), current, implemented, move |new| {
        *ui.cfg.borrow_mut() = new;
        save_config(&ui);
        apply_settings(&ui);
    });
}

/// Make the running player follow the settings (after Options > OK/Apply, and at startup).
fn apply_settings(ui: &Ui) {
    *ui.bindings.borrow_mut() = commands::load(&ui.cfg.borrow().keys);
    apply_keys(ui);
    let cfg = ui.cfg.borrow().clone();
    let auto = |on: bool| if on { "fuzzy" } else { "no" };
    let langs = |s: &str| s.split([',', ' ', ';']).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(",");
    for (name, value) in [
        ("sub-auto", auto(cfg.autoload_subtitles).to_string()),
        ("audio-file-auto", auto(cfg.autoload_audio).to_string()),
        ("slang", langs(&cfg.subtitle_langs)),
        ("alang", langs(&cfg.audio_langs)),
    ] {
        if let Err(e) = ui.mpv.set_property(name, value.as_str()) {
            eprintln!("mpv {name}: {e}");
        }
    }
    // MPC's color controls are -100..100, hue -180..180; mpv's are all -100..100.
    for (name, v) in [("brightness", cfg.brightness), ("contrast", cfg.contrast), ("hue", cfg.hue * 100 / 180), ("saturation", cfg.saturation)] {
        let _ = ui.mpv.set_property(name, v as i64);
    }
    let custom = (!cfg.logo_file.is_empty()).then(|| gdk::Texture::from_filename(&cfg.logo_file).ok()).flatten();
    match custom {
        Some(t) => ui.logo.set_paintable(Some(&t)),
        None => ui.logo.set_paintable(Some(&gdk::Texture::from_bytes(&glib::Bytes::from_static(include_bytes!("../res/logo.png"))).unwrap())),
    }
    ui.pl.borrow_mut().shuffle = cfg.shuffle;
    apply_audio(ui);
    apply_sub_style(ui);
    let _ = ui.mpv.set_property("audio-delay", cfg.audio_time_shift_ms as f64 / 1000.0);
    ui.marks.set((-1, -1)); // rebuild (or drop) the chapter marks
    sync_repeat(ui);
    if !cfg.keep_history && !ui.history.borrow().recent.is_empty() {
        ui.history.borrow_mut().recent.clear();
        config::save_history(&ui.history.borrow());
    }
    refresh_recent(ui);
    relayout(ui);
}

fn prop<T: GetData>(mpv: &Mpv, name: &str) -> Option<T> {
    mpv.get_property(name).ok()
}

fn cmd(mpv: &Mpv, name: &str, args: &[&str]) {
    if let Err(e) = mpv.command(name, args) {
        eprintln!("mpv {name} {args:?}: {e}");
    }
}

fn hms(t: f64) -> String {
    let s = t.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

fn is_sub(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, ext)| SUB_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Open files, folders, playlist files and URLs: they replace the playlist (or are added with `append`).
/// Subtitles go to the playing video, or to the file they were opened with.
fn open_targets(ui: &Ui, targets: &[String], append: bool) {
    let playing = !prop::<bool>(ui.mpv, "idle-active").unwrap_or(true);
    let mut entries: Vec<Entry> = Vec::new();
    for t in targets {
        let path = Path::new(t);
        if is_sub(t) {
            if let Some(e) = entries.last_mut() {
                e.subs.push(t.clone());
            } else if playing {
                cmd(ui.mpv, "sub-add", &[t]);
                let mut pl = ui.pl.borrow_mut();
                if let Some(c) = pl.current {
                    pl.items[c].subs.push(t.clone());
                }
            }
        } else if path.is_dir() {
            entries.extend(playlist::folder_files(path).into_iter().map(Entry::new));
        } else if playlist::is_playlist(t) && !t.contains("://") {
            match playlist::load(path) {
                Some(v) => entries.extend(v),
                None => eprintln!("{t}: not a playlist"),
            }
        } else {
            entries.push(Entry::new(t.clone()));
        }
    }
    if entries.is_empty() {
        return;
    }
    let first = {
        let mut pl = ui.pl.borrow_mut();
        if !append {
            pl.clear();
            ui.playlist_loops.set(0);
        }
        pl.add(entries)
    };
    playlist_changed(ui);
    if !append || !playing {
        play(ui, first);
    }
}

/// Open from outside the window (command line, another launch handing over its files).
pub fn open_files(files: &[gio::File], append: bool) {
    let targets: Vec<String> = files
        .iter()
        .map(|f| f.path().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| f.uri().into()))
        .collect();
    if let Some(ui) = UI.with_borrow(|u| u.clone()) {
        open_targets(&ui, &targets, append);
    }
}

fn rand(n: usize) -> usize {
    glib::random_int_range(0, n as i32) as usize
}

/// Redraw the playlist panel and save it as the playlist to restore next time.
fn playlist_changed(ui: &Ui) {
    ui.plbar.refresh();
    if ui.cfg.borrow().remember_playlist {
        let path = config::dir().join("default.mpcpl");
        let _ = std::fs::create_dir_all(config::dir());
        if let Err(e) = playlist::save(&path, &ui.pl.borrow().items) {
            eprintln!("{}: {e}", path.display());
        }
    }
}

/// Remember where the loaded file was stopped (MPC "Remember file position", off by default).
fn save_position(ui: &Ui) {
    let Some(path) = ui.loaded.borrow().clone() else { return };
    if !ui.cfg.borrow().remember_file_pos || prop::<bool>(ui.mpv, "eof-reached").unwrap_or(false) {
        return;
    }
    if let Some(pos) = prop::<f64>(ui.mpv, "time-pos").filter(|p| *p > 0.0) {
        let max = ui.cfg.borrow().recent_files_number;
        ui.history.borrow_mut().set_position(&path, Some(pos), max);
        config::save_history(&ui.history.borrow());
    }
}

/// Play playlist item `idx`.
fn play(ui: &Ui, idx: usize) {
    save_position(ui);
    let Some(path) = ui.pl.borrow().items.get(idx).map(|e| e.path.clone()) else { return };
    ui.pl.borrow_mut().current = Some(idx);
    *ui.loaded.borrow_mut() = Some(path.clone());
    ui.stopped.set(false);
    // The old file may still report eof-reached for a moment; the flag clears once the new one plays.
    ui.eof_handled.set(true);
    ui.file_loops.set(0);
    cmd(ui.mpv, "loadfile", &[&path, "replace"]);
    let _ = ui.mpv.set_property("pause", ui.start_paused.replace(false));
    if ui.cfg.borrow().keep_history {
        let max = ui.cfg.borrow().recent_files_number;
        ui.history.borrow_mut().add_recent(&path, max);
        config::save_history(&ui.history.borrow());
        refresh_recent(ui);
    }
    ui.plbar.refresh();
}

/// mpv has opened the file: add its subtitles, restore its position, note its duration.
fn on_loaded(ui: &Ui) {
    let Some(cur) = ui.pl.borrow().current else { return };
    let (subs, audio) = ui.pl.borrow().items.get(cur).map(|e| (e.subs.clone(), e.audio.clone())).unwrap_or_default();
    for s in subs {
        cmd(ui.mpv, "sub-add", &[&s]);
    }
    for a in audio {
        cmd(ui.mpv, "audio-add", &[&a]);
    }
    let path = ui.loaded.borrow().clone().unwrap_or_default();
    let remembered = if ui.cfg.borrow().remember_file_pos { ui.history.borrow().position(&path) } else { None };
    if let Some(pos) = ui.start_at.take().or(remembered) {
        cmd(ui.mpv, "seek", &[&pos.to_string(), "absolute"]);
    }
    if let Some(d) = prop::<f64>(ui.mpv, "duration").filter(|d| *d > 0.0) {
        if let Some(e) = ui.pl.borrow_mut().items.get_mut(cur) {
            e.duration = Some(d);
        }
        ui.plbar.refresh_row(cur);
    }
}

/// The file played to its end: repeat it, play the next item, or stay on the last frame.
fn on_eof(ui: &Ui) {
    if let Some(path) = ui.loaded.borrow().clone() {
        ui.history.borrow_mut().set_position(&path, None, 0);
        config::save_history(&ui.history.borrow());
    }
    let (forever, mode, loops) = {
        let c = ui.cfg.borrow();
        (c.repeat_forever, c.repeat_mode, c.loops.max(1))
    };
    // "Play N times" / "Repeat forever" (Options > Playback, Play > Repeat).
    if mode == RepeatMode::File && (forever || ui.file_loops.get() + 1 < loops) {
        ui.file_loops.set(ui.file_loops.get() + 1);
        cmd(ui.mpv, "seek", &["0", "absolute"]);
        let _ = ui.mpv.set_property("pause", false);
        return;
    }
    let mut next = ui.pl.borrow_mut().step(1, false, &mut rand);
    if next.is_none() && mode == RepeatMode::Playlist && (forever || ui.playlist_loops.get() + 1 < loops) {
        ui.playlist_loops.set(ui.playlist_loops.get() + 1);
        next = ui.pl.borrow_mut().step(1, true, &mut rand);
    }
    match next {
        Some(i) => play(ui, i),
        None => after_playback(ui),
    }
}

/// End of the playlist: MPC's "After Playback" (or /close, /playnext for this run).
fn after_playback(ui: &Ui) {
    let (setting, exit_fs) = (ui.cfg.borrow().after_playback, ui.cfg.borrow().exit_fullscreen_at_end);
    match ui.after_override.get().unwrap_or(setting) {
        AfterPlayback::DoNothing => {}
        AfterPlayback::PlayNext => {
            let next = ui.loaded.borrow().clone().and_then(|p| playlist::folder_neighbor(&p, 1));
            if let Some(f) = next {
                open_targets(ui, &[f], false);
                return;
            }
        }
        AfterPlayback::Rewind => {
            cmd(ui.mpv, "seek", &["0", "absolute"]);
            let _ = ui.mpv.set_property("pause", true);
            ui.stopped.set(true);
        }
        AfterPlayback::Close => close_media(ui),
        AfterPlayback::Exit => {
            ui.win.close();
            return;
        }
    }
    if exit_fs {
        ui.win.unfullscreen();
    }
}

/// File > Close: unload the file, keep the playlist.
fn close_media(ui: &Ui) {
    save_position(ui);
    cmd(ui.mpv, "stop", &[]);
    ui.loaded.take();
    ui.pl.borrow_mut().current = None;
    ui.plbar.refresh();
}

/// The command line of this launch, or of a later one handed over to this running player.
pub fn command_line(app: &gtk::Application, mpv: &'static Mpv, c: CmdLine) {
    let fresh = app.active_window().is_none();
    if fresh || !c.nofocus {
        window(app, mpv);
    }
    let Some(ui) = UI.with_borrow(|u| u.clone()) else { return };
    if let Some(n) = c.viewpreset {
        act(&ui.win, ["view-presets-minimal", "view-presets-compact", "view-presets-normal"][n as usize - 1]);
    }
    if let Some((w, h)) = c.fixedsize {
        ui.win.set_default_size(w, h);
    }
    if let Some(v) = c.volume {
        let _ = mpv.set_property("volume", v as i64);
    }
    if c.mute {
        let _ = mpv.set_property("mute", true);
    }
    if c.close {
        ui.after_override.set(Some(AfterPlayback::Exit));
    } else if c.playnext {
        ui.after_override.set(Some(AfterPlayback::PlayNext));
    }
    if c.fullscreen || (fresh && ui.cfg.borrow().launch_fullscreen && !c.files.is_empty()) {
        ui.win.fullscreen();
    }
    if c.minimized {
        ui.win.minimize();
    }
    if c.files.is_empty() {
        if c.play {
            start_playlist(&ui);
        }
        return;
    }
    ui.start_at.set(c.start);
    ui.start_paused.set(c.open && !c.play);
    let before = ui.pl.borrow().items.len();
    let first = if c.add { before } else { 0 };
    open_targets(&ui, &c.files, c.add);
    {
        let mut pl = ui.pl.borrow_mut();
        if let Some(e) = pl.items.get_mut(first) {
            e.subs.extend(c.subs.iter().cloned());
            e.audio.extend(c.dubs.iter().cloned());
        }
    }
    if c.randomize {
        ui.pl.borrow_mut().randomize(&mut rand);
        playlist_changed(&ui);
        play(&ui, 0);
    }
}

/// With nothing loaded, Play starts the playlist (its current item, else the first). Returns whether it did.
fn start_playlist(ui: &Ui) -> bool {
    if !prop::<bool>(ui.mpv, "idle-active").unwrap_or(false) || ui.pl.borrow().items.is_empty() {
        return false;
    }
    let i = ui.pl.borrow().current.unwrap_or(0);
    play(ui, i);
    true
}

/// Previous/next file (MPC: with a single-item playlist this opens the neighbor file in the folder).
fn skip_file(ui: &Ui, dir: i64) {
    let (len, path) = {
        let pl = ui.pl.borrow();
        (pl.items.len(), pl.current.and_then(|c| pl.items.get(c)).map(|e| e.path.clone()))
    };
    if len <= 1 {
        if !ui.cfg.borrow().search_in_folder {
            return;
        }
        if let Some(f) = path.and_then(|p| playlist::folder_neighbor(&p, dir)) {
            open_targets(ui, &[f], false);
        }
        return;
    }
    let next = ui.pl.borrow_mut().step(dir, true, &mut rand);
    if let Some(i) = next {
        play(ui, i);
    }
}

/// File > Recent Files.
fn refresh_recent(ui: &Ui) {
    ui.recent.remove_all();
    let files = gio::Menu::new();
    for path in &ui.history.borrow().recent {
        let name = Path::new(path).file_name().map_or(path.clone(), |n| n.to_string_lossy().into_owned());
        let item = gio::MenuItem::new(Some(&name.replace('_', "__")), None);
        item.set_action_and_target_value(Some("win.open-recent"), Some(&path.to_variant()));
        files.append_item(&item);
    }
    ui.recent.append_section(None, &files);
    let clear = gio::Menu::new();
    clear.append(Some("_Clear list"), Some("win.recent-clear"));
    ui.recent.append_section(None, &clear);
}

pub fn window(app: &gtk::Application, mpv: &'static Mpv) {
    if let Some(win) = app.active_window() {
        win.present();
        return;
    }

    let cfg = config::load();
    let (w, h) = if cfg.remember_window_size { cfg.window_size } else { Config::default().window_size };
    let win = gtk::ApplicationWindow::builder().application(app).title("mpc-rs").default_width(w).default_height(h).build();

    // --- widgets ---
    let recent = gio::Menu::new();
    let track_menus = [gio::Menu::new(), gio::Menu::new(), gio::Menu::new()];
    let model = menu_model(&recent, &track_menus);
    let menubar = gtk::PopoverMenuBar::from_model(Some(&model));

    let video = video::new(mpv);
    let logo_tex = gdk::Texture::from_bytes(&glib::Bytes::from_static(include_bytes!("../res/logo.png"))).unwrap();
    let logo = gtk::Picture::for_paintable(&logo_tex);
    logo.set_can_shrink(false);
    logo.set_halign(gtk::Align::Center);
    logo.set_valign(gtk::Align::Center);
    logo.set_can_target(false);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&video));
    overlay.add_overlay(&logo);

    let info = gtk::Label::builder().xalign(0.0).margin_start(6).margin_end(6).ellipsize(gtk::pango::EllipsizeMode::End).build();
    let stats = gtk::Label::builder().xalign(0.0).margin_start(6).margin_end(6).ellipsize(gtk::pango::EllipsizeMode::End).build();

    let seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
    seek.set_draw_value(false);
    seek.set_focusable(false);
    seek.set_has_tooltip(true);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    controls.set_margin_start(4);
    controls.set_margin_end(4);
    for (icon, action, tip) in [
        ("media-playback-start-symbolic", "win.play-play", "Play"),
        ("media-playback-pause-symbolic", "win.play-pause", "Pause"),
        ("media-playback-stop-symbolic", "win.play-stop", "Stop"),
        ("media-skip-backward-symbolic", "win.navigate-skipback", "Previous"),
        ("media-skip-forward-symbolic", "win.navigate-skipforward", "Next"),
        ("go-last-symbolic", "win.play-framestep", "Frame Step"),
    ] {
        let b = gtk::Button::from_icon_name(icon);
        b.set_action_name(Some(action));
        b.set_tooltip_text(Some(tip));
        b.set_focusable(false);
        b.add_css_class("flat");
        controls.append(&b);
    }
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    controls.append(&spacer);
    let mute = gtk::ToggleButton::builder().icon_name("audio-volume-high-symbolic").tooltip_text("Mute").focusable(false).build();
    mute.add_css_class("flat");
    controls.append(&mute);
    let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    volume.set_draw_value(false);
    volume.set_focusable(false);
    volume.set_width_request(110);
    volume.set_value(100.0);
    controls.append(&volume);

    let status = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    status.set_margin_start(6);
    status.set_margin_end(6);
    let state = gtk::Label::builder().xalign(0.0).hexpand(true).label("Stopped").build();
    let time = gtk::Label::new(None);
    status.append(&state);
    status.append(&time);

    let pl = Rc::new(RefCell::new(Playlist::default()));
    pl.borrow_mut().shuffle = cfg.shuffle;
    if cfg.remember_playlist {
        pl.borrow_mut().add(playlist::load(&config::dir().join("default.mpcpl")).unwrap_or_default());
    }
    // The panel talks back through `UI`, which is set below before any of its signals can fire.
    let plbar = playlistbar::new(pl.clone(), cfg.hide_playlist_fullscreen, |req| {
        let Some(ui) = UI.with_borrow(|u| u.clone()) else { return };
        match req {
            Request::Play(i) => play(&ui, i),
            Request::Add(paths) => open_targets(&ui, &paths, true),
            Request::Changed { shuffle, hide_fullscreen } => {
                playlist_changed(&ui);
                let mut cfg = ui.cfg.borrow_mut();
                if (cfg.shuffle, cfg.hide_playlist_fullscreen) != (shuffle, hide_fullscreen) {
                    (cfg.shuffle, cfg.hide_playlist_fullscreen) = (shuffle, hide_fullscreen);
                    drop(cfg);
                    save_config(&ui);
                    relayout(&ui);
                }
            }
        }
    });
    let paned = gtk::Paned::builder().orientation(gtk::Orientation::Horizontal).start_child(&overlay).end_child(&plbar.widget).vexpand(true).build();
    paned.set_resize_end_child(false);
    paned.set_shrink_end_child(false);

    let resync = subresync::new(mpv);
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for w in [menubar.upcast_ref::<gtk::Widget>(), paned.upcast_ref(), resync.widget.upcast_ref(), info.upcast_ref(), stats.upcast_ref(), seek.upcast_ref(), controls.upcast_ref(), status.upcast_ref()] {
        vbox.append(w);
    }
    win.set_child(Some(&vbox));

    // Context menu (MPC's "Player Menu", right click by default): the same menus as the menu bar.
    let menu = gtk::PopoverMenu::from_model(Some(&model));
    menu.set_parent(&overlay);
    menu.set_has_arrow(false);
    menu.set_halign(gtk::Align::Start);
    overlay.connect_destroy({
        let menu = menu.clone();
        move |_| menu.unparent()
    });

    let bindings = Rc::new(RefCell::new(commands::load(&cfg.keys)));
    let mpris = Mpris::start(app, &win, {
        let (win, bindings) = (win.clone(), bindings.clone());
        move |m| {
            run_bound(&win, &bindings.borrow(), |b| b.media == m && m != Media::None);
        }
    });

    let ui = Rc::new(Ui {
        app: app.clone(),
        win: win.clone(),
        mpv,
        bindings,
        mpris,
        video: video.clone(),
        menu,
        pointer: Cell::new((0.0, 0.0)),
        logo,
        bars: vec![
            ("view-captionmenu", menubar.upcast()),
            ("view-information", info.clone().upcast()),
            ("view-statistics", stats.clone().upcast()),
            ("view-seeker", seek.clone().upcast()),
            ("view-controls", controls.upcast()),
            ("view-status", status.upcast()),
            ("view-playlist", plbar.widget.clone().upcast()),
            ("view-subresync", resync.widget.clone().upcast()),
        ],
        seek: seek.clone(),
        volume: volume.clone(),
        mute: mute.clone(),
        info,
        stats,
        state,
        time,
        stopped: Cell::new(false),
        fs_reveal: Cell::new(false),
        last_motion: Cell::new(Instant::now()),
        last_user_seek: Cell::new(Instant::now() - Duration::from_secs(1)),
        marks: Cell::new((-1, -1)),
        cfg: RefCell::new(cfg),
        history: RefCell::new(config::load_history()),
        pl,
        plbar,
        recent,
        loaded: RefCell::new(None),
        eof_handled: Cell::new(false),
        file_loops: Cell::new(0),
        playlist_loops: Cell::new(0),
        start_at: Cell::new(None),
        start_paused: Cell::new(false),
        after_override: Cell::new(None),
        fs_zone_at: Cell::new(None),
        subresync: resync,
        track_menus,
        tracks_sig: RefCell::new(String::new()),
    });
    UI.with_borrow_mut(|u| *u = Some(ui.clone()));

    add_actions(app, &ui);
    apply_settings(&ui);
    let _ = mpv.set_property("volume", ui.cfg.borrow().volume as i64);
    win.connect_close_request({
        let ui = ui.clone();
        move |win| {
            save_position(&ui);
            ui.subresync.cleanup();
            // MPC keeps the last volume (and, if asked, the window size) for the next start.
            let mut cfg = ui.cfg.borrow_mut();
            cfg.volume = prop::<f64>(ui.mpv, "volume").map_or(cfg.volume, |v| v.round() as u32);
            if cfg.remember_window_size && !win.is_fullscreen() && !win.is_maximized() {
                cfg.window_size = (win.width(), win.height());
            }
            drop(cfg);
            save_config(&ui);
            glib::Propagation::Proceed
        }
    });

    // --- seek bar ---
    seek.connect_change_value({
        let ui = ui.clone();
        move |_, _, v| {
            ui.last_user_seek.set(Instant::now());
            cmd(ui.mpv, "seek", &[&v.to_string(), "absolute"]);
            glib::Propagation::Proceed
        }
    });
    seek.connect_query_tooltip({
        let ui = ui.clone();
        move |s, x, _, _, tip| {
        let adj = s.adjustment();
        if adj.upper() <= 1.0 || !ui.cfg.borrow().time_tooltip {
            return false;
        }
        let frac = (x as f64 / s.width().max(1) as f64).clamp(0.0, 1.0);
        tip.set_text(Some(&hms(adj.lower() + frac * (adj.upper() - adj.lower()))));
        true
        }
    });

    // --- volume ---
    volume.connect_value_changed(move |v| {
        let _ = mpv.set_property("volume", v.value());
    });
    mute.connect_toggled(move |b| {
        let _ = mpv.set_property("mute", b.is_active());
        b.set_icon_name(if b.is_active() { "audio-volume-muted-symbolic" } else { "audio-volume-high-symbolic" });
    });

    // --- mouse on video: buttons and wheel run their bound commands (MPC's CMouse::OnButton) ---
    // The context menu is a child of the overlay, so its clicks bubble up here too; ignore those.
    let on_video = {
        let win = win.clone();
        move |g: &gtk::GestureClick| g.current_event().and_then(|e| e.surface()) == win.surface()
    };
    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.connect_pressed({
        let (ui, on_video) = (ui.clone(), on_video.clone());
        move |g, n, x, y| {
            if on_video(g) {
                ui.pointer.set((x, y));
                run_mouse(&ui, Mouse::button(g.current_button(), Click::Down));
                if n == 2 {
                    run_mouse(&ui, Mouse::button(g.current_button(), Click::Double));
                }
            }
        }
    });
    click.connect_released({
        let ui = ui.clone();
        move |g, _, x, y| {
            if on_video(g) {
                ui.pointer.set((x, y));
                run_mouse(&ui, Mouse::button(g.current_button(), Click::Up));
            }
        }
    });
    overlay.add_controller(click);
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE);
    scroll.connect_scroll({
        let ui = ui.clone();
        move |_, _, dy| {
            if dy != 0.0 && run_mouse(&ui, if dy < 0.0 { Mouse::WUp } else { Mouse::WDown }) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });
    overlay.add_controller(scroll);

    // --- fullscreen: reveal seek bar + controls near the bottom edge, auto-hide cursor ---
    let motion = gtk::EventControllerMotion::new();
    motion.connect_motion({
        let ui = ui.clone();
        move |_, _, y| {
            ui.last_motion.set(Instant::now());
            ui.video.set_cursor(None);
            if ui.win.is_fullscreen() && y > ui.win.height() as f64 - 150.0 {
                ui.fs_zone_at.set(Some(Instant::now()));
            }
            update_fs_reveal(&ui);
        }
    });
    win.add_controller(motion);
    win.connect_fullscreened_notify({
        let ui = ui.clone();
        move |_| {
            ui.fs_zone_at.set(None);
            ui.fs_reveal.set(false);
            update_fs_reveal(&ui);
            relayout(&ui);
        }
    });

    // --- drag and drop: files, folders, subtitles, URLs ---
    let drop = gtk::DropTarget::new(glib::Type::INVALID, gdk::DragAction::COPY);
    drop.set_types(&[gdk::FileList::static_type(), String::static_type()]);
    drop.connect_drop(move |_, value, _, _| {
        if let Ok(list) = value.get::<gdk::FileList>() {
            open_files(&list.files(), false);
        } else if let Ok(text) = value.get::<String>() {
            let targets: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
            if let Some(ui) = UI.with_borrow(|u| u.clone()) {
                open_targets(&ui, &targets, false);
            }
        }
        true
    });
    win.add_controller(drop);

    // ponytail: UI polls mpv every 100 ms (like MPC's own timers). Switch to observe_property if this ever shows up in a profile.
    glib::timeout_add_local(Duration::from_millis(100), {
        let ui = ui.clone();
        move || {
            update(&ui);
            glib::ControlFlow::Continue
        }
    });

    relayout(&ui);
    update(&ui);
    win.present();
}

/// Repeat menu check marks from the config (File and Playlist act as radio items).
fn sync_repeat(ui: &Ui) {
    let (forever, mode) = (ui.cfg.borrow().repeat_forever, ui.cfg.borrow().repeat_mode);
    for (name, on) in [
        ("play-repeat-forever", forever),
        ("play-repeat-onefile", mode == RepeatMode::File),
        ("play-repeat-wholeplaylist", mode == RepeatMode::Playlist),
    ] {
        // set_state, not change_action_state: that would re-enter the action's handler.
        if let Some(a) = ui.win.lookup_action(name).and_downcast::<gio::SimpleAction>() {
            a.set_state(&on.to_variant());
        }
    }
}

/// Fullscreen controls show while the pointer is near the bottom edge, and for the hide delay after
/// (Options > Fullscreen), or always when hiding is off.
fn update_fs_reveal(ui: &Ui) {
    let (hide, delay) = (ui.cfg.borrow().hide_fullscreen_controls, ui.cfg.borrow().hide_fullscreen_controls_delay_ms);
    let near = ui.fs_zone_at.get().is_some_and(|t| t.elapsed() <= Duration::from_millis(delay as u64 + 100));
    let reveal = ui.win.is_fullscreen() && (!hide || near);
    if reveal != ui.fs_reveal.get() {
        ui.fs_reveal.set(reveal);
        relayout(ui);
    }
}

/// Show/hide bars from the View toggles. In fullscreen only the seek bar and controls (when revealed)
/// and the playlist (unless hidden on fullscreen, or docked panels hide with the controls).
fn relayout(ui: &Ui) {
    let fs = ui.win.is_fullscreen();
    for (name, w) in &ui.bars {
        let on = ui.win.action_state(name).and_then(|v| v.get::<bool>()).unwrap_or(true);
        let cfg = ui.cfg.borrow();
        let fs_ok = match *name {
            "view-seeker" | "view-controls" => ui.fs_reveal.get(),
            "view-playlist" => !cfg.hide_playlist_fullscreen && (!cfg.hide_fullscreen_docked_panels || ui.fs_reveal.get()),
            "view-subresync" => !cfg.hide_fullscreen_docked_panels || ui.fs_reveal.get(),
            _ => false,
        };
        w.set_visible(on && (!fs || fs_ok));
    }
    let caption = ui.win.action_state("view-captionmenu").and_then(|v| v.get::<bool>()).unwrap_or(true);
    ui.win.set_decorated(caption);
}

fn update(ui: &Ui) {
    let mpv = ui.mpv;
    while let Some(ev) = mpv.wait_event(0.0) {
        match ev {
            Ok(libmpv2::events::Event::FileLoaded) => on_loaded(ui),
            // A file that fails to open is skipped, as MPC moves on to the next playlist item.
            Ok(libmpv2::events::Event::EndFile(libmpv2::mpv_end_file_reason::Error)) => {
                let next = ui.pl.borrow_mut().step(1, false, &mut rand);
                if let Some(i) = next {
                    play(ui, i);
                }
            }
            _ => {}
        }
    }
    update_tracks(ui);
    ui.subresync.tick();
    // keep-open pauses on the last frame instead of ending the file; that's the "end of file" moment.
    if prop::<bool>(mpv, "eof-reached").unwrap_or(false) {
        if !ui.eof_handled.replace(true) {
            on_eof(ui);
        }
    } else {
        ui.eof_handled.set(false);
    }

    let idle = prop::<bool>(mpv, "idle-active").unwrap_or(true);
    let paused = prop::<bool>(mpv, "pause").unwrap_or(false);
    let pos = prop::<f64>(mpv, "time-pos").unwrap_or(0.0);
    let dur = prop::<f64>(mpv, "duration").unwrap_or(0.0);

    // Seek bar (left alone briefly after the user moves it, so it doesn't jump back while dragging).
    if ui.last_user_seek.get().elapsed() > Duration::from_millis(300) {
        ui.seek.set_range(0.0, dur.max(1.0));
        ui.seek.set_value(pos);
    }
    let chapters = if ui.cfg.borrow().show_chapter_marks { prop::<i64>(mpv, "chapters").unwrap_or(0) } else { 0 };
    let key = (chapters, (dur * 1000.0) as i64);
    if ui.marks.get() != key {
        ui.marks.set(key);
        ui.seek.clear_marks();
        for i in 0..chapters {
            if let Some(t) = prop::<f64>(mpv, &format!("chapter-list/{i}/time")) {
                ui.seek.add_mark(t, gtk::PositionType::Bottom, None);
            }
        }
    }
    ui.seek.set_sensitive(!idle && dur > 0.0);

    // Volume / mute (setting the same value back is a no-op in mpv).
    let vol = prop::<f64>(mpv, "volume").unwrap_or(100.0);
    if (vol - ui.volume.value()).abs() > 0.5 {
        ui.volume.set_value(vol);
    }
    let muted = prop::<bool>(mpv, "mute").unwrap_or(false);
    if muted != ui.mute.is_active() {
        ui.mute.set_active(muted);
    }

    // Status bar, title, logo.
    if !paused {
        ui.stopped.set(false);
    }
    let state = if idle || ui.stopped.get() {
        "Stopped"
    } else if prop::<bool>(mpv, "paused-for-cache").unwrap_or(false) {
        "Buffering"
    } else if paused {
        "Paused"
    } else {
        "Playing"
    };
    ui.state.set_label(state);
    ui.mpris.set_status(match state {
        "Stopped" => "Stopped",
        "Paused" => "Paused",
        _ => "Playing",
    });
    ui.time.set_label(&if idle { String::new() } else { format!("{} / {}", hms(pos), hms(dur)) });
    let title = if idle { None } else { prop::<String>(mpv, "media-title") };
    ui.win.set_title(Some(&window_title(ui, idle)));
    ui.logo.set_visible(idle || !prop::<bool>(mpv, "vo-configured").unwrap_or(false));

    if ui.info.is_visible() {
        let mut s = format!("Title: {}", title.as_deref().unwrap_or("-"));
        if let Some(a) = prop::<String>(mpv, "metadata/by-key/artist") {
            s += &format!("    Author: {a}");
        }
        ui.info.set_label(&s);
    }
    if ui.stats.is_visible() {
        let kbps = |p| prop::<f64>(mpv, p).map_or("-".into(), |b| format!("{:.0}", b / 1000.0));
        ui.stats.set_label(&format!(
            "Frame rate: {:.3}    Bitrate: {} / {} kb/s    Dropped frames: {}",
            prop::<f64>(mpv, "estimated-vf-fps").unwrap_or(0.0),
            kbps("video-bitrate"),
            kbps("audio-bitrate"),
            prop::<i64>(mpv, "frame-drop-count").unwrap_or(0) + prop::<i64>(mpv, "vo-drop-frame-count").unwrap_or(0),
        ));
    }

    // Hide the cursor after 2 s without mouse movement: in fullscreen, and during playback in a window
    // (Options > Tweaks).
    let hide_cursor = ui.win.is_fullscreen() || (ui.cfg.borrow().hide_windowed_mouse_pointer && state == "Playing");
    if hide_cursor && ui.last_motion.get().elapsed() > Duration::from_secs(2) {
        ui.video.set_cursor_from_name(Some("none"));
    }
    update_fs_reveal(ui);
}

/// MPC's title bar setting: full path, file name (or the media's own title), or just the program name.
fn window_title(ui: &Ui, idle: bool) -> String {
    let path = ui.loaded.borrow().clone().filter(|_| !idle);
    let Some(path) = path else { return "mpc-rs".into() };
    let cfg = ui.cfg.borrow();
    match cfg.title_bar {
        TitleBar::FullPath => path,
        TitleBar::FileName => {
            let tagged = prop::<String>(ui.mpv, "metadata/by-key/title").filter(|_| cfg.title_bar_media_title);
            tagged.unwrap_or_else(|| Entry::new(path).title())
        }
        TitleBar::NoPrefix => "mpc-rs".into(),
    }
}

/// The menus, shared by the menu bar and the context menu. `recent` is filled by `refresh_recent`.
fn menu_model(recent: &gio::Menu, tracks: &[gio::Menu; 3]) -> gio::Menu {
    fn section(items: &[(&str, &str)]) -> gio::Menu {
        let m = gio::Menu::new();
        for (label, action) in items {
            m.append(Some(label), Some(action));
        }
        m
    }
    let root = gio::Menu::new();

    let file = gio::Menu::new();
    file.append_section(None, &section(&[
        ("_Quick Open File...", "win.file-openquick"),
        ("_Open File/URL...", "win.file-openmedia"),
        ("Open Dir_ectory...", "win.open-dir"),
    ]));
    file.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some("Recent _Files"), recent);
        m
    });
    file.append_section(None, &section(&[("_Load Subtitle...", "win.file-subtitles-load"), ("_Save Subtitle...", "win.file-subtitles-save")]));
    file.append_section(None, &section(&[("_Close", "win.file-close-and-restore")]));
    file.append_section(None, &section(&[("E_xit", "win.file-exit")]));
    root.append_submenu(Some("_File"), &file);

    let view = gio::Menu::new();
    let bars: Vec<(String, String)> = BARS.iter().map(|(a, l, _)| (l.replace("&&", "&"), format!("win.{a}"))).collect();
    let mut bars: Vec<(&str, &str)> = bars.iter().map(|(l, a)| (l.as_str(), a.as_str())).collect();
    bars.push(("Su_bresync", "win.view-subresync"));
    bars.push(("_Playlist", "win.view-playlist"));
    view.append_section(None, &section(&bars));
    let presets = section(&[
        ("_Minimal", "win.view-presets-minimal"),
        ("_Compact", "win.view-presets-compact"),
        ("_Normal", "win.view-presets-normal"),
    ]);
    view.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some("_Presets"), &presets);
        m.append(Some("F_ull Screen"), Some("win.view-fullscreen"));
        m
    });
    view.append_section(None, &section(&[("_Options...", "win.view-options")]));
    root.append_submenu(Some("_View"), &view);

    let play = gio::Menu::new();
    play.append_section(None, &section(&[
        ("_Play/Pause", "win.play-playpause"),
        ("_Stop", "win.play-stop"),
        ("Fra_me Step", "win.play-framestep"),
        ("Frame Step _Back", "win.play-framestepcancel"),
    ]));
    play.append_section(None, &{
        let m = gio::Menu::new();
        let repeat = gio::Menu::new();
        repeat.append_section(None, &section(&[("F_orever", "win.play-repeat-forever")]));
        repeat.append_section(None, &section(&[("_File", "win.play-repeat-onefile"), ("_Playlist", "win.play-repeat-wholeplaylist")]));
        m.append_submenu(Some("_Repeat"), &repeat);
        m
    });
    play.append_section(None, &section(&[
        ("_Decrease Rate", "win.play-decrate"),
        ("_Increase Rate", "win.play-incrate"),
        ("R_eset Rate", "win.play-resetrate"),
    ]));
    play.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some("_Audio Track"), &tracks[0]);
        m.append_submenu(Some("Su_btitle Track"), &tracks[1]);
        m.append_submenu(Some("Vide_o Track"), &tracks[2]);
        m
    });
    play.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some("_Volume"), &section(&[("_Up", "win.volume-up"), ("_Down", "win.volume-down"), ("_Mute", "win.volume-mute")]));
        m
    });
    root.append_submenu(Some("_Play"), &play);

    root.append_submenu(Some("_Navigate"), &section(&[("_Previous", "win.navigate-skipback"), ("_Next", "win.navigate-skipforward")]));
    root.append_submenu(Some("_Help"), &section(&[("_About...", "win.about")]));
    root
}

/// Actions for the commands implemented so far. Names are from `commands::COMMANDS` (keys are bound in
/// `apply_keys`), except a few menu-only entries MPC doesn't list in its key table.
fn add_actions(app: &gtk::Application, ui: &Rc<Ui>) {
    let win = &ui.win;
    let mpv = ui.mpv;
    let add = |name: &str, f: Box<dyn Fn()>| {
        let a = gio::SimpleAction::new(name, None);
        a.connect_activate(move |_, _| f());
        win.add_action(&a);
    };
    // Jump distances and fast (keyframe) seeking come from Options > Tweaks.
    let seek = |name: &str, dir: i32, dist: fn(&Config) -> u32| {
        let ui = ui.clone();
        add(name, Box::new(move || {
            let (ms, fast) = (dist(&ui.cfg.borrow()), ui.cfg.borrow().fast_seek);
            let secs = format!("{}", dir as f64 * ms as f64 / 1000.0);
            cmd(mpv, "seek", &[&secs, if fast { "relative+keyframes" } else { "relative+exact" }]);
        }))
    };

    // File
    add("file-openquick", Box::new({
        let win = win.clone();
        move || {
            gtk::FileDialog::builder().title("Quick Open").build().open_multiple(Some(&win), gio::Cancellable::NONE, move |res| {
                if let Ok(model) = res {
                    let files: Vec<gio::File> = (0..model.n_items()).filter_map(|i| model.item(i).and_downcast()).collect();
                    open_files(&files, false);
                }
            });
        }
    }));
    add("file-openmedia", Box::new({
        let win = win.clone();
        move || open_dialog(&win)
    }));
    add("open-dir", Box::new({
        let win = win.clone();
        move || {
            gtk::FileDialog::builder().title("Open Directory").build().select_folder(Some(&win), gio::Cancellable::NONE, move |res| {
                if let Ok(dir) = res {
                    open_files(&[dir], false);
                }
            });
        }
    }));
    add("file-close-and-restore", Box::new({
        let ui = ui.clone();
        move || close_media(&ui)
    }));
    let open_recent = gio::SimpleAction::new("open-recent", Some(glib::VariantTy::STRING));
    open_recent.connect_activate({
        let ui = ui.clone();
        move |_, v| {
            if let Some(path) = v.and_then(|v| v.get::<String>()) {
                open_targets(&ui, &[path], false);
            }
        }
    });
    win.add_action(&open_recent);
    add("recent-clear", Box::new({
        let ui = ui.clone();
        move || {
            ui.history.borrow_mut().recent.clear();
            config::save_history(&ui.history.borrow());
            refresh_recent(&ui);
        }
    }));
    add("file-exit", Box::new({
        let win = win.clone();
        move || win.close()
    }));

    // Play
    add("play-playpause", Box::new({
        let ui = ui.clone();
        move || {
            if !start_playlist(&ui) {
                ui.stopped.set(false);
                cmd(mpv, "cycle", &["pause"]);
            }
        }
    }));
    add("play-play", Box::new({
        let ui = ui.clone();
        move || {
            if !start_playlist(&ui) {
                let _ = mpv.set_property("pause", false);
            }
        }
    }));
    add("play-pause", Box::new(move || {
        let _ = mpv.set_property("pause", true);
    }));
    add("play-stop", Box::new({
        let ui = ui.clone();
        move || {
            let _ = mpv.set_property("pause", true);
            cmd(mpv, "seek", &["0", "absolute"]);
            ui.stopped.set(true);
        }
    }));
    add("play-framestep", Box::new(move || cmd(mpv, "frame-step", &[])));
    add("play-framestepcancel", Box::new(move || cmd(mpv, "frame-back-step", &[])));
    seek("play-seekforwardsmall", 1, |c| c.jump_small_ms);
    seek("play-seekbackwardsmall", -1, |c| c.jump_small_ms);
    seek("play-seekforwardmed", 1, |c| c.jump_medium_ms);
    seek("play-seekbackwardmed", -1, |c| c.jump_medium_ms);
    seek("play-seekforwardlarge", 1, |c| c.jump_large_ms);
    seek("play-seekbackwardlarge", -1, |c| c.jump_large_ms);
    add("play-seekset", Box::new(move || cmd(mpv, "seek", &["0", "absolute"])));
    for (name, dir) in [("volume-up", 1), ("volume-down", -1)] {
        let ui = ui.clone();
        add(name, Box::new(move || {
            let step = ui.cfg.borrow().volume_step as i64 * dir;
            cmd(mpv, "add", &["volume", &step.to_string()]);
        }));
    }
    add("volume-mute", Box::new(move || cmd(mpv, "cycle", &["mute"])));

    // Navigate: chapters first, then files (MPC "skip back/forward"); the "file" variants skip files only.
    let skip = |dir: i64, chapters: bool| {
        let ui = ui.clone();
        Box::new(move || {
            let (n, ch) = (prop::<i64>(mpv, "chapters").unwrap_or(0), prop::<i64>(mpv, "chapter").unwrap_or(-1));
            if chapters && n > 0 && (0..n).contains(&(ch + dir)) {
                cmd(mpv, "add", &["chapter", &dir.to_string()]);
            } else {
                skip_file(&ui, dir);
            }
        }) as Box<dyn Fn()>
    };
    add("navigate-skipback", skip(-1, true));
    add("navigate-skipforward", skip(1, true));
    add("navigate-skipbackfile", skip(-1, false));
    add("navigate-skipforwardfile", skip(1, false));

    // Repeat (Play > Repeat): "Forever" plus a File / Playlist choice, saved in the config.
    let repeat = |name: &str, state: bool, f: fn(&mut Config, bool)| {
        let a = gio::SimpleAction::new_stateful(name, None, &state.to_variant());
        let ui = ui.clone();
        a.connect_change_state(move |_, v| {
            if let Some(on) = v.and_then(|v| v.get::<bool>()) {
                f(&mut ui.cfg.borrow_mut(), on);
                save_config(&ui);
                sync_repeat(&ui);
            }
        });
        win.add_action(&a);
    };
    let (forever, mode) = (ui.cfg.borrow().repeat_forever, ui.cfg.borrow().repeat_mode);
    repeat("play-repeat-forever", forever, |c, on| c.repeat_forever = on);
    repeat("play-repeat-onefile", mode == RepeatMode::File, |c, _| c.repeat_mode = RepeatMode::File);
    repeat("play-repeat-wholeplaylist", mode == RepeatMode::Playlist, |c, _| c.repeat_mode = RepeatMode::Playlist);
    let show_pl = gio::SimpleAction::new_stateful("view-playlist", None, &ui.cfg.borrow().show_playlist.to_variant());
    show_pl.connect_change_state({
        let ui = ui.clone();
        move |a, v| {
            if let Some(v) = v {
                a.set_state(v);
                ui.cfg.borrow_mut().show_playlist = v.get().unwrap_or(false);
                save_config(&ui);
                relayout(&ui);
            }
        }
    });
    win.add_action(&show_pl);

    // View: bar toggles (stateful boolean actions toggle themselves on activate).
    for (name, _, normal) in BARS {
        let a = gio::SimpleAction::new_stateful(name, None, &normal.to_variant());
        a.connect_change_state({
            let ui = ui.clone();
            move |a, v| {
                if let Some(v) = v {
                    a.set_state(v);
                    relayout(&ui);
                }
            }
        });
        win.add_action(&a);
    }
    let preset = |shown: &'static [&'static str]| {
        let win = win.clone();
        Box::new(move || {
            for (name, _, _) in BARS {
                win.change_action_state(name, &shown.contains(name).to_variant());
            }
        }) as Box<dyn Fn()>
    };
    add("view-presets-minimal", preset(&[]));
    add("view-presets-compact", preset(&["view-seeker"]));
    add("view-presets-normal", preset(&["view-captionmenu", "view-seeker", "view-controls", "view-status"]));
    for name in ["view-fullscreen", "view-fullscreen-secondary"] {
        add(name, Box::new({
            let win = win.clone();
            move || win.set_fullscreened(!win.is_fullscreen())
        }));
    }
    add("leave-fullscreen", Box::new({
        let win = win.clone();
        move || win.unfullscreen()
    }));
    app.set_accels_for_action("win.leave-fullscreen", &["Escape"]);
    add("view-options", Box::new({
        let ui = ui.clone();
        move || open_options(&ui)
    }));
    for name in ["menu-player-short", "menu-player-long"] {
        add(name, Box::new({
            let ui = ui.clone();
            move || {
                let (x, y) = ui.pointer.get();
                ui.menu.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                ui.menu.popup();
            }
        }));
    }

    // Tracks: menu items carry the track id; "a"/"s" cycle like MPC's next/previous stream.
    for (_, property, action) in TRACK_KINDS {
        let a = gio::SimpleAction::new_stateful(action, Some(glib::VariantTy::STRING), &"".to_variant());
        a.connect_activate(move |_, v| {
            if let Some(id) = v.and_then(|v| v.get::<String>()) {
                let _ = mpv.set_property(property, id.as_str());
            }
        });
        win.add_action(&a);
    }
    add("stream-audio-next", Box::new(move || cycle_track(mpv, "audio", "aid", 1)));
    add("stream-audio-prev", Box::new(move || cycle_track(mpv, "audio", "aid", -1)));
    add("stream-sub-next", Box::new(move || cycle_track(mpv, "sub", "sid", 1)));
    add("stream-sub-prev", Box::new(move || cycle_track(mpv, "sub", "sid", -1)));
    let sub_onoff = gio::SimpleAction::new_stateful("stream-sub-onoff", None, &true.to_variant());
    sub_onoff.connect_activate(move |_, _| cmd(mpv, "cycle", &["sub-visibility"]));
    win.add_action(&sub_onoff);
    add("subtitles-subitem-2", Box::new(move || cmd(mpv, "sub-reload", &[])));

    // Rate (MPC: speed step 0 doubles/halves), audio delay +-10 ms.
    let rate = |name: &str, dir: i32| {
        let ui = ui.clone();
        add(name, Box::new(move || {
            let cur = prop::<f64>(mpv, "speed").unwrap_or(1.0);
            let step = ui.cfg.borrow().speed_step as f64 / 100.0;
            let new = match dir {
                0 => 1.0,
                d if step > 0.0 => cur + d as f64 * step,
                1 => cur * 2.0,
                _ => cur / 2.0,
            };
            let new = (new * 1000.0).round() / 1000.0;
            let _ = mpv.set_property("speed", new.clamp(0.125, 16.0));
            osd(mpv, &format!("Rate: {:.2}x", new.clamp(0.125, 16.0)));
        }))
    };
    rate("play-incrate", 1);
    rate("play-decrate", -1);
    rate("play-resetrate", 0);
    for (name, ms) in [("play-incauddelay", 10), ("play-decauddelay", -10)] {
        add(name, Box::new(move || {
            cmd(mpv, "add", &["audio-delay", &format!("{}", ms as f64 / 1000.0)]);
            osd(mpv, &format!("Audio delay: {:.0} ms", prop::<f64>(mpv, "audio-delay").unwrap_or(0.0) * 1000.0));
        }));
    }

    // Audio: boost (0..300 %, by the volume step), normalize and downmix toggles; saved like MPC does.
    let audio = |name: &str, f: fn(&mut Config)| {
        let ui = ui.clone();
        add(name, Box::new(move || {
            f(&mut ui.cfg.borrow_mut());
            save_config(&ui);
            apply_audio(&ui);
            let c = ui.cfg.borrow();
            osd(mpv, &format!("Boost: {}%  Normalize: {}  Downmix: {}", c.audio_boost,
                if c.audio_normalize { "on" } else { "off" }, if c.downmix_stereo { "on" } else { "off" }));
        }))
    };
    audio("volume-boost-inc", |c| c.audio_boost = (c.audio_boost + c.volume_step).min(300));
    audio("volume-boost-dec", |c| c.audio_boost = c.audio_boost.saturating_sub(c.volume_step));
    audio("volume-boost-min", |c| c.audio_boost = 0);
    audio("volume-boost-max", |c| c.audio_boost = 300);
    audio("normalize", |c| c.audio_normalize = !c.audio_normalize);
    audio("custom-channel-mapping", |c| c.downmix_stereo = !c.downmix_stereo);

    // Subtitles
    add("file-subtitles-load", Box::new({
        let ui = ui.clone();
        move || load_subtitle_dialog(&ui)
    }));
    add("file-subtitles-save", Box::new({
        let ui = ui.clone();
        move || save_subtitle_dialog(&ui)
    }));
    for (name, dir) in [("sub-delay-down", -1.0), ("sub-delay-up", 1.0)] {
        let ui = ui.clone();
        add(name, Box::new(move || {
            let step = ui.cfg.borrow().sub_delay_step_ms as f64 / 1000.0;
            cmd(mpv, "add", &["sub-delay", &(dir * step).to_string()]);
            osd(mpv, &format!("Subtitle delay: {:.0} ms", prop::<f64>(mpv, "sub-delay").unwrap_or(0.0) * 1000.0));
        }));
    }
    add("goto-prev-sub", Box::new(move || cmd(mpv, "sub-seek", &["-1"])));
    add("goto-next-sub", Box::new(move || cmd(mpv, "sub-seek", &["1"])));
    for (name, delta) in [("shift-sub-down", -0.1), ("shift-sub-up", 0.1)] {
        let ui = ui.clone();
        add(name, Box::new(move || ui.subresync.shift_current(delta)));
    }
    let show_resync = gio::SimpleAction::new_stateful("view-subresync", None, &ui.cfg.borrow().show_subresync.to_variant());
    show_resync.connect_change_state({
        let ui = ui.clone();
        move |a, v| {
            if let Some(v) = v {
                a.set_state(v);
                ui.cfg.borrow_mut().show_subresync = v.get().unwrap_or(false);
                save_config(&ui);
                relayout(&ui);
            }
        }
    });
    win.add_action(&show_resync);

    // Help
    add("about", Box::new({
        let win = win.clone();
        move || {
            gtk::AboutDialog::builder()
                .transient_for(&win)
                .modal(true)
                .program_name("mpc-rs")
                .version(env!("CARGO_PKG_VERSION"))
                .comments("Media Player Classic - Home Cinema, rewritten in Rust.\nPlayback by libmpv, UI by GTK4.")
                .license_type(gtk::License::Gpl30)
                .build()
                .present();
        }
    }));
}

/// File > Load Subtitle: add it to the playing file (and remember it with the playlist item).
fn load_subtitle_dialog(ui: &Rc<Ui>) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Subtitle files"));
    for e in SUB_EXTS {
        filter.add_suffix(e);
    }
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let ui2 = ui.clone();
    gtk::FileDialog::builder().title("Load Subtitle").filters(&filters).build().open(Some(&ui.win), gio::Cancellable::NONE, move |res| {
        let Some(path) = res.ok().and_then(|f| f.path()) else { return };
        open_targets(&ui2, &[path.to_string_lossy().into_owned()], true);
    });
}

/// File > Save Subtitle: the current external text subtitle with its delay and resync applied.
fn save_subtitle_dialog(ui: &Rc<Ui>) {
    let delay = prop::<f64>(ui.mpv, "sub-delay").unwrap_or(0.0);
    let Some((text, _, source)) = ui.subresync.text(delay) else {
        gtk::AlertDialog::builder()
            .message("Nothing to save")
            .detail("Only external SRT, WebVTT and ASS/SSA subtitles can be saved.")
            .build()
            .show(Some(&ui.win));
        return;
    };
    let name = Path::new(&source).file_name().map_or("subtitle.srt".into(), |n| n.to_string_lossy().into_owned());
    gtk::FileDialog::builder().title("Save Subtitle").initial_name(name.as_str()).build().save(Some(&ui.win), gio::Cancellable::NONE, move |res| {
        let Some(path) = res.ok().and_then(|f| f.path()) else { return };
        if let Err(e) = std::fs::write(&path, &text) {
            eprintln!("{}: {e}", path.display());
        }
    });
}

/// MPC's "Open" dialog: a file name or URL, a Browse button and "Add to playlist".
fn open_dialog(parent: &gtk::ApplicationWindow) {
    let dlg = gtk::Window::builder().title("Open").transient_for(parent).modal(true).resizable(false).build();
    let entry = gtk::Entry::builder().placeholder_text("File name or URL").width_chars(48).activates_default(true).build();
    let browse = gtk::Button::with_label("Browse...");
    let add = gtk::CheckButton::with_label("Add to playlist");
    let ok = gtk::Button::with_label("OK");
    ok.add_css_class("suggested-action");
    let cancel = gtk::Button::with_label("Cancel");

    let grid = gtk::Grid::builder().row_spacing(8).column_spacing(8).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    grid.attach(&gtk::Label::new(Some("Open:")), 0, 0, 1, 1);
    grid.attach(&entry, 1, 0, 1, 1);
    grid.attach(&browse, 2, 0, 1, 1);
    grid.attach(&add, 1, 1, 1, 1);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.append(&cancel);
    buttons.append(&ok);
    grid.attach(&buttons, 1, 2, 2, 1);
    dlg.set_child(Some(&grid));
    dlg.set_default_widget(Some(&ok));

    browse.connect_clicked({
        let (dlg, entry) = (dlg.clone(), entry.clone());
        move |_| {
            let entry = entry.clone();
            gtk::FileDialog::new().open(Some(&dlg), gio::Cancellable::NONE, move |res| {
                if let Some(p) = res.ok().and_then(|f| f.path()) {
                    entry.set_text(&p.to_string_lossy());
                }
            });
        }
    });
    cancel.connect_clicked({
        let dlg = dlg.clone();
        move |_| dlg.close()
    });
    ok.connect_clicked({
        let dlg = dlg.clone();
        move |_| {
            let t = entry.text().trim().to_string();
            if let Some(ui) = UI.with_borrow(|u| u.clone()).filter(|_| !t.is_empty()) {
                open_targets(&ui, &[t], add.is_active());
            }
            dlg.close();
        }
    });
    dlg.present();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(hms(0.0), "00:00:00");
        assert_eq!(hms(3725.9), "01:02:05");
        assert_eq!(hms(-3.0), "00:00:00");
        assert!(is_sub("/a/b/Movie.SRT"));
        assert!(is_sub("x.ass"));
        assert!(!is_sub("movie.mkv"));
        assert!(!is_sub("noext"));
    }
}
