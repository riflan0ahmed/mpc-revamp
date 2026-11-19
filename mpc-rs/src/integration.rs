//! Desktop integration: the .desktop entry and icon, file associations (MPC's Formats page and
//! /regvid /regaud /regpl /regall /unregall), and the system actions of Play > After Playback.

use crate::playlist::{MEDIA_EXTS, PLAYLIST_EXTS};
use gtk4::{gio, glib, prelude::*};
use std::path::PathBuf;

pub const APP_ID: &str = "io.github.mpcrs.Player";
const ICONS: [(u32, &[u8]); 3] = [(48, include_bytes!("../res/icons/48.png")), (128, include_bytes!("../res/icons/128.png")), (256, include_bytes!("../res/icons/256.png"))];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Video,
    Audio,
    Playlist,
}

/// MIME types of the formats we play, by kind (from the file extensions, as the desktop sees them).
pub fn mime_types(kind: Kind) -> Vec<String> {
    let exts: &[&str] = if kind == Kind::Playlist { PLAYLIST_EXTS } else { MEDIA_EXTS };
    let mut out: Vec<String> = exts
        .iter()
        .map(|e| gio::content_type_get_mime_type(&gio::content_type_guess(Some(format!("x.{e}").as_str()), None::<&[u8]>).0).map(|m| m.to_string()).unwrap_or_default())
        .filter(|m| match kind {
            Kind::Video => m.starts_with("video/"),
            Kind::Audio => m.starts_with("audio/") && !m.contains("mpegurl") && !m.contains("scpls"),
            Kind::Playlist => !m.is_empty() && m != "application/octet-stream" && m != "text/plain",
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn data_dir() -> PathBuf {
    glib::user_data_dir()
}

/// The .desktop entry (Exec points at this binary, so a build run from anywhere works).
pub fn desktop_entry() -> String {
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "mpc-rs".into());
    let mimes: Vec<String> = [Kind::Video, Kind::Audio, Kind::Playlist].into_iter().flat_map(mime_types).collect();
    format!(
        "[Desktop Entry]\nType=Application\nName=mpc-rs\nGenericName=Media Player\nComment=Media Player Classic - Home Cinema, rewritten in Rust\n\
         Exec=\"{exe}\" %U\nIcon={APP_ID}\nTerminal=false\nCategories=AudioVideo;Audio;Video;Player;\nStartupWMClass=mpc-rs\n\
         Keywords=video;movie;music;player;mpc;\nMimeType={};\n",
        mimes.join(";")
    )
}

/// Write the desktop entry and icons into the user's data folder.
pub fn install() -> std::io::Result<()> {
    let apps = data_dir().join("applications");
    std::fs::create_dir_all(&apps)?;
    std::fs::write(apps.join(format!("{APP_ID}.desktop")), desktop_entry())?;
    for (size, png) in ICONS {
        let dir = data_dir().join(format!("icons/hicolor/{size}x{size}/apps"));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(format!("{APP_ID}.png")), png)?;
    }
    Ok(())
}

/// `$XDG_CONFIG_HOME/mimeapps.list`, where the desktop keeps the user's default applications.
fn mimeapps() -> (PathBuf, glib::KeyFile) {
    let path = glib::user_config_dir().join("mimeapps.list");
    let kf = glib::KeyFile::new();
    let _ = kf.load_from_file(&path, glib::KeyFileFlags::KEEP_COMMENTS);
    (path, kf)
}

/// Make this player the default for `kinds` (installs the desktop entry first). Returns how many
/// MIME types now open with it.
pub fn associate(kinds: &[Kind]) -> Result<usize, String> {
    install().map_err(|e| e.to_string())?;
    let (path, kf) = mimeapps();
    let id = format!("{APP_ID}.desktop");
    let mut n = 0;
    for kind in kinds {
        for m in mime_types(*kind) {
            kf.set_string("Default Applications", &m, &id);
            let added = kf.string("Added Associations", &m).map(|s| s.to_string()).unwrap_or_default();
            if !added.split(';').any(|a| a == id) {
                kf.set_string("Added Associations", &m, &format!("{id};{added}"));
            }
            n += 1;
        }
    }
    kf.save_to_file(&path).map_err(|e| e.to_string())?;
    Ok(n)
}

/// Undo `associate`: drop this player as the default; the system's choice applies again.
pub fn unassociate() -> usize {
    let (path, kf) = mimeapps();
    let id = format!("{APP_ID}.desktop");
    let mut n = 0;
    for kind in [Kind::Video, Kind::Audio, Kind::Playlist] {
        for m in mime_types(kind) {
            if kf.string("Default Applications", &m).is_ok_and(|v| v == id) {
                let _ = kf.remove_key("Default Applications", &m);
                n += 1;
            }
        }
    }
    let _ = kf.save_to_file(&path);
    n
}

/// Play > After Playback system actions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SystemAction {
    MonitorOff,
    Standby,
    Hibernate,
    Shutdown,
    LogOff,
    Lock,
}

/// Ask systemd-logind (and GNOME, for log off and screen blanking) over D-Bus.
pub fn system_action(action: SystemAction) {
    let call = |bus: gio::BusType, dest: &str, path: &str, iface: &str, method: &str, args: Option<glib::Variant>| {
        let r = gio::bus_get_sync(bus, gio::Cancellable::NONE)
            .and_then(|c| c.call_sync(Some(dest), path, iface, method, args.as_ref(), None, gio::DBusCallFlags::NONE, 10_000, gio::Cancellable::NONE));
        if let Err(e) = &r {
            eprintln!("{method}: {e}");
        }
        r.is_ok()
    };
    let login = |method: &str, args: glib::Variant| call(gio::BusType::System, "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager", method, Some(args));
    let session = std::env::var("XDG_SESSION_ID").unwrap_or_default();
    match action {
        SystemAction::Standby => {
            login("Suspend", (false,).to_variant());
        }
        SystemAction::Hibernate => {
            login("Hibernate", (false,).to_variant());
        }
        SystemAction::Shutdown => {
            login("PowerOff", (false,).to_variant());
        }
        SystemAction::Lock => {
            login("LockSession", (session,).to_variant());
        }
        SystemAction::LogOff => {
            // GNOME's session manager logs out cleanly; otherwise end the logind session.
            if !call(gio::BusType::Session, "org.gnome.SessionManager", "/org/gnome/SessionManager", "org.gnome.SessionManager", "Logout", Some((1u32,).to_variant())) {
                login("TerminateSession", (session,).to_variant());
            }
        }
        SystemAction::MonitorOff => {
            // The screensaver blanks the screen (and locks it if the desktop is set up to).
            call(gio::BusType::Session, "org.gnome.ScreenSaver", "/org/gnome/ScreenSaver", "org.gnome.ScreenSaver", "SetActive", Some((true,).to_variant()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_lists() {
        let v = mime_types(Kind::Video);
        assert!(v.contains(&"video/x-matroska".to_string()) && v.contains(&"video/mp4".to_string()), "{v:?}");
        assert!(mime_types(Kind::Audio).iter().all(|m| m.starts_with("audio/")));
        assert!(!mime_types(Kind::Playlist).is_empty());
        assert!(desktop_entry().contains("MimeType=") && desktop_entry().contains(&format!("Icon={APP_ID}")));
    }
}
