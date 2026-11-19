//! DVB TV (MPC's BDA tuner support, the Linux way): channels come from a zap-format `channels.conf`
//! made by `dvbv5-scan` (dvb-tools, with the frequency tables from dtv-scan-tables), and mpv plays
//! them as `dvb://<channel>`. mpv has no program guide, so there's no EPG; only channel names.
//!
//! Untested on real hardware so far (the development machine has no tuner).

use crate::config;
use gtk4::{self as gtk, gio, glib, prelude::*};
use std::path::{Path, PathBuf};

/// Where the scan writes the channel list, and where mpv reads it (`dvbin-file`).
pub fn channels_file() -> PathBuf {
    config::dir().join("channels.conf")
}

/// Channel names from a channels.conf: zap format ("Name:freq:...") or VDR ("Name;Provider:freq:...").
pub fn parse_channels(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && l.contains(':'))
        .filter_map(|l| {
            let name = l.split(':').next()?.split(';').next()?.trim();
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

pub fn channels() -> Vec<String> {
    std::fs::read_to_string(channels_file()).map(|t| parse_channels(&t)).unwrap_or_default()
}

/// The tuners: /dev/dvb/adapterN (N).
pub fn adapters() -> Vec<u32> {
    let mut v: Vec<u32> = std::fs::read_dir("/dev/dvb")
        .map(|rd| rd.flatten().filter_map(|e| e.file_name().to_string_lossy().strip_prefix("adapter")?.parse().ok()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Initial scan tables (dtv-scan-tables): (label "dvb-t/de-Berlin", path).
pub fn scan_tables() -> Vec<(String, PathBuf)> {
    let root = Path::new("/usr/share/dvb");
    let mut out = Vec::new();
    for kind in ["dvb-t", "dvb-c", "dvb-s", "atsc", "isdb-t"] {
        if let Ok(rd) = std::fs::read_dir(root.join(kind)) {
            for e in rd.flatten() {
                out.push((format!("{kind}/{}", e.file_name().to_string_lossy()), e.path()));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

pub fn is_dvb(path: &str) -> bool {
    path.starts_with("dvb://")
}

pub fn channel_url(name: &str) -> String {
    format!("dvb://{name}")
}

/// Navigate > Tuner Scan: run dvbv5-scan for a tuner and region, show its output, and save the
/// channels. `on_done` runs after a scan that found channels.
pub fn scan_dialog(parent: &gtk::Window, on_done: impl Fn() + 'static) {
    let dlg = gtk::Window::builder().title("Tuner Scan").transient_for(parent).modal(true).default_width(640).default_height(480).build();
    let adapters = adapters();
    let tables = scan_tables();
    let tool = glib::find_program_in_path("dvbv5-scan");
    let adapter = gtk::DropDown::from_strings(&adapters.iter().map(|a| format!("Adapter {a} (/dev/dvb/adapter{a})")).collect::<Vec<_>>().iter().map(String::as_str).collect::<Vec<_>>());
    let table = gtk::DropDown::from_strings(&tables.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>());
    table.set_enable_search(true);
    let log = gtk::TextView::builder().editable(false).monospace(true).build();
    let status = gtk::Label::builder().xalign(0.0).wrap(true).build();
    let scan = gtk::Button::with_label("Start Scan");
    scan.add_css_class("suggested-action");
    let close = gtk::Button::with_label("Close");
    close.connect_clicked({
        let dlg = dlg.clone();
        move |_| dlg.close()
    });
    let problem = if adapters.is_empty() {
        Some("No DVB tuner found (/dev/dvb is empty). Connect a tuner and check that its driver is loaded.")
    } else if tool.is_none() || tables.is_empty() {
        Some("Scanning needs dvbv5-scan and the frequency tables: sudo apt install dvb-tools dtv-scan-tables")
    } else {
        None
    };
    if let Some(p) = problem {
        status.set_label(p);
        scan.set_sensitive(false);
    }
    let on_done = std::rc::Rc::new(on_done);
    scan.connect_clicked({
        let (adapter, table, log, status) = (adapter.clone(), table.clone(), log.clone(), status.clone());
        move |b| {
            let (Some(a), Some((_, file))) = (adapters.get(adapter.selected() as usize), tables.get(table.selected() as usize)) else { return };
            let out = channels_file();
            let _ = std::fs::create_dir_all(config::dir());
            let tmp = out.with_extension("conf.new");
            let args = ["dvbv5-scan", "-a", &a.to_string(), "-O", "ZAP", "-o", &tmp.to_string_lossy(), &file.to_string_lossy()];
            let proc = match gio::Subprocess::newv(&args.map(std::ffi::OsStr::new), gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_MERGE) {
                Ok(p) => p,
                Err(e) => {
                    status.set_label(&format!("Couldn't start dvbv5-scan: {e}"));
                    return;
                }
            };
            b.set_sensitive(false);
            status.set_label("Scanning... (this can take a few minutes)");
            log.buffer().set_text("");
            // Stream the scanner's output into the log as it comes.
            let stream = gio::DataInputStream::new(&proc.stdout_pipe().unwrap());
            read_lines(stream, log.clone());
            let (b, status, on_done) = (b.clone(), status.clone(), on_done.clone());
            proc.wait_async(gio::Cancellable::NONE, move |_| {
                b.set_sensitive(true);
                let found = std::fs::read_to_string(&tmp).map(|t| parse_channels(&t)).unwrap_or_default();
                if found.is_empty() {
                    status.set_label("No channels found. Check the antenna/cable and the region.");
                    let _ = std::fs::remove_file(&tmp);
                } else if std::fs::rename(&tmp, &out).is_ok() {
                    status.set_label(&format!("Found {} channels, saved to {}.", found.len(), out.display()));
                    on_done();
                }
            });
        }
    });
    let grid = gtk::Grid::builder().row_spacing(8).column_spacing(8).margin_top(12).margin_start(12).margin_end(12).build();
    grid.attach(&gtk::Label::builder().label("Tuner:").xalign(0.0).build(), 0, 0, 1, 1);
    grid.attach(&adapter, 1, 0, 1, 1);
    grid.attach(&gtk::Label::builder().label("Region / transponders:").xalign(0.0).build(), 0, 1, 1, 1);
    grid.attach(&table, 1, 1, 1, 1);
    let buttons = gtk::Box::builder().spacing(8).halign(gtk::Align::End).margin_bottom(12).margin_end(12).build();
    buttons.append(&close);
    buttons.append(&scan);
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 8);
    vbox.append(&grid);
    let s = gtk::ScrolledWindow::builder().vexpand(true).child(&log).margin_start(12).margin_end(12).build();
    vbox.append(&s);
    status.set_margin_start(12);
    status.set_margin_end(12);
    vbox.append(&status);
    vbox.append(&buttons);
    dlg.set_child(Some(&vbox));
    dlg.present();
}

fn read_lines(stream: gio::DataInputStream, log: gtk::TextView) {
    stream.clone().read_line_utf8_async(glib::Priority::DEFAULT, gio::Cancellable::NONE, move |r| {
        if let Ok(Some(line)) = r {
            let b = log.buffer();
            b.insert(&mut b.end_iter(), &format!("{line}\n"));
            let mark = b.create_mark(None, &b.end_iter(), false);
            log.scroll_mark_onscreen(&mark);
            read_lines(stream, log);
        }
    });
}
