//! Main window: menu bar, video, playlist, seek bar, toolbar, info/statistics/status bars.
//! Layout and default keys follow MPC-HC (`MainFrm.cpp`, `default_wmcmds` in `AppSettings.cpp`).

use crate::{
    commands::{self, Binding, COMMANDS, Click, Media, Mouse},
    config::{self, Config, History, RepeatMode},
    mpris::Mpris,
    options,
    playlist::{self, Entry, Playlist},
    playlistbar::{self, Bar, Request},
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
    cfg: RefCell<Config>,
    history: RefCell<History>,
    pl: Rc<RefCell<Playlist>>,
    plbar: Rc<Bar>,
    recent: gio::Menu,
    /// The file mpv was last asked to load (for its saved position) and whether its end was handled.
    loaded: RefCell<Option<String>>,
    eof_handled: Cell<bool>,
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
    let current = ui.bindings.borrow().clone();
    let ui = ui.clone();
    options::show(&ui.win.clone(), current, implemented, move |new| {
        *ui.bindings.borrow_mut() = new;
        apply_keys(&ui);
        ui.cfg.borrow_mut().keys = commands::overrides(&ui.bindings.borrow());
        save_config(&ui);
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
    cmd(ui.mpv, "loadfile", &[&path, "replace"]);
    let _ = ui.mpv.set_property("pause", false);
    let max = ui.cfg.borrow().recent_files_number;
    ui.history.borrow_mut().add_recent(&path, max);
    config::save_history(&ui.history.borrow());
    refresh_recent(ui);
    ui.plbar.refresh();
}

/// mpv has opened the file: add its subtitles, restore its position, note its duration.
fn on_loaded(ui: &Ui) {
    let Some(cur) = ui.pl.borrow().current else { return };
    let subs = ui.pl.borrow().items.get(cur).map(|e| e.subs.clone()).unwrap_or_default();
    for s in subs {
        cmd(ui.mpv, "sub-add", &[&s]);
    }
    if ui.cfg.borrow().remember_file_pos {
        let path = ui.loaded.borrow().clone().unwrap_or_default();
        if let Some(pos) = ui.history.borrow().position(&path) {
            cmd(ui.mpv, "seek", &[&pos.to_string(), "absolute"]);
        }
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
    let (forever, mode) = (ui.cfg.borrow().repeat_forever, ui.cfg.borrow().repeat_mode);
    if forever && mode == RepeatMode::File {
        cmd(ui.mpv, "seek", &["0", "absolute"]);
        let _ = ui.mpv.set_property("pause", false);
        return;
    }
    let next = ui.pl.borrow_mut().step(1, forever, &mut rand);
    if let Some(i) = next {
        play(ui, i);
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

    let win = gtk::ApplicationWindow::builder()
        .application(app)
        .title("mpc-rs")
        .default_width(960)
        .default_height(620)
        .build();

    // --- widgets ---
    let recent = gio::Menu::new();
    let model = menu_model(&recent);
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

    let cfg = config::load();
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

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for w in [menubar.upcast_ref::<gtk::Widget>(), paned.upcast_ref(), info.upcast_ref(), stats.upcast_ref(), seek.upcast_ref(), controls.upcast_ref(), status.upcast_ref()] {
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
    });
    UI.with_borrow_mut(|u| *u = Some(ui.clone()));

    add_actions(app, &ui);
    apply_keys(&ui);
    refresh_recent(&ui);
    win.connect_close_request({
        let ui = ui.clone();
        move |_| {
            save_position(&ui);
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

/// Show/hide bars from the View toggles; in fullscreen only seek bar + controls, and only when revealed.
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

fn relayout(ui: &Ui) {
    let fs = ui.win.is_fullscreen();
    for (name, w) in &ui.bars {
        let on = ui.win.action_state(name).and_then(|v| v.get::<bool>()).unwrap_or(true);
        let fs_ok = match *name {
            "view-seeker" | "view-controls" => ui.fs_reveal.get(),
            "view-playlist" => !ui.cfg.borrow().hide_playlist_fullscreen,
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

/// The menus, shared by the menu bar and the context menu. `recent` is filled by `refresh_recent`.
fn menu_model(recent: &gio::Menu) -> gio::Menu {
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
    file.append_section(None, &section(&[("_Close", "win.file-close-and-restore")]));
    file.append_section(None, &section(&[("E_xit", "win.file-exit")]));
    root.append_submenu(Some("_File"), &file);

    let view = gio::Menu::new();
    let bars: Vec<(String, String)> = BARS.iter().map(|(a, l, _)| (l.replace("&&", "&"), format!("win.{a}"))).collect();
    let mut bars: Vec<(&str, &str)> = bars.iter().map(|(l, a)| (l.as_str(), a.as_str())).collect();
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
        move || {
            save_position(&ui);
            cmd(mpv, "stop", &[]);
            ui.loaded.take();
            ui.pl.borrow_mut().current = None;
            ui.plbar.refresh();
        }
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
