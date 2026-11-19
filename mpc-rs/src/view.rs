//! View > Zoom, Video Frame, Pan&Scan (MPC's `MainFrm.cpp` OnViewZoom / OnViewDefaultVideoFrame /
//! OnViewPanNScan), and File > Save Thumbnails. mpv does the scaling; this maps MPC's modes onto its
//! `video-zoom`, `video-unscaled`, `panscan`, `video-scale-*` and `video-pan-*` properties.

use crate::config::{VideoFrame, Zoom};
use gtk4::{self as gtk, cairo, gdk, glib, prelude::*};
use libmpv2::Mpv;
use std::{cell::Cell, path::PathBuf, rc::Rc, time::Duration};

fn prop<T: libmpv2::GetData>(mpv: &Mpv, name: &str) -> Option<T> {
    mpv.get_property(name).ok()
}

fn set(mpv: &Mpv, name: &str, value: &str) {
    if let Err(e) = mpv.set_property(name, value) {
        eprintln!("mpv {name}={value}: {e}");
    }
}

/// Resize the window so the video shows at `zoom` (MPC's ZoomVideoWindow). Fit modes use `autofit` %
/// of the screen. Does nothing in fullscreen or maximized, or before the video size is known.
pub fn zoom_window(win: &gtk::ApplicationWindow, video: &impl IsA<gtk::Widget>, mpv: &Mpv, zoom: Zoom, autofit: u32) {
    if win.is_fullscreen() || win.is_maximized() {
        return;
    }
    let (Some(vw), Some(vh)) = (prop::<i64>(mpv, "dwidth"), prop::<i64>(mpv, "dheight")) else { return };
    let (vw, vh) = (vw as f64, vh as f64);
    // Everything around the video (menu, bars, playlist) keeps its size.
    let (cw, ch) = ((win.width() - video.as_ref().width()) as f64, (win.height() - video.as_ref().height()) as f64);
    let scale = match zoom {
        Zoom::Half => 0.5,
        Zoom::Normal => 1.0,
        Zoom::Double => 2.0,
        Zoom::AutoFit | Zoom::AutoFitLarger => {
            let monitor = win.surface().and_then(|s| WidgetExt::display(win).monitor_at_surface(&s));
            let Some(geo) = monitor.map(|m| m.geometry()) else { return };
            let f = autofit.clamp(25, 100) as f64 / 100.0;
            let fit = ((geo.width() as f64 * f - cw) / vw).min((geo.height() as f64 * f - ch) / vh);
            if zoom == Zoom::AutoFitLarger { fit.min(1.0) } else { fit }
        }
    };
    win.set_default_size((vw * scale + cw).round() as i32, (vh * scale + ch).round() as i32);
}

/// View > Video Frame.
pub fn apply_frame(mpv: &Mpv, frame: VideoFrame) {
    use VideoFrame::*;
    let (unscaled, zoom, panscan, keepaspect) = match frame {
        Half => ("yes", -1.0, 0.0, "yes"),
        Normal => ("yes", 0.0, 0.0, "yes"),
        Double => ("yes", 1.0, 0.0, "yes"),
        Stretch => ("no", 0.0, 0.0, "no"),
        FromInside => ("no", 0.0, 0.0, "yes"),
        Zoom1 => ("no", 0.0, 1.0 / 3.0, "yes"),
        Zoom2 => ("no", 0.0, 2.0 / 3.0, "yes"),
        FromOutside => ("no", 0.0, 1.0, "yes"),
    };
    set(mpv, "video-unscaled", unscaled);
    set(mpv, "video-zoom", &zoom.to_string());
    set(mpv, "panscan", &panscan.to_string());
    set(mpv, "keepaspect", keepaspect);
}

/// MPC's pan & scan state: zoom per axis (1 = none) and position (0..1, 0.5 = centered).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PanScan {
    pub zoom_x: f64,
    pub zoom_y: f64,
    pub pos_x: f64,
    pub pos_y: f64,
}

impl Default for PanScan {
    fn default() -> PanScan {
        PanScan { zoom_x: 1.0, zoom_y: 1.0, pos_x: 0.5, pos_y: 0.5 }
    }
}

impl PanScan {
    /// MPC's OnViewPanNScan: size steps of 2 % (0.2..3), moves of 0.5 % of the zoomed size.
    pub fn step(&mut self, grow_x: i32, grow_y: i32, move_x: i32, move_y: i32) {
        let zoom = |z: &mut f64, d: i32| {
            if d > 0 && *z < 3.0 {
                *z *= 1.02;
            } else if d < 0 && *z > 0.2 {
                *z /= 1.02;
            }
        };
        zoom(&mut self.zoom_x, grow_x);
        zoom(&mut self.zoom_y, grow_y);
        self.pos_x = (self.pos_x + move_x as f64 * 0.005 * self.zoom_x).clamp(0.0, 1.0);
        self.pos_y = (self.pos_y + move_y as f64 * 0.005 * self.zoom_y).clamp(0.0, 1.0);
    }

    pub fn apply(&self, mpv: &Mpv) {
        set(mpv, "video-scale-x", &self.zoom_x.to_string());
        set(mpv, "video-scale-y", &self.zoom_y.to_string());
        set(mpv, "video-pan-x", &(self.pos_x - 0.5).to_string());
        set(mpv, "video-pan-y", &(self.pos_y - 0.5).to_string());
    }
}

/// File > Save Thumbnails: a `rows` x `cols` sheet `width` pixels wide with a header, like MPC's.
/// Pauses, seeks to each point, grabs the frame, then goes back to where playback was. `done` gets the
/// result when finished.
pub fn save_thumbnails(mpv: &'static Mpv, out: PathBuf, rows: u32, cols: u32, width: u32, done: impl Fn(Result<PathBuf, String>) + 'static) {
    let (Some(dur), Some(vw), Some(vh)) = (prop::<f64>(mpv, "duration"), prop::<i64>(mpv, "dwidth"), prop::<i64>(mpv, "dheight")) else {
        done(Err("No video is playing.".into()));
        return;
    };
    let (rows, cols) = (rows.clamp(1, 16) as usize, cols.clamp(1, 16) as usize);
    let n = rows * cols;
    let (resume_at, was_paused) = (prop::<f64>(mpv, "time-pos").unwrap_or(0.0), prop::<bool>(mpv, "pause").unwrap_or(false));
    let _ = mpv.set_property("pause", true);
    let tmp = glib::user_runtime_dir().join(format!("mpc-rs-thumb-{}.png", std::process::id()));
    let shots: Rc<std::cell::RefCell<Vec<(f64, gdk::gdk_pixbuf::Pixbuf)>>> = Rc::default();
    let i = Rc::new(Cell::new(0usize));
    let waiting = Rc::new(Cell::new(false));
    let target = move |i: usize| dur * (i + 1) as f64 / (n + 1) as f64;
    let done = Rc::new(done);
    glib::timeout_add_local(Duration::from_millis(40), move || {
        let k = i.get();
        if k < n {
            if !waiting.get() {
                let _ = mpv.command("seek", &[&target(k).to_string(), "absolute+exact"]);
                waiting.set(true);
            } else if !prop::<bool>(mpv, "seeking").unwrap_or(true) {
                waiting.set(false);
                let _ = mpv.command("screenshot-to-file", &[&tmp.to_string_lossy(), "video"]);
                if let Ok(p) = gdk::gdk_pixbuf::Pixbuf::from_file(&tmp) {
                    shots.borrow_mut().push((target(k), p));
                }
                i.set(k + 1);
            }
            return glib::ControlFlow::Continue;
        }
        let _ = std::fs::remove_file(&tmp);
        let _ = mpv.command("seek", &[&resume_at.to_string(), "absolute+exact"]);
        let _ = mpv.set_property("pause", was_paused);
        let title = prop::<String>(mpv, "filename").unwrap_or_default();
        let size = prop::<i64>(mpv, "file-size").unwrap_or(0);
        done(compose(&out, &shots.borrow(), (rows, cols, width as i32), (vw, vh, dur), &title, size));
        glib::ControlFlow::Break
    });
}

fn hms(t: f64) -> String {
    let s = t.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

fn compose(out: &PathBuf, shots: &[(f64, gdk::gdk_pixbuf::Pixbuf)], (rows, cols, width): (usize, usize, i32), (vw, vh, dur): (i64, i64, f64), title: &str, size: i64) -> Result<PathBuf, String> {
    let err = |e: &dyn std::fmt::Display| e.to_string();
    let margin = 8.0;
    let header = 60.0;
    let tw = (width as f64 - margin * (cols as f64 + 1.0)) / cols as f64;
    let th = tw * vh as f64 / vw as f64;
    let height = (header + margin + rows as f64 * (th + margin)).ceil() as i32;
    let surface = cairo::ImageSurface::create(cairo::Format::Rgb24, width, height).map_err(|e| err(&e))?;
    {
        let cr = cairo::Context::new(&surface).map_err(|e| err(&e))?;
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.paint().map_err(|e| err(&e))?;
        cr.set_source_rgb(0.0, 0.0, 0.0);
        cr.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        cr.set_font_size(13.0);
        let lines = [
            format!("File: {title}"),
            format!("Size: {:.1} MiB    Resolution: {vw}x{vh}    Duration: {}", size as f64 / 1048576.0, hms(dur)),
        ];
        for (k, line) in lines.iter().enumerate() {
            cr.move_to(margin, 22.0 + k as f64 * 20.0);
            cr.show_text(line).map_err(|e| err(&e))?;
        }
        for (k, (t, shot)) in shots.iter().enumerate() {
            let (x, y) = (margin + (k % cols) as f64 * (tw + margin), header + margin + (k / cols) as f64 * (th + margin));
            let scaled = shot.scale_simple(tw as i32, th as i32, gdk::gdk_pixbuf::InterpType::Bilinear).ok_or("scale failed")?;
            cr.set_source_pixbuf(&scaled, x, y);
            cr.paint().map_err(|e| err(&e))?;
            // Time stamp in the corner, white on a dark shadow.
            cr.set_font_size(12.0);
            for (dx, c) in [(1.0, 0.0), (0.0, 1.0)] {
                cr.set_source_rgb(c, c, c);
                cr.move_to(x + tw - 64.0 + dx, y + th - 6.0 + dx);
                cr.show_text(&hms(*t)).map_err(|e| err(&e))?;
            }
        }
    }
    surface.flush();
    // Cairo's Rgb24 is BGRx in memory (little endian); the pixbuf wants plain RGB without alpha,
    // which every encoder (JPEG included) accepts.
    let stride = surface.stride() as usize;
    let data = surface.take_data().map_err(|e| err(&e))?;
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for row in data.chunks(stride).take(height as usize) {
        for px in row[..width as usize * 4].chunks(4) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
    }
    let pixbuf = gdk::gdk_pixbuf::Pixbuf::from_bytes(&glib::Bytes::from_owned(rgb), gdk::gdk_pixbuf::Colorspace::Rgb, false, 8, width, height, width * 3);
    let png = out.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"));
    // Anything but PNG is written as JPEG (MPC's default).
    let (kind, opts): (&str, &[(&str, &str)]) = if png { ("png", &[]) } else { ("jpeg", &[("quality", "90")]) };
    pixbuf.savev(out, kind, opts).map_err(|e| err(&e))?;
    Ok(out.clone())
}
