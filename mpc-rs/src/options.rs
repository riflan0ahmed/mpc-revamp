//! Options dialog (MPC's `PPageSheet`): a tree of pages on the left, the page on the right. Pages for
//! features that come in later phases (Formats, Web Interface, Output, Shaders, Sync, Subtitles, ...)
//! are added with those features.

use crate::{
    commands::{self, Binding, COMMANDS, Media, Mouse},
    config::{self, AfterPlayback, Config, RepeatMode, TitleBar},
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
        ("Fullscreen", true, fullscreen_page(&work)),
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
    let recent = spin(w, 0.0, 100.0, 1.0, |c| c.recent_files_number as i64, |c, v| c.recent_files_number = v as usize);
    page(&[
        group("Open options", &[open.upcast_ref()]).upcast_ref(),
        group("Title bar", &[title.upcast_ref(), media_title.upcast_ref()]).upcast_ref(),
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
    page(&[
        group("Audio", &[&row("Volume:", &volume, "%")]).upcast_ref(),
        group("Playback", &[&row("Play", &loops, "time(s)"), forever.upcast_ref(), &row("Repeat mode:", &mode, ""), &row("After Playback:", &after, "")]).upcast_ref(),
        group("Default track preference", &[&row("Subtitles:", &subs, ""), &row("Audio:", &audio, "")]).upcast_ref(),
        group("Open settings", &[auto_audio.upcast_ref(), auto_subs.upcast_ref()]).upcast_ref(),
        group("Control", &[&row("Volume step:", &step, "%")]).upcast_ref(),
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
    page(&[
        group("Jump distances (small, medium, large in ms)", &[jumps.upcast_ref(), fast.upcast_ref()]).upcast_ref(),
        group("Tweaks", &[marks.upcast_ref(), folder.upcast_ref(), tooltip.upcast_ref(), pointer.upcast_ref()]).upcast_ref(),
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
