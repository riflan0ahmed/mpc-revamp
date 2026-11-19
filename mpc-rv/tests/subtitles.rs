//! Subtitle files read from disk, resynced and written back (subresync bar, File > Save Subtitle).

use mpc_rv::subtitles::{self, Kind};
use std::path::PathBuf;

fn temp_dir(test: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mpc-rv-{test}-{}", std::process::id()))
}

/// A file in a temporary folder for this test (removed with `cleanup`).
fn temp_file(test: &str, name: &str, text: &str) -> String {
    let dir = temp_dir(test);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

const SRT: &str = "\u{feff}1\r\n00:00:01,000 --> 00:00:02,000\r\nOne\r\n\r\n2\r\n00:00:10,000 --> 00:00:11,500\r\nTwo\r\nlines\r\n";

#[test]
fn load_by_extension() {
    let srt = temp_file("load", "a.srt", SRT);
    let doc = subtitles::load(&srt).expect("SRT with a BOM and CRLF line ends");
    assert_eq!(doc.kind, Kind::Srt);
    assert_eq!(doc.lines.len(), 2);
    assert_eq!(doc.lines[1].text, "Two\nlines");
    let vtt = temp_file("load", "a.vtt", "WEBVTT\n\n00:01.000 --> 00:02.000\nHi\n");
    assert_eq!(subtitles::load(&vtt).map(|d| d.kind), Some(Kind::Vtt));
    assert!(subtitles::load(&temp_file("load", "a.sup", "binary")).is_none(), "image subtitles can't be resynced");
    assert!(subtitles::load(&temp_file("load", "empty.srt", "")).is_none());
    let _ = std::fs::remove_dir_all(temp_dir("load"));
}

#[test]
fn resync_and_save() {
    let path = temp_file("resync", "film.srt", SRT);
    let mut doc = subtitles::load(&path).unwrap();
    doc.set_start(0, 3.0); // the first line should start 2 s later; everything moves with it
    assert_eq!(doc.start(1), 12.0);
    assert_eq!(doc.line_at(12.5), Some(1));
    // Saved with a 0.5 s subtitle delay on top, then read back.
    let saved = temp_file("resync", "film.fixed.srt", &doc.to_text(0.5));
    let back = subtitles::load(&saved).unwrap();
    assert_eq!((back.lines[0].orig_start, back.lines[1].orig_end), (3.5, 14.0));
    assert_eq!(back.lines[1].text, "Two\nlines");
    let _ = std::fs::remove_dir_all(temp_dir("resync"));
}

#[test]
fn ass_keeps_styles() {
    let ass = "[Script Info]\nTitle: t\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize\nStyle: Big,Arial,40\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:05.00,0:00:06.00,Big,,0,0,0,,{\\\\i1}Hello{\\\\i0}, world\n";
    let mut doc = subtitles::load(&temp_file("ass", "a.ass", ass)).unwrap();
    doc.set_start(0, 4.0);
    let out = doc.to_text(0.0);
    assert!(out.contains("Style: Big,Arial,40"), "styles are kept");
    assert!(out.contains("Dialogue: 0,0:00:04.00,0:00:05.00,Big,,0,0,0,,{\\\\i1}Hello{\\\\i0}, world"), "{out}");
    let _ = std::fs::remove_dir_all(temp_dir("ass"));
}
