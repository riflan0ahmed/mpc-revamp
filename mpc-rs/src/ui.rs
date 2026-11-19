//! Main window: menu bar, video, playlist, seek bar, toolbar, info/statistics/status bars.
//! Layout and default keys follow MPC-HC (`MainFrm.cpp`, `default_wmcmds` in `AppSettings.cpp`).

use crate::{
    cmdline::CmdLine,
    commands::{self, Binding, COMMANDS, Click, Media, Mouse},
    i18n::tr,
    config::{self, AfterPlayback, Config, Favorite, History, RepeatMode, TitleBar, VideoFrame, Zoom},
    dialogs, dvb,
    integration::{self, SystemAction},
    mpris::{self, Mpris, PlayerState},
    options,
    playlist::{self, Entry, Playlist},
    playlistbar::{self, Bar, Request},
    remote::{self, Job},
    shaders,
    subresync::{self, Subresync},
    video,
    view::{self, PanScan},
};
use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use libmpv2::{GetData, Mpv};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    sync::mpsc,
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
    pan_scan: Cell<PanScan>,
    /// The file the window was last auto-zoomed for (Options > Playback "Auto-zoom").
    zoomed_for: RefCell<Option<String>>,
    /// Play > Shaders > Enabled (for this run, like MPC's shader toggle).
    shaders_on: Cell<bool>,
    /// Recent mpv error messages (the shader editor shows compile errors from here).
    mpv_errors: RefCell<Vec<String>>,
    /// File > Open Disc, Navigate > Chapters / Titles, Favorites: rebuilt when their contents change.
    disc_menu: gio::Menu,
    chapter_menu: gio::Menu,
    title_menu: gio::Menu,
    fav_menu: gio::Menu,
    nav_sig: RefCell<String>,
    /// Capture bar (View > Capture): device name, record button.
    capture_label: gtk::Label,
    record: gtk::ToggleButton,
    /// Navigation bar (View > Navigation): the DVB channel list.
    channel_list: gtk::ListBox,
    /// The window's rows, and the fullscreen panel that the seek bar, controls and status move into.
    vbox: gtk::Box,
    fs_panel: gtk::Box,
    fs_revealer: gtk::Revealer,
    /// Play, Pause, Stop: the one matching the play state is shown pressed.
    state_buttons: [gtk::ToggleButton; 3],
    /// JSON socket: its requests, answered on this thread in `update`.
    jobs: mpsc::Receiver<Job>,
    ipc: Option<remote::Clients>,
    /// Last play state sent to JSON clients.
    last_state: Cell<&'static str>,
}

/// MPRIS's loop status and shuffle, from the settings.
fn player_state(ui: &Ui) -> PlayerState {
    let c = ui.cfg.borrow();
    let loop_status = match (c.repeat_forever, c.repeat_mode) {
        (false, _) => "None",
        (true, RepeatMode::File) => "Track",
        (true, RepeatMode::Playlist) => "Playlist",
    };
    PlayerState { loop_status, shuffle: c.shuffle }
}

fn broadcast(ui: &Ui, event: Value) {
    if let Some(c) = &ui.ipc {
        remote::broadcast(c, &event);
    }
}

/// Answer a JSON socket request (runs on the GTK thread).
fn handle_job(ui: &Rc<Ui>, job: Job) {
    let reply = json_command(ui, &job.request).unwrap_or_else(|e| json!({"ok": false, "error": e}));
    let _ = job.reply.send(reply);
}

/// Tracks of one type as JSON (index = position among that type, as MpcApi numbers them).
fn track_list(mpv: &Mpv, kind: &str) -> Vec<Value> {
    (0..prop::<i64>(mpv, "track-list/count").unwrap_or(0))
        .filter(|i| prop::<String>(mpv, &format!("track-list/{i}/type")).as_deref() == Some(kind))
        .map(|i| {
            let s = |k: &str| prop::<String>(mpv, &format!("track-list/{i}/{k}"));
            json!({"id": prop::<i64>(mpv, &format!("track-list/{i}/id")), "title": s("title"), "lang": s("lang"), "codec": s("codec"),
                   "selected": prop::<bool>(mpv, &format!("track-list/{i}/selected")).unwrap_or(false)})
        })
        .collect()
}

/// The JSON socket's commands: MpcApi.h's, by name.
fn json_command(ui: &Rc<Ui>, v: &Value) -> Result<Value, String> {
    let mpv = ui.mpv;
    let name = v.get("cmd").and_then(Value::as_str).ok_or("missing \"cmd\"")?.to_ascii_uppercase();
    let name = name.trim_start_matches("CMD_");
    let num = |k: &str| v.get(k).and_then(Value::as_f64).ok_or(format!("missing number \"{k}\""));
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(String::from).ok_or(format!("missing string \"{k}\""));
    let ok = json!({"ok": true});
    let run = |action: &str| {
        act(&ui.win, action);
        Ok(json!({"ok": true}))
    };
    match name {
        "OPENFILE" => {
            open_targets(ui, &[text("path")?], false);
            Ok(ok)
        }
        "ADDTOPLAYLIST" => {
            open_targets(ui, &[text("path")?], true);
            Ok(ok)
        }
        "STOP" => run("play-stop"),
        "CLOSEFILE" => {
            close_media(ui);
            Ok(ok)
        }
        "PLAYPAUSE" => run("play-playpause"),
        "PLAY" => run("play-play"),
        "PAUSE" => run("play-pause"),
        "CLEARPLAYLIST" => {
            ui.pl.borrow_mut().clear();
            playlist_changed(ui);
            Ok(ok)
        }
        "STARTPLAYLIST" => {
            if !ui.pl.borrow().items.is_empty() {
                play(ui, 0);
            }
            Ok(ok)
        }
        "REMOVEFROMPLAYLIST" => {
            ui.pl.borrow_mut().remove(&[num("index")? as usize]);
            playlist_changed(ui);
            Ok(ok)
        }
        "SETINDEXPLAYLIST" => {
            let i = num("index")? as usize;
            if i >= ui.pl.borrow().items.len() {
                return Err("no such playlist item".into());
            }
            play(ui, i);
            Ok(ok)
        }
        "SETPOSITION" => {
            cmd(mpv, "seek", &[&num("seconds")?.to_string(), "absolute+exact"]);
            Ok(ok)
        }
        "JUMPOFNSECONDS" => {
            cmd(mpv, "seek", &[&num("seconds")?.to_string(), "relative+exact"]);
            Ok(ok)
        }
        "SETAUDIODELAY" => {
            let _ = mpv.set_property("audio-delay", num("ms")? / 1000.0);
            Ok(ok)
        }
        "SETSUBTITLEDELAY" => {
            let _ = mpv.set_property("sub-delay", num("ms")? / 1000.0);
            Ok(ok)
        }
        "SETAUDIOTRACK" | "SETSUBTITLETRACK" => {
            let (kind, property) = if name == "SETAUDIOTRACK" { ("audio", "aid") } else { ("sub", "sid") };
            let i = num("index")? as i64;
            if i < 0 {
                let _ = mpv.set_property(property, "no");
            } else {
                let id = track_list(mpv, kind).get(i as usize).and_then(|t| t["id"].as_i64()).ok_or("no such track")?;
                let _ = mpv.set_property(property, id);
            }
            Ok(ok)
        }
        "GETAUDIOTRACKS" => Ok(json!({"ok": true, "tracks": track_list(mpv, "audio")})),
        "GETSUBTITLETRACKS" => Ok(json!({"ok": true, "tracks": track_list(mpv, "sub")})),
        "GETNOWPLAYING" => {
            let s = |k: &str| prop::<String>(mpv, k);
            Ok(json!({"ok": true, "title": s("media-title"), "author": s("metadata/by-key/artist"), "description": s("metadata/by-key/comment"),
                      "file": ui.loaded.borrow().clone(), "duration": prop::<f64>(mpv, "duration")}))
        }
        "GETPLAYLIST" => {
            let pl = ui.pl.borrow();
            let items: Vec<Value> = pl.items.iter().map(|e| json!({"path": e.path, "title": e.title(), "duration": e.duration})).collect();
            Ok(json!({"ok": true, "items": items, "current": pl.current}))
        }
        "GETCURRENTPOSITION" => Ok(json!({"ok": true, "seconds": prop::<f64>(mpv, "time-pos")})),
        "GETSTATE" => Ok(json!({"ok": true, "state": ui.last_state.get()})),
        "GETVERSION" => Ok(json!({"ok": true, "version": env!("CARGO_PKG_VERSION")})),
        "TOGGLEFULLSCREEN" => run("view-fullscreen"),
        "JUMPFORWARDMED" => run("play-seekforwardmed"),
        "JUMPBACKWARDMED" => run("play-seekbackwardmed"),
        "INCREASEVOLUME" => run("volume-up"),
        "DECREASEVOLUME" => run("volume-down"),
        "SHADER_TOGGLE" => {
            let on = !ui.shaders_on.get();
            ui.win.change_action_state("shaders-enabled", &on.to_variant());
            Ok(ok)
        }
        "CLOSEAPP" => {
            ui.win.close();
            Ok(ok)
        }
        "SETSPEED" => {
            let _ = mpv.set_property("speed", num("speed")?.clamp(0.125, 16.0));
            Ok(ok)
        }
        "OSDSHOWMESSAGE" => {
            let ms = v.get("duration").and_then(Value::as_i64).unwrap_or(3000);
            cmd(mpv, "show-text", &[&text("text")?, &ms.to_string()]);
            Ok(ok)
        }
        "WM_COMMAND" => {
            let id = num("id")? as i64;
            let c = COMMANDS.iter().find(|c| c.id as i64 == id).ok_or("unknown command id")?;
            if ui.win.lookup_action(c.action).is_none() {
                return Err(format!("{} isn't available yet", c.label));
            }
            run(c.action)
        }
        _ => Err(format!("unknown command {name}")),
    }
}

const ASPECTS: [(&str, &str); 7] = [
    ("_Default (DAR)", "no"),
    ("_4:3", "4:3"),
    ("_5:4", "5:4"),
    ("_16:9", "16:9"),
    ("_235:100", "2.35"),
    ("1_85:100", "1.85"),
    ("Assume _square pixels (SAR)", "0"),
];
const FRAMES: [(&str, &str, VideoFrame); 8] = [
    ("_Half Size", "view-vf-half", VideoFrame::Half),
    ("_Normal Size", "view-vf-normal", VideoFrame::Normal),
    ("_Double Size", "view-vf-double", VideoFrame::Double),
    ("_Stretch To Window", "view-vf-stretch", VideoFrame::Stretch),
    ("Touch Window From _Inside", "view-vf-frominside", VideoFrame::FromInside),
    ("Zoom _1", "view-vf-zoom1", VideoFrame::Zoom1),
    ("Zoom _2", "view-vf-zoom2", VideoFrame::Zoom2),
    ("Touch Window From _Outside", "view-vf-fromoutside", VideoFrame::FromOutside),
];

/// Load the shader chain into mpv (Options > Shaders, Play > Shaders > Enabled).
fn apply_shaders(ui: &Ui) {
    let (pre, post) = if ui.shaders_on.get() { (ui.cfg.borrow().shaders_pre.clone(), ui.cfg.borrow().shaders_post.clone()) } else { Default::default() };
    let files: Vec<String> = shaders::materialize(&pre, &post).iter().map(|p| p.to_string_lossy().into_owned()).collect();
    if let Err(e) = ui.mpv.set_property("glsl-shaders", files.join(":").as_str()) {
        eprintln!("mpv glsl-shaders: {e}");
    }
}

/// Options > Output and Sync, and the OSD.
fn apply_video(ui: &Ui) {
    let c = ui.cfg.borrow();
    // display-resample needs the refresh rate, which the render API can't see by itself.
    let refresh = ui.win.surface().and_then(|s| WidgetExt::display(&ui.win).monitor_at_surface(&s)).map_or(0.0, |m| m.refresh_rate() as f64 / 1000.0);
    for (name, value) in [
        ("hwdec", if c.hwdec { "auto-safe" } else { "no" }.to_string()),
        ("icc-profile-auto", if c.color_management { "yes" } else { "no" }.into()),
        ("icc-intent", c.icc_intent.to_string()),
        ("video-output-levels", if c.output_limited { "limited" } else { "full" }.into()),
        ("video-sync", if c.sync_to_display { "display-resample" } else { "audio" }.into()),
        ("interpolation", if c.interpolation && c.sync_to_display { "yes" } else { "no" }.into()),
        ("display-fps-override", format!("{refresh:.3}")),
        ("osd-level", if c.show_osd { "1" } else { "0" }.into()),
        ("osd-font", c.osd_font.clone()),
        ("osd-font-size", c.osd_font_size.to_string()),
    ] {
        if let Err(e) = ui.mpv.set_property(name, value.as_str()) {
            eprintln!("mpv {name}={value}: {e}");
        }
    }
    drop(c);
    view::apply_frame(ui.mpv, ui.cfg.borrow().video_frame);
    if let Some(a) = ui.win.lookup_action("video-frame").and_downcast::<gio::SimpleAction>() {
        let name = FRAMES.iter().find(|f| f.2 == ui.cfg.borrow().video_frame).map_or("", |f| f.1);
        a.set_state(&name.to_variant());
    }
    apply_shaders(ui);
}

fn set_frame(ui: &Ui, frame: VideoFrame) {
    ui.cfg.borrow_mut().video_frame = frame;
    save_config(ui);
    apply_video(ui);
    let label = FRAMES.iter().find(|f| f.2 == frame).map_or("", |f| f.0).replace('_', "");
    osd(ui.mpv, &label);
}

fn pan_scan(ui: &Ui, f: impl FnOnce(&mut PanScan)) {
    let mut p = ui.pan_scan.get();
    f(&mut p);
    ui.pan_scan.set(p);
    p.apply(ui.mpv);
}

/// File > Save Image: the current frame as shown (with subtitles); the format follows the extension.
fn save_image_dialog(ui: &Rc<Ui>) {
    let name = format!("{}.{}", snapshot_name(ui), ui.cfg.borrow().snapshot_ext);
    let mpv = ui.mpv;
    gtk::FileDialog::builder().title("Save Image").initial_name(name.as_str()).initial_folder(&gio::File::for_path(&ui.cfg.borrow().snapshot_dir)).build().save(
        Some(&ui.win),
        gio::Cancellable::NONE,
        move |res| {
            if let Some(p) = res.ok().and_then(|f| f.path()) {
                cmd(mpv, "screenshot-to-file", &[&p.to_string_lossy(), "subtitles"]);
            }
        },
    );
}

/// "name_hh.mm.ss" like MPC's snapshot names.
fn snapshot_name(ui: &Ui) -> String {
    let stem = ui.loaded.borrow().as_deref().and_then(|p| Path::new(p).file_stem().map(|s| s.to_string_lossy().into_owned())).unwrap_or("snapshot".into());
    format!("{stem}_{}", hms(prop::<f64>(ui.mpv, "time-pos").unwrap_or(0.0)).replace(':', "."))
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
    apply_video(ui);
    let _ = ui.mpv.set_property("dvbin-file", dvb::channels_file().to_string_lossy().as_ref());
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
        } else if path.is_dir() && let Some(url) = dialogs::disc_url(path) {
            let mut e = Entry::new(url);
            e.label = Some(format!("DVD/BD - {}", path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())));
            entries.push(e);
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
    let disc = dialogs::is_disc_url(&path);
    let on = if disc { ui.cfg.borrow().remember_dvd_pos } else { ui.cfg.borrow().remember_file_pos };
    // Live sources (capture devices, TV) have no position to come back to.
    if !on || path.starts_with("av://") || dvb::is_dvb(&path) || prop::<bool>(ui.mpv, "eof-reached").unwrap_or(false) {
        return;
    }
    if let Some(pos) = prop::<f64>(ui.mpv, "time-pos").filter(|p| *p > 0.0) {
        let max = ui.cfg.borrow().recent_files_number;
        let mut h = ui.history.borrow_mut();
        h.set_position(&path, Some(pos), max);
        // A disc position only means something within its title.
        if disc && let Some(title) = prop::<i64>(ui.mpv, "disc-title") {
            h.disc_titles.retain(|(p, _)| *p != path);
            h.disc_titles.insert(0, (path.clone(), title));
            h.disc_titles.truncate(max);
        }
        drop(h);
        config::save_history(&ui.history.borrow());
    }
}

/// Play playlist item `idx`.
fn play(ui: &Ui, idx: usize) {
    save_position(ui);
    let Some((path, options)) = ui.pl.borrow().items.get(idx).map(|e| (e.path.clone(), e.options.clone())) else { return };
    ui.pl.borrow_mut().current = Some(idx);
    *ui.loaded.borrow_mut() = Some(path.clone());
    ui.stopped.set(false);
    // The old file may still report eof-reached for a moment; the flag clears once the new one plays.
    ui.eof_handled.set(true);
    ui.file_loops.set(0);
    if options.is_empty() {
        cmd(ui.mpv, "loadfile", &[&path, "replace"]);
    } else {
        cmd(ui.mpv, "loadfile", &[&path, "replace", "-1", &options]);
    }
    // The capture bar shows while a device is open.
    let device = path.starts_with("av://");
    ui.capture_label.set_label(&if device { format!("Device: {}", path.trim_start_matches("av://")) } else { "No capture device is open (File > Open Device).".into() });
    if !device && ui.record.is_active() {
        ui.record.set_active(false);
    }
    let _ = ui.mpv.set_property("pause", ui.start_paused.replace(false));
    if ui.cfg.borrow().keep_history {
        let max = ui.cfg.borrow().recent_files_number;
        ui.history.borrow_mut().add_recent(&path, max);
        config::save_history(&ui.history.borrow());
        refresh_recent(ui);
    }
    ui.plbar.refresh();
}

/// Navigate > Chapters / Titles: rebuilt when the file's chapters or disc titles change.
fn update_nav_menus(ui: &Ui) {
    let chapters: Vec<(i64, String)> = (0..prop::<i64>(ui.mpv, "chapter-list/count").unwrap_or(0))
        .map(|i| {
            let t = prop::<f64>(ui.mpv, &format!("chapter-list/{i}/time")).unwrap_or(0.0);
            let name = prop::<String>(ui.mpv, &format!("chapter-list/{i}/title")).unwrap_or_else(|| format!("Chapter {}", i + 1));
            (i, format!("{} - {name}", hms(t)))
        })
        .collect();
    let titles: Vec<(i64, String)> = (0..prop::<i64>(ui.mpv, "disc-titles/count").unwrap_or(0))
        .map(|i| {
            let id = prop::<i64>(ui.mpv, &format!("disc-titles/{i}/id")).unwrap_or(i);
            let len = prop::<f64>(ui.mpv, &format!("disc-titles/{i}/length")).unwrap_or(0.0);
            (id, format!("Title {} ({})", id + 1, hms(len)))
        })
        .collect();
    let sig = format!("{chapters:?}{titles:?}");
    if *ui.nav_sig.borrow() == sig {
        return;
    }
    *ui.nav_sig.borrow_mut() = sig;
    for (menu, items, action) in [(&ui.chapter_menu, &chapters, "win.chapter"), (&ui.title_menu, &titles, "win.disc-title")] {
        menu.remove_all();
        for (i, label) in items {
            let item = gio::MenuItem::new(Some(&label.replace('_', "__")), None);
            item.set_action_and_target_value(Some(action), Some(&i.to_string().to_variant()));
            menu.append_item(&item);
        }
    }
}

/// The Navigation bar's channel list, from channels.conf.
fn refresh_channels(ui: &Rc<Ui>) {
    ui.channel_list.remove_all();
    let channels = dvb::channels();
    if channels.is_empty() {
        ui.channel_list.append(&gtk::Label::builder().label(tr("No channels yet: use Scan... (Navigate > Tuner Scan).")).xalign(0.0).margin_start(6).css_classes(["dim-label"]).build());
    }
    for name in channels {
        ui.channel_list.append(&gtk::Label::builder().label(name.as_str()).xalign(0.0).margin_start(6).build());
    }
}

/// Watch a DVB channel (replaces the playlist, like opening a file).
fn open_channel(ui: &Ui, name: &str) {
    let first = {
        let mut pl = ui.pl.borrow_mut();
        pl.clear();
        pl.add(vec![Entry::new(dvb::channel_url(name))])
    };
    playlist_changed(ui);
    play(ui, first);
    osd(ui.mpv, &format!("{}: {name}", tr("Channel")));
}

/// File > Open Disc: the discs and drives currently present.
fn refresh_discs(ui: &Ui) {
    ui.disc_menu.remove_all();
    let discs = dialogs::discs();
    if discs.is_empty() {
        ui.disc_menu.append(Some("(no disc)"), Some("win.no-op"));
    }
    for (label, url) in discs {
        let item = gio::MenuItem::new(Some(&label.replace('_', "__")), None);
        item.set_action_and_target_value(Some("win.open-disc"), Some(&url.to_variant()));
        ui.disc_menu.append_item(&item);
    }
}

/// The Favorites menu: Add / Organize, then the saved entries.
fn refresh_favorites(ui: &Ui) {
    ui.fav_menu.remove_all();
    let top = gio::Menu::new();
    top.append(Some("_Add to Favorites..."), Some("win.favorites-add"));
    top.append(Some("_Organize Favorites..."), Some("win.favorites-organize"));
    ui.fav_menu.append_section(None, &top);
    let list = gio::Menu::new();
    for (i, f) in ui.history.borrow().favorites.iter().enumerate() {
        let item = gio::MenuItem::new(Some(&f.name.replace('_', "__")), None);
        item.set_action_and_target_value(Some("win.open-favorite"), Some(&(i as u32).to_variant()));
        list.append_item(&item);
    }
    ui.fav_menu.append_section(None, &list);
}

fn add_favorite(ui: &Ui, name: String, with_pos: bool) {
    let Some(path) = ui.loaded.borrow().clone() else { return };
    let position = prop::<f64>(ui.mpv, "time-pos").filter(|_| with_pos);
    ui.history.borrow_mut().favorites.push(Favorite { name, path, position });
    config::save_history(&ui.history.borrow());
    refresh_favorites(ui);
}

/// Favorites > Add to Favorites: a name and whether to start at the current position.
fn add_favorite_dialog(ui: &Rc<Ui>) {
    let Some(path) = ui.loaded.borrow().clone() else { return };
    let dlg = gtk::Window::builder().title("Add Favorite").transient_for(&ui.win).modal(true).resizable(false).build();
    let name = gtk::Entry::builder().text(Entry::new(path).title()).width_chars(40).activates_default(true).build();
    let pos = gtk::CheckButton::with_label("Remember position");
    pos.set_active(true);
    let ok = gtk::Button::with_label("OK");
    ok.add_css_class("suggested-action");
    let cancel = gtk::Button::with_label("Cancel");
    ok.connect_clicked({
        let (ui, dlg, name, pos) = (ui.clone(), dlg.clone(), name.clone(), pos.clone());
        move |_| {
            add_favorite(&ui, name.text().to_string(), pos.is_active());
            dlg.close();
        }
    });
    cancel.connect_clicked({
        let dlg = dlg.clone();
        move |_| dlg.close()
    });
    let buttons = gtk::Box::builder().spacing(8).halign(gtk::Align::End).build();
    buttons.append(&cancel);
    buttons.append(&ok);
    let vbox = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    vbox.append(&gtk::Label::builder().label("Name:").xalign(0.0).build());
    vbox.append(&name);
    vbox.append(&pos);
    vbox.append(&buttons);
    dlg.set_child(Some(&vbox));
    dlg.set_default_widget(Some(&ok));
    dlg.present();
}

/// File > Open Device: a v4l2 capture device (with an optional size), plus the default audio input.
fn open_device_dialog(ui: &Rc<Ui>) {
    let devices = dialogs::video_devices();
    let dlg = gtk::Window::builder().title("Open Device").transient_for(&ui.win).modal(true).resizable(false).build();
    let labels: Vec<String> = devices.iter().map(|(n, d)| format!("{n} ({d})")).collect();
    let video = gtk::DropDown::from_strings(&labels.iter().map(String::as_str).collect::<Vec<_>>());
    let audio = gtk::DropDown::from_strings(&["None", "Default input (PulseAudio/PipeWire)"]);
    let size = gtk::Entry::builder().placeholder_text("Device default, or e.g. 1280x720").build();
    let ok = gtk::Button::with_label("OK");
    ok.add_css_class("suggested-action");
    let none = devices.is_empty();
    ok.set_sensitive(!none);
    let cancel = gtk::Button::with_label("Cancel");
    ok.connect_clicked({
        let (ui, dlg, video, audio, size) = (ui.clone(), dlg.clone(), video.clone(), audio.clone(), size.clone());
        move |_| {
            let Some((name, dev)) = devices.get(video.selected() as usize) else { return };
            // Live sources: no cache, show frames as they come.
            let mut opts = vec!["cache=no".to_string(), "untimed=yes".to_string()];
            let wh = size.text().trim().to_lowercase();
            if !wh.is_empty() {
                opts.push(format!("demuxer-lavf-o=video_size={wh}"));
            }
            if audio.selected() == 1 {
                opts.push("audio-file=av://pulse:default".into());
            }
            let mut e = Entry::new(format!("av://v4l2:{dev}"));
            e.label = Some(name.clone());
            e.options = opts.join(",");
            let first = {
                let mut pl = ui.pl.borrow_mut();
                pl.clear();
                pl.add(vec![e])
            };
            playlist_changed(&ui);
            play(&ui, first);
            ui.win.change_action_state("view-capture", &true.to_variant());
            dlg.close();
        }
    });
    cancel.connect_clicked({
        let dlg = dlg.clone();
        move |_| dlg.close()
    });
    let grid = gtk::Grid::builder().row_spacing(8).column_spacing(8).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    let label = |s: &str| gtk::Label::builder().label(s).xalign(0.0).build();
    grid.attach(&label("Video:"), 0, 0, 1, 1);
    grid.attach(&video, 1, 0, 1, 1);
    grid.attach(&label("Audio:"), 0, 1, 1, 1);
    grid.attach(&audio, 1, 1, 1, 1);
    grid.attach(&label("Size:"), 0, 2, 1, 1);
    grid.attach(&size, 1, 2, 1, 1);
    if none {
        grid.attach(&gtk::Label::builder().label("No capture devices found.").css_classes(["error"]).build(), 0, 3, 2, 1);
    }
    let buttons = gtk::Box::builder().spacing(8).halign(gtk::Align::End).build();
    buttons.append(&cancel);
    buttons.append(&ok);
    grid.attach(&buttons, 0, 4, 2, 1);
    dlg.set_child(Some(&grid));
    dlg.present();
}

/// Options > Playback "Auto-zoom": size the window to the default zoom once the new video's size is known.
fn auto_zoom(ui: &Ui) {
    let loaded = ui.loaded.borrow().clone();
    if loaded.is_none() || *ui.zoomed_for.borrow() == loaded || prop::<i64>(ui.mpv, "dwidth").is_none() {
        return;
    }
    *ui.zoomed_for.borrow_mut() = loaded;
    let (on, zoom, fit) = (ui.cfg.borrow().auto_zoom, ui.cfg.borrow().zoom, ui.cfg.borrow().autofit_factor);
    if on {
        view::zoom_window(&ui.win, &ui.video, ui.mpv, zoom, fit);
    }
}

/// mpv has opened the file: add its subtitles, restore its position, note its duration.
fn on_loaded(ui: &Ui) {
    // MPC resets pan & scan for each file.
    ui.pan_scan.set(PanScan::default());
    ui.pan_scan.get().apply(ui.mpv);
    let Some(cur) = ui.pl.borrow().current else { return };
    let (subs, audio) = ui.pl.borrow().items.get(cur).map(|e| (e.subs.clone(), e.audio.clone())).unwrap_or_default();
    for s in subs {
        cmd(ui.mpv, "sub-add", &[&s]);
    }
    for a in audio {
        cmd(ui.mpv, "audio-add", &[&a]);
    }
    let path = ui.loaded.borrow().clone().unwrap_or_default();
    let disc = dialogs::is_disc_url(&path);
    let on = if disc { ui.cfg.borrow().remember_dvd_pos } else { ui.cfg.borrow().remember_file_pos };
    let remembered = if on { ui.history.borrow().position(&path) } else { None };
    if disc && remembered.is_some() {
        let title = ui.history.borrow().disc_titles.iter().find(|(p, _)| *p == path).map(|(_, t)| *t);
        if let Some(t) = title.filter(|t| prop::<i64>(ui.mpv, "disc-title") != Some(*t)) {
            let _ = ui.mpv.set_property("disc-title", t);
        }
    }
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
    // The Play > After Playback choice is for this time only (MPC resets it too).
    let choice = ui.after_override.take().unwrap_or(setting);
    if let Some(a) = ui.win.lookup_action("after-playback").and_downcast::<gio::SimpleAction>() {
        a.set_state(&"".to_variant());
    }
    let system = |a: SystemAction| {
        if exit_fs {
            ui.win.unfullscreen();
        }
        integration::system_action(a);
    };
    match choice {
        AfterPlayback::MonitorOff => return system(SystemAction::MonitorOff),
        AfterPlayback::Standby => return system(SystemAction::Standby),
        AfterPlayback::Hibernate => return system(SystemAction::Hibernate),
        AfterPlayback::Shutdown => return system(SystemAction::Shutdown),
        AfterPlayback::LogOff => return system(SystemAction::LogOff),
        AfterPlayback::Lock => return system(SystemAction::Lock),
        _ => {}
    }
    match choice {
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
        _ => {}
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
    // Watching TV: previous/next channel (MPC does the same for DVB).
    if let Some(cur) = path.as_deref().and_then(|p| p.strip_prefix("dvb://")) {
        let channels = dvb::channels();
        if let Some(i) = channels.iter().position(|c| c == cur) {
            let next = (i as i64 + dir).rem_euclid(channels.len() as i64) as usize;
            open_channel(ui, &channels[next]);
        }
        return;
    }
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

/// Icons the icon theme doesn't have, as symbolic SVGs (GTK tints them like the theme's own).
fn install_icons() {
    // Frame step: a play arrow followed by a bar, like MPC's toolbar.
    const FRAME_STEP: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><path d="M2.5 2.5v11l8-5.5z" fill="#2e3436"/><rect x="11.5" y="2.5" width="2.5" height="11" rx="0.6" fill="#2e3436"/></svg>"##;
    let dir = glib::user_runtime_dir().join("mpc-rs-icons");
    if std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("mpc-frame-step-symbolic.svg"), FRAME_STEP)).is_ok()
        && let Some(display) = gdk::Display::default()
    {
        gtk::IconTheme::for_display(&display).add_search_path(&dir);
    }
}

pub fn window(app: &gtk::Application, mpv: &'static Mpv) {
    if let Some(win) = app.active_window() {
        win.present();
        return;
    }

    gtk::Window::set_default_icon_name(integration::APP_ID);
    install_icons();
    let cfg = config::load();
    let (w, h) = if cfg.remember_window_size { cfg.window_size } else { Config::default().window_size };
    let win = gtk::ApplicationWindow::builder().application(app).title("mpc-rs").default_width(w).default_height(h).build();

    // --- widgets ---
    let recent = gio::Menu::new();
    let track_menus = [gio::Menu::new(), gio::Menu::new(), gio::Menu::new()];
    let nav_menus = NavMenus { disc: gio::Menu::new(), chapters: gio::Menu::new(), titles: gio::Menu::new(), favorites: gio::Menu::new() };
    let model = menu_model(&recent, &track_menus, &nav_menus);
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

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    controls.set_margin_start(4);
    controls.set_margin_end(4);
    // Play / Pause / Stop show which one is in effect (pressed in, as in MPC's toolbar).
    let mut state_buttons = Vec::new();
    for (icon, action, tip) in [
        ("media-playback-start-symbolic", "win.play-play", "Play"),
        ("media-playback-pause-symbolic", "win.play-pause", "Pause"),
        ("media-playback-stop-symbolic", "win.play-stop", "Stop"),
        ("media-skip-backward-symbolic", "win.navigate-skipback", "Previous"),
        ("media-skip-forward-symbolic", "win.navigate-skipforward", "Next"),
        ("mpc-frame-step-symbolic", "win.play-framestep", "Frame Step"),
    ] {
        let b: gtk::Button = if state_buttons.len() < 3 {
            let t = gtk::ToggleButton::builder().icon_name(icon).css_classes(["mpc-state"]).build();
            state_buttons.push(t.clone());
            t.upcast()
        } else {
            gtk::Button::from_icon_name(icon)
        };
        b.set_action_name(Some(action));
        b.set_tooltip_text(Some(&tr(tip)));
        b.set_focusable(false);
        b.add_css_class("flat");
        controls.append(&b);
    }
    let state_buttons: [gtk::ToggleButton; 3] = state_buttons.try_into().expect("play, pause and stop buttons");
    // The theme's accent color (the one the seek and volume bars use), so it follows the desktop's
    // accent setting; a light tint behind the icon, the icon itself in the accent color.
    // Plain GTK only rounds a window's top corners (libadwaita rounds all four); round the bottom
    // ones too, except when the window is maximized, tiled or fullscreen.
    let css = gtk::CssProvider::new();
    css.load_from_string(
        ".mpc-state:checked { color: @accent_color; background-color: alpha(@accent_color, 0.2); }\n\
         .mpc-state:checked:hover { background-color: alpha(@accent_color, 0.3); }\n\
         .mpc-fs-panel { background-color: alpha(black, 0.65); padding-top: 4px; }\n\
         window.csd:not(.maximized):not(.fullscreen):not(.tiled):not(.tiled-top):not(.tiled-bottom):not(.tiled-left):not(.tiled-right) \
           { border-bottom-left-radius: 12px; border-bottom-right-radius: 12px; }",
    );
    gtk::style_context_add_provider_for_display(&WidgetExt::display(&win), &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
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
    status.set_margin_start(8);
    status.set_margin_end(8);
    status.set_margin_top(4);
    status.set_margin_bottom(6);
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
    // Capture bar (MPC's PlayerCaptureBar, reduced to what mpv offers): the device and recording.
    let capture = gtk::Box::builder().spacing(8).margin_start(6).margin_end(6).margin_top(2).margin_bottom(2).build();
    let capture_label = gtk::Label::builder().xalign(0.0).hexpand(true).ellipsize(gtk::pango::EllipsizeMode::End).build();
    let record = gtk::ToggleButton::builder().label("Record").focusable(false).build();
    capture.append(&capture_label);
    capture.append(&record);
    record.connect_toggled(move |b| {
        let path = if b.is_active() {
            let dir = glib::user_special_dir(glib::UserDirectory::Videos).unwrap_or_else(glib::home_dir);
            let stamp = glib::DateTime::now_local().ok().and_then(|d| d.format("%Y-%m-%d_%H.%M.%S").ok()).map_or("capture".into(), |s| s.to_string());
            dir.join(format!("capture_{stamp}.mkv")).to_string_lossy().into_owned()
        } else {
            String::new()
        };
        let _ = mpv.set_property("stream-record", path.as_str());
        b.set_label(if b.is_active() { "Stop recording" } else { "Record" });
        if !path.is_empty() {
            osd(mpv, &format!("Recording to {path}"));
        }
    });
    // Navigation bar (MPC's PlayerNavigationBar): DVB channels, double-click to watch.
    let navigation = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let nav_head = gtk::Box::builder().spacing(8).margin_start(6).margin_end(6).margin_top(2).build();
    nav_head.append(&gtk::Label::builder().label(tr("Channels")).xalign(0.0).hexpand(true).css_classes(["heading"]).build());
    let scan_button = gtk::Button::builder().label(tr("Scan...")).focusable(false).action_name("win.navigate-tunerscan").build();
    nav_head.append(&scan_button);
    let channel_list = gtk::ListBox::new();
    navigation.append(&nav_head);
    navigation.append(&gtk::ScrolledWindow::builder().min_content_height(140).max_content_height(140).child(&channel_list).build());
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for w in [menubar.upcast_ref::<gtk::Widget>(), paned.upcast_ref(), resync.widget.upcast_ref(), capture.upcast_ref(), navigation.upcast_ref(), info.upcast_ref(), stats.upcast_ref(), seek.upcast_ref(), controls.upcast_ref(), status.upcast_ref()] {
        vbox.append(w);
    }
    win.set_child(Some(&vbox));
    // Fullscreen: the seek bar, controls and status move into this panel, which slides up over the
    // bottom of the video (the video keeps its size).
    let fs_panel = gtk::Box::builder().orientation(gtk::Orientation::Vertical).css_classes(["mpc-fs-panel"]).build();
    let fs_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideUp)
        .transition_duration(250)
        .valign(gtk::Align::End)
        .child(&fs_panel)
        .visible(false)
        .build();
    overlay.add_overlay(&fs_revealer);

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
    // MPRIS talks back through `UI` (set below, before any D-Bus call can arrive).
    let mpris = Mpris::start(
        app,
        &win,
        mpv,
        {
            let (win, bindings) = (win.clone(), bindings.clone());
            move |req| match req {
                mpris::Request::Media(m) => {
                    run_bound(&win, &bindings.borrow(), |b| b.media == m && m != Media::None);
                }
                mpris::Request::OpenUri(uri) => {
                    if let Some(ui) = UI.with_borrow(|u| u.clone()) {
                        let path = gio::File::for_uri(&uri).path().map_or(uri, |p| p.to_string_lossy().into_owned());
                        open_targets(&ui, &[path], false);
                    }
                }
                mpris::Request::SetLoop(l) => {
                    if let Some(ui) = UI.with_borrow(|u| u.clone()) {
                        {
                            let mut c = ui.cfg.borrow_mut();
                            c.repeat_forever = l != "None";
                            if l == "Track" {
                                c.repeat_mode = RepeatMode::File;
                            } else if l == "Playlist" {
                                c.repeat_mode = RepeatMode::Playlist;
                            }
                        }
                        save_config(&ui);
                        sync_repeat(&ui);
                        ui.mpris.refresh(ui.mpv, player_state(&ui));
                    }
                }
                mpris::Request::SetShuffle(on) => {
                    if let Some(ui) = UI.with_borrow(|u| u.clone()) {
                        ui.cfg.borrow_mut().shuffle = on;
                        ui.pl.borrow_mut().shuffle = on;
                        save_config(&ui);
                        ui.mpris.refresh(ui.mpv, player_state(&ui));
                    }
                }
            }
        },
        || UI.with_borrow(|u| u.as_ref().map(|ui| player_state(ui))).unwrap_or(PlayerState { loop_status: "None", shuffle: false }),
    );
    let (jobs_tx, jobs) = mpsc::channel();
    let ipc = remote::start_ipc(jobs_tx.clone());

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
            ("view-capture", capture.clone().upcast()),
            ("view-navigation", navigation.clone().upcast()),
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
        pan_scan: Cell::new(PanScan::default()),
        zoomed_for: RefCell::new(None),
        shaders_on: Cell::new(true),
        mpv_errors: RefCell::new(Vec::new()),
        disc_menu: nav_menus.disc,
        chapter_menu: nav_menus.chapters,
        title_menu: nav_menus.titles,
        fav_menu: nav_menus.favorites,
        nav_sig: RefCell::new(String::new()),
        capture_label,
        record,
        channel_list: channel_list.clone(),
        vbox: vbox.clone(),
        fs_panel: fs_panel.clone(),
        fs_revealer: fs_revealer.clone(),
        state_buttons: state_buttons.clone(),
        jobs,
        ipc,
        last_state: Cell::new("Stopped"),
    });
    UI.with_borrow_mut(|u| *u = Some(ui.clone()));

    add_actions(app, &ui);
    apply_settings(&ui);
    for b in &ui.state_buttons {
        let ui = ui.clone();
        b.connect_clicked(move |_| sync_state_buttons(&ui));
    }
    refresh_discs(&ui);
    refresh_favorites(&ui);
    refresh_channels(&ui);
    ui.channel_list.connect_row_activated({
        let ui = ui.clone();
        move |_, row| {
            let name = row.child().and_downcast::<gtk::Label>().map(|l| l.label().to_string());
            if let Some(n) = name.filter(|_| !dvb::channels().is_empty()) {
                open_channel(&ui, &n);
            }
        }
    });
    ui.capture_label.set_label("No capture device is open (File > Open Device).");
    // Keep File > Open Disc current as discs come and go.
    let monitor = gio::VolumeMonitor::get();
    for signal in ["mount-added", "mount-removed", "drive-changed"] {
        let ui = ui.clone();
        monitor.connect_local(signal, false, move |_| {
            refresh_discs(&ui);
            None
        });
    }
    let _ = mpv.set_property("volume", ui.cfg.borrow().volume as i64);
    win.connect_close_request({
        let ui = ui.clone();
        move |win| {
            save_position(&ui);
            ui.subresync.cleanup();
            shaders::cleanup();
            if ui.ipc.is_some() {
                remote::cleanup_socket();
            }
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
    // Hover time: a small bubble above the pointer that follows it along the seek bar
    // (Options > Tweaks > Show time tooltip).
    let hover_label = gtk::Label::builder().css_classes(["numeric"]).build();
    let hover = gtk::Popover::builder().child(&hover_label).autohide(false).can_focus(false).can_target(false).position(gtk::PositionType::Top).build();
    hover.set_parent(&seek);
    seek.connect_destroy({
        let hover = hover.clone();
        move |_| hover.unparent()
    });
    let hover_motion = gtk::EventControllerMotion::new();
    // Bumped on every enter/motion; a pending hide only runs if nothing happened since.
    let hover_gen = Rc::new(Cell::new(0u64));
    let show_hover = {
        let (ui, seek, hover, hover_gen) = (ui.clone(), seek.clone(), hover.clone(), hover_gen.clone());
        move |x: f64| {
            hover_gen.set(hover_gen.get() + 1);
            let adj = seek.adjustment();
            if adj.upper() <= 1.0 || !seek.is_sensitive() || !ui.cfg.borrow().time_tooltip {
                hover.popdown();
                return;
            }
            // The knob's center runs from one end of the track to the other; map x the same way.
            let track = seek.range_rect();
            let (a, b) = seek.slider_range();
            let half = (b - a) as f64 / 2.0;
            let (x0, x1) = (track.x() as f64 + half, (track.x() + track.width()) as f64 - half);
            let frac = ((x - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0);
            hover_label.set_label(&hms(adj.lower() + frac * (adj.upper() - adj.lower())));
            // Point a little above the bar, so the bubble's surface (arrow, shadow) never covers the
            // spot being hovered; that would send the bar a "leave" and make the bubble blink.
            let px = x.clamp(x0, x1) as i32;
            hover.set_pointing_to(Some(&gdk::Rectangle::new(px, -6, 1, 1)));
            if !hover.is_visible() {
                hover.popup();
            }
        }
    };
    hover_motion.connect_enter({
        let show = show_hover.clone();
        move |_, x, _| show(x)
    });
    hover_motion.connect_motion(move |_, x, _| show_hover(x));
    // Hide shortly after leaving, unless the pointer comes back (a jitter at the edge).
    hover_motion.connect_leave({
        let (hover, hover_gen) = (hover.clone(), hover_gen.clone());
        move |_| {
            let (hover, hover_gen, at_leave) = (hover.clone(), hover_gen.clone(), hover_gen.get());
            glib::timeout_add_local_once(Duration::from_millis(150), move || {
                if hover_gen.get() == at_leave {
                    hover.popdown();
                }
            });
        }
    });
    seek.add_controller(hover_motion);

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
        let (win, overlay, panel) = (win.clone(), overlay.clone(), fs_panel.clone());
        move |g: &gtk::GestureClick, x: f64, y: f64| {
            let on_panel = overlay.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|w| w == panel || w.is_ancestor(&panel));
            g.current_event().and_then(|e| e.surface()) == win.surface() && !on_panel
        }
    };
    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.connect_pressed({
        let (ui, on_video) = (ui.clone(), on_video.clone());
        move |g, n, x, y| {
            if on_video(g, x, y) {
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
            if on_video(g, x, y) {
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
        move |win| {
            ui.fs_zone_at.set(None);
            ui.fs_reveal.set(false);
            move_bottom_bars(&ui, win.is_fullscreen());
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
    // The monitor (for display-resample's refresh rate) is only known once the window is on screen.
    win.connect_map({
        let ui = ui.clone();
        move |_| apply_video(&ui)
    });
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

/// Put the seek bar, controls and status into the fullscreen panel (over the video), or back into the
/// window's rows below it.
fn move_bottom_bars(ui: &Ui, fullscreen: bool) {
    let bar = |name: &str| ui.bars.iter().find(|(n, _)| *n == name).map(|(_, w)| w.clone()).unwrap();
    let (anchor, rows) = (bar("view-statistics"), [bar("view-seeker"), bar("view-controls"), bar("view-status")]);
    let mut after = anchor;
    for w in rows {
        let in_panel = w.parent().as_ref() == Some(ui.fs_panel.upcast_ref());
        if fullscreen && !in_panel {
            ui.vbox.remove(&w);
            ui.fs_panel.append(&w);
        } else if !fullscreen && in_panel {
            ui.fs_panel.remove(&w);
            ui.vbox.insert_child_after(&w, Some(&after));
        }
        after = w;
    }
    ui.fs_revealer.set_visible(fullscreen);
}

/// Fullscreen controls show while the pointer is near the bottom edge, and for the hide delay after
/// (Options > Fullscreen), or always when hiding is off.
fn update_fs_reveal(ui: &Ui) {
    let (hide, delay) = (ui.cfg.borrow().hide_fullscreen_controls, ui.cfg.borrow().hide_fullscreen_controls_delay_ms);
    let near = ui.fs_zone_at.get().is_some_and(|t| t.elapsed() <= Duration::from_millis(delay as u64 + 100));
    let reveal = ui.win.is_fullscreen() && (!hide || near);
    if reveal != ui.fs_reveal.get() {
        ui.fs_reveal.set(reveal);
        // Slide in reasonably quickly, slide out more gently.
        ui.fs_revealer.set_transition_duration(if reveal { 250 } else { 400 });
        ui.fs_revealer.set_reveal_child(reveal);
        relayout(ui);
    }
}

/// Show/hide bars from the View toggles. In fullscreen the seek bar, controls and status are in the
/// panel over the video; the playlist shows unless hidden on fullscreen (or with the controls).
fn relayout(ui: &Ui) {
    let fs = ui.win.is_fullscreen();
    for (name, w) in &ui.bars {
        let on = ui.win.action_state(name).and_then(|v| v.get::<bool>()).unwrap_or(true);
        let cfg = ui.cfg.borrow();
        let fs_ok = match *name {
            // In the fullscreen panel, which slides in and out by itself.
            "view-seeker" | "view-controls" | "view-status" => true,
            "view-playlist" => !cfg.hide_playlist_fullscreen && (!cfg.hide_fullscreen_docked_panels || ui.fs_reveal.get()),
            "view-subresync" | "view-capture" | "view-navigation" => !cfg.hide_fullscreen_docked_panels || ui.fs_reveal.get(),
            _ => false,
        };
        w.set_visible(on && (!fs || fs_ok));
    }
    let caption = ui.win.action_state("view-captionmenu").and_then(|v| v.get::<bool>()).unwrap_or(true);
    ui.win.set_decorated(caption);
}

fn update(ui: &Rc<Ui>) {
    let mpv = ui.mpv;
    while let Ok(job) = ui.jobs.try_recv() {
        handle_job(ui, job);
    }
    while let Some(ev) = mpv.wait_event(0.0) {
        match ev {
            Ok(libmpv2::events::Event::FileLoaded) => {
                on_loaded(ui);
                ui.mpris.refresh(ui.mpv, player_state(ui));
                broadcast(ui, json!({"event": "NOWPLAYING", "title": prop::<String>(ui.mpv, "media-title"), "file": ui.loaded.borrow().clone(), "duration": prop::<f64>(ui.mpv, "duration")}));
            }
            Ok(libmpv2::events::Event::Seek) => {
                ui.mpris.seeked(ui.mpv);
                broadcast(ui, json!({"event": "NOTIFYSEEK", "seconds": prop::<f64>(ui.mpv, "time-pos")}));
            }
            Ok(libmpv2::events::Event::LogMessage { prefix, text, .. }) => {
                let mut errors = ui.mpv_errors.borrow_mut();
                errors.push(format!("[{prefix}] {}", text.trim_end()));
                let n = errors.len();
                errors.drain(..n.saturating_sub(50));
            }
            Ok(libmpv2::events::Event::VideoReconfig) => auto_zoom(ui),
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
    update_nav_menus(ui);
    ui.subresync.tick();
    // keep-open pauses on the last frame instead of ending the file; that's the "end of file" moment.
    if prop::<bool>(mpv, "eof-reached").unwrap_or(false) {
        if !ui.eof_handled.replace(true) {
            broadcast(ui, json!({"event": "NOTIFYENDOFSTREAM"}));
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
    sync_state_buttons(ui);
    let mpris_state = match state {
        "Stopped" => "Stopped",
        "Paused" => "Paused",
        _ => "Playing",
    };
    ui.mpris.set_status(mpris_state);
    if ui.last_state.replace(mpris_state) != mpris_state {
        broadcast(ui, json!({"event": "STATE", "state": mpris_state}));
    }
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

/// Press in the button for the current play state (and release the others).
fn sync_state_buttons(ui: &Ui) {
    let idle = prop::<bool>(ui.mpv, "idle-active").unwrap_or(true);
    let paused = prop::<bool>(ui.mpv, "pause").unwrap_or(false);
    let current = if idle || ui.stopped.get() { 2 } else if paused { 1 } else { 0 };
    for (i, b) in ui.state_buttons.iter().enumerate() {
        if b.is_active() != (i == current) {
            b.set_active(i == current);
        }
    }
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
/// Play > After Playback (one-shot): label, table action (MPC's ID_AFTERPLAYBACK_*), choice.
const AFTER_MENU: [(&str, &str); 9] = [
    ("Do no_thing", "afterplayback-donothing"),
    ("Play _next file in the folder", "afterplayback-playnext"),
    ("Turn off the _monitor", "afterplayback-monitoroff"),
    ("_Exit", "afterplayback-exit"),
    ("_Stand By", "afterplayback-standby"),
    ("_Hibernate", "afterplayback-hibernate"),
    ("Shut_down", "afterplayback-shutdown"),
    ("Log _Off", "afterplayback-logoff"),
    ("_Lock", "afterplayback-lock"),
];

fn after_choice(action: &str) -> AfterPlayback {
    use AfterPlayback::*;
    match action {
        "afterplayback-playnext" => PlayNext,
        "afterplayback-monitoroff" => MonitorOff,
        "afterplayback-exit" => Exit,
        "afterplayback-standby" => Standby,
        "afterplayback-hibernate" => Hibernate,
        "afterplayback-shutdown" => Shutdown,
        "afterplayback-logoff" => LogOff,
        "afterplayback-lock" => Lock,
        _ => DoNothing,
    }
}

/// Menus whose contents change (see `refresh_discs`, `update_nav_menus`, `refresh_favorites`).
struct NavMenus {
    disc: gio::Menu,
    chapters: gio::Menu,
    titles: gio::Menu,
    favorites: gio::Menu,
}

fn menu_model(recent: &gio::Menu, tracks: &[gio::Menu; 3], nav: &NavMenus) -> gio::Menu {
    fn section(items: &[(&str, &str)]) -> gio::Menu {
        let m = gio::Menu::new();
        for (label, action) in items {
            m.append(Some(tr(label).as_str()), Some(action));
        }
        m
    }
    let root = gio::Menu::new();

    let file = gio::Menu::new();
    file.append_section(None, &section(&[
        ("_Quick Open File...", "win.file-openquick"),
        ("_Open File/URL...", "win.file-openmedia"),
        ("Open Dir_ectory...", "win.open-dir"),
        ("Open _DVD/BD...", "win.file-opendvdbd"),
        ("Open De_vice...", "win.file-opendevice"),
    ]));
    file.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some(tr("Open Dis_c").as_str()), &nav.disc);
        m.append(Some(tr("Re_open File").as_str()), Some("win.file-reopen"));
        m
    });
    file.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some(tr("Recent _Files").as_str()), recent);
        m
    });
    file.append_section(None, &section(&[
        ("Save a _Copy...", "win.file-save-copy"),
        ("Save _Image...", "win.file-save-image"),
        ("Save Image (_auto)", "win.file-save-image-auto"),
        ("Save _Thumbnails...", "win.file-save-thumbnails"),
        ("Cop_y Image to Clipboard", "win.copy-image"),
    ]));
    file.append_section(None, &section(&[("_Load Subtitle...", "win.file-subtitles-load"), ("_Save Subtitle...", "win.file-subtitles-save")]));
    file.append_section(None, &section(&[("P_roperties", "win.file-properties")]));
    file.append_section(None, &section(&[("_Close", "win.file-close-and-restore")]));
    file.append_section(None, &section(&[("E_xit", "win.file-exit")]));
    root.append_submenu(Some(tr("_File").as_str()), &file);

    let view = gio::Menu::new();
    let bars: Vec<(String, String)> = BARS.iter().map(|(a, l, _)| (l.replace("&&", "&"), format!("win.{a}"))).collect();
    let mut bars: Vec<(&str, &str)> = bars.iter().map(|(l, a)| (l.as_str(), a.as_str())).collect();
    bars.push(("Su_bresync", "win.view-subresync"));
    bars.push(("Captu_re", "win.view-capture"));
    bars.push(("Na_vigation", "win.view-navigation"));
    bars.push(("_Playlist", "win.view-playlist"));
    view.append_section(None, &section(&bars));
    let presets = section(&[
        ("_Minimal", "win.view-presets-minimal"),
        ("_Compact", "win.view-presets-compact"),
        ("_Normal", "win.view-presets-normal"),
    ]);
    view.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some(tr("_Presets").as_str()), &presets);
        m.append(Some(tr("F_ull Screen").as_str()), Some("win.view-fullscreen"));
        m.append_submenu(Some(tr("_Zoom").as_str()), &section(&[
            ("_50%", "win.view-zoom-50"),
            ("_100%", "win.view-zoom-100"),
            ("_200%", "win.view-zoom-200"),
            ("Auto _Fit", "win.view-zoom-autofit"),
            ("Auto Fit (_Larger Only)", "win.view-zoom-autofit-larger"),
        ]));
        m
    });
    view.append_section(None, &{
        let m = gio::Menu::new();
        let renderer = section(&[
            ("_Display Statistics", "win.view-display-renderer-stats"),
            ("Display Current T_ime", "win.view-osd-display-time"),
            ("Show _File Name", "win.view-osd-show-filename"),
            ("_VSync (sync video to display)", "win.view-vsync"),
        ]);
        m.append_submenu(Some(tr("R_enderer Settings").as_str()), &renderer);
        let frame = gio::Menu::new();
        let modes = gio::Menu::new();
        for (label, action, _) in FRAMES {
            let item = gio::MenuItem::new(Some(tr(label).as_str()), None);
            item.set_action_and_target_value(Some("win.video-frame"), Some(&action.to_variant()));
            modes.append_item(&item);
        }
        frame.append_section(None, &modes);
        let aspect = gio::Menu::new();
        for (label, value) in ASPECTS {
            let item = gio::MenuItem::new(Some(tr(label).as_str()), None);
            item.set_action_and_target_value(Some("win.aspect"), Some(&value.to_variant()));
            aspect.append_item(&item);
        }
        frame.append_section(None, &{
            let m = gio::Menu::new();
            m.append_submenu(Some(tr("Override _Aspect Ratio").as_str()), &aspect);
            m.append(Some(tr("D_einterlace").as_str()), Some("win.deinterlace"));
            m
        });
        m.append_submenu(Some(tr("_Video Frame").as_str()), &frame);
        let pns = gio::Menu::new();
        pns.append_section(None, &section(&[
            ("_Increase Size", "win.view-incsize"),
            ("_Decrease Size", "win.view-decsize"),
            ("I_ncrease Width", "win.view-incwidth"),
            ("D_ecrease Width", "win.view-decwidth"),
            ("In_crease Height", "win.view-incheight"),
            ("Decre_ase Height", "win.view-decheight"),
        ]));
        pns.append_section(None, &section(&[
            ("Move _Right", "win.panscan-moveright"),
            ("Move _Left", "win.panscan-moveleft"),
            ("Move _Up", "win.panscan-moveup"),
            ("Move _Down", "win.panscan-movedown"),
            ("Cen_ter", "win.panscan-center"),
        ]));
        pns.append_section(None, &section(&[
            ("Rotate _Clockwise", "win.panscan-rotatezm"),
            ("Rotate Counterc_lockwise", "win.panscan-rotatezp"),
            ("_Flip Vertically", "win.panscan-rotatexm"),
            ("_Mirror", "win.panscan-rotateym"),
        ]));
        pns.append_section(None, &section(&[("Re_set", "win.view-reset")]));
        pns.append_section(Some(tr("Presets").as_str()), &pns_presets_menu());
        m.append_submenu(Some(tr("_Pan&Scan").as_str()), &pns);
        m
    });
    view.append_section(None, &section(&[("_Options...", "win.view-options")]));
    root.append_submenu(Some(tr("_View").as_str()), &view);

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
        m.append_submenu(Some(tr("_Repeat").as_str()), &repeat);
        let after = gio::Menu::new();
        for (label, value) in AFTER_MENU {
            let item = gio::MenuItem::new(Some(tr(label).as_str()), None);
            item.set_action_and_target_value(Some("win.after-playback"), Some(&value.to_variant()));
            after.append_item(&item);
        }
        m.append_submenu(Some(tr("Af_ter Playback").as_str()), &after);
        m.append_submenu(Some(tr("A-_B Repeat").as_str()), &section(&[("Set _A (start)", "win.ab-set-a"), ("Set _B (end)", "win.ab-set-b"), ("_Clear", "win.ab-clear")]));
        m
    });
    play.append_section(None, &section(&[
        ("_Decrease Rate", "win.play-decrate"),
        ("_Increase Rate", "win.play-incrate"),
        ("R_eset Rate", "win.play-resetrate"),
    ]));
    play.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some(tr("S_haders").as_str()), &section(&[("_Enabled", "win.shaders-enabled"), ("Shader _Editor...", "win.shader-editor")]));
        m
    });
    play.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some(tr("_Audio Track").as_str()), &tracks[0]);
        m.append_submenu(Some(tr("Su_btitle Track").as_str()), &tracks[1]);
        m.append_submenu(Some(tr("Vide_o Track").as_str()), &tracks[2]);
        m
    });
    play.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some(tr("_Volume").as_str()), &section(&[("_Up", "win.volume-up"), ("_Down", "win.volume-down"), ("_Mute", "win.volume-mute")]));
        m
    });
    root.append_submenu(Some(tr("_Play").as_str()), &play);

    let navigate = section(&[("_Previous", "win.navigate-skipback"), ("_Next", "win.navigate-skipforward"), ("_Go To...", "win.navigate-goto")]);
    navigate.append_section(None, &section(&[("Tuner _Scan...", "win.navigate-tunerscan")]));
    navigate.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some(tr("_Chapters").as_str()), &nav.chapters);
        m.append_submenu(Some(tr("_Titles").as_str()), &nav.titles);
        m
    });
    root.append_submenu(Some(tr("_Navigate").as_str()), &navigate);
    root.append_submenu(Some(tr("F_avorites").as_str()), &nav.favorites);
    root.append_submenu(Some(tr("_Help").as_str()), &section(&[("_About...", "win.about")]));
    root
}

/// View > Pan&Scan > presets (from the config; MPC's three defaults unless changed).
fn pns_presets_menu() -> gio::Menu {
    let m = gio::Menu::new();
    for (i, p) in config::load().pan_scan_presets.iter().enumerate() {
        let item = gio::MenuItem::new(Some(&p.name.replace('_', "__")), None);
        item.set_action_and_target_value(Some("win.pns-preset"), Some(&(i as u32).to_variant()));
        m.append_item(&item);
    }
    m
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

    // View > Zoom: resize the window around the video.
    for (name, zoom) in [("view-zoom-50", Zoom::Half), ("view-zoom-100", Zoom::Normal), ("view-zoom-200", Zoom::Double), ("view-zoom-autofit", Zoom::AutoFit), ("view-zoom-autofit-larger", Zoom::AutoFitLarger)] {
        let ui = ui.clone();
        add(name, Box::new(move || {
            let fit = ui.cfg.borrow().autofit_factor;
            view::zoom_window(&ui.win, &ui.video, ui.mpv, zoom, fit);
        }));
    }

    // View > Video Frame: the table's commands, and a radio action for the menu.
    for (_, name, frame) in FRAMES {
        let ui = ui.clone();
        add(name, Box::new(move || set_frame(&ui, frame)));
    }
    let frame = gio::SimpleAction::new_stateful("video-frame", Some(glib::VariantTy::STRING), &"".to_variant());
    frame.connect_activate({
        let ui = ui.clone();
        move |_, v| {
            if let Some(f) = v.and_then(|v| v.get::<String>()).and_then(|n| FRAMES.iter().find(|f| f.1 == n)) {
                set_frame(&ui, f.2);
            }
        }
    });
    win.add_action(&frame);
    add("view-vf-switchzoom", Box::new({
        let ui = ui.clone();
        move || {
            use VideoFrame::*;
            let next = match ui.cfg.borrow().video_frame {
                FromInside => Zoom1,
                Zoom1 => Zoom2,
                Zoom2 => FromOutside,
                _ => FromInside,
            };
            set_frame(&ui, next);
        }
    }));
    let aspect = gio::SimpleAction::new_stateful("aspect", Some(glib::VariantTy::STRING), &"no".to_variant());
    aspect.connect_activate(move |a, v| {
        if let Some(value) = v.and_then(|v| v.get::<String>()) {
            let _ = mpv.set_property("video-aspect-override", value.as_str());
            a.set_state(&value.to_variant());
            let label = ASPECTS.iter().find(|x| x.1 == value).map_or(value.clone(), |x| x.0.replace('_', ""));
            osd(mpv, &format!("Aspect Ratio: {label}"));
        }
    });
    win.add_action(&aspect);
    add("aspectratio-next", Box::new({
        let win = win.clone();
        move || {
            let cur = win.action_state("aspect").and_then(|v| v.get::<String>()).unwrap_or_default();
            let i = ASPECTS.iter().position(|a| a.1 == cur).map_or(0, |i| (i + 1) % ASPECTS.len());
            gio::prelude::ActionGroupExt::activate_action(&win, "aspect", Some(&ASPECTS[i].1.to_variant()));
        }
    }));
    let deint = gio::SimpleAction::new_stateful("deinterlace", None, &false.to_variant());
    deint.connect_change_state(move |a, v| {
        if let Some(on) = v.and_then(|v| v.get::<bool>()) {
            a.set_state(&on.to_variant());
            let _ = mpv.set_property("deinterlace", on);
            osd(mpv, if on { "Deinterlace: on" } else { "Deinterlace: off" });
        }
    });
    win.add_action(&deint);

    // Pan & scan (numpad by default), presets and rotation.
    for (name, gx, gy, mx, my) in [
        ("view-incsize", 1, 1, 0, 0), ("view-decsize", -1, -1, 0, 0), ("view-incwidth", 1, 0, 0, 0), ("view-decwidth", -1, 0, 0, 0),
        ("view-incheight", 0, 1, 0, 0), ("view-decheight", 0, -1, 0, 0), ("panscan-moveleft", 0, 0, -1, 0), ("panscan-moveright", 0, 0, 1, 0),
        ("panscan-moveup", 0, 0, 0, -1), ("panscan-movedown", 0, 0, 0, 1), ("panscan-moveupleft", 0, 0, -1, -1),
        ("panscan-moveupright", 0, 0, 1, -1), ("panscan-movedownleft", 0, 0, -1, 1), ("panscan-movedownright", 0, 0, 1, 1),
    ] {
        let ui = ui.clone();
        add(name, Box::new(move || pan_scan(&ui, |p| p.step(gx, gy, mx, my))));
    }
    add("panscan-center", Box::new({
        let ui = ui.clone();
        move || pan_scan(&ui, |p| (p.pos_x, p.pos_y) = (0.5, 0.5))
    }));
    add("view-reset", Box::new({
        let ui = ui.clone();
        move || {
            pan_scan(&ui, |p| *p = PanScan::default());
            let _ = mpv.set_property("video-rotate", 0i64);
            let _ = mpv.set_property("vf", "");
            osd(mpv, "Pan&Scan: reset");
        }
    }));
    let preset = gio::SimpleAction::new("pns-preset", Some(glib::VariantTy::UINT32));
    preset.connect_activate({
        let ui = ui.clone();
        move |_, v| {
            let i = v.and_then(|v| v.get::<u32>()).unwrap_or(0) as usize;
            let Some(p) = ui.cfg.borrow().pan_scan_presets.get(i).cloned() else { return };
            pan_scan(&ui, |s| *s = PanScan { zoom_x: p.zoom_x, zoom_y: p.zoom_y, pos_x: p.pos_x, pos_y: p.pos_y });
            osd(mpv, &p.name);
        }
    });
    win.add_action(&preset);
    for (name, deg) in [("panscan-rotatezp", 270), ("panscan-rotatezm", 90)] {
        add(name, Box::new(move || {
            let cur = prop::<i64>(mpv, "video-rotate").unwrap_or(0);
            let new = (cur + deg) % 360;
            let _ = mpv.set_property("video-rotate", new);
            osd(mpv, &format!("Rotation: {new}°"));
        }));
    }
    // MPC's X/Y "rotate -" flip the picture by 180 degrees around that axis.
    add("panscan-rotatexm", Box::new(move || cmd(mpv, "vf", &["toggle", "vflip"])));
    add("panscan-rotateym", Box::new(move || cmd(mpv, "vf", &["toggle", "hflip"])));

    // OSD and statistics.
    add("view-osd-display-time", Box::new(move || osd(mpv, "${time-pos/full} / ${duration/full}")));
    add("view-osd-show-filename", Box::new(move || osd(mpv, "${filename}")));
    add("view-display-renderer-stats", Box::new(move || cmd(mpv, "script-binding", &["stats/display-stats-toggle"])));
    add("view-vsync", Box::new({
        let ui = ui.clone();
        move || {
            let on = !ui.cfg.borrow().sync_to_display;
            ui.cfg.borrow_mut().sync_to_display = on;
            save_config(&ui);
            apply_video(&ui);
            osd(mpv, if on { "Sync video to display: on" } else { "Sync video to display: off" });
        }
    }));

    // Images
    add("file-save-image", Box::new({
        let ui = ui.clone();
        move || save_image_dialog(&ui)
    }));
    add("file-save-image-auto", Box::new({
        let ui = ui.clone();
        move || {
            let (dir, ext) = (ui.cfg.borrow().snapshot_dir.clone(), ui.cfg.borrow().snapshot_ext.clone());
            let path = Path::new(&dir).join(format!("{}.{ext}", snapshot_name(&ui)));
            cmd(mpv, "screenshot-to-file", &[&path.to_string_lossy(), "subtitles"]);
            osd(mpv, &format!("Image saved to {}", path.display()));
        }
    }));
    add("file-save-thumbnails", Box::new({
        let ui = ui.clone();
        move || {
            let name = format!("{}_thumbs.{}", snapshot_name(&ui).rsplit_once('_').map_or("thumbs".into(), |(s, _)| s.to_string()), ui.cfg.borrow().snapshot_ext);
            let ui2 = ui.clone();
            gtk::FileDialog::builder().title("Save Thumbnails").initial_name(name.as_str()).initial_folder(&gio::File::for_path(&ui.cfg.borrow().snapshot_dir)).build().save(
                Some(&ui.win),
                gio::Cancellable::NONE,
                move |res| {
                    let Some(path) = res.ok().and_then(|f| f.path()) else { return };
                    let c = ui2.cfg.borrow();
                    let win = ui2.win.clone();
                    view::save_thumbnails(mpv, path, c.thumb_rows, c.thumb_cols, c.thumb_width, move |r| match r {
                        Ok(p) => osd(mpv, &format!("Thumbnails saved to {}", p.display())),
                        Err(e) => gtk::AlertDialog::builder().message("Couldn't save thumbnails").detail(e.as_str()).build().show(Some(&win)),
                    });
                },
            );
        }
    }));
    add("copy-image", Box::new({
        let win = win.clone();
        move || {
            let tmp = glib::user_runtime_dir().join(format!("mpc-rs-copy-{}.png", std::process::id()));
            cmd(mpv, "screenshot-to-file", &[&tmp.to_string_lossy(), "subtitles"]);
            if let Ok(t) = gdk::Texture::from_filename(&tmp) {
                WidgetExt::display(&win).clipboard().set_texture(&t);
                osd(mpv, "Image copied to the clipboard");
            }
            let _ = std::fs::remove_file(&tmp);
        }
    }));

    // Shaders
    let shaders_on = gio::SimpleAction::new_stateful("shaders-enabled", None, &true.to_variant());
    shaders_on.connect_change_state({
        let ui = ui.clone();
        move |a, v| {
            if let Some(on) = v.and_then(|v| v.get::<bool>()) {
                a.set_state(&on.to_variant());
                ui.shaders_on.set(on);
                apply_shaders(&ui);
                osd(mpv, if on { "Shaders: on" } else { "Shaders: off" });
            }
        }
    });
    win.add_action(&shaders_on);
    add("shader-editor", Box::new({
        let ui = ui.clone();
        move || {
            let (u1, u2) = (ui.clone(), ui.clone());
            shaders::editor(ui.win.upcast_ref(), move || apply_shaders(&u1), move || u2.mpv_errors.take());
        }
    }));

    // Discs, devices
    add("file-opendvdbd", Box::new({
        let ui = ui.clone();
        move || {
            let ui2 = ui.clone();
            gtk::FileDialog::builder().title("Open DVD/BD (choose the disc or its VIDEO_TS / BDMV folder)").build().select_folder(Some(&ui.win), gio::Cancellable::NONE, move |res| {
                let Some(dir) = res.ok().and_then(|f| f.path()) else { return };
                match dialogs::disc_url(&dir) {
                    Some(_) => open_targets(&ui2, &[dir.to_string_lossy().into_owned()], false),
                    None => gtk::AlertDialog::builder().message("Not a DVD or Blu-ray").detail("Choose a folder with a VIDEO_TS (DVD) or BDMV (Blu-ray) folder in it.").build().show(Some(&ui2.win)),
                }
            });
        }
    }));
    let open_disc = gio::SimpleAction::new("open-disc", Some(glib::VariantTy::STRING));
    open_disc.connect_activate({
        let ui = ui.clone();
        move |_, v| {
            if let Some(url) = v.and_then(|v| v.get::<String>()) {
                open_targets(&ui, &[url], false);
            }
        }
    });
    win.add_action(&open_disc);
    let no_op = gio::SimpleAction::new("no-op", None);
    no_op.set_enabled(false);
    win.add_action(&no_op);
    add("file-opendevice", Box::new({
        let ui = ui.clone();
        move || open_device_dialog(&ui)
    }));
    add("file-reopen", Box::new({
        let ui = ui.clone();
        move || {
            let cur = ui.pl.borrow().current;
            if let Some(i) = cur {
                play(&ui, i);
            }
        }
    }));
    let show_capture = gio::SimpleAction::new_stateful("view-capture", None, &false.to_variant());
    show_capture.connect_change_state({
        let ui = ui.clone();
        move |a, v| {
            if let Some(v) = v {
                a.set_state(v);
                relayout(&ui);
            }
        }
    });
    win.add_action(&show_capture);

    // DVB: tuner scan and the Navigation bar.
    add("navigate-tunerscan", Box::new({
        let ui = ui.clone();
        move || {
            let ui2 = ui.clone();
            dvb::scan_dialog(ui.win.upcast_ref(), move || {
                refresh_channels(&ui2);
                ui2.win.change_action_state("view-navigation", &true.to_variant());
            });
        }
    }));
    let show_nav = gio::SimpleAction::new_stateful("view-navigation", None, &false.to_variant());
    show_nav.connect_change_state({
        let ui = ui.clone();
        move |a, v| {
            if let Some(v) = v {
                a.set_state(v);
                relayout(&ui);
            }
        }
    });
    win.add_action(&show_nav);

    // Navigate
    add("navigate-goto", Box::new({
        let win = win.clone();
        move || dialogs::goto(win.upcast_ref(), mpv)
    }));
    for (name, property) in [("chapter", "chapter"), ("disc-title", "disc-title")] {
        let a = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
        a.connect_activate(move |_, v| {
            if let Some(i) = v.and_then(|v| v.get::<String>()) {
                let _ = mpv.set_property(property, i.as_str());
            }
        });
        win.add_action(&a);
    }
    add("dvd-angle-next", Box::new(move || cmd(mpv, "cycle", &["angle"])));
    add("dvd-angle-prev", Box::new(move || cmd(mpv, "cycle", &["angle", "down"])));
    add("dvd-audio-next", Box::new(move || cycle_track(mpv, "audio", "aid", 1)));
    add("dvd-audio-prev", Box::new(move || cycle_track(mpv, "audio", "aid", -1)));
    add("dvd-sub-next", Box::new(move || cycle_track(mpv, "sub", "sid", 1)));
    add("dvd-sub-prev", Box::new(move || cycle_track(mpv, "sub", "sid", -1)));
    add("dvd-sub-onoff", Box::new(move || cmd(mpv, "cycle", &["sub-visibility"])));

    // A-B repeat (mpv's ab-loop)
    for (name, point) in [("ab-set-a", "ab-loop-a"), ("ab-set-b", "ab-loop-b")] {
        add(name, Box::new(move || {
            let t = prop::<f64>(mpv, "time-pos").unwrap_or(0.0);
            let _ = mpv.set_property(point, t);
            osd(mpv, &format!("A-B repeat: {} = {}", if point.ends_with('a') { "A" } else { "B" }, hms(t)));
        }));
    }
    add("ab-clear", Box::new(move || {
        let _ = mpv.set_property("ab-loop-a", "no");
        let _ = mpv.set_property("ab-loop-b", "no");
        osd(mpv, "A-B repeat: off");
    }));

    // File: properties, save a copy
    add("file-properties", Box::new({
        let ui = ui.clone();
        move || {
            let path = ui.loaded.borrow().clone();
            if let Some(p) = path {
                dialogs::properties(ui.win.upcast_ref(), mpv, &p);
            }
        }
    }));
    add("file-save-copy", Box::new({
        let ui = ui.clone();
        move || {
            let path = ui.loaded.borrow().clone();
            match path {
                Some(p) if !dialogs::is_disc_url(&p) && !p.starts_with("av://") => dialogs::save_copy(ui.win.upcast_ref(), &p),
                _ => gtk::AlertDialog::builder().message("Nothing to save").detail("Save a Copy works for files and web streams, not discs or devices.").build().show(Some(&ui.win)),
            }
        }
    }));

    // Favorites
    add("favorites-add", Box::new({
        let ui = ui.clone();
        move || add_favorite_dialog(&ui)
    }));
    add("favorites-quickaddfavorite", Box::new({
        let ui = ui.clone();
        move || {
            let name = ui.loaded.borrow().clone().map(|p| Entry::new(p).title());
            if let Some(n) = name {
                add_favorite(&ui, n.clone(), true);
                osd(mpv, &format!("Added to favorites: {n}"));
            }
        }
    }));
    add("favorites-organize", Box::new({
        let ui = ui.clone();
        move || {
            let favs = ui.history.borrow().favorites.clone();
            let ui2 = ui.clone();
            dialogs::organize_favorites(ui.win.upcast_ref(), favs, move |new| {
                ui2.history.borrow_mut().favorites = new;
                config::save_history(&ui2.history.borrow());
                refresh_favorites(&ui2);
            });
        }
    }));
    let open_fav = gio::SimpleAction::new("open-favorite", Some(glib::VariantTy::UINT32));
    open_fav.connect_activate({
        let ui = ui.clone();
        move |_, v| {
            let i = v.and_then(|v| v.get::<u32>()).unwrap_or(0) as usize;
            let Some(f) = ui.history.borrow().favorites.get(i).cloned() else { return };
            ui.start_at.set(f.position);
            open_targets(&ui, &[f.path], false);
        }
    });
    win.add_action(&open_fav);

    // Play > After Playback: one-shot choices (radio items) and MPC's commands for them.
    let after = gio::SimpleAction::new_stateful("after-playback", Some(glib::VariantTy::STRING), &"".to_variant());
    after.connect_activate({
        let ui = ui.clone();
        move |a, v| {
            if let Some(name) = v.and_then(|v| v.get::<String>()) {
                a.set_state(&name.to_variant());
                ui.after_override.set(Some(after_choice(&name)));
                osd(mpv, &format!("After playback: {}", after_choice(&name).label()));
            }
        }
    });
    win.add_action(&after);
    for (_, name) in AFTER_MENU {
        let win2 = win.clone();
        add(name, Box::new(move || gio::prelude::ActionGroupExt::activate_action(&win2, "after-playback", Some(&name.to_variant()))));
    }

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
