//! Main window: menu bar, video, seek bar, toolbar, info/statistics/status bars.
//! Layout and default keys follow MPC-HC (`MainFrm.cpp`, `default_wmcmds` in `AppSettings.cpp`).

use crate::{
    commands::{self, Binding, COMMANDS, Click, Media, Mouse},
    config,
    mpris::Mpris,
    options, video,
};
use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use libmpv2::{GetData, Mpv};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

const SUB_EXTS: &[&str] = &["srt", "ass", "ssa", "sub", "idx", "sup", "vtt", "smi", "usf"];
const VOLUME_STEP: &str = "5"; // MPC nVolumeStep
// MPC DEFAULT_JUMPDISTANCE_1/2/3 (1000 / 5000 / 20000 ms)
const SEEK_SMALL: i32 = 1;
const SEEK_MED: i32 = 5;
const SEEK_LARGE: i32 = 20;

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
    let current = ui.bindings.borrow().clone();
    let ui = ui.clone();
    options::show(&ui.win.clone(), current, implemented, move |new| {
        *ui.bindings.borrow_mut() = new;
        apply_keys(&ui);
        let mut cfg = config::load();
        cfg.keys = commands::overrides(&ui.bindings.borrow());
        config::save(&cfg);
    });
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

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for w in [menubar.upcast_ref::<gtk::Widget>(), overlay.upcast_ref(), info.upcast_ref(), stats.upcast_ref(), seek.upcast_ref(), controls.upcast_ref(), status.upcast_ref()] {
        vbox.append(w);
    }
    win.set_child(Some(&vbox));

    // Context menu (MPC's "Player Menu", right click by default): the same menus as the menu bar.
    let menu = gtk::PopoverMenu::from_model(Some(&menu_model()));
    menu.set_parent(&overlay);
    menu.set_has_arrow(false);
    menu.set_halign(gtk::Align::Start);
    overlay.connect_destroy({
        let menu = menu.clone();
        move |_| menu.unparent()
    });

    let bindings = Rc::new(RefCell::new(commands::load(&config::load().keys)));
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
    apply_keys(&ui);

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
        let fs_ok = ui.fs_reveal.get() && matches!(*name, "view-seeker" | "view-controls");
        w.set_visible(on && (!fs || fs_ok));
    }
    let caption = ui.win.action_state("view-captionmenu").and_then(|v| v.get::<bool>()).unwrap_or(true);
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
        ("_Quick Open File...", "win.file-openquick"),
        ("_Open File/URL...", "win.file-openmedia"),
        ("Open Dir_ectory...", "win.open-dir"),
    ]));
    file.append_section(None, &section(&[("_Close", "win.file-close-and-restore")]));
    file.append_section(None, &section(&[("E_xit", "win.file-exit")]));
    root.append_submenu(Some("_File"), &file);

    let view = gio::Menu::new();
    let bars: Vec<(String, String)> = BARS.iter().map(|(a, l, _)| (l.replace("&&", "&"), format!("win.{a}"))).collect();
    let bars: Vec<(&str, &str)> = bars.iter().map(|(l, a)| (l.as_str(), a.as_str())).collect();
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
    let seek = |name: &str, secs: i32| add(name, Box::new(move || cmd(mpv, "seek", &[&secs.to_string()])));

    // File
    add("file-openquick", Box::new({
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
    add("file-openmedia", Box::new({
        let win = win.clone();
        move || open_dialog(&win, mpv)
    }));
    add("open-dir", Box::new({
        let win = win.clone();
        move || {
            gtk::FileDialog::builder().title("Open Directory").build().select_folder(Some(&win), gio::Cancellable::NONE, move |res| {
                if let Ok(dir) = res {
                    open_files(mpv, &[dir], false);
                }
            });
        }
    }));
    add("file-close-and-restore", Box::new(move || cmd(mpv, "stop", &[])));
    add("file-exit", Box::new({
        let win = win.clone();
        move || win.close()
    }));

    // Play
    add("play-playpause", Box::new({
        let ui = ui.clone();
        move || {
            ui.stopped.set(false);
            cmd(mpv, "cycle", &["pause"]);
        }
    }));
    add("play-play", Box::new(move || {
        let _ = mpv.set_property("pause", false);
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
    seek("play-seekforwardsmall", SEEK_SMALL);
    seek("play-seekbackwardsmall", -SEEK_SMALL);
    seek("play-seekforwardmed", SEEK_MED);
    seek("play-seekbackwardmed", -SEEK_MED);
    seek("play-seekforwardlarge", SEEK_LARGE);
    seek("play-seekbackwardlarge", -SEEK_LARGE);
    add("play-seekset", Box::new(move || cmd(mpv, "seek", &["0", "absolute"])));
    add("volume-up", Box::new(move || cmd(mpv, "add", &["volume", VOLUME_STEP])));
    add("volume-down", Box::new(move || cmd(mpv, "add", &["volume", &format!("-{VOLUME_STEP}")])));
    add("volume-mute", Box::new(move || cmd(mpv, "cycle", &["mute"])));

    // Navigate: chapters first, then files (MPC "skip back/forward"); the "file" variants skip files only.
    let skip = move |dir: i64| {
        if prop::<i64>(mpv, "chapters").unwrap_or(0) > 0 {
            cmd(mpv, "add", &["chapter", &dir.to_string()]);
        } else {
            cmd(mpv, if dir > 0 { "playlist-next" } else { "playlist-prev" }, &[]);
        }
    };
    add("navigate-skipback", Box::new(move || skip(-1)));
    add("navigate-skipforward", Box::new(move || skip(1)));
    add("navigate-skipbackfile", Box::new(move || cmd(mpv, "playlist-prev", &[])));
    add("navigate-skipforwardfile", Box::new(move || cmd(mpv, "playlist-next", &[])));

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
