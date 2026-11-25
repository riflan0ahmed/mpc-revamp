//! The settings as mpv options, and pan & scan steps (MPC's sizes and moves). None of these need a
//! running mpv: the option lists are checked as data.

use mpc_core::{
    config::{Config, VideoFrame},
    player::{self, PanScan},
};

/// The value an option list gives `name`.
fn get<'a>(options: &'a [(&str, String)], name: &str) -> &'a str {
    options.iter().find(|(n, _)| *n == name).map(|(_, v)| v.as_str()).unwrap_or_else(|| panic!("no option {name}"))
}

#[test]
fn pan_scan_steps() {
    let mut p = PanScan::default();
    p.step(1, 1, 0, 0);
    assert!((p.zoom_x - 1.02).abs() < 1e-9 && (p.zoom_y - 1.02).abs() < 1e-9);
    for _ in 0..500 {
        p.step(1, 0, 1, 0);
    }
    assert!(p.zoom_x >= 3.0 && p.zoom_x < 3.0 * 1.02 && p.pos_x == 1.0); // stops growing once past 3
    p.step(0, -1, -1, -1);
    assert!(p.zoom_y < 1.02 && p.pos_x < 1.0 && p.pos_y < 0.5);
    // Centered = no pan.
    let o = PanScan::default().options();
    assert_eq!((get(&o, "video-pan-x"), get(&o, "video-scale-x")), ("0", "1"));
}

#[test]
fn helpers() {
    assert_eq!(player::hms(3725.9), "01:02:05");
    assert_eq!(player::hms(-3.0), "00:00:00");
    // Our "#RRGGBBAA" to mpv's "#AARRGGBB".
    assert_eq!(player::mpv_color("#11223380"), "#80112233");
    assert_eq!(player::mpv_color("#112233"), "#112233");
}

#[test]
fn audio() {
    let mut c = Config::default();
    let o = player::audio_options(&c);
    assert_eq!((get(&o, "af"), get(&o, "volume-gain")), ("", "0.00"));
    c.audio_boost = 100; // +100% = twice the amplitude = +6.02 dB
    c.audio_normalize = true;
    c.audio_max_norm_factor = 400;
    c.crossfeed = true;
    let o = player::audio_options(&c);
    assert_eq!(get(&o, "volume-gain"), "6.02");
    assert_eq!(get(&o, "af"), "lavfi=[dynaudnorm=m=4.00],lavfi=[bs2b]");
}

#[test]
fn subtitles() {
    let mut c = Config::default();
    let o = player::sub_style_options(&c);
    assert_eq!(get(&o, "sub-pos"), "100"); // placement not overridden
    assert_eq!(get(&o, "sub-color"), "#ffffffff");
    c.sub_override_placement = true;
    c.sub_pos_v = 200;
    assert_eq!(get(&player::sub_style_options(&c), "sub-pos"), "150"); // mpv's limit
}

#[test]
fn video() {
    let mut c = Config { interpolation: true, sync_to_display: false, ..Default::default() };
    let o = player::video_options(&c, 59.94);
    assert_eq!(get(&o, "interpolation"), "no"); // needs display sync
    assert_eq!(get(&o, "display-fps-override"), "59.940");
    c.sync_to_display = true;
    let o = player::video_options(&c, 0.0);
    assert_eq!((get(&o, "interpolation"), get(&o, "video-sync")), ("yes", "display-resample"));

    let o = player::frame_options(VideoFrame::Stretch);
    assert_eq!((get(&o, "video-unscaled"), get(&o, "keepaspect")), ("no", "no"));
    let o = player::frame_options(VideoFrame::Half);
    assert_eq!((get(&o, "video-unscaled"), get(&o, "video-zoom")), ("yes", "-1"));
}

#[test]
fn playback() {
    let c = Config { subtitle_langs: "en, de;fr".into(), hue: 90, audio_time_shift_ms: -250, ..Default::default() };
    let o = player::playback_options(&c);
    assert_eq!(get(&o, "slang"), "en,de,fr");
    assert_eq!(get(&o, "hue"), "50"); // MPC's -180..180 to mpv's -100..100
    assert_eq!(get(&o, "audio-delay"), "-0.25");
}
