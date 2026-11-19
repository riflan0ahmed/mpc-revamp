//! Options dialog (MPC's `PPageSheet`). Only the Keys page so far (`PPageAccelTbl`); more pages come with
//! the settings phase.

use crate::commands::{Binding, COMMANDS, Media, Mouse};
use gtk4::{self as gtk, gdk, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

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

/// Show the dialog with a copy of `bindings`. OK and Apply hand the edited copy to `on_apply`.
pub fn show(parent: &gtk::ApplicationWindow, bindings: Vec<Binding>, implemented: Vec<bool>, on_apply: impl Fn(Vec<Binding>) + 'static) {
    let dlg = gtk::Window::builder().title("Options").transient_for(parent).modal(true).default_width(1000).default_height(640).build();

    let (keys, page) = keys_page(&dlg, bindings, implemented);
    let stack = gtk::Stack::new();
    stack.add_titled(&page, Some("keys"), "Keys");
    let sidebar = gtk::StackSidebar::new();
    sidebar.set_stack(&stack);

    let on_apply = Rc::new(on_apply);
    let ok = gtk::Button::with_label("OK");
    ok.add_css_class("suggested-action");
    let cancel = gtk::Button::with_label("Cancel");
    let apply = gtk::Button::with_label("Apply");
    ok.connect_clicked({
        let (dlg, keys, on_apply) = (dlg.clone(), keys.clone(), on_apply.clone());
        move |_| {
            on_apply(keys.work.borrow().clone());
            dlg.close();
        }
    });
    apply.connect_clicked(move |_| on_apply(keys.work.borrow().clone()));
    cancel.connect_clicked({
        let dlg = dlg.clone();
        move |_| dlg.close()
    });
    let buttons = gtk::Box::builder().spacing(8).halign(gtk::Align::End).margin_top(8).margin_bottom(12).margin_end(12).build();
    buttons.append(&ok);
    buttons.append(&cancel);
    buttons.append(&apply);

    let main = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    main.append(&sidebar);
    main.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    main.append(&stack);
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.append(&main);
    vbox.append(&buttons);
    dlg.set_child(Some(&vbox));
    dlg.set_default_widget(Some(&ok));
    dlg.present();
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
