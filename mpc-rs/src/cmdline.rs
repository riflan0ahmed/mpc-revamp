//! MPC's command line switches (`CAppSettings::ParseCommandLine`). Switches start with `/`, `-` or `--`.
//! On Linux a leading `/` is also an absolute path, so `/name` only counts as a switch when `name` is a
//! known switch and no such file exists.

use std::path::Path;

pub const USAGE: &str = "\
Usage: mpc-rs \"pathname\" [switches]

\"pathname\"\tThe main file or directory to be loaded
/dub \"dubname\"\tLoad an additional audio file
/sub \"subname\"\tLoad an additional subtitle file
/open\t\tOpen the file, don't automatically start playback
/play\t\tStart playing the file as soon the player is launched
/close\t\tClose the player after playback (only works when used with /play)
/playnext\tOpen next file in the folder after playback
/mute\t\tMute the audio
/volume N\tSet Volume, where N is a range from 0 to 100
/fullscreen\tStart in fullscreen mode
/viewpreset N\tStart with specific preset, where N is either \"1\" Minimal, \"2\" Compact or \"3\" Normal
/minimized\tStart in minimized mode
/new\t\tUse a new instance of the player
/add\t\tAdd \"pathname\" to playlist, can be combined with /open and /play
/randomize\tRandomize the playlist
/start ms\tStart playing at \"ms\" (= milliseconds)
/startpos hh:mm:ss\tStart playing at position hh:mm:ss
/fixedsize w,h\tSet a fixed window size
/regvid\t\tCreate file associations for video files
/regaud\t\tCreate file associations for audio files
/regpl\t\tCreate file associations for playlist files
/regall\t\tCreate file associations for all supported file types
/unregall\tRemove all file associations
/nofocus\tOpen in background
/reset\t\tRestore default settings
/help /h /?\tShow help about command line switches";

/// Switches MPC has that need Windows or a later phase: accepted (with their argument) and ignored.
const IGNORED: &[(&str, bool)] = &[
    ("dubdelay", true), ("d3dfs", false), ("filter", true), ("dvd", false), ("dvdpos", true), ("cd", false),
    ("device", false), ("shutdown", false), ("standby", false), ("hibernate", false), ("logoff", false),
    ("lock", false), ("monitoroff", false), ("monitor", true), ("audiorenderer", true), ("shaderpreset", true),
    ("pns", true), ("iconsassoc", false), ("webport", true), ("debug", false), ("nocrashreporter", false),
    ("slave", true), ("hwgpu", true),
];
const FLAGS: &[&str] = &[
    "open", "play", "close", "playnext", "mute", "fullscreen", "minimized", "new", "add", "randomize", "nofocus",
    "reset", "help", "h", "?", "regvid", "regaud", "regpl", "regall", "unregall",
];
const WITH_ARG: &[&str] = &["dub", "sub", "volume", "viewpreset", "start", "startpos", "fixedsize"];

#[derive(Default, Debug, PartialEq)]
pub struct CmdLine {
    pub files: Vec<String>,
    pub subs: Vec<String>,
    pub dubs: Vec<String>,
    pub open: bool,
    pub play: bool,
    pub close: bool,
    pub playnext: bool,
    pub mute: bool,
    pub volume: Option<u32>,
    pub fullscreen: bool,
    pub viewpreset: Option<u8>,
    pub minimized: bool,
    pub new: bool,
    pub add: bool,
    pub randomize: bool,
    /// Start position in seconds (/start or /startpos).
    pub start: Option<f64>,
    pub fixedsize: Option<(i32, i32)>,
    pub nofocus: bool,
    pub reset: bool,
    pub help: bool,
    /// /regvid /regaud /regpl (/regall = all three), /unregall.
    pub register: (bool, bool, bool),
    pub unregister: bool,
    /// Switches that were recognized but don't do anything here.
    pub ignored: Vec<String>,
}

/// The switch name in `arg`, if it is one.
fn switch_name(arg: &str) -> Option<String> {
    let name = if let Some(n) = arg.strip_prefix("--") {
        n
    } else if let Some(n) = arg.strip_prefix('-') {
        n
    } else {
        let n = arg.strip_prefix('/')?;
        let known = FLAGS.contains(&n.to_ascii_lowercase().as_str())
            || WITH_ARG.contains(&n.to_ascii_lowercase().as_str())
            || IGNORED.iter().any(|(s, _)| s.eq_ignore_ascii_case(n));
        if !known || Path::new(arg).exists() {
            return None;
        }
        n
    };
    (!name.is_empty()).then(|| name.to_ascii_lowercase())
}

fn hms(s: &str) -> Option<f64> {
    s.split(':').try_fold(0.0, |acc, part| part.trim().parse::<f64>().ok().map(|v| acc * 60.0 + v))
}

/// Parse the arguments after the program name. Relative paths are made absolute against `cwd`.
pub fn parse(args: &[String], cwd: &Path) -> Result<CmdLine, String> {
    let mut c = CmdLine::default();
    // Relative paths are made absolute (and tidied, e.g. "a/../b", when the file exists).
    let abs = |p: &str| {
        if p.contains("://") || Path::new(p).is_absolute() {
            return p.to_string();
        }
        let full = cwd.join(p);
        full.canonicalize().unwrap_or(full).to_string_lossy().into_owned()
    };
    let mut it = args.iter();
    let mut unknown = Vec::new();
    while let Some(arg) = it.next() {
        let Some(name) = switch_name(arg) else {
            c.files.push(abs(arg));
            continue;
        };
        let mut value = |what: &str| it.next().cloned().ok_or(format!("/{name} needs {what}"));
        match name.as_str() {
            "open" => c.open = true,
            "play" => c.play = true,
            "close" => c.close = true,
            "playnext" => c.playnext = true,
            "mute" => c.mute = true,
            "fullscreen" => c.fullscreen = true,
            "minimized" => c.minimized = true,
            "new" => c.new = true,
            "add" => c.add = true,
            "randomize" => c.randomize = true,
            "nofocus" => c.nofocus = true,
            "reset" => c.reset = true,
            "help" | "h" | "?" => c.help = true,
            "regvid" => c.register.0 = true,
            "regaud" => c.register.1 = true,
            "regpl" => c.register.2 = true,
            "regall" => c.register = (true, true, true),
            "unregall" => c.unregister = true,
            "dub" => c.dubs.push(abs(&value("a file")?)),
            "sub" => c.subs.push(abs(&value("a file")?)),
            "volume" => c.volume = Some(value("a number")?.parse::<u32>().map_err(|e| e.to_string())?.min(100)),
            "viewpreset" => c.viewpreset = value("1, 2 or 3")?.parse().ok().filter(|n| (1..=3).contains(n)),
            "start" => c.start = Some(value("milliseconds")?.parse::<f64>().map_err(|e| e.to_string())? / 1000.0),
            "startpos" => c.start = Some(hms(&value("hh:mm:ss")?).ok_or("/startpos needs hh:mm:ss")?),
            "fixedsize" => {
                let v = value("w,h")?;
                let (w, h) = v.split_once(',').ok_or("/fixedsize needs w,h")?;
                c.fixedsize = Some((w.trim().parse().map_err(|_| "bad width")?, h.trim().parse().map_err(|_| "bad height")?));
            }
            _ => match IGNORED.iter().find(|(s, _)| *s == name) {
                Some((s, takes_arg)) => {
                    if *takes_arg {
                        it.next();
                    }
                    c.ignored.push(format!("/{s}"));
                }
                None => unknown.push(arg.clone()),
            },
        }
    }
    if unknown.is_empty() { Ok(c) } else { Err(format!("Unrecognized switch(es) found in command line string:\n\n{}", unknown.join(" "))) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<CmdLine, String> {
        parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>(), Path::new("/cwd"))
    }

    #[test]
    fn switches() {
        let c = p(&["a.mkv", "/play", "-fullscreen", "--sub", "s.srt", "/START", "1500", "/viewpreset", "2", "/home/x.mkv"]).unwrap();
        assert_eq!(c.files, ["/cwd/a.mkv", "/home/x.mkv"]);
        assert!(c.play && c.fullscreen);
        assert_eq!(c.subs, ["/cwd/s.srt"]);
        assert_eq!(c.start, Some(1.5));
        assert_eq!(c.viewpreset, Some(2));
        assert_eq!(p(&["/startpos", "1:02:03"]).unwrap().start, Some(3723.0));
        assert_eq!(p(&["/fixedsize", "640, 480"]).unwrap().fixedsize, Some((640, 480)));
        assert_eq!(p(&["/volume", "250"]).unwrap().volume, Some(100));
        assert_eq!(p(&["/dvd", "/monitor", "2"]).unwrap().ignored, ["/dvd", "/monitor"]);
        assert_eq!(p(&["/regall", "/webport", "8080"]).unwrap().register, (true, true, true));
        assert_eq!(p(&["/webport", "8080"]).unwrap().ignored, ["/webport"]);
        assert!(p(&["/frobnicate"]).unwrap().files == ["/frobnicate"], "unknown /name is a path");
        assert!(p(&["-frobnicate"]).is_err());
        assert!(p(&["/sub"]).is_err());
        assert!(p(&["https://x/y.mp4"]).unwrap().files == ["https://x/y.mp4"]);
    }
}
