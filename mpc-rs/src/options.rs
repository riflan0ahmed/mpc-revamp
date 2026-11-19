//! Options dialog (MPC's `PPageSheet`): a tree of pages on the left, the page on the right. Pages for
//! features that come in later phases (Formats, Web Interface, Output, Shaders, Sync, Subtitles, ...)
//! are added with those features.

use crate::{
    commands::{self, Binding, COMMANDS, Media, Mouse},
    config::{self, AfterPlayback, Config, RepeatMode, TitleBar, Zoom},
    shaders,
};
use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type Work = Rc<RefCell<Config>>;

struct Keys {
    work: RefCell<Vec<Binding>>,
    implemented: Vec<bool>,
    store: gtk::StringList, // one item per command: its index in COMMANDS
    sel: gtk::SingleSelection,
    key: gtk::Button,
    mouse: gtk::DropDown,
    mouse_fs: gtk::DropDown,
    media: gtk::DropDown,
    conflict: gtk::Label,
    recording: Cell<bool>,
    loading: Cell<bool>, // set while the editor is filled in, so its change handlers don't write back
}

/// Show the dialog on a copy of the settings. OK and Apply hand the edited copy to `on_apply`.
pub fn show(parent: &gtk::ApplicationWindow, cfg: Config, implemented: Vec<bool>, on_apply: impl Fn(Config) + 'static) {
    let dlg = gtk::Window::builder().title("Options").transient_for(parent).modal(true).default_width(1000).default_height(640).build();
    let on_apply: Rc<dyn Fn(Config)> = Rc::new(on_apply);
    let bindings = commands::load(&cfg.keys);
    let work: Work = Rc::new(RefCell::new(cfg));
    let (keys, keys_box) = keys_page(&dlg, bindings, implemented);

    // MPC's page tree; children are indented.
    let pages: Vec<(&str, bool, gtk::Widget)> = vec![
        ("Player", false, player_page(&work)),
        ("Keys", true, keys_box.upcast()),
        ("Logo", true, logo_page(&work)),
        ("Playback", false, playback_page(&work)),
        ("Output", true, output_page(&work)),
        ("Shaders", true, shaders_page(&work)),
        ("Fullscreen", true, fullscreen_page(&work)),
        ("Sync", true, sync_page(&work)),
        ("Audio", true, audio_page(&work)),
        ("Subtitles", false, subtitles_page(&work)),
        ("Default Style", true, sub_style_page(&work)),
        ("Tweaks", false, tweaks_page(&work)),
        ("Miscellaneous", false, misc_page(&work, &dlg, on_apply.clone())),
    ];
    let stack = gtk::Stack::new();
    let tree = gtk::ListBox::new();
    tree.add_css_class("navigation-sidebar");
    for (i, (title, child, page)) in pages.into_iter().enumerate() {
        stack.add_named(&page, Some(&i.to_string()));
        let label = gtk::Label::builder().label(title).xalign(0.0).margin_start(if child { 24 } else { 6 }).margin_end(12).build();
        tree.append(&label);
    }
    tree.connect_row_selected({
        let stack = stack.clone();
        move |_, row| {
            if let Some(row) = row {
                stack.set_visible_child_name(&row.index().to_string());
            }
        }
    });
    tree.select_row(tree.row_at_index(0).as_ref());

    let apply_now = {
        let (work, keys, on_apply) = (work.clone(), keys.clone(), on_apply.clone());
        move || {
            work.borrow_mut().keys = commands::overrides(&keys.work.borrow());
            on_apply(work.borrow().clone());
        }
    };
    let ok = gtk::Button::with_label("OK");
    ok.add_css_class("suggested-action");
    let cancel = gtk::Button::with_label("Cancel");
    let apply = gtk::Button::with_label("Apply");
    ok.connect_clicked({
        let (dlg, apply_now) = (dlg.clone(), apply_now.clone());
        move |_| {
            apply_now();
            dlg.close();
        }
    });
    apply.connect_clicked(move |_| apply_now());
    cancel.connect_clicked({
        let dlg = dlg.clone();
        move |_| dlg.close()
    });
    let buttons = gtk::Box::builder().spacing(8).halign(gtk::Align::End).margin_top(8).margin_bottom(12).margin_end(12).build();
    buttons.append(&ok);
    buttons.append(&cancel);
    buttons.append(&apply);

    let main = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    main.append(&gtk::ScrolledWindow::builder().child(&tree).hscrollbar_policy(gtk::PolicyType::Never).width_request(170).build());
    main.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    main.append(&stack);
    stack.set_hexpand(true);
    main.set_vexpand(true);
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.append(&main);
    vbox.append(&buttons);
    dlg.set_child(Some(&vbox));
    dlg.set_default_widget(Some(&ok));
    dlg.present();
}

// --- small widgets bound to one setting of the working copy ---

fn check(work: &Work, label: &str, get: fn(&Config) -> bool, set: fn(&mut Config, bool)) -> gtk::CheckButton {
    let b = gtk::CheckButton::with_label(label);
    b.set_active(get(&work.borrow()));
    let work = work.clone();
    b.connect_toggled(move |b| set(&mut work.borrow_mut(), b.is_active()));
    b
}

fn spin(work: &Work, min: f64, max: f64, step: f64, get: fn(&Config) -> i64, set: fn(&mut Config, i64)) -> gtk::SpinButton {
    let s = gtk::SpinButton::with_range(min, max, step);
    s.set_value(get(&work.borrow()) as f64);
    let work = work.clone();
    s.connect_value_changed(move |s| set(&mut work.borrow_mut(), s.value() as i64));
    s
}

fn entry(work: &Work, placeholder: &str, get: fn(&Config) -> String, set: fn(&mut Config, String)) -> gtk::Entry {
    let e = gtk::Entry::builder().placeholder_text(placeholder).hexpand(true).build();
    e.set_text(&get(&work.borrow()));
    let work = work.clone();
    e.connect_changed(move |e| set(&mut work.borrow_mut(), e.text().to_string()));
    e
}

fn combo(work: &Work, labels: &[&str], get: fn(&Config) -> usize, set: fn(&mut Config, usize)) -> gtk::DropDown {
    let d = gtk::DropDown::from_strings(labels);
    d.set_selected(get(&work.borrow()) as u32);
    let work = work.clone();
    d.connect_selected_notify(move |d| set(&mut work.borrow_mut(), d.selected() as usize));
    d
}

/// A labeled box like MPC's group boxes.
fn group(title: &str, rows: &[&gtk::Widget]) -> gtk::Frame {
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).margin_top(6).margin_bottom(8).margin_start(8).margin_end(8).build();
    for r in rows {
        b.append(*r);
    }
    gtk::Frame::builder().label(title).child(&b).build()
}

/// "Label: [widget] suffix" on one line.
fn row(label: &str, w: &impl IsA<gtk::Widget>, suffix: &str) -> gtk::Widget {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    b.append(&gtk::Label::builder().label(label).xalign(0.0).width_chars(18).build());
    b.append(w);
    if !suffix.is_empty() {
        b.append(&gtk::Label::new(Some(suffix)));
    }
    b.upcast()
}

fn page(groups: &[&gtk::Widget]) -> gtk::Widget {
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    for g in groups {
        b.append(*g);
    }
    gtk::ScrolledWindow::builder().child(&b).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build().upcast()
}

// --- pages ---

fn player_page(w: &Work) -> gtk::Widget {
    let open = combo(w, &["Use the same player for each media file", "Open a new player for each media file played"],
        |c| c.allow_multiple_instances as usize, |c, i| c.allow_multiple_instances = i == 1);
    let title = combo(w, &["Display full path", "File name only", "Don't prefix anything"],
        |c| c.title_bar as usize, |c, i| c.title_bar = [TitleBar::FullPath, TitleBar::FileName, TitleBar::NoPrefix][i]);
    let media_title = check(w, "Replace file name with title", |c| c.title_bar_media_title, |c, v| c.title_bar_media_title = v);
    let history = check(w, "Keep history of recently opened files", |c| c.keep_history, |c, v| c.keep_history = v);
    let playlist = check(w, "Remember last playlist", |c| c.remember_playlist, |c, v| c.remember_playlist = v);
    let pos = check(w, "Remember File position", |c| c.remember_file_pos, |c, v| c.remember_file_pos = v);
    let size = check(w, "Remember last window size", |c| c.remember_window_size, |c, v| c.remember_window_size = v);
    let osd = check(w, "Show OSD", |c| c.show_osd, |c, v| c.show_osd = v);
    let recent = spin(w, 0.0, 100.0, 1.0, |c| c.recent_files_number as i64, |c, v| c.recent_files_number = v as usize);
    page(&[
        group("Open options", &[open.upcast_ref()]).upcast_ref(),
        group("Title bar", &[title.upcast_ref(), media_title.upcast_ref()]).upcast_ref(),
        group("Other", &[osd.upcast_ref()]).upcast_ref(),
        group("History", &[history.upcast_ref(), &row("Recent files:", &recent, ""), playlist.upcast_ref(), pos.upcast_ref(), size.upcast_ref()]).upcast_ref(),
    ])
}

fn logo_page(w: &Work) -> gtk::Widget {
    let path = entry(w, "Built-in logo", |c| c.logo_file.clone(), |c, v| c.logo_file = v);
    let browse = gtk::Button::with_label("Browse...");
    browse.connect_clicked({
        let path = path.clone();
        move |b| {
            let images = gtk::FileFilter::new();
            images.set_name(Some("Images"));
            images.add_pixbuf_formats();
            let filters = gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&images);
            let path = path.clone();
            let parent = b.root().and_downcast::<gtk::Window>();
            gtk::FileDialog::builder().title("Logo").filters(&filters).build().open(parent.as_ref(), gio::Cancellable::NONE, move |res| {
                if let Some(p) = res.ok().and_then(|f| f.path()) {
                    path.set_text(&p.to_string_lossy());
                }
            });
        }
    });
    let internal = gtk::Button::with_label("Use built-in");
    internal.connect_clicked({
        let path = path.clone();
        move |_| path.set_text("")
    });
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    line.append(&path);
    line.append(&browse);
    line.append(&internal);
    page(&[group("Logo shown when nothing is playing", &[line.upcast_ref()]).upcast_ref()])
}

fn playback_page(w: &Work) -> gtk::Widget {
    let volume = spin(w, 0.0, 100.0, 1.0, |c| c.volume as i64, |c, v| c.volume = v as u32);
    let step = spin(w, 1.0, 100.0, 1.0, |c| c.volume_step as i64, |c, v| c.volume_step = v as u32);
    let loops = spin(w, 1.0, 999.0, 1.0, |c| c.loops as i64, |c, v| c.loops = v as u32);
    let forever = check(w, "Repeat forever", |c| c.repeat_forever, |c, v| c.repeat_forever = v);
    let mode = combo(w, &["File", "Playlist"], |c| c.repeat_mode as usize, |c, i| c.repeat_mode = [RepeatMode::File, RepeatMode::Playlist][i]);
    let after = combo(w, &AfterPlayback::ALL.map(|a| a.label()), |c| c.after_playback as usize, |c, i| c.after_playback = AfterPlayback::ALL[i]);
    let subs = entry(w, "e.g. eng,jpn", |c| c.subtitle_langs.clone(), |c, v| c.subtitle_langs = v);
    let audio = entry(w, "e.g. jpn,eng", |c| c.audio_langs.clone(), |c, v| c.audio_langs = v);
    let auto_audio = check(w, "Auto-load audio files", |c| c.autoload_audio, |c, v| c.autoload_audio = v);
    let auto_subs = check(w, "Auto-load subtitles", |c| c.autoload_subtitles, |c, v| c.autoload_subtitles = v);
    let speed = spin(w, 0.0, 100.0, 5.0, |c| c.speed_step as i64, |c, v| c.speed_step = v as u32);
    let pitch = check(w, "Keep the pitch when changing the rate", |c| c.pitch_correction, |c, v| c.pitch_correction = v);
    let zoom = combo(w, &Zoom::ALL.map(|z| z.label()), |c| c.zoom as usize, |c, i| c.zoom = Zoom::ALL[i]);
    let auto_zoom = check(w, "Auto-zoom (resize the window when a file opens)", |c| c.auto_zoom, |c, v| c.auto_zoom = v);
    let fit = spin(w, 25.0, 100.0, 5.0, |c| c.autofit_factor as i64, |c, v| c.autofit_factor = v as u32);
    page(&[
        group("Audio", &[&row("Volume:", &volume, "%")]).upcast_ref(),
        group("Playback", &[&row("Play", &loops, "time(s)"), forever.upcast_ref(), &row("Repeat mode:", &mode, ""), &row("After Playback:", &after, "")]).upcast_ref(),
        group("Default zoom", &[&row("Zoom:", &zoom, ""), auto_zoom.upcast_ref(), &row("Auto fit factor:", &fit, "%")]).upcast_ref(),
        group("Default track preference", &[&row("Subtitles:", &subs, ""), &row("Audio:", &audio, "")]).upcast_ref(),
        group("Open settings", &[auto_audio.upcast_ref(), auto_subs.upcast_ref()]).upcast_ref(),
        group("Control", &[&row("Volume step:", &step, "%"), &row("Speed step:", &speed, "% (0 = double/halve)"), pitch.upcast_ref()]).upcast_ref(),
    ])
}

fn fullscreen_page(w: &Work) -> gtk::Widget {
    let launch = check(w, "Launch files in fullscreen", |c| c.launch_fullscreen, |c, v| c.launch_fullscreen = v);
    let hide = check(w, "Hide controls in fullscreen", |c| c.hide_fullscreen_controls, |c, v| c.hide_fullscreen_controls = v);
    let delay = spin(w, 0.0, 10000.0, 50.0, |c| c.hide_fullscreen_controls_delay_ms as i64, |c, v| c.hide_fullscreen_controls_delay_ms = v as u32);
    let panels = check(w, "Hide docked panels", |c| c.hide_fullscreen_docked_panels, |c, v| c.hide_fullscreen_docked_panels = v);
    let exit = check(w, "Exit fullscreen at the end of playback", |c| c.exit_fullscreen_at_end, |c, v| c.exit_fullscreen_at_end = v);
    page(&[group("Fullscreen", &[launch.upcast_ref(), hide.upcast_ref(), &row("Hide delay:", &delay, "ms"), panels.upcast_ref(), exit.upcast_ref()]).upcast_ref()])
}

fn tweaks_page(w: &Work) -> gtk::Widget {
    let small = spin(w, 0.0, 3_600_000.0, 100.0, |c| c.jump_small_ms as i64, |c, v| c.jump_small_ms = v as u32);
    let medium = spin(w, 0.0, 3_600_000.0, 100.0, |c| c.jump_medium_ms as i64, |c, v| c.jump_medium_ms = v as u32);
    let large = spin(w, 0.0, 3_600_000.0, 100.0, |c| c.jump_large_ms as i64, |c, v| c.jump_large_ms = v as u32);
    let default = gtk::Button::with_label("Default");
    default.connect_clicked({
        let (small, medium, large) = (small.clone(), medium.clone(), large.clone());
        move |_| {
            let d = Config::default();
            small.set_value(d.jump_small_ms as f64);
            medium.set_value(d.jump_medium_ms as f64);
            large.set_value(d.jump_large_ms as f64);
        }
    });
    let jumps = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    for w in [small.upcast_ref::<gtk::Widget>(), medium.upcast_ref(), large.upcast_ref(), default.upcast_ref()] {
        jumps.append(w);
    }
    let fast = check(w, "Fast seek (on keyframe)", |c| c.fast_seek, |c, v| c.fast_seek = v);
    let marks = check(w, "Show chapter marks in seek bar", |c| c.show_chapter_marks, |c, v| c.show_chapter_marks = v);
    let folder = check(w, "Open previous/next file in folder on \"Skip back/forward\" when there is only one item in playlist",
        |c| c.search_in_folder, |c, v| c.search_in_folder = v);
    let tooltip = check(w, "Show time tooltip", |c| c.time_tooltip, |c, v| c.time_tooltip = v);
    let pointer = check(w, "Auto-hide the mouse pointer during playback in windowed mode", |c| c.hide_windowed_mouse_pointer, |c, v| c.hide_windowed_mouse_pointer = v);
    let osd_font = gtk::FontDialogButton::new(Some(gtk::FontDialog::new()));
    osd_font.set_level(gtk::FontLevel::Family);
    osd_font.set_font_desc(&gtk::pango::FontDescription::from_string(&w.borrow().osd_font));
    osd_font.connect_font_desc_notify({
        let work = w.clone();
        move |b| {
            if let Some(family) = b.font_desc().and_then(|d| d.family()) {
                work.borrow_mut().osd_font = family.to_string();
            }
        }
    });
    let osd_size = spin(w, 8.0, 200.0, 1.0, |c| c.osd_font_size as i64, |c, v| c.osd_font_size = v as u32);
    page(&[
        group("Jump distances (small, medium, large in ms)", &[jumps.upcast_ref(), fast.upcast_ref()]).upcast_ref(),
        group("Tweaks", &[marks.upcast_ref(), folder.upcast_ref(), tooltip.upcast_ref(), pointer.upcast_ref()]).upcast_ref(),
        group("OSD", &[&row("OSD font:", &osd_font, ""), &row("Size:", &osd_size, "(on a 720-line screen)")]).upcast_ref(),
    ])
}

/// A color button for a "#RRGGBBAA" setting.
fn color(work: &Work, get: fn(&Config) -> String, set: fn(&mut Config, String)) -> gtk::ColorDialogButton {
    let b = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::builder().with_alpha(true).build()));
    if let Ok(rgba) = gdk::RGBA::parse(get(&work.borrow()).as_str()) {
        b.set_rgba(&rgba);
    }
    let work = work.clone();
    b.connect_rgba_notify(move |b| {
        let c = b.rgba();
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        set(&mut work.borrow_mut(), format!("#{:02x}{:02x}{:02x}{:02x}", byte(c.red()), byte(c.green()), byte(c.blue()), byte(c.alpha())));
    });
    b
}

/// MPC's Output page: the parts that mean something with mpv (decoding, color, levels, screenshots).
fn output_page(w: &Work) -> gtk::Widget {
    let hwdec = check(w, "Hardware decoding (when the GPU supports the format)", |c| c.hwdec, |c, v| c.hwdec = v);
    let cm = check(w, "Color management (use the display's ICC profile)", |c| c.color_management, |c, v| c.color_management = v);
    let intent = combo(w, &["Perceptual", "Relative Colorimetric", "Saturation", "Absolute Colorimetric"], |c| c.icc_intent as usize, |c, i| c.icc_intent = i as u32);
    let range = combo(w, &["0 - 255", "16 - 235"], |c| c.output_limited as usize, |c, i| c.output_limited = i == 1);
    let dir = entry(w, "Folder", |c| c.snapshot_dir.clone(), |c, v| c.snapshot_dir = v);
    let ext = combo(w, &["jpg", "png", "webp"], |c| ["jpg", "png", "webp"].iter().position(|e| *e == c.snapshot_ext).unwrap_or(0), |c, i| c.snapshot_ext = ["jpg", "png", "webp"][i].into());
    let rows = spin(w, 1.0, 16.0, 1.0, |c| c.thumb_rows as i64, |c, v| c.thumb_rows = v as u32);
    let cols = spin(w, 1.0, 16.0, 1.0, |c| c.thumb_cols as i64, |c, v| c.thumb_cols = v as u32);
    let width = spin(w, 256.0, 4096.0, 16.0, |c| c.thumb_width as i64, |c, v| c.thumb_width = v as u32);
    page(&[
        group("Video", &[hwdec.upcast_ref(), &row("Output range:", &range, "")]).upcast_ref(),
        group("Color management", &[cm.upcast_ref(), &row("Rendering intent:", &intent, "")]).upcast_ref(),
        group("Screenshots", &[&row("Folder:", &dir, ""), &row("Format:", &ext, ""), &row("Thumbnail rows:", &rows, ""), &row("Thumbnail columns:", &cols, ""), &row("Thumbnail width:", &width, "px")]).upcast_ref(),
    ])
}

fn sync_page(w: &Work) -> gtk::Widget {
    let sync = check(w, "Sync video to display (resample audio to the refresh rate)", |c| c.sync_to_display, |c, v| c.sync_to_display = v);
    let interp = check(w, "Frame interpolation (smooth motion; needs sync to display)", |c| c.interpolation, |c, v| c.interpolation = v);
    page(&[group("Sync", &[sync.upcast_ref(), interp.upcast_ref()]).upcast_ref()])
}

/// MPC's Shaders page: the available shaders, and the pre-resize and post-resize chains.
fn shaders_page(w: &Work) -> gtk::Widget {
    let names = gtk::StringList::new(&shaders::available().iter().map(String::as_str).collect::<Vec<_>>());
    let avail = gtk::ListBox::new();
    avail.bind_model(Some(&names), |o| gtk::Label::builder().label(o.downcast_ref::<gtk::StringObject>().unwrap().string()).xalign(0.0).build().upcast());
    // One chain: a list box kept in step with the config list it edits.
    let chain = |get: fn(&Config) -> &Vec<String>, set: fn(&mut Config) -> &mut Vec<String>| {
        let list = gtk::ListBox::new();
        let refill = {
            let (list, work) = (list.clone(), w.clone());
            Rc::new(move || {
                list.remove_all();
                for n in get(&work.borrow()) {
                    list.append(&gtk::Label::builder().label(n.as_str()).xalign(0.0).build());
                }
            })
        };
        refill();
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let add = gtk::Button::with_label("Add");
        add.connect_clicked({
            let (avail, names, work, refill) = (avail.clone(), names.clone(), w.clone(), refill.clone());
            move |_| {
                if let Some(n) = avail.selected_row().and_then(|r| names.string(r.index() as u32)) {
                    set(&mut work.borrow_mut()).push(n.to_string());
                    refill();
                }
            }
        });
        let edit = |label: &str, f: fn(&mut Vec<String>, usize)| {
            let b = gtk::Button::with_label(label);
            let (list, work, refill) = (list.clone(), w.clone(), refill.clone());
            b.connect_clicked(move |_| {
                if let Some(r) = list.selected_row() {
                    let i = r.index() as usize;
                    f(set(&mut work.borrow_mut()), i);
                    refill();
                }
            });
            b
        };
        buttons.append(&add);
        buttons.append(&edit("Remove", |v, i| {
            v.remove(i);
        }));
        buttons.append(&edit("Up", |v, i| {
            if i > 0 {
                v.swap(i, i - 1);
            }
        }));
        buttons.append(&edit("Down", |v, i| {
            if i + 1 < v.len() {
                v.swap(i, i + 1);
            }
        }));
        let b = gtk::Box::new(gtk::Orientation::Vertical, 4);
        b.append(&gtk::ScrolledWindow::builder().min_content_height(120).child(&list).build());
        b.append(&buttons);
        b
    };
    let pre = chain(|c| &c.shaders_pre, |c| &mut c.shaders_pre);
    let post = chain(|c| &c.shaders_post, |c| &mut c.shaders_post);
    let note = gtk::Label::builder()
        .label("Pre-resize shaders run on the video at its own size, post-resize ones on the scaled picture. Your own mpv shaders go in the shaders folder (Play > Shaders > Shader Editor).")
        .wrap(true)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build();
    page(&[
        group("Available shaders", &[gtk::ScrolledWindow::builder().min_content_height(160).child(&avail).build().upcast_ref()]).upcast_ref(),
        group("Pre-resize", &[pre.upcast_ref()]).upcast_ref(),
        group("Post-resize", &[post.upcast_ref()]).upcast_ref(),
        note.upcast_ref(),
    ])
}

/// MPC's audio switcher page, done with mpv's audio filters.
fn audio_page(w: &Work) -> gtk::Widget {
    let normalize = check(w, "Normalize", |c| c.audio_normalize, |c, v| c.audio_normalize = v);
    let max = spin(w, 100.0, 10000.0, 50.0, |c| c.audio_max_norm_factor as i64, |c, v| c.audio_max_norm_factor = v as u32);
    let boost = spin(w, 0.0, 300.0, 5.0, |c| c.audio_boost as i64, |c, v| c.audio_boost = v as u32);
    let shift = spin(w, -10000.0, 10000.0, 10.0, |c| c.audio_time_shift_ms as i64, |c, v| c.audio_time_shift_ms = v as i32);
    let downsample = check(w, "Down-sample to 44100 Hz", |c| c.downsample_44100, |c, v| c.downsample_44100 = v);
    let downmix = check(w, "Downmix to stereo", |c| c.downmix_stereo, |c, v| c.downmix_stereo = v);
    let crossfeed = check(w, "Headphone crossfeed (bs2b)", |c| c.crossfeed, |c, v| c.crossfeed = v);
    page(&[
        group("Volume", &[normalize.upcast_ref(), &row("Max amplification:", &max, "%"), &row("Boost:", &boost, "%")]).upcast_ref(),
        group("Output", &[downsample.upcast_ref(), downmix.upcast_ref(), crossfeed.upcast_ref(), &row("Audio time shift:", &shift, "ms")]).upcast_ref(),
    ])
}

fn subtitles_page(w: &Work) -> gtk::Widget {
    let place = check(w, "Override placement", |c| c.sub_override_placement, |c, v| c.sub_override_placement = v);
    let vert = spin(w, 0.0, 150.0, 1.0, |c| c.sub_pos_v as i64, |c, v| c.sub_pos_v = v as u32);
    let delay = spin(w, 1.0, 10000.0, 50.0, |c| c.sub_delay_step_ms as i64, |c, v| c.sub_delay_step_ms = v as u32);
    page(&[
        group("Placement", &[place.upcast_ref(), &row("Vertical:", &vert, "% (100 = bottom)")]).upcast_ref(),
        group("Delay step", &[&row("Delay step:", &delay, "ms")]).upcast_ref(),
    ])
}

fn sub_style_page(w: &Work) -> gtk::Widget {
    let font = gtk::FontDialogButton::new(Some(gtk::FontDialog::new()));
    font.set_level(gtk::FontLevel::Family);
    font.set_font_desc(&gtk::pango::FontDescription::from_string(&w.borrow().sub_font));
    font.connect_font_desc_notify({
        let work = w.clone();
        move |b| {
            if let Some(family) = b.font_desc().and_then(|d| d.family()) {
                work.borrow_mut().sub_font = family.to_string();
            }
        }
    });
    let size = spin(w, 8.0, 200.0, 1.0, |c| c.sub_font_size as i64, |c, v| c.sub_font_size = v as u32);
    let bold = check(w, "Bold", |c| c.sub_bold, |c, v| c.sub_bold = v);
    let italic = check(w, "Italic", |c| c.sub_italic, |c, v| c.sub_italic = v);
    let text = color(w, |c| c.sub_color.clone(), |c, v| c.sub_color = v);
    let outline = color(w, |c| c.sub_border_color.clone(), |c, v| c.sub_border_color = v);
    let shadow = color(w, |c| c.sub_shadow_color.clone(), |c, v| c.sub_shadow_color = v);
    let back = color(w, |c| c.sub_back_color.clone(), |c, v| c.sub_back_color = v);
    let outline_w = spin(w, 0.0, 20.0, 1.0, |c| c.sub_border_size as i64, |c, v| c.sub_border_size = v as f64);
    let shadow_d = spin(w, 0.0, 20.0, 1.0, |c| c.sub_shadow_offset as i64, |c, v| c.sub_shadow_offset = v as f64);
    let opaque = check(w, "Opaque box instead of outline", |c| c.sub_opaque_box, |c, v| c.sub_opaque_box = v);
    let margin = spin(w, 0.0, 300.0, 1.0, |c| c.sub_margin_y as i64, |c, v| c.sub_margin_y = v as u32);
    let ass = check(w, "Also apply to styled (ASS/SSA) subtitles", |c| c.sub_override_ass, |c, v| c.sub_override_ass = v);
    page(&[
        group("Font", &[&row("Font:", &font, ""), &row("Size:", &size, "(on a 720-line screen)"), bold.upcast_ref(), italic.upcast_ref()]).upcast_ref(),
        group("Border style", &[&row("Outline width:", &outline_w, ""), &row("Shadow depth:", &shadow_d, ""), opaque.upcast_ref()]).upcast_ref(),
        group("Colors and transparency", &[&row("Text:", &text, ""), &row("Outline:", &outline, ""), &row("Shadow:", &shadow, ""), &row("Box:", &back, "")]).upcast_ref(),
        group("Position", &[&row("Bottom margin:", &margin, ""), ass.upcast_ref()]).upcast_ref(),
    ])
}

fn misc_page(w: &Work, dlg: &gtk::Window, on_apply: Rc<dyn Fn(Config)>) -> gtk::Widget {
    // Color controls: MPC's ranges, applied to mpv's equalizer properties.
    let color = |label: &str, range: f64, get: fn(&Config) -> i64, set: fn(&mut Config, i64)| {
        let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, -range, range, 1.0);
        s.set_value(get(&w.borrow()) as f64);
        s.set_hexpand(true);
        s.set_draw_value(true);
        s.set_value_pos(gtk::PositionType::Right);
        let work = w.clone();
        s.connect_value_changed(move |s| set(&mut work.borrow_mut(), s.value() as i64));
        (row(label, &s, ""), s)
    };
    let (b, bs) = color("Brightness", 100.0, |c| c.brightness as i64, |c, v| c.brightness = v as i32);
    let (c_, cs) = color("Contrast", 100.0, |c| c.contrast as i64, |c, v| c.contrast = v as i32);
    let (h, hs) = color("Hue", 180.0, |c| c.hue as i64, |c, v| c.hue = v as i32);
    let (s, ss) = color("Saturation", 100.0, |c| c.saturation as i64, |c, v| c.saturation = v as i32);
    let reset_colors = gtk::Button::with_label("Reset");
    reset_colors.set_halign(gtk::Align::End);
    reset_colors.connect_clicked(move |_| {
        for s in [&bs, &cs, &hs, &ss] {
            s.set_value(0.0);
        }
    });

    let reset = gtk::Button::with_label("Reset");
    reset.connect_clicked({
        let dlg = dlg.clone();
        move |_| {
            let (dlg, on_apply) = (dlg.clone(), on_apply.clone());
            gtk::AlertDialog::builder()
                .message("Restore all settings to their defaults?")
                .detail("Key bindings, playback and all other options are reset.")
                .buttons(["Cancel", "Reset"])
                .cancel_button(0)
                .default_button(0)
                .build()
                .choose(Some(&dlg.clone()), gio::Cancellable::NONE, move |res| {
                    if res == Ok(1) {
                        on_apply(Config::default());
                        dlg.close();
                    }
                });
        }
    });
    let export = gtk::Button::with_label("Export");
    export.connect_clicked({
        let work = w.clone();
        move |b| {
            let parent = b.root().and_downcast::<gtk::Window>();
            let work = work.clone();
            gtk::FileDialog::builder().title("Export").initial_name("mpc-rs.toml").build().save(parent.as_ref(), gio::Cancellable::NONE, move |res| {
                let Some(p) = res.ok().and_then(|f| f.path()) else { return };
                if let Err(e) = std::fs::write(&p, config::to_text(&work.borrow())) {
                    eprintln!("{}: {e}", p.display());
                }
            });
        }
    });
    let mgmt = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    mgmt.append(&reset);
    mgmt.append(&export);
    page(&[
        group("Color controls", &[&b, &c_, &h, &s, reset_colors.upcast_ref()]).upcast_ref(),
        group("Settings management", &[mgmt.upcast_ref()]).upcast_ref(),
    ])
}

fn key_label(accel: &str) -> String {
    gtk::accelerator_parse(accel).map_or(String::new(), |(k, m)| gtk::accelerator_get_label(k, m).into())
}

fn same_key(a: &str, b: &str) -> bool {
    !a.is_empty() && gtk::accelerator_parse(a).is_some() && gtk::accelerator_parse(a) == gtk::accelerator_parse(b)
}

fn index_of(item: &glib::Object) -> usize {
    item.downcast_ref::<gtk::StringObject>().unwrap().string().parse().unwrap()
}

fn is_modifier(k: gdk::Key) -> bool {
    use gdk::Key;
    [Key::Shift_L, Key::Shift_R, Key::Control_L, Key::Control_R, Key::Alt_L, Key::Alt_R, Key::Meta_L, Key::Meta_R,
     Key::Super_L, Key::Super_R, Key::Hyper_L, Key::Hyper_R, Key::ISO_Level3_Shift, Key::Caps_Lock, Key::Num_Lock]
    .contains(&k)
}

impl Keys {
    fn selected(&self) -> Option<usize> {
        self.sel.selected_item().map(|i| index_of(&i))
    }

    /// Redraw one row after its binding changed (a new list item forces the cells to bind again).
    fn refresh(&self, idx: usize) {
        let pos = self.sel.selected();
        self.store.splice(idx as u32, 1, &[&idx.to_string()]);
        self.sel.set_selected(pos);
        self.load_editor();
    }

    fn edit(&self, f: impl FnOnce(&mut Binding)) {
        if let Some(idx) = self.selected() {
            f(&mut self.work.borrow_mut()[idx]);
            self.refresh(idx);
        }
    }

    /// Fill the editor below the list from the selected command.
    fn load_editor(&self) {
        let Some(idx) = self.selected() else { return };
        let b = self.work.borrow()[idx].clone();
        self.loading.set(true);
        self.recording.set(false);
        self.key.set_label(&if b.key.is_empty() { "None".into() } else { key_label(&b.key) });
        self.mouse.set_selected(b.mouse as u32);
        self.mouse_fs.set_selected(b.mouse_fs as u32);
        self.media.set_selected(b.media as u32);
        self.loading.set(false);

        let work = self.work.borrow();
        let others: Vec<&str> = COMMANDS
            .iter()
            .zip(work.iter())
            .enumerate()
            .filter(|(i, (_, o))| *i != idx && same_key(&b.key, &o.key))
            .map(|(_, (c, _))| c.label)
            .collect();
        self.conflict.set_label(&if others.is_empty() { String::new() } else { format!("{} is also used by: {}", key_label(&b.key), others.join(", ")) });
    }
}

fn keys_page(dlg: &gtk::Window, bindings: Vec<Binding>, implemented: Vec<bool>) -> (Rc<Keys>, gtk::Box) {
    let store = gtk::StringList::new(&[]);
    for i in 0..COMMANDS.len() {
        store.append(&i.to_string());
    }
    let search = gtk::SearchEntry::builder().placeholder_text("Filter commands").build();
    let filter = gtk::CustomFilter::new({
        let search = search.clone();
        move |item| {
            let text = search.text().to_lowercase();
            let c = &COMMANDS[index_of(item)];
            text.is_empty() || c.label.to_lowercase().contains(&text) || c.id.to_string() == text
        }
    });
    search.connect_search_changed({
        let filter = filter.clone();
        move |_| filter.changed(gtk::FilterChange::Different)
    });
    let sel = gtk::SingleSelection::new(Some(gtk::FilterListModel::new(Some(store.clone()), Some(filter))));

    let dropdown = |labels: Vec<&'static str>| gtk::DropDown::from_strings(&labels);
    let keys = Rc::new(Keys {
        work: RefCell::new(bindings),
        implemented,
        store,
        sel: sel.clone(),
        key: gtk::Button::new(),
        mouse: dropdown(Mouse::ALL.iter().map(|m| m.label()).collect()),
        mouse_fs: dropdown(Mouse::ALL.iter().map(|m| m.label()).collect()),
        media: dropdown(Media::ALL.iter().map(|m| m.label()).collect()),
        conflict: gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["error"]).build(),
        recording: Cell::new(false),
        loading: Cell::new(false),
    });

    // --- the list ---
    let view = gtk::ColumnView::new(Some(sel.clone()));
    view.add_css_class("data-table");
    let column = |title: &str, expand: bool, text: fn(usize, &Binding) -> String| {
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            item.downcast_ref::<gtk::ListItem>().unwrap().set_child(Some(&gtk::Label::builder().xalign(0.0).build()));
        });
        let keys = keys.clone();
        factory.connect_bind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let idx = index_of(&item.item().unwrap());
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&text(idx, &keys.work.borrow()[idx]));
            // Commands that don't do anything yet are dimmed, like MPC's disabled menu items.
            if keys.implemented[idx] {
                label.remove_css_class("dim-label");
            } else {
                label.add_css_class("dim-label");
            }
        });
        let col = gtk::ColumnViewColumn::new(Some(title), Some(factory));
        col.set_expand(expand);
        col.set_resizable(true);
        view.append_column(&col);
    };
    column("Command", true, |i, _| COMMANDS[i].label.to_string());
    column("Key", false, |_, b| key_label(&b.key));
    column("ID", false, |i, _| COMMANDS[i].id.to_string());
    column("Mouse Window", false, |_, b| if b.mouse == Mouse::None { String::new() } else { b.mouse.label().into() });
    column("Mouse Fullscreen", false, |_, b| if b.mouse_fs == Mouse::None { String::new() } else { b.mouse_fs.label().into() });
    column("Media Key", false, |_, b| if b.media == Media::None { String::new() } else { b.media.label().into() });
    let scroll = gtk::ScrolledWindow::builder().vexpand(true).child(&view).build();
    sel.connect_selected_notify({
        let keys = keys.clone();
        move |_| keys.load_editor()
    });

    // --- editor for the selected command ---
    keys.key.connect_clicked({
        let keys = keys.clone();
        move |b| {
            if keys.selected().is_some() {
                keys.recording.set(true);
                b.set_label("Press a key... (Esc to cancel)");
            }
        }
    });
    // Record the next key press for the selected command.
    let rec = gtk::EventControllerKey::new();
    rec.set_propagation_phase(gtk::PropagationPhase::Capture);
    rec.connect_key_pressed({
        let keys = keys.clone();
        move |ctl, keyval, keycode, state| {
            if !keys.recording.get() {
                return glib::Propagation::Proceed;
            }
            if is_modifier(keyval) {
                return glib::Propagation::Stop;
            }
            let mods = state & gtk::accelerator_get_default_mod_mask();
            if keyval == gdk::Key::Escape && mods.is_empty() {
                keys.load_editor();
                return glib::Propagation::Stop;
            }
            // Store the unshifted key ("<Shift>1", not "<Shift>exclam"), except on the keypad.
            let mut key = keyval.to_lower();
            let keypad = key.name().is_some_and(|n| n.starts_with("KP_"));
            if mods.contains(gdk::ModifierType::SHIFT_MASK) && !keypad {
                let base = ctl.widget().and_then(|w| w.display().map_keycode(keycode)).and_then(|v| {
                    v.into_iter().find(|(k, _)| k.level() == 0 && k.group() as u32 == ctl.group()).map(|(_, k)| k)
                });
                key = base.unwrap_or(key);
            }
            let accel = gtk::accelerator_name(key, mods).to_string();
            keys.edit(|b| b.key = accel);
            glib::Propagation::Stop
        }
    });
    dlg.add_controller(rec);

    let clear = gtk::Button::with_label("Clear");
    clear.connect_clicked({
        let keys = keys.clone();
        move |_| keys.edit(|b| b.key.clear())
    });
    for (dd, set) in [
        (&keys.mouse, (|b: &mut Binding, i| b.mouse = Mouse::ALL[i]) as fn(&mut Binding, usize)),
        (&keys.mouse_fs, |b, i| b.mouse_fs = Mouse::ALL[i]),
        (&keys.media, |b, i| b.media = Media::ALL[i]),
    ] {
        let k = keys.clone();
        dd.connect_selected_notify(move |dd| {
            if !k.loading.get() {
                let i = dd.selected() as usize;
                k.edit(|b| set(b, i));
            }
        });
    }

    let reset = gtk::Button::with_label("Reset Selected");
    reset.connect_clicked({
        let keys = keys.clone();
        move |_| {
            if let Some(idx) = keys.selected() {
                keys.edit(|b| *b = COMMANDS[idx].default_binding());
            }
        }
    });
    let reset_all = gtk::Button::with_label("Reset All");
    reset_all.connect_clicked({
        let keys = keys.clone();
        move |_| {
            *keys.work.borrow_mut() = COMMANDS.iter().map(|c| c.default_binding()).collect();
            let pos = keys.sel.selected();
            let all: Vec<String> = (0..COMMANDS.len()).map(|i| i.to_string()).collect();
            keys.store.splice(0, COMMANDS.len() as u32, &all.iter().map(String::as_str).collect::<Vec<_>>());
            keys.sel.set_selected(pos);
            keys.load_editor();
        }
    });

    let grid = gtk::Grid::builder().row_spacing(6).column_spacing(8).build();
    let label = |s: &str| gtk::Label::builder().label(s).xalign(1.0).build();
    grid.attach(&label("Key:"), 0, 0, 1, 1);
    let key_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    keys.key.set_width_request(220);
    key_row.append(&keys.key);
    key_row.append(&clear);
    grid.attach(&key_row, 1, 0, 1, 1);
    grid.attach(&label("Mouse Window:"), 0, 1, 1, 1);
    grid.attach(&keys.mouse, 1, 1, 1, 1);
    grid.attach(&label("Mouse Fullscreen:"), 0, 2, 1, 1);
    grid.attach(&keys.mouse_fs, 1, 2, 1, 1);
    grid.attach(&label("Media Key:"), 0, 3, 1, 1);
    grid.attach(&keys.media, 1, 3, 1, 1);
    let resets = gtk::Box::builder().spacing(8).valign(gtk::Align::End).halign(gtk::Align::End).hexpand(true).build();
    resets.append(&reset);
    resets.append(&reset_all);
    grid.attach(&resets, 2, 0, 1, 4);

    let page = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).margin_top(12).margin_start(12).margin_end(12).hexpand(true).build();
    page.append(&search);
    page.append(&scroll);
    page.append(&grid);
    page.append(&keys.conflict);
    keys.load_editor();
    (keys, page)
}
