//! Subresync bar (MPC's `CPlayerSubresyncBar`): the lines of the current subtitle with their times.
//! Changing a line's start makes it an anchor: lines between anchors are stretched, the rest shifted.
//! mpv plays a resynced copy of the file (in the runtime dir), reloaded after every change.
//! Works for external SRT, WebVTT and ASS/SSA files; embedded subtitles can't be read back from mpv.

use crate::subtitles::{self, Doc, Kind};
use gtk4::{self as gtk, glib, prelude::*};
use libmpv2::Mpv;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};

pub struct Subresync {
    pub widget: gtk::Box,
    mpv: &'static Mpv,
    doc: RefCell<Option<Doc>>,
    /// The subtitle file the lines came from, and the resynced copy mpv is playing.
    source: RefCell<Option<String>>,
    copy: RefCell<Option<PathBuf>>,
    store: gtk::StringList,
    sel: gtk::SingleSelection,
    view: gtk::ColumnView,
    start: gtk::Entry,
    note: gtk::Label,
    current: Cell<Option<usize>>,
}

fn index_of(item: &glib::Object) -> usize {
    item.downcast_ref::<gtk::StringObject>().unwrap().string().parse().unwrap()
}

fn prop<T: libmpv2::GetData>(mpv: &Mpv, name: &str) -> Option<T> {
    mpv.get_property(name).ok()
}

impl Subresync {
    /// Follow the current subtitle track; called every UI tick.
    pub fn tick(&self) {
        let file = prop::<String>(self.mpv, "current-tracks/sub/external-filename");
        let ours = file.as_deref().map(PathBuf::from) == *self.copy.borrow();
        if !ours && file != *self.source.borrow() {
            self.load(file);
        }
        if !self.widget.is_visible() {
            return;
        }
        let t = prop::<f64>(self.mpv, "time-pos").unwrap_or(0.0) - prop::<f64>(self.mpv, "sub-delay").unwrap_or(0.0);
        let cur = self.doc.borrow().as_ref().and_then(|d| d.line_at(t));
        if cur != self.current.get() {
            let old = self.current.replace(cur);
            for i in [old, cur].into_iter().flatten() {
                self.redraw(i);
            }
            if let Some(i) = cur {
                self.view.scroll_to(i as u32, None, gtk::ListScrollFlags::NONE, None);
            }
        }
    }

    fn load(&self, file: Option<String>) {
        let doc = file.as_deref().and_then(subtitles::load);
        self.note.set_label(match (&file, &doc) {
            (None, _) => "No external subtitle file is selected.",
            (Some(_), None) => "Only SRT, WebVTT and ASS/SSA subtitle files can be resynced.",
            _ => "Select a line, then set its start time. Lines between changed lines are stretched, the others shifted.",
        });
        *self.source.borrow_mut() = file.filter(|_| doc.is_some());
        let n = doc.as_ref().map_or(0, |d| d.lines.len());
        *self.doc.borrow_mut() = doc;
        self.copy.take();
        self.current.set(None);
        let items: Vec<String> = (0..n).map(|i| i.to_string()).collect();
        self.store.splice(0, self.store.n_items(), &items.iter().map(String::as_str).collect::<Vec<_>>());
    }

    fn redraw(&self, i: usize) {
        if (i as u32) < self.store.n_items() {
            let pos = self.sel.selected();
            self.store.splice(i as u32, 1, &[&i.to_string()]);
            self.sel.set_selected(pos);
        }
    }

    /// The resynced subtitle text, shifted by `offset` seconds (for File > Save Subtitles).
    pub fn text(&self, offset: f64) -> Option<(String, Kind, String)> {
        let doc = self.doc.borrow();
        let doc = doc.as_ref()?;
        Some((doc.to_text(offset), doc.kind, self.source.borrow().clone()?))
    }

    /// MPC's "Shift Subtitle Left/Right": move the current line by `delta` seconds.
    pub fn shift_current(&self, delta: f64) {
        let Some(i) = self.current.get() else { return };
        let start = match self.doc.borrow().as_ref() {
            Some(d) => d.start(i) + delta,
            None => return,
        };
        self.set_start(i, start);
    }

    fn set_start(&self, i: usize, t: f64) {
        if let Some(d) = self.doc.borrow_mut().as_mut() {
            d.set_start(i, t);
        }
        self.changed();
    }

    /// Redraw everything and make mpv play the resynced copy.
    fn changed(&self) {
        let pos = self.sel.selected();
        let n = self.store.n_items();
        let items: Vec<String> = (0..n).map(|i| i.to_string()).collect();
        self.store.splice(0, n, &items.iter().map(String::as_str).collect::<Vec<_>>());
        self.sel.set_selected(pos);
        let Some((text, _, source)) = self.text(0.0) else { return };
        let ext = std::path::Path::new(&source).extension().map_or("srt".into(), |e| e.to_string_lossy().into_owned());
        let copy = glib::user_runtime_dir().join(format!("mpc-rv-subresync-{}.{ext}", std::process::id()));
        if let Err(e) = std::fs::write(&copy, text) {
            eprintln!("{}: {e}", copy.display());
            return;
        }
        // Replace the playing track with the new copy (keeps the subtitle delay and visibility).
        if let Some(id) = prop::<i64>(self.mpv, "current-tracks/sub/id") {
            let _ = self.mpv.command("sub-remove", &[&id.to_string()]);
        }
        let title = std::path::Path::new(&source).file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let _ = self.mpv.command("sub-add", &[&copy.to_string_lossy(), "select", &title]);
        *self.copy.borrow_mut() = Some(copy);
    }

    fn fill_editor(&self) {
        let start = self.sel.selected_item().map(|i| index_of(&i)).and_then(|i| self.doc.borrow().as_ref().map(|d| d.start(i)));
        self.start.set_text(&start.map_or(String::new(), |t| subtitles::format_time(t, Kind::Vtt)));
    }
}

impl Subresync {
    /// Remove the resynced copy (when the window closes).
    pub fn cleanup(&self) {
        if let Some(c) = self.copy.take() {
            let _ = std::fs::remove_file(c);
        }
    }
}

pub fn new(mpv: &'static Mpv) -> Rc<Subresync> {
    let css = gtk::CssProvider::new();
    css.load_from_string(".mpc-anchor { color: #3584e4; font-style: italic; }");
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
    let store = gtk::StringList::new(&[]);
    let sel = gtk::SingleSelection::new(Some(store.clone()));
    let view = gtk::ColumnView::new(Some(sel.clone()));
    view.add_css_class("data-table");
    let widget = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let bar = Rc::new(Subresync {
        widget: widget.clone(),
        mpv,
        doc: RefCell::new(None),
        source: RefCell::new(None),
        copy: RefCell::new(None),
        store,
        sel: sel.clone(),
        view: view.clone(),
        start: gtk::Entry::builder().width_chars(14).placeholder_text("hh:mm:ss.mmm").build(),
        note: gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["dim-label"]).build(),
        current: Cell::new(None),
    });

    let column = |title: &str, expand: bool, text: fn(&Doc, usize) -> String| {
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let label = gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
            item.downcast_ref::<gtk::ListItem>().unwrap().set_child(Some(&label));
        });
        let bar = bar.clone();
        factory.connect_bind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let i = index_of(&item.item().unwrap());
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            let doc = bar.doc.borrow();
            let Some(doc) = doc.as_ref() else { return };
            label.set_text(&text(doc, i));
            // The line on screen is bold; changed (anchor) lines are highlighted.
            for (class, on) in [("heading", bar.current.get() == Some(i)), ("mpc-anchor", doc.anchors.iter().any(|(a, _)| *a == i))] {
                if on {
                    label.add_css_class(class);
                } else {
                    label.remove_css_class(class);
                }
            }
        });
        let col = gtk::ColumnViewColumn::new(Some(title), Some(factory));
        col.set_expand(expand);
        col.set_resizable(true);
        view.append_column(&col);
    };
    column("Start", false, |d, i| subtitles::format_time(d.start(i), Kind::Vtt));
    column("End", false, |d, i| subtitles::format_time(d.end(i), Kind::Vtt));
    column("Text", true, |d, i| d.lines[i].text.replace('\n', " / "));

    // Double-click / Enter: jump to the line.
    view.connect_activate({
        let bar = bar.clone();
        move |_, pos| {
            let Some(i) = bar.sel.item(pos).map(|o| index_of(&o)) else { return };
            let Some(t) = bar.doc.borrow().as_ref().map(|d| d.start(i)) else { return };
            let delay = prop::<f64>(bar.mpv, "sub-delay").unwrap_or(0.0);
            let _ = bar.mpv.command("seek", &[&(t + delay).to_string(), "absolute+exact"]);
        }
    });
    sel.connect_selected_notify({
        let bar = bar.clone();
        move |_| bar.fill_editor()
    });
    let selected = {
        let bar = bar.clone();
        move || bar.sel.selected_item().map(|i| index_of(&i))
    };
    // The time field only takes the keyboard while it's being edited (click it, Enter applies), so the
    // player's single-key shortcuts (space, arrows, ...) keep working otherwise.
    bar.start.set_can_focus(false);
    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    click.connect_pressed({
        let entry = bar.start.clone();
        move |_, _, _, _| {
            entry.set_can_focus(true);
            entry.grab_focus();
        }
    });
    bar.start.add_controller(click);
    let leave = gtk::EventControllerFocus::new();
    leave.connect_leave(|c| {
        if let Some(w) = c.widget() {
            w.set_can_focus(false);
        }
    });
    bar.start.add_controller(leave);
    bar.start.connect_activate({
        let (bar, selected) = (bar.clone(), selected.clone());
        move |e| {
            let t = e.text().replace(',', ".");
            let secs = t.split(':').try_fold(0.0, |acc, p| p.trim().parse::<f64>().ok().map(|v| acc * 60.0 + v));
            if let (Some(i), Some(t)) = (selected(), secs) {
                bar.set_start(i, t);
            }
            if let Some(win) = e.root().and_downcast::<gtk::Window>() {
                gtk::prelude::GtkWindowExt::set_focus(&win, None::<&gtk::Widget>);
            }
        }
    });
    let now = gtk::Button::builder().label("Set to current time").focusable(false).build();
    now.set_tooltip_text(Some("The selected line starts at the current playback position"));
    now.connect_clicked({
        let (bar, selected) = (bar.clone(), selected.clone());
        move |_| {
            let t = prop::<f64>(bar.mpv, "time-pos").unwrap_or(0.0) - prop::<f64>(bar.mpv, "sub-delay").unwrap_or(0.0);
            if let Some(i) = selected() {
                bar.set_start(i, t);
            }
        }
    });
    let reset = gtk::Button::builder().label("Reset").focusable(false).build();
    reset.set_tooltip_text(Some("Undo all timing changes"));
    reset.connect_clicked({
        let bar = bar.clone();
        move |_| {
            if let Some(d) = bar.doc.borrow_mut().as_mut() {
                d.anchors.clear();
            }
            bar.changed();
        }
    });

    let tools = gtk::Box::builder().spacing(8).margin_start(6).margin_end(6).build();
    tools.append(&gtk::Label::new(Some("Start:")));
    tools.append(&bar.start);
    tools.append(&now);
    tools.append(&reset);
    bar.note.set_hexpand(true);
    tools.append(&bar.note);
    widget.append(&tools);
    // Fixed height, so the video keeps the rest of the window (MPC docks this bar under the video).
    widget.append(&gtk::ScrolledWindow::builder().min_content_height(160).max_content_height(160).child(&view).build());
    bar.load(None);
    bar
}
