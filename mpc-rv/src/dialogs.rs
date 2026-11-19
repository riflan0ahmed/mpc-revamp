//! Phase 8 dialogs and helpers: discs and capture devices, Go To, Properties, Organize Favorites and
//! Save a Copy (MPC's `GoToDlg`, `PPageFileInfo*`, `FavoriteOrganizeDlg`, `SaveDlg`).

use crate::config::Favorite;
use gtk4::{self as gtk, gio, glib, prelude::*};
use libmpv2::Mpv;
use std::{cell::RefCell, path::Path, rc::Rc};

fn prop<T: libmpv2::GetData>(mpv: &Mpv, name: &str) -> Option<T> {
    mpv.get_property(name).ok()
}

fn hms(t: f64) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

// --- discs and devices ---

/// The mpv URL for a DVD/Blu-ray folder or image (`dvd://[title]/device`; the device path is absolute,
/// hence the extra slash), or None if `path` isn't one.
pub fn disc_url(path: &Path) -> Option<String> {
    let has = |name: &str| path.join(name).is_dir() || path.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name));
    let base = if path.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("VIDEO_TS") || n.to_string_lossy().eq_ignore_ascii_case("BDMV")) {
        path.parent()?
    } else {
        path
    };
    let abs = base.canonicalize().ok()?;
    if has("VIDEO_TS") {
        Some(format!("dvd:///{}", abs.display()))
    } else if has("BDMV") {
        Some(format!("bd:///{}", abs.display()))
    } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("iso")) {
        Some(format!("dvd:///{}", path.canonicalize().ok()?.display()))
    } else {
        None
    }
}

pub fn is_disc_url(url: &str) -> bool {
    ["dvd://", "bd://", "cdda://"].iter().any(|p| url.starts_with(p))
}

/// File > Open Disc: (label, mpv URL) for mounted DVDs/Blu-rays, audio CDs and empty-looking drives.
pub fn discs() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut devices = Vec::new();
    for mount in gio::VolumeMonitor::get().mounts() {
        let name = mount.name().to_string();
        let device = mount.volume().and_then(|v| v.identifier("unix-device")).map(|d| d.to_string());
        if mount.root().uri().starts_with("cdda://") {
            if let Some(d) = &device {
                out.push((format!("Audio CD - {name}"), format!("cdda:///{d}")));
                devices.push(d.clone());
            }
        } else if let Some(url) = mount.root().path().and_then(|root| disc_url(&root)) {
            let kind = if url.starts_with("bd") { "Blu-ray" } else { "DVD" };
            out.push((format!("{kind} - {name}"), url));
            devices.extend(device);
        }
    }
    // Drives with nothing mounted: offer them as audio CDs, which aren't mounted as file systems.
    for i in 0..8 {
        let d = format!("/dev/sr{i}");
        if Path::new(&d).exists() && !devices.contains(&d) {
            out.push((format!("Audio CD - {d}"), format!("cdda:///{d}")));
        }
    }
    out
}

/// Capture devices: (name, /dev/videoN), main nodes only (metadata nodes have index > 0).
pub fn video_devices() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir("/sys/class/video4linux")
        .map(|rd| {
            rd.flatten()
                .filter(|e| std::fs::read_to_string(e.path().join("index")).is_ok_and(|i| i.trim() == "0"))
                .map(|e| {
                    let node = e.file_name().to_string_lossy().into_owned();
                    let name = std::fs::read_to_string(e.path().join("name")).unwrap_or_default().trim().to_string();
                    (if name.is_empty() { node.clone() } else { name }, format!("/dev/{node}"))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

// --- Go To ---

/// Navigate > Go To: a time (hh:mm:ss.ms, mm:ss or seconds) or a frame number.
pub fn goto(parent: &gtk::Window, mpv: &'static Mpv) {
    let dlg = gtk::Window::builder().title("Go To...").transient_for(parent).modal(true).resizable(false).build();
    let pos = prop::<f64>(mpv, "time-pos").unwrap_or(0.0);
    let fps = prop::<f64>(mpv, "container-fps").or_else(|| prop::<f64>(mpv, "estimated-vf-fps")).unwrap_or(25.0);
    let time = gtk::Entry::builder().text(hms(pos)).activates_default(true).width_chars(14).build();
    let frame = gtk::Entry::builder().text((pos * fps).round().to_string()).width_chars(14).build();
    let go_time = gtk::Button::with_label("Go!");
    go_time.add_css_class("suggested-action");
    let go_frame = gtk::Button::with_label("Go!");
    let error = gtk::Label::builder().css_classes(["error"]).xalign(0.0).build();
    let seek = {
        let (dlg, error) = (dlg.clone(), error.clone());
        move |t: Option<f64>| match t {
            Some(t) => {
                let _ = mpv.command("seek", &[&t.to_string(), "absolute+exact"]);
                dlg.close();
            }
            None => error.set_label("Not a valid time or frame number."),
        }
    };
    go_time.connect_clicked({
        let (time, seek) = (time.clone(), seek.clone());
        move |_| seek(time.text().replace(',', ".").split(':').try_fold(0.0, |acc, p| p.trim().parse::<f64>().ok().map(|v| acc * 60.0 + v)))
    });
    go_frame.connect_clicked({
        let frame = frame.clone();
        move |_| seek(frame.text().trim().parse::<f64>().ok().map(|f| f / fps))
    });
    frame.connect_activate({
        let go = go_frame.clone();
        move |_| go.emit_clicked()
    });
    let grid = gtk::Grid::builder().row_spacing(8).column_spacing(8).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    grid.attach(&gtk::Label::builder().label("Time (hh:mm:ss.ms):").xalign(0.0).build(), 0, 0, 1, 1);
    grid.attach(&time, 1, 0, 1, 1);
    grid.attach(&go_time, 2, 0, 1, 1);
    grid.attach(&gtk::Label::builder().label(format!("Frame ({fps:.3} fps):")).xalign(0.0).build(), 0, 1, 1, 1);
    grid.attach(&frame, 1, 1, 1, 1);
    grid.attach(&go_frame, 2, 1, 1, 1);
    grid.attach(&error, 0, 2, 3, 1);
    dlg.set_child(Some(&grid));
    dlg.set_default_widget(Some(&go_time));
    dlg.present();
}

// --- Properties ---

/// File > Properties: Details, Clip and MediaInfo tabs, from what mpv knows about the file.
pub fn properties(parent: &gtk::Window, mpv: &Mpv, path: &str) {
    let dlg = gtk::Window::builder().title(format!("{} Properties", Path::new(path).file_name().map_or(path.into(), |n| n.to_string_lossy().into_owned()))).transient_for(parent).default_width(560).default_height(520).build();
    let s = |n: &str| prop::<String>(mpv, n);
    let meta = |k: &str| s(&format!("metadata/by-key/{k}")).or_else(|| s(&format!("metadata/by-key/{}", k.to_uppercase())));
    let file = gio::File::for_commandline_arg(path);
    let info = file.query_info("standard::*,time::*", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE).ok();
    let size = prop::<i64>(mpv, "file-size").or_else(|| info.as_ref().map(|i| i.size()));
    let created = info.as_ref().and_then(|i| i.modification_date_time()).and_then(|d| d.to_local().ok()).and_then(|d| d.format("%x %X").ok()).map(|s| s.to_string());
    let (w, h) = (prop::<i64>(mpv, "width"), prop::<i64>(mpv, "height"));
    let details: Vec<(&str, String)> = vec![
        ("File name", path.to_string()),
        ("Type", info.as_ref().and_then(|i| i.content_type()).map(|c| gio::content_type_get_description(&c).to_string()).or(s("file-format")).unwrap_or_default()),
        ("Size", size.map_or(String::new(), |b| format!("{:.1} MiB ({b} bytes)", b as f64 / 1048576.0))),
        ("Media length", prop::<f64>(mpv, "duration").map(hms).unwrap_or_default()),
        ("Video size", match (w, h) {
            (Some(w), Some(h)) => format!("{w} x {h}{}", prop::<f64>(mpv, "video-params/aspect").map_or(String::new(), |a| format!(" ({a:.3}:1)"))),
            _ => String::new(),
        }),
        ("Modified", created.unwrap_or_default()),
    ];
    let clip: Vec<(&str, String)> = vec![
        ("Title", meta("title").unwrap_or_default()),
        ("Author", meta("artist").or_else(|| meta("author")).unwrap_or_default()),
        ("Copyright", meta("copyright").unwrap_or_default()),
        ("Rating", meta("rating").unwrap_or_default()),
        ("Location", path.to_string()),
        ("Description", meta("description").or_else(|| meta("comment")).unwrap_or_default()),
    ];
    let table = |rows: &[(&str, String)]| {
        let g = gtk::Grid::builder().row_spacing(6).column_spacing(12).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
        for (i, (k, v)) in rows.iter().enumerate() {
            g.attach(&gtk::Label::builder().label(format!("{k}:")).xalign(0.0).yalign(0.0).build(), 0, i as i32, 1, 1);
            g.attach(&gtk::Label::builder().label(v.as_str()).xalign(0.0).wrap(true).selectable(true).hexpand(true).build(), 1, i as i32, 1, 1);
        }
        g
    };
    let text = media_info(mpv, path);
    let view = gtk::TextView::builder().editable(false).monospace(true).build();
    view.buffer().set_text(&text);
    let copy = gtk::Button::with_label("Copy to clipboard");
    copy.connect_clicked(move |b| b.clipboard().set_text(&text));
    let mi = gtk::Box::new(gtk::Orientation::Vertical, 6);
    mi.append(&gtk::ScrolledWindow::builder().vexpand(true).child(&view).build());
    copy.set_halign(gtk::Align::End);
    copy.set_margin_end(8);
    copy.set_margin_bottom(8);
    mi.append(&copy);
    let tabs = gtk::Notebook::new();
    tabs.append_page(&table(&details), Some(&gtk::Label::new(Some("Details"))));
    tabs.append_page(&table(&clip), Some(&gtk::Label::new(Some("Clip"))));
    tabs.append_page(&mi, Some(&gtk::Label::new(Some("MediaInfo"))));
    dlg.set_child(Some(&tabs));
    dlg.present();
}

/// The MediaInfo tab: general info, metadata and every track, as plain text.
fn media_info(mpv: &Mpv, path: &str) -> String {
    let s = |n: &str| prop::<String>(mpv, n);
    let mut out = format!("General\nComplete name: {path}\n");
    for (label, p) in [("Format", "file-format"), ("Duration", "duration"), ("File size", "file-size"), ("Chapters", "chapters")] {
        if let Some(v) = s(p) {
            out += &format!("{label}: {v}\n");
        }
    }
    let n = prop::<i64>(mpv, "metadata/list/count").unwrap_or(0);
    for i in 0..n {
        if let (Some(k), Some(v)) = (s(&format!("metadata/list/{i}/key")), s(&format!("metadata/list/{i}/value"))) {
            out += &format!("{k}: {v}\n");
        }
    }
    let tracks = prop::<i64>(mpv, "track-list/count").unwrap_or(0);
    for i in 0..tracks {
        let t = |k: &str| s(&format!("track-list/{i}/{k}"));
        let kind = t("type").unwrap_or_default();
        out += &format!("\n{} #{}\n", match kind.as_str() { "video" => "Video", "audio" => "Audio", "sub" => "Text", k => k }, t("id").unwrap_or_default());
        for (label, k) in [
            ("Title", "title"), ("Language", "lang"), ("Format", "codec"), ("Codec description", "codec-desc"), ("Codec profile", "codec-profile"),
            ("Width", "demux-w"), ("Height", "demux-h"), ("Frame rate", "demux-fps"), ("Channels", "demux-channels"),
            ("Sampling rate", "demux-samplerate"), ("Bit rate", "demux-bitrate"), ("Default", "default"), ("Forced", "forced"),
            ("External file", "external-filename"),
        ] {
            if let Some(v) = t(k) {
                out += &format!("{label}: {v}\n");
            }
        }
    }
    out
}

// --- Favorites ---

/// Favorites > Organize: rename, delete and reorder. `on_change` gets the new list.
pub fn organize_favorites(parent: &gtk::Window, favorites: Vec<Favorite>, on_change: impl Fn(Vec<Favorite>) + 'static) {
    let dlg = gtk::Window::builder().title("Organize Favorites").transient_for(parent).modal(true).default_width(560).default_height(380).build();
    let list = gtk::ListBox::new();
    let favs = Rc::new(RefCell::new(favorites));
    let on_change = Rc::new(on_change);
    let refill: Rc<dyn Fn(Option<i32>)> = Rc::new({
        let (list, favs) = (list.clone(), favs.clone());
        move |select| {
            list.remove_all();
            for f in favs.borrow().iter() {
                let pos = f.position.map_or(String::new(), |p| format!("  [{}]", hms(p)));
                let row = gtk::Label::builder().label(format!("{}{pos}\n<small>{}</small>", glib::markup_escape_text(&f.name), glib::markup_escape_text(&f.path))).use_markup(true).xalign(0.0).build();
                list.append(&row);
            }
            if let Some(i) = select {
                list.select_row(list.row_at_index(i).as_ref());
            }
        }
    });
    refill(None);
    let name = gtk::Entry::builder().placeholder_text("Name").hexpand(true).build();
    list.connect_row_selected({
        let (name, favs) = (name.clone(), favs.clone());
        move |_, row| {
            if let Some(f) = row.and_then(|r| favs.borrow().get(r.index() as usize).cloned()) {
                name.set_text(&f.name);
            }
        }
    });
    let edit = |label: &str, f: fn(&mut Vec<Favorite>, usize, &str) -> Option<i32>| {
        let b = gtk::Button::with_label(label);
        let (list, favs, name, refill, on_change) = (list.clone(), favs.clone(), name.clone(), refill.clone(), on_change.clone());
        b.connect_clicked(move |_| {
            let Some(i) = list.selected_row().map(|r| r.index() as usize) else { return };
            let select = f(&mut favs.borrow_mut(), i, name.text().as_str());
            refill(select);
            on_change(favs.borrow().clone());
        });
        b
    };
    let rename = edit("Rename", |v, i, n| {
        if !n.trim().is_empty() {
            v[i].name = n.trim().to_string();
        }
        Some(i as i32)
    });
    let delete = edit("Delete", |v, i, _| {
        v.remove(i);
        None
    });
    let up = edit("Move Up", |v, i, _| {
        if i > 0 {
            v.swap(i, i - 1);
        }
        Some(i.saturating_sub(1) as i32)
    });
    let down = edit("Move Down", |v, i, _| {
        let j = (i + 1).min(v.len() - 1);
        v.swap(i, j);
        Some(j as i32)
    });
    let close = gtk::Button::with_label("Close");
    close.connect_clicked({
        let dlg = dlg.clone();
        move |_| dlg.close()
    });
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    for w in [name.upcast_ref::<gtk::Widget>(), rename.upcast_ref(), delete.upcast_ref(), up.upcast_ref(), down.upcast_ref(), close.upcast_ref()] {
        row.append(w);
    }
    let vbox = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
    vbox.append(&gtk::ScrolledWindow::builder().vexpand(true).child(&list).build());
    vbox.append(&row);
    dlg.set_child(Some(&vbox));
    dlg.present();
}

// --- Save a Copy ---

/// File > Save a Copy: copy the playing file (local, or anything gio can read, like http) with progress.
pub fn save_copy(parent: &gtk::Window, source: &str) {
    let src = gio::File::for_commandline_arg(source);
    let name = src.basename().map_or("copy".into(), |n| n.to_string_lossy().into_owned());
    let parent2 = parent.clone();
    gtk::FileDialog::builder().title("Save a Copy").initial_name(name.as_str()).build().save(Some(parent), gio::Cancellable::NONE, move |res| {
        let Ok(dest) = res else { return };
        let dlg = gtk::Window::builder().title("Saving...").transient_for(&parent2).modal(true).resizable(false).default_width(420).build();
        let bar = gtk::ProgressBar::builder().show_text(true).build();
        let cancel = gtk::Button::with_label("Cancel");
        let stop = gio::Cancellable::new();
        cancel.connect_clicked({
            let stop = stop.clone();
            move |_| stop.cancel()
        });
        let vbox = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).margin_top(16).margin_bottom(16).margin_start(16).margin_end(16).build();
        vbox.append(&gtk::Label::new(Some(&format!("Saving to {}", dest.parse_name()))));
        vbox.append(&bar);
        cancel.set_halign(gtk::Align::End);
        vbox.append(&cancel);
        dlg.set_child(Some(&vbox));
        dlg.present();
        let progress = {
            let bar = bar.clone();
            Box::new(move |done: i64, total: i64| {
                if total > 0 {
                    bar.set_fraction(done as f64 / total as f64);
                    bar.set_text(Some(&format!("{:.1} / {:.1} MiB", done as f64 / 1048576.0, total as f64 / 1048576.0)));
                } else {
                    bar.pulse();
                }
            })
        };
        let parent3 = parent2.clone();
        src.copy_async(&dest, gio::FileCopyFlags::OVERWRITE, glib::Priority::LOW, Some(&stop), Some(progress), move |r| {
            dlg.close();
            if let Err(e) = r
                && !e.matches(gio::IOErrorEnum::Cancelled)
            {
                gtk::AlertDialog::builder().message("Couldn't save a copy").detail(e.message()).build().show(Some(&parent3));
            }
        });
    });
}
