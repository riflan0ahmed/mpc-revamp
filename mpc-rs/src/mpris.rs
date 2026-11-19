//! MPRIS D-Bus service: media keys, desktop media widgets and tools like playerctl can see and control
//! the player (play state, metadata, position and seeking, volume, rate, loop and shuffle, open URI).

use crate::commands::Media;
use gtk4::{self as gtk, gio, glib, prelude::*};
use libmpv2::Mpv;
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
    <property name="DesktopEntry" type="s" access="read"/>
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
    <method name="Seek"><arg direction="in" name="Offset" type="x"/></method>
    <method name="SetPosition"><arg direction="in" name="TrackId" type="o"/><arg direction="in" name="Position" type="x"/></method>
    <method name="OpenUri"><arg direction="in" name="Uri" type="s"/></method>
    <signal name="Seeked"><arg name="Position" type="x"/></signal>
    <property name="PlaybackStatus" type="s" access="read"/>
    <property name="LoopStatus" type="s" access="readwrite"/>
    <property name="Rate" type="d" access="readwrite"/>
    <property name="Shuffle" type="b" access="readwrite"/>
    <property name="Metadata" type="a{sv}" access="read"/>
    <property name="Volume" type="d" access="readwrite"/>
    <property name="Position" type="x" access="read"/>
    <property name="MinimumRate" type="d" access="read"/>
    <property name="MaximumRate" type="d" access="read"/>
    <property name="CanGoNext" type="b" access="read"/>
    <property name="CanGoPrevious" type="b" access="read"/>
    <property name="CanPlay" type="b" access="read"/>
    <property name="CanPause" type="b" access="read"/>
    <property name="CanSeek" type="b" access="read"/>
    <property name="CanControl" type="b" access="read"/>
  </interface>
</node>"#;

/// What MPRIS asks of the player that isn't plain mpv state.
pub enum Request {
    Media(Media),
    OpenUri(String),
    /// "None", "Track" or "Playlist".
    SetLoop(String),
    SetShuffle(bool),
}

/// The player's own state that MPRIS reports (loop mode and shuffle live in its settings).
pub struct PlayerState {
    pub loop_status: &'static str,
    pub shuffle: bool,
}

pub struct Mpris {
    conn: Option<gio::DBusConnection>,
    status: Rc<RefCell<&'static str>>,
}

fn prop<T: libmpv2::GetData>(mpv: &Mpv, name: &str) -> Option<T> {
    mpv.get_property(name).ok()
}

/// MPRIS metadata for the playing file.
fn metadata(mpv: &Mpv) -> HashMap<String, glib::Variant> {
    let mut m = HashMap::new();
    let Some(path) = prop::<String>(mpv, "path") else {
        m.insert("mpris:trackid".into(), glib::Variant::from(glib::variant::ObjectPath::try_from("/org/mpris/MediaPlayer2/TrackList/NoTrack").unwrap()));
        return m;
    };
    m.insert("mpris:trackid".into(), glib::Variant::from(glib::variant::ObjectPath::try_from("/io/github/mpcrs/Track/current").unwrap()));
    if let Some(d) = prop::<f64>(mpv, "duration") {
        m.insert("mpris:length".into(), ((d * 1e6) as i64).to_variant());
    }
    let url = if path.contains("://") { path.clone() } else { gio::File::for_path(&path).uri().to_string() };
    m.insert("xesam:url".into(), url.to_variant());
    m.insert("xesam:title".into(), prop::<String>(mpv, "media-title").unwrap_or(path).to_variant());
    for (key, tag) in [("xesam:artist", "artist"), ("xesam:album", "album")] {
        if let Some(v) = prop::<String>(mpv, &format!("metadata/by-key/{tag}")) {
            let v = if key == "xesam:artist" { vec![v].to_variant() } else { v.to_variant() };
            m.insert(key.into(), v);
        }
    }
    m
}

impl Mpris {
    /// Register on the app's session bus connection. `on` handles requests that need the player;
    /// `state` reports its loop/shuffle settings. Raise/Quit act on `win` directly.
    pub fn start(
        app: &gtk::Application,
        win: &gtk::ApplicationWindow,
        mpv: &'static Mpv,
        on: impl Fn(Request) + 'static,
        state: impl Fn() -> PlayerState + 'static,
    ) -> Mpris {
        let status = Rc::new(RefCell::new("Stopped"));
        let Some(conn) = app.dbus_connection() else {
            return Mpris { conn: None, status };
        };
        let node = gio::DBusNodeInfo::for_xml(XML).expect("valid MPRIS introspection XML");
        let on = Rc::new(on);
        let state = Rc::new(state);
        for iface in node.interfaces() {
            let (win, on, current) = (win.clone(), on.clone(), status.clone());
            let (on2, state) = (on.clone(), state.clone());
            let reg = conn
                .register_object(PATH, iface)
                .method_call(move |conn, _, _, _, method, params, inv| {
                    match method {
                        "Raise" => win.present(),
                        "Quit" => win.close(),
                        "Next" => on(Request::Media(Media::Next)),
                        "Previous" => on(Request::Media(Media::Previous)),
                        "Pause" => on(Request::Media(Media::Pause)),
                        "PlayPause" => on(Request::Media(Media::PlayPause)),
                        "Stop" => on(Request::Media(Media::Stop)),
                        "Play" => on(Request::Media(Media::Play)),
                        "Seek" => {
                            if let Some((off,)) = params.get::<(i64,)>() {
                                let _ = mpv.command("seek", &[&(off as f64 / 1e6).to_string(), "relative+exact"]);
                            }
                        }
                        "SetPosition" => {
                            if let Some((_, pos)) = params.get::<(glib::variant::ObjectPath, i64)>()
                                && pos >= 0
                            {
                                let _ = mpv.command("seek", &[&(pos as f64 / 1e6).to_string(), "absolute+exact"]);
                            }
                        }
                        "OpenUri" => {
                            if let Some((uri,)) = params.get::<(String,)>() {
                                on(Request::OpenUri(uri));
                            }
                        }
                        _ => {}
                    }
                    let _ = conn;
                    inv.return_value(None);
                })
                .property(move |_, _, _, _, p| match p {
                    "Identity" => "mpc-rs".to_variant(),
                    "DesktopEntry" => "io.github.mpcrs.Player".to_variant(),
                    "SupportedUriSchemes" => vec!["file", "http", "https", "dvd", "bd", "cdda"].to_variant(),
                    "SupportedMimeTypes" => Vec::<String>::new().to_variant(),
                    "PlaybackStatus" => current.borrow().to_variant(),
                    "LoopStatus" => state().loop_status.to_variant(),
                    "Shuffle" => state().shuffle.to_variant(),
                    "Rate" => prop::<f64>(mpv, "speed").unwrap_or(1.0).to_variant(),
                    "MinimumRate" => 0.125f64.to_variant(),
                    "MaximumRate" => 16.0f64.to_variant(),
                    "Volume" => (prop::<f64>(mpv, "volume").unwrap_or(100.0) / 100.0).to_variant(),
                    "Position" => ((prop::<f64>(mpv, "time-pos").unwrap_or(0.0) * 1e6) as i64).to_variant(),
                    "Metadata" => metadata(mpv).to_variant(),
                    "HasTrackList" => false.to_variant(),
                    "CanSeek" => prop::<bool>(mpv, "seekable").unwrap_or(false).to_variant(),
                    _ => true.to_variant(),
                })
                .set_property(move |_, _, _, _, p, v| {
                    match p {
                        "Volume" => v.get::<f64>().is_some_and(|x| mpv.set_property("volume", (x.clamp(0.0, 1.0) * 100.0).round()).is_ok()),
                        "Rate" => v.get::<f64>().is_some_and(|x| x > 0.0 && mpv.set_property("speed", x.clamp(0.125, 16.0)).is_ok()),
                        "LoopStatus" => v.get::<String>().map(|s| on2(Request::SetLoop(s))).is_some(),
                        "Shuffle" => v.get::<bool>().map(|b| on2(Request::SetShuffle(b))).is_some(),
                        _ => false,
                    }
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

    fn changed(&self, props: HashMap<String, glib::Variant>) {
        if let Some(conn) = &self.conn {
            let args = (PLAYER, props, Vec::<String>::new()).to_variant();
            if let Err(e) = conn.emit_signal(None, PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged", Some(&args)) {
                eprintln!("MPRIS: {e}");
            }
        }
    }

    /// "Playing", "Paused" or "Stopped"; tells listeners when it changes.
    pub fn set_status(&self, s: &'static str) {
        if *self.status.borrow() != s {
            *self.status.borrow_mut() = s;
            self.changed(HashMap::from([("PlaybackStatus".to_string(), s.to_variant())]));
        }
    }

    /// A new file is playing (or the volume/rate/loop/shuffle changed): send the current values.
    pub fn refresh(&self, mpv: &Mpv, state: PlayerState) {
        self.changed(HashMap::from([
            ("Metadata".to_string(), metadata(mpv).to_variant()),
            ("Volume".to_string(), (prop::<f64>(mpv, "volume").unwrap_or(100.0) / 100.0).to_variant()),
            ("Rate".to_string(), prop::<f64>(mpv, "speed").unwrap_or(1.0).to_variant()),
            ("LoopStatus".to_string(), state.loop_status.to_variant()),
            ("Shuffle".to_string(), state.shuffle.to_variant()),
            ("CanSeek".to_string(), prop::<bool>(mpv, "seekable").unwrap_or(false).to_variant()),
        ]));
    }

    /// After a jump (not normal playback), MPRIS clients expect a Seeked signal.
    pub fn seeked(&self, mpv: &Mpv) {
        if let Some(conn) = &self.conn {
            let pos = (prop::<f64>(mpv, "time-pos").unwrap_or(0.0) * 1e6) as i64;
            let _ = conn.emit_signal(None, PATH, PLAYER, "Seeked", Some(&(pos,).to_variant()));
        }
    }
}
