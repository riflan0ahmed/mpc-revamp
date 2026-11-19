//! MPRIS D-Bus service, so desktop media keys (and media widgets) can control the player.
//! Only what media keys need for now; metadata, seeking and volume come with the web remote phase.

use crate::commands::Media;
use gtk4::{self as gtk, gio, glib, prelude::*};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const XML: &str = r#"<node>
  <interface name="org.mpris.MediaPlayer2">
    <method name="Raise"/>
    <method name="Quit"/>
    <property name="CanQuit" type="b" access="read"/>
    <property name="CanRaise" type="b" access="read"/>
    <property name="HasTrackList" type="b" access="read"/>
    <property name="Identity" type="s" access="read"/>
    <property name="SupportedUriSchemes" type="as" access="read"/>
    <property name="SupportedMimeTypes" type="as" access="read"/>
  </interface>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="Next"/>
    <method name="Previous"/>
    <method name="Pause"/>
    <method name="PlayPause"/>
    <method name="Stop"/>
    <method name="Play"/>
    <property name="PlaybackStatus" type="s" access="read"/>
    <property name="Metadata" type="a{sv}" access="read"/>
    <property name="CanGoNext" type="b" access="read"/>
    <property name="CanGoPrevious" type="b" access="read"/>
    <property name="CanPlay" type="b" access="read"/>
    <property name="CanPause" type="b" access="read"/>
    <property name="CanSeek" type="b" access="read"/>
    <property name="CanControl" type="b" access="read"/>
  </interface>
</node>"#;

pub struct Mpris {
    conn: Option<gio::DBusConnection>,
    status: Rc<RefCell<&'static str>>,
}

impl Mpris {
    /// Register on the app's session bus connection. `on_media` runs the command bound to a media key;
    /// Raise/Quit act on `win` directly.
    pub fn start(app: &gtk::Application, win: &gtk::ApplicationWindow, on_media: impl Fn(Media) + 'static) -> Mpris {
        let status = Rc::new(RefCell::new("Stopped"));
        let Some(conn) = app.dbus_connection() else {
            return Mpris { conn: None, status };
        };
        let node = gio::DBusNodeInfo::for_xml(XML).expect("valid MPRIS introspection XML");
        let on_media = Rc::new(on_media);
        for iface in node.interfaces() {
            let (win, on_media, current) = (win.clone(), on_media.clone(), status.clone());
            let reg = conn
                .register_object(PATH, iface)
                .method_call(move |_, _, _, _, method, _, inv| {
                    match method {
                        "Raise" => win.present(),
                        "Quit" => win.close(),
                        "Next" => on_media(Media::Next),
                        "Previous" => on_media(Media::Previous),
                        "Pause" => on_media(Media::Pause),
                        "PlayPause" => on_media(Media::PlayPause),
                        "Stop" => on_media(Media::Stop),
                        "Play" => on_media(Media::Play),
                        _ => {}
                    }
                    inv.return_value(None);
                })
                .property(move |_, _, _, _, prop| match prop {
                    "Identity" => "mpc-rs".to_variant(),
                    "SupportedUriSchemes" | "SupportedMimeTypes" => Vec::<String>::new().to_variant(),
                    "PlaybackStatus" => current.borrow().to_variant(),
                    "Metadata" => HashMap::<String, glib::Variant>::new().to_variant(),
                    "HasTrackList" | "CanSeek" => false.to_variant(),
                    _ => true.to_variant(),
                })
                .build();
            if let Err(e) = reg {
                eprintln!("MPRIS: {e}");
                return Mpris { conn: None, status };
            }
        }
        gio::bus_own_name_on_connection(&conn, "org.mpris.MediaPlayer2.mpc_rs", gio::BusNameOwnerFlags::NONE, |_, _| {}, |_, name| {
            eprintln!("MPRIS: lost bus name {name}");
        });
        Mpris { conn: Some(conn), status }
    }

    /// "Playing", "Paused" or "Stopped"; tells listeners when it changes.
    pub fn set_status(&self, s: &'static str) {
        if *self.status.borrow() == s {
            return;
        }
        *self.status.borrow_mut() = s;
        if let Some(conn) = &self.conn {
            let changed = HashMap::from([("PlaybackStatus".to_string(), s.to_variant())]);
            let args = (PLAYER, changed, Vec::<String>::new()).to_variant();
            if let Err(e) = conn.emit_signal(None, PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged", Some(&args)) {
                eprintln!("MPRIS: {e}");
            }
        }
    }
}
