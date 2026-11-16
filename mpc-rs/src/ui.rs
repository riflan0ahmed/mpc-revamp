//! Main window: menu bar, video, seek bar, toolbar, info/statistics/status bars.
//! Layout and default keys follow MPC-HC (`MainFrm.cpp`, `default_wmcmds` in `AppSettings.cpp`).

use crate::video;
use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use libmpv2::{GetData, Mpv};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

const SUB_EXTS: &[&str] = &["srt", "ass", "ssa", "sub", "idx", "sup", "vtt", "smi", "usf"];
const VOLUME_STEP: &str = "5"; // MPC nVolumeStep
const SEEK_MED: &str = "5"; // MPC DEFAULT_JUMPDISTANCE_2 (5000 ms)

/// Bars that the View menu toggles, in top-to-bottom order: (action name, label, shown in Normal preset).
const BARS: &[(&str, &str, bool)] = &[
    ("caption", "Caption&&Menu", true),
    ("info", "Information", false),
    ("stats", "Statistics", false),
    ("seekbar", "Seek Bar", true),
    ("controls", "Controls", true),
    ("status", "Status", true),
];

struct Ui {
    win: gtk::ApplicationWindow,
    mpv: &'static Mpv,
    video: gtk::GLArea,
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
}

fn act(win: &gtk::ApplicationWindow, name: &str) {
    gio::prelude::ActionGroupExt::activate_action(win, name, None);
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

/// Open files/URLs. Subtitles are added to the current video; everything else goes to the playlist.
pub fn open_targets(mpv: &Mpv, targets: &[String], append: bool) {
    let playing = !prop::<bool>(mpv, "idle-active").unwrap_or(true);
    let mut replace = !append;
    for t in targets {
        if playing && is_sub(t) {
            cmd(mpv, "sub-add", &[t]);
        } else {
            cmd(mpv, "loadfile", &[t, if replace { "replace" } else { "append-play" }]);
            replace = false;
        }
    }
}

pub fn open_files(mpv: &Mpv, files: &[gio::File], append: bool) {
    let targets: Vec<String> = files
        .iter()
        .map(|f| f.path().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| f.uri().into()))
        .collect();
    open_targets(mpv, &targets, append);
}

pub fn window(app: &gtk::Application, mpv: &'static Mpv) {
    if let Some(win) = app.active_window() {
        win.present();
        return;
    }

    let win = gtk::ApplicationWindow::builder()
        .application(app)
        .title("mpc-rs")
        .default_width(960)
        .default_height(620)
        .build();

    // --- widgets ---
    let menubar = gtk::PopoverMenuBar::from_model(Some(&menu_model()));

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
        ("media-playback-start-symbolic", "win.play", "Play"),
        ("media-playback-pause-symbolic", "win.pause", "Pause"),
        ("media-playback-stop-symbolic", "win.stop", "Stop"),
        ("media-skip-backward-symbolic", "win.prev", "Previous"),
        ("media-skip-forward-symbolic", "win.next", "Next"),
        ("go-last-symbolic", "win.frame-step", "Frame Step"),
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

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for w in [menubar.upcast_ref::<gtk::Widget>(), overlay.upcast_ref(), info.upcast_ref(), stats.upcast_ref(), seek.upcast_ref(), controls.upcast_ref(), status.upcast_ref()] {
        vbox.append(w);
    }
    win.set_child(Some(&vbox));

    let ui = Rc::new(Ui {
        win: win.clone(),
        mpv,
        video: video.clone(),
        logo,
        bars: vec![
            ("caption", menubar.upcast()),
            ("info", info.clone().upcast()),
            ("stats", stats.clone().upcast()),
            ("seekbar", seek.clone().upcast()),
            ("controls", controls.upcast()),
            ("status", status.upcast()),
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
    });

    add_actions(app, &ui);

    // --- seek bar ---
    seek.connect_change_value({
        let ui = ui.clone();
        move |_, _, v| {
            ui.last_user_seek.set(Instant::now());
            cmd(ui.mpv, "seek", &[&v.to_string(), "absolute"]);
            glib::Propagation::Proceed
        }
    });
    seek.connect_query_tooltip(|s, x, _, _, tip| {
        let adj = s.adjustment();
        if adj.upper() <= 1.0 {
            return false;
        }
        let frac = (x as f64 / s.width().max(1) as f64).clamp(0.0, 1.0);
        tip.set_text(Some(&hms(adj.lower() + frac * (adj.upper() - adj.lower()))));
        true
    });

    // --- volume ---
    volume.connect_value_changed(move |v| {
        let _ = mpv.set_property("volume", v.value());
    });
    mute.connect_toggled(move |b| {
        let _ = mpv.set_property("mute", b.is_active());
        b.set_icon_name(if b.is_active() { "audio-volume-muted-symbolic" } else { "audio-volume-high-symbolic" });
    });

    // --- mouse on video: click = play/pause, double-click = fullscreen, wheel = volume (MPC defaults) ---
    let click = gtk::GestureClick::new();
    click.connect_released({
        let win = win.clone();
        move |_, n, _, _| {
            act(&win, "play-pause");
            if n == 2 {
                act(&win, "fullscreen");
            }
        }
    });
    overlay.add_controller(click);
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.connect_scroll({
        let win = win.clone();
        move |_, _, dy| {
            act(&win, if dy < 0.0 { "volume-up" } else { "volume-down" });
            glib::Propagation::Stop
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
            let reveal = ui.win.is_fullscreen() && y > ui.win.height() as f64 - 150.0;
            if reveal != ui.fs_reveal.get() {
                ui.fs_reveal.set(reveal);
                relayout(&ui);
            }
        }
    });
    win.add_controller(motion);
    win.connect_fullscreened_notify({
        let ui = ui.clone();
        move |_| {
            ui.fs_reveal.set(false);
            relayout(&ui);
        }
    });

    // --- drag and drop: files, folders, subtitles, URLs ---
    let drop = gtk::DropTarget::new(glib::Type::INVALID, gdk::DragAction::COPY);
    drop.set_types(&[gdk::FileList::static_type(), String::static_type()]);
    drop.connect_drop(move |_, value, _, _| {
        if let Ok(list) = value.get::<gdk::FileList>() {
            open_files(mpv, &list.files(), false);
        } else if let Ok(text) = value.get::<String>() {
            let targets: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
            open_targets(mpv, &targets, false);
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

/// Show/hide bars from the View toggles; in fullscreen only seek bar + controls, and only when revealed.
fn relayout(ui: &Ui) {
    let fs = ui.win.is_fullscreen();
    for (name, w) in &ui.bars {
        let on = ui.win.action_state(name).and_then(|v| v.get::<bool>()).unwrap_or(true);
        let fs_ok = ui.fs_reveal.get() && matches!(*name, "seekbar" | "controls");
        w.set_visible(on && (!fs || fs_ok));
    }
    let caption = ui.win.action_state("caption").and_then(|v| v.get::<bool>()).unwrap_or(true);
    ui.win.set_decorated(caption);
}

fn update(ui: &Ui) {
    let mpv = ui.mpv;
    while mpv.wait_event(0.0).is_some() {} // drain; nothing listens to events yet

    let idle = prop::<bool>(mpv, "idle-active").unwrap_or(true);
    let paused = prop::<bool>(mpv, "pause").unwrap_or(false);
    let pos = prop::<f64>(mpv, "time-pos").unwrap_or(0.0);
    let dur = prop::<f64>(mpv, "duration").unwrap_or(0.0);

    // Seek bar (left alone briefly after the user moves it, so it doesn't jump back while dragging).
    if ui.last_user_seek.get().elapsed() > Duration::from_millis(300) {
        ui.seek.set_range(0.0, dur.max(1.0));
        ui.seek.set_value(pos);
    }
    let chapters = prop::<i64>(mpv, "chapters").unwrap_or(0);
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
    let state = if idle {
        "Stopped"
    } else if ui.stopped.get() {
        "Stopped"
    } else if prop::<bool>(mpv, "paused-for-cache").unwrap_or(false) {
        "Buffering"
    } else if paused {
        "Paused"
    } else {
        "Playing"
    };
    ui.state.set_label(state);
    ui.time.set_label(&if idle { String::new() } else { format!("{} / {}", hms(pos), hms(dur)) });
    let title = if idle { None } else { prop::<String>(mpv, "media-title") };
    ui.win.set_title(Some(title.as_deref().unwrap_or("mpc-rs")));
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

    // Fullscreen: hide the cursor after 2 s without mouse movement.
    if ui.win.is_fullscreen() && ui.last_motion.get().elapsed() > Duration::from_secs(2) {
        ui.video.set_cursor_from_name(Some("none"));
    }
}

fn menu_model() -> gio::Menu {
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
        ("_Quick Open File...", "win.open-quick"),
        ("_Open File/URL...", "win.open-file"),
        ("Open Dir_ectory...", "win.open-dir"),
    ]));
    file.append_section(None, &section(&[("_Close", "win.close")]));
    file.append_section(None, &section(&[("E_xit", "win.exit")]));
    root.append_submenu(Some("_File"), &file);

    let view = gio::Menu::new();
    let bars: Vec<(String, String)> = BARS.iter().map(|(a, l, _)| (l.replace("&&", "&"), format!("win.{a}"))).collect();
    let bars: Vec<(&str, &str)> = bars.iter().map(|(l, a)| (l.as_str(), a.as_str())).collect();
    view.append_section(None, &section(&bars));
    let presets = section(&[("_Minimal", "win.preset-minimal"), ("_Compact", "win.preset-compact"), ("_Normal", "win.preset-normal")]);
    view.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some("_Presets"), &presets);
        m.append(Some("F_ull Screen"), Some("win.fullscreen"));
        m
    });
    root.append_submenu(Some("_View"), &view);

    let play = gio::Menu::new();
    play.append_section(None, &section(&[("_Play/Pause", "win.play-pause"), ("_Stop", "win.stop"), ("Fra_me Step", "win.frame-step"), ("Frame Step _Back", "win.frame-back")]));
    play.append_section(None, &{
        let m = gio::Menu::new();
        m.append_submenu(Some("_Volume"), &section(&[("_Up", "win.volume-up"), ("_Down", "win.volume-down"), ("_Mute", "win.mute")]));
        m
    });
    root.append_submenu(Some("_Play"), &play);

    root.append_submenu(Some("_Navigate"), &section(&[("_Previous", "win.prev"), ("_Next", "win.next")]));
    root.append_submenu(Some("_Help"), &section(&[("_About...", "win.about")]));
    root
}

fn add_actions(app: &gtk::Application, ui: &Rc<Ui>) {
    let win = &ui.win;
    let mpv = ui.mpv;
    let add = |name: &str, accels: &[&str], f: Box<dyn Fn()>| {
        let a = gio::SimpleAction::new(name, None);
        a.connect_activate(move |_, _| f());
        win.add_action(&a);
        if !accels.is_empty() {
            app.set_accels_for_action(&format!("win.{name}"), accels);
        }
    };

    // File
    add("open-quick", &["<Control>q"], Box::new({
        let win = win.clone();
        move || {
            gtk::FileDialog::builder().title("Quick Open").build().open_multiple(Some(&win), gio::Cancellable::NONE, move |res| {
                if let Ok(model) = res {
                    let files: Vec<gio::File> = (0..model.n_items()).filter_map(|i| model.item(i).and_downcast()).collect();
                    open_files(mpv, &files, false);
                }
            });
        }
    }));
    add("open-file", &["<Control>o"], Box::new({
        let win = win.clone();
        move || open_dialog(&win, mpv)
    }));
    add("open-dir", &[], Box::new({
        let win = win.clone();
        move || {
            gtk::FileDialog::builder().title("Open Directory").build().select_folder(Some(&win), gio::Cancellable::NONE, move |res| {
                if let Ok(dir) = res {
                    open_files(mpv, &[dir], false);
                }
            });
        }
    }));
    add("close", &["<Control>c"], Box::new(move || cmd(mpv, "stop", &[])));
    add("exit", &["<Alt>x"], Box::new({
        let win = win.clone();
        move || win.close()
    }));

    // Play
    add("play-pause", &["space"], Box::new({
        let ui = ui.clone();
        move || {
            ui.stopped.set(false);
            cmd(mpv, "cycle", &["pause"]);
        }
    }));
    add("play", &[], Box::new(move || {
        let _ = mpv.set_property("pause", false);
    }));
    add("pause", &[], Box::new(move || {
        let _ = mpv.set_property("pause", true);
    }));
    add("stop", &["period"], Box::new({
        let ui = ui.clone();
        move || {
            let _ = mpv.set_property("pause", true);
            cmd(mpv, "seek", &["0", "absolute"]);
            ui.stopped.set(true);
        }
    }));
    add("frame-step", &["<Control>Right"], Box::new(move || cmd(mpv, "frame-step", &[])));
    add("frame-back", &["<Control>Left"], Box::new(move || cmd(mpv, "frame-back-step", &[])));
    add("seek-forward", &["Right"], Box::new(move || cmd(mpv, "seek", &[SEEK_MED])));
    add("seek-backward", &["Left"], Box::new(move || cmd(mpv, "seek", &[&format!("-{SEEK_MED}")])));
    add("seek-start", &["Home"], Box::new(move || cmd(mpv, "seek", &["0", "absolute"])));
    add("volume-up", &["Up"], Box::new(move || cmd(mpv, "add", &["volume", VOLUME_STEP])));
    add("volume-down", &["Down"], Box::new(move || cmd(mpv, "add", &["volume", &format!("-{VOLUME_STEP}")])));
    add("mute", &["<Control>m"], Box::new(move || cmd(mpv, "cycle", &["mute"])));

    // Navigate: chapters first, then files (MPC "skip back/forward").
    let skip = move |dir: i64| {
        if prop::<i64>(mpv, "chapters").unwrap_or(0) > 0 {
            cmd(mpv, "add", &["chapter", &dir.to_string()]);
        } else {
            cmd(mpv, if dir > 0 { "playlist-next" } else { "playlist-prev" }, &[]);
        }
    };
    add("prev", &["Page_Up"], Box::new(move || skip(-1)));
    add("next", &["Page_Down"], Box::new(move || skip(1)));

    // View: bar toggles (stateful boolean actions toggle themselves on activate).
    for (i, (name, _, _)) in BARS.iter().enumerate() {
        let a = gio::SimpleAction::new_stateful(name, None, &true.to_variant());
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
        app.set_accels_for_action(&format!("win.{name}"), &[&format!("<Control>{i}")]);
    }
    // Info and Statistics start hidden, as in MPC.
    for (name, _, normal) in BARS {
        win.change_action_state(name, &normal.to_variant());
    }
    let preset = |shown: &'static [&'static str]| {
        let win = win.clone();
        Box::new(move || {
            for (name, _, _) in BARS {
                win.change_action_state(name, &shown.contains(name).to_variant());
            }
        }) as Box<dyn Fn()>
    };
    add("preset-minimal", &["1"], preset(&[]));
    add("preset-compact", &["2"], preset(&["seekbar"]));
    add("preset-normal", &["3"], preset(&["caption", "seekbar", "controls", "status"]));
    add("fullscreen", &["<Alt>Return", "F11"], Box::new({
        let win = win.clone();
        move || win.set_fullscreened(!win.is_fullscreen())
    }));
    add("leave-fullscreen", &["Escape"], Box::new({
        let win = win.clone();
        move || win.unfullscreen()
    }));

    // Help
    add("about", &[], Box::new({
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

/// MPC's "Open" dialog: a file name or URL, a Browse button and "Add to playlist".
fn open_dialog(parent: &gtk::ApplicationWindow, mpv: &'static Mpv) {
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
            if !t.is_empty() {
                open_targets(mpv, &[t], add.is_active());
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
