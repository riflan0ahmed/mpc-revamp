//! Video shaders (MPC's Options > Shaders and shader editor). MPC's bundled HLSL shaders are ported to
//! mpv's GLSL user-shader format in `res/shaders/`; the user's own live in `$XDG_CONFIG_HOME/mpc-rv/shaders/`
//! (same name = the user's copy wins). Pre-resize shaders hook mpv's MAIN texture, post-resize ones OUTPUT.

use crate::config;
use gtk4::{self as gtk, gio, glib, prelude::*};
use std::{path::PathBuf, rc::Rc};

const BUNDLED: &[(&str, &str)] = &[
    ("0-255 to 16-235", include_str!("../../res/shaders/0-255 to 16-235.glsl")),
    ("16-235 to 0-255 [SD]", include_str!("../../res/shaders/16-235 to 0-255 [SD].glsl")),
    ("16-235 to 0-255 [SD][HD]", include_str!("../../res/shaders/16-235 to 0-255 [SD][HD].glsl")),
    ("BT.601 to BT.709", include_str!("../../res/shaders/BT.601 to BT.709.glsl")),
    ("Contour", include_str!("../../res/shaders/Contour.glsl")),
    ("Deinterlace (blend)", include_str!("../../res/shaders/Deinterlace (blend).glsl")),
    ("Denoise", include_str!("../../res/shaders/Denoise.glsl")),
    ("Edge sharpen", include_str!("../../res/shaders/Edge sharpen.glsl")),
    ("Emboss", include_str!("../../res/shaders/Emboss.glsl")),
    ("Grayscale", include_str!("../../res/shaders/Grayscale.glsl")),
    ("Invert", include_str!("../../res/shaders/Invert.glsl")),
    ("LCD angle correction", include_str!("../../res/shaders/LCD angle correction.glsl")),
    ("Letterbox", include_str!("../../res/shaders/Letterbox.glsl")),
    ("Nightvision", include_str!("../../res/shaders/Nightvision.glsl")),
    ("Procamp", include_str!("../../res/shaders/Procamp.glsl")),
    ("Sharpen complex 2", include_str!("../../res/shaders/Sharpen complex 2.glsl")),
    ("Sharpen complex", include_str!("../../res/shaders/Sharpen complex.glsl")),
    ("Sharpen", include_str!("../../res/shaders/Sharpen.glsl")),
    ("Sphere", include_str!("../../res/shaders/Sphere.glsl")),
    ("Spotlight", include_str!("../../res/shaders/Spotlight.glsl")),
    ("Wave", include_str!("../../res/shaders/Wave.glsl")),
    ("YV12 chroma upsampling", include_str!("../../res/shaders/YV12 chroma upsampling.glsl")),
];

pub fn user_dir() -> PathBuf {
    config::dir().join("shaders")
}

/// Every shader name: the bundled ones and the user's, sorted.
pub fn available() -> Vec<String> {
    let mut names: Vec<String> = BUNDLED.iter().map(|(n, _)| n.to_string()).collect();
    if let Ok(rd) = std::fs::read_dir(user_dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "glsl" || x == "hook") {
                names.push(p.file_stem().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

pub fn source(name: &str) -> Option<String> {
    std::fs::read_to_string(user_dir().join(format!("{name}.glsl")))
        .ok()
        .or_else(|| BUNDLED.iter().find(|(n, _)| *n == name).map(|(_, s)| s.to_string()))
}

/// Write the shader chain where mpv can read it; returns the files in order (pre-resize first).
pub fn materialize(pre: &[String], post: &[String]) -> Vec<PathBuf> {
    let dir = glib::user_runtime_dir().join(format!("mpc-rv-shaders-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    if pre.is_empty() && post.is_empty() || std::fs::create_dir_all(&dir).is_err() {
        return Vec::new();
    }
    let mut files = Vec::new();
    for (stage, names) in [("pre", pre), ("post", post)] {
        for (i, name) in names.iter().enumerate() {
            let Some(mut src) = source(name) else { continue };
            if stage == "post" {
                src = src.lines().map(|l| if l.trim() == "//!HOOK MAIN" { "//!HOOK OUTPUT" } else { l }).collect::<Vec<_>>().join("\n");
            }
            let path = dir.join(format!("{stage}-{i}.glsl"));
            if std::fs::write(&path, src).is_ok() {
                files.push(path);
            }
        }
    }
    files
}

pub fn cleanup() {
    let _ = std::fs::remove_dir_all(glib::user_runtime_dir().join(format!("mpc-rv-shaders-{}", std::process::id())));
}

/// MPC's shader editor: pick or name a shader, edit it, save (into the user folder). `on_saved` reloads
/// the chain; `errors` returns mpv's messages since the last save (compile errors show up there).
pub fn editor(parent: &gtk::Window, on_saved: impl Fn() + 'static, errors: impl Fn() -> Vec<String> + 'static) {
    let win = gtk::Window::builder().title("Shader Editor").transient_for(parent).default_width(760).default_height(560).build();
    let names = gtk::StringList::new(&available().iter().map(String::as_str).collect::<Vec<_>>());
    let pick = gtk::DropDown::builder().model(&names).build();
    let name = gtk::Entry::builder().placeholder_text("Name").hexpand(true).build();
    let text = gtk::TextView::builder().monospace(true).wrap_mode(gtk::WrapMode::None).build();
    let log = gtk::Label::builder().xalign(0.0).wrap(true).selectable(true).css_classes(["dim-label"]).build();
    let load = {
        let (names, name, text) = (names.clone(), name.clone(), text.clone());
        move |i: u32| {
            if let Some(n) = names.string(i) {
                name.set_text(&n);
                text.buffer().set_text(&source(&n).unwrap_or_default());
            }
        }
    };
    pick.connect_selected_notify({
        let load = load.clone();
        move |d| load(d.selected())
    });
    load(0);
    let new = gtk::Button::with_label("New");
    new.connect_clicked({
        let (name, text) = (name.clone(), text.clone());
        move |_| {
            name.set_text("New shader");
            text.buffer().set_text("//!HOOK MAIN\n//!BIND HOOKED\n//!DESC New shader\n\nvec4 hook() {\n    return HOOKED_tex(HOOKED_pos);\n}\n");
        }
    });
    let save = gtk::Button::with_label("Save");
    save.add_css_class("suggested-action");
    let on_saved = Rc::new(on_saved);
    let errors = Rc::new(errors);
    save.connect_clicked({
        let (name, text, log, names, pick) = (name.clone(), text.clone(), log.clone(), names.clone(), pick.clone());
        move |_| {
            let n = name.text().trim().replace(['/', '\\'], "_");
            if n.is_empty() {
                return;
            }
            let b = text.buffer();
            let path = user_dir().join(format!("{n}.glsl"));
            let saved = std::fs::create_dir_all(user_dir()).and_then(|_| std::fs::write(&path, b.text(&b.start_iter(), &b.end_iter(), false)));
            if let Err(e) = saved {
                log.set_label(&format!("{}: {e}", path.display()));
                return;
            }
            if !available_contains(&names, &n) {
                names.append(&n);
                pick.set_selected(names.n_items() - 1);
            }
            errors(); // drop older messages
            on_saved();
            // mpv compiles on the next frame; report what it said shortly after.
            let (log, errors) = (log.clone(), errors.clone());
            glib::timeout_add_local_once(std::time::Duration::from_millis(700), move || {
                let e = errors();
                log.set_label(&if e.is_empty() { format!("Saved to {}. No errors reported (shaders compile when they are in use).", path.display()) } else { e.join("\n") });
            });
        }
    });
    let folder = gtk::Button::with_label("Open Folder");
    folder.connect_clicked(|b| {
        let _ = std::fs::create_dir_all(user_dir());
        let launcher = gtk::FileLauncher::new(Some(&gio::File::for_path(user_dir())));
        launcher.launch(b.root().and_downcast_ref::<gtk::Window>(), gio::Cancellable::NONE, |_| {});
    });
    let top = gtk::Box::builder().spacing(8).margin_top(8).margin_start(8).margin_end(8).build();
    top.append(&pick);
    top.append(&name);
    top.append(&new);
    top.append(&folder);
    top.append(&save);
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
    vbox.append(&top);
    vbox.append(&gtk::ScrolledWindow::builder().vexpand(true).child(&text).build());
    vbox.append(&gtk::ScrolledWindow::builder().min_content_height(70).max_content_height(140).child(&log).margin_start(8).margin_end(8).margin_bottom(8).build());
    win.set_child(Some(&vbox));
    win.present();
}

fn available_contains(names: &gtk::StringList, n: &str) -> bool {
    (0..names.n_items()).any(|i| names.string(i).as_deref() == Some(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_shaders_are_mpv_hooks() {
        assert_eq!(BUNDLED.len(), 22);
        for (name, src) in BUNDLED {
            assert!(src.contains("//!HOOK MAIN") && src.contains("vec4 hook()"), "{name}");
        }
    }
}
