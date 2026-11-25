//! MPC's command line switches.

use mpc_core::cmdline::{CmdLine, parse};
use std::path::Path;

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
