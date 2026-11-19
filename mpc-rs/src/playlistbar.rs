//! The playlist panel (MPC's `CPlayerPlaylistBar`): a list beside the video with MPC's context menu,
//! drag to reorder, Delete to remove, double-click/Enter to play.

use crate::playlist::{self, Entry, Playlist};
use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
};

/// What the panel needs the main window to do.
pub enum Request {
    Play(usize),
    Add(Vec<String>),
    /// The list changed (save it); `shuffle` / `hide_fullscreen` are the menu toggles.
    Changed { shuffle: bool, hide_fullscreen: bool },
}

pub struct Bar {
    pub widget: gtk::Box,
    pl: Rc<RefCell<Playlist>>,
    store: gtk::StringList, // one item per playlist entry: its index
}

fn index_of(item: &glib::Object) -> usize {
    item.downcast_ref::<gtk::StringObject>().unwrap().string().parse().unwrap()
}

fn hms(t: f64) -> String {
    let s = t.max(0.0) as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

fn rand(n: usize) -> usize {
    glib::random_int_range(0, n as i32) as usize
}

impl Bar {
    /// Redraw every row (after the list or the current item changed).
    pub fn refresh(&self) {
        let n = self.pl.borrow().items.len();
        let items: Vec<String> = (0..n).map(|i| i.to_string()).collect();
        self.store.splice(0, self.store.n_items(), &items.iter().map(String::as_str).collect::<Vec<_>>());
    }

    /// Redraw one row (its duration became known).
    pub fn refresh_row(&self, i: usize) {
        if (i as u32) < self.store.n_items() {
            self.store.splice(i as u32, 1, &[&i.to_string()]);
        }
    }
}

pub fn new(pl: Rc<RefCell<Playlist>>, hide_fullscreen: bool, on: impl Fn(Request) + 'static) -> Rc<Bar> {
    let on: Rc<dyn Fn(Request)> = Rc::new(on);
    let store = gtk::StringList::new(&[]);
    let sel = gtk::MultiSelection::new(Some(store.clone()));
    let view = gtk::ColumnView::new(Some(sel.clone()));
    view.set_show_column_separators(false);
    let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
    widget.set_width_request(220);
    let bar = Rc::new(Bar { widget: widget.clone(), pl: pl.clone(), store: store.clone() });
    let hide_fs = Rc::new(Cell::new(hide_fullscreen));
    let changed = {
        let (on, pl, hide_fs) = (on.clone(), pl.clone(), hide_fs.clone());
        Rc::new(move || on(Request::Changed { shuffle: pl.borrow().shuffle, hide_fullscreen: hide_fs.get() }))
    };

    // --- columns: title and duration; rows can be dragged onto each other to reorder ---
    let column = |title: &str, expand: bool, text: fn(&Entry) -> String| {
        let factory = gtk::SignalListItemFactory::new();
        let (spl, sbar, schanged) = (pl.clone(), bar.clone(), changed.clone());
        factory.connect_setup(move |_, item| {
            let label = gtk::Label::builder().xalign(if expand { 0.0 } else { 1.0 }).ellipsize(gtk::pango::EllipsizeMode::End).build();
            // The row's playlist index is kept in the label's widget name (set in bind).
            let row = |w: Option<gtk::Widget>| w.and_then(|w| w.widget_name().parse::<usize>().ok());
            let drag = gtk::DragSource::new();
            drag.set_actions(gdk::DragAction::MOVE);
            drag.connect_prepare(move |src, _, _| {
                let i = row(src.widget())?;
                Some(gdk::ContentProvider::for_value(&format!("mpc-rs-row:{i}").to_value()))
            });
            label.add_controller(drag);
            let drop = gtk::DropTarget::new(String::static_type(), gdk::DragAction::MOVE);
            drop.connect_drop({
                let (pl, bar, changed) = (spl.clone(), sbar.clone(), schanged.clone());
                move |target, value, _, _| {
                    let from = value.get::<String>().ok().and_then(|s| s.strip_prefix("mpc-rs-row:")?.parse().ok());
                    let (Some(from), Some(to)) = (from, row(target.widget())) else { return false };
                    pl.borrow_mut().move_item(from, to);
                    bar.refresh();
                    changed();
                    true
                }
            });
            label.add_controller(drop);
            item.downcast_ref::<gtk::ListItem>().unwrap().set_child(Some(&label));
        });
        let pl = pl.clone();
        factory.connect_bind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let idx = index_of(&item.item().unwrap());
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_widget_name(&idx.to_string());
            let pl = pl.borrow();
            label.set_text(&pl.items.get(idx).map(text).unwrap_or_default());
            // The playing item is bold, as MPC highlights it.
            if pl.current == Some(idx) {
                label.add_css_class("heading");
            } else {
                label.remove_css_class("heading");
            }
        });
        let col = gtk::ColumnViewColumn::new(Some(title), Some(factory));
        col.set_expand(expand);
        view.append_column(&col);
    };
    column("File", true, Entry::title);
    column("Duration", false, |e| e.duration.map(hms).unwrap_or_default());
    view.connect_activate({
        let on = on.clone();
        move |_, pos| on(Request::Play(pos as usize))
    });
    let selected = {
        let sel = sel.clone();
        move || -> Vec<usize> {
            let set = sel.selection();
            (0..set.size()).map(|i| set.nth(i as u32) as usize).collect()
        }
    };

    // --- actions for the context menu (MPC's CPlayerPlaylistBar::OnContextMenu) ---
    let group = gio::SimpleActionGroup::new();
    let add = |name: &str, f: Box<dyn Fn()>| {
        let a = gio::SimpleAction::new(name, None);
        a.connect_activate(move |_, _| f());
        group.add_action(&a);
    };
    // Run an edit on the playlist, then redraw and save it.
    let edit = |f: fn(&mut Playlist)| {
        let (pl, bar, changed) = (pl.clone(), bar.clone(), changed.clone());
        Box::new(move || {
            f(&mut pl.borrow_mut());
            bar.refresh();
            changed();
        }) as Box<dyn Fn()>
    };
    add("open", Box::new({
        let (on, selected) = (on.clone(), selected.clone());
        move || {
            if let Some(&i) = selected().first() {
                on(Request::Play(i));
            }
        }
    }));
    add("add", Box::new({
        let (on, widget) = (on.clone(), widget.clone());
        move || {
            let on = on.clone();
            let parent = widget.root().and_downcast::<gtk::Window>();
            gtk::FileDialog::builder().title("Add").build().open_multiple(parent.as_ref(), gio::Cancellable::NONE, move |res| {
                if let Ok(model) = res {
                    let paths = (0..model.n_items())
                        .filter_map(|i| model.item(i).and_downcast::<gio::File>())
                        .map(|f| f.path().map_or_else(|| f.uri().to_string(), |p| p.to_string_lossy().into_owned()))
                        .collect();
                    on(Request::Add(paths));
                }
            });
        }
    }));
    let remove = {
        let (pl, bar, changed, selected) = (pl.clone(), bar.clone(), changed.clone(), selected.clone());
        move || {
            pl.borrow_mut().remove(&selected());
            bar.refresh();
            changed();
        }
    };
    add("remove", Box::new(remove.clone()));
    add("clear", edit(Playlist::clear));
    add("copy", Box::new({
        let (pl, widget, selected) = (pl.clone(), widget.clone(), selected.clone());
        move || {
            let pl = pl.borrow();
            let text: Vec<&str> = selected().iter().filter_map(|&i| pl.items.get(i)).map(|e| e.path.as_str()).collect();
            widget.clipboard().set_text(&text.join("\n"));
        }
    }));
    add("addfolder", Box::new({
        let (pl, bar, changed, selected) = (pl.clone(), bar.clone(), changed.clone(), selected.clone());
        move || {
            let Some(dir) = selected().first().and_then(|&i| pl.borrow().items.get(i).and_then(|e| Path::new(&e.path).parent().map(Path::to_path_buf))) else {
                return;
            };
            let new: Vec<Entry> = {
                let pl = pl.borrow();
                playlist::folder_files(&dir).into_iter().filter(|f| !pl.items.iter().any(|e| &e.path == f)).map(Entry::new).collect()
            };
            pl.borrow_mut().add(new);
            bar.refresh();
            changed();
        }
    }));
    add("saveas", Box::new({
        let (pl, widget) = (pl.clone(), widget.clone());
        move || save_as(&widget, pl.clone())
    }));
    add("sortbylabel", edit(Playlist::sort_by_label));
    add("sortbypath", edit(Playlist::sort_by_path));
    add("randomize", edit(|pl| pl.randomize(&mut rand)));
    add("restore", edit(Playlist::restore_order));
    let shuffle = gio::SimpleAction::new_stateful("shuffle", None, &pl.borrow().shuffle.to_variant());
    shuffle.connect_change_state({
        let (pl, changed) = (pl.clone(), changed.clone());
        move |a, v| {
            if let Some(v) = v {
                a.set_state(v);
                pl.borrow_mut().shuffle = v.get().unwrap_or(false);
                changed();
            }
        }
    });
    group.add_action(&shuffle);
    let hidefs = gio::SimpleAction::new_stateful("hidefullscreen", None, &hide_fullscreen.to_variant());
    hidefs.connect_change_state({
        let (hide_fs, changed) = (hide_fs.clone(), changed.clone());
        move |a, v| {
            if let Some(v) = v {
                a.set_state(v);
                hide_fs.set(v.get().unwrap_or(false));
                changed();
            }
        }
    });
    group.add_action(&hidefs);
    widget.insert_action_group("pl", Some(&group));

    let menu = gio::Menu::new();
    let section = |items: &[(&str, &str)]| {
        let s = gio::Menu::new();
        for (label, action) in items {
            s.append(Some(label), Some(action));
        }
        menu.append_section(None, &s);
    };
    section(&[("_Open", "pl.open"), ("A_dd...", "pl.add"), ("_Remove", "pl.remove"), ("C_lear", "pl.clear")]);
    section(&[("_Copy to clipboard", "pl.copy"), ("Add containing _folder", "pl.addfolder"), ("_Save As...", "pl.saveas")]);
    section(&[("Sort by _label", "pl.sortbylabel"), ("Sort by _path", "pl.sortbypath"), ("R_andomize", "pl.randomize"), ("R_estore", "pl.restore")]);
    section(&[("S_huffle", "pl.shuffle"), ("_Hide on Fullscreen", "pl.hidefullscreen")]);
    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(&widget);
    popover.set_has_arrow(false);
    popover.set_halign(gtk::Align::Start);
    widget.connect_destroy({
        let popover = popover.clone();
        move |_| popover.unparent()
    });
    let right = gtk::GestureClick::builder().button(3).build();
    right.connect_pressed(move |_, _, x, y| {
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.popup();
    });
    widget.add_controller(right);

    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Delete {
            remove();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    view.add_controller(keys);

    // Files dropped on the panel are added (dropping on the video replaces the playlist instead).
    let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    drop.connect_drop({
        let on = on.clone();
        move |_, value, _, _| {
            let Ok(list) = value.get::<gdk::FileList>() else { return false };
            let paths = list.files().iter().map(|f| f.path().map_or_else(|| f.uri().to_string(), |p| p.to_string_lossy().into_owned())).collect();
            on(Request::Add(paths));
            true
        }
    });
    widget.add_controller(drop);

    let scroll = gtk::ScrolledWindow::builder().vexpand(true).hscrollbar_policy(gtk::PolicyType::Never).child(&view).build();
    widget.append(&scroll);
    bar.refresh();
    bar
}

fn save_as(widget: &gtk::Box, pl: Rc<RefCell<Playlist>>) {
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    for (name, pattern) in [
        ("MPC-HC playlist (*.mpcpl)", "*.mpcpl"),
        ("Playlist (*.pls)", "*.pls"),
        ("Winamp playlist (*.m3u)", "*.m3u"),
        ("UTF-8 playlist (*.m3u8)", "*.m3u8"),
        ("Windows Media playlist (*.asx)", "*.asx"),
    ] {
        let f = gtk::FileFilter::new();
        f.set_name(Some(name));
        f.add_pattern(pattern);
        filters.append(&f);
    }
    let parent = widget.root().and_downcast::<gtk::Window>();
    gtk::FileDialog::builder()
        .title("Save As")
        .initial_name("playlist.mpcpl")
        .filters(&filters)
        .build()
        .save(parent.as_ref(), gio::Cancellable::NONE, move |res| {
            let Some(mut path) = res.ok().and_then(|f| f.path()) else { return };
            let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
            if !matches!(ext.as_deref(), Some("mpcpl" | "pls" | "m3u" | "m3u8" | "asx")) {
                path.set_extension("mpcpl");
            }
            if let Err(e) = playlist::save(&path, &pl.borrow().items) {
                eprintln!("{}: {e}", path.display());
            }
        });
}
