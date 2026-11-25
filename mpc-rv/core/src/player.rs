//! Talking to mpv: starting it with MPC's defaults, small property/command helpers, and the settings
//! turned into mpv options. Shared by every frontend; drawing the video stays in the frontend.
//!
//! The `*_options` functions only build (option, value) lists from the settings, so they can be
//! tested without mpv; `apply` sends such a list to mpv.

use crate::config::{Config, VideoFrame};
use libmpv2::{GetData, Mpv, SetData};

/// Start mpv with MPC's defaults. `vo` is the video output: `libmpv` when the frontend draws through
/// mpv's render API (GTK), or e.g. `gpu-next` when mpv draws into a window of its own (`wid`).
pub fn create(vo: &str) -> Result<Mpv, libmpv2::Error> {
    let mpv = Mpv::with_initializer(|init| {
        init.set_property("vo", vo)?;
        init.set_property("idle", "yes")?;
        // MPC "after playback: do nothing" = stay on the last frame.
        init.set_property("keep-open", "yes")?;
        init.set_property("volume-max", "100")?;
        // Room for MPC's volume boost (up to +300% = +12.04 dB).
        init.set_property("volume-gain-max", "13")?;
        // Auto-loaded subtitles are also looked for in these subfolders (MPC's default paths).
        init.set_property("sub-file-paths", "subtitles:subs")?;
        init.set_property("terminal", "yes")?;
        // Debug knob: MPC_RS_MPV_LOG=all=v (any mpv --msg-level value).
        init.set_property("msg-level", std::env::var("MPC_RS_MPV_LOG").unwrap_or("all=warn".into()))?;
        // ponytail: WSLg's llvmpipe renders black with mpv's default rgba16f FBO. Real GPUs keep the default.
        if std::env::var_os("WSL_DISTRO_NAME").is_some() {
            init.set_property("fbo-format", "rgba8")?;
        }
        Ok(())
    })?;
    // Errors arrive as events too, so the shader editor can show compile errors.
    unsafe extern "C" {
        fn mpv_request_log_messages(ctx: *mut std::ffi::c_void, min_level: *const std::ffi::c_char) -> std::ffi::c_int;
    }
    // SAFETY: the handle is valid; it lives as long as `mpv`.
    unsafe { mpv_request_log_messages(mpv.ctx.as_ptr().cast(), c"error".as_ptr()) };
    Ok(mpv)
}

/// A property's value, or `None` if mpv doesn't have it (yet).
pub fn prop<T: GetData>(mpv: &Mpv, name: &str) -> Option<T> {
    mpv.get_property(name).ok()
}

/// Set a property; a failure is printed, not returned (there's nothing a caller could do).
pub fn set<T: SetData + std::fmt::Display + Copy>(mpv: &Mpv, name: &str, value: T) {
    if let Err(e) = mpv.set_property(name, value) {
        eprintln!("mpv {name}={value}: {e}");
    }
}

/// Run an mpv command; a failure is printed.
pub fn cmd(mpv: &Mpv, name: &str, args: &[&str]) {
    if let Err(e) = mpv.command(name, args) {
        eprintln!("mpv {name} {args:?}: {e}");
    }
}

/// mpv's OSD line, as MPC shows rate and delay changes on its OSD.
pub fn osd(mpv: &Mpv, text: &str) {
    cmd(mpv, "show-text", &[text, "1500"]);
}

/// Send a list of (option, value) pairs to mpv.
pub fn apply(mpv: &Mpv, options: &[(&str, String)]) {
    for (name, value) in options {
        set(mpv, name, value.as_str());
    }
}

/// Seconds as "hh:mm:ss".
pub fn hms(t: f64) -> String {
    let s = t.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// Hex "#RRGGBBAA" (our config, like CSS) to mpv's "#AARRGGBB".
pub fn mpv_color(c: &str) -> String {
    let h = c.trim_start_matches('#');
    if h.len() == 8 { format!("#{}{}", &h[6..], &h[..6]) } else { format!("#{h}") }
}

fn yes(b: bool) -> String {
    if b { "yes" } else { "no" }.into()
}

/// Options > Playback and Options > Subtitles' loading choices, the color controls and the audio
/// time shift.
pub fn playback_options(c: &Config) -> Vec<(&'static str, String)> {
    let auto = |on: bool| if on { "fuzzy" } else { "no" }.to_string();
    let langs = |s: &str| s.split([',', ' ', ';']).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(",");
    vec![
        ("sub-auto", auto(c.autoload_subtitles)),
        ("audio-file-auto", auto(c.autoload_audio)),
        ("slang", langs(&c.subtitle_langs)),
        ("alang", langs(&c.audio_langs)),
        // MPC's color controls are -100..100, hue -180..180; mpv's are all -100..100.
        ("brightness", c.brightness.to_string()),
        ("contrast", c.contrast.to_string()),
        ("hue", (c.hue * 100 / 180).to_string()),
        ("saturation", c.saturation.to_string()),
        ("audio-delay", (c.audio_time_shift_ms as f64 / 1000.0).to_string()),
    ]
}

/// The audio filter chain and output options from Options > Audio.
pub fn audio_options(c: &Config) -> Vec<(&'static str, String)> {
    let mut af = Vec::new();
    if c.audio_normalize {
        af.push(format!("lavfi=[dynaudnorm=m={:.2}]", (c.audio_max_norm_factor as f64 / 100.0).clamp(1.0, 100.0)));
    }
    if c.crossfeed {
        af.push("lavfi=[bs2b]".to_string());
    }
    let gain = 20.0 * (1.0 + c.audio_boost as f64 / 100.0).log10();
    vec![
        ("af", af.join(",")),
        ("volume-gain", format!("{gain:.2}")),
        ("audio-channels", if c.downmix_stereo { "stereo" } else { "auto-safe" }.into()),
        ("audio-samplerate", if c.downsample_44100 { "44100" } else { "0" }.into()),
        ("audio-pitch-correction", yes(c.pitch_correction)),
    ]
}

/// Options > Subtitles and Subtitle style.
pub fn sub_style_options(c: &Config) -> Vec<(&'static str, String)> {
    vec![
        ("sub-font", c.sub_font.clone()),
        ("sub-font-size", c.sub_font_size.to_string()),
        ("sub-bold", yes(c.sub_bold)),
        ("sub-italic", yes(c.sub_italic)),
        ("sub-color", mpv_color(&c.sub_color)),
        ("sub-outline-color", mpv_color(&c.sub_border_color)),
        ("sub-outline-size", c.sub_border_size.to_string()),
        ("sub-shadow-offset", c.sub_shadow_offset.to_string()),
        ("sub-shadow-color", mpv_color(&c.sub_shadow_color)),
        ("sub-border-style", if c.sub_opaque_box { "opaque-box" } else { "outline-and-shadow" }.into()),
        ("sub-back-color", mpv_color(&c.sub_back_color)),
        ("sub-margin-y", c.sub_margin_y.to_string()),
        ("sub-ass-override", if c.sub_override_ass { "force" } else { "scale" }.into()),
        ("sub-pos", if c.sub_override_placement { c.sub_pos_v.min(150) } else { 100 }.to_string()),
    ]
}

/// Options > Output and Sync, and the OSD. `refresh` is the display's refresh rate in Hz (0 if
/// unknown): display-resample needs it, and the render API can't see it by itself.
pub fn video_options(c: &Config, refresh: f64) -> Vec<(&'static str, String)> {
    vec![
        ("hwdec", if c.hwdec { "auto-safe" } else { "no" }.into()),
        ("icc-profile-auto", yes(c.color_management)),
        ("icc-intent", c.icc_intent.to_string()),
        ("video-output-levels", if c.output_limited { "limited" } else { "full" }.into()),
        ("video-sync", if c.sync_to_display { "display-resample" } else { "audio" }.into()),
        ("interpolation", yes(c.interpolation && c.sync_to_display)),
        ("display-fps-override", format!("{refresh:.3}")),
        ("osd-level", if c.show_osd { "1" } else { "0" }.into()),
        ("osd-font", c.osd_font.clone()),
        ("osd-font-size", c.osd_font_size.to_string()),
    ]
}

/// View > Video Frame (MPC's OnViewDefaultVideoFrame), as mpv's scaling options.
pub fn frame_options(frame: VideoFrame) -> Vec<(&'static str, String)> {
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
    vec![("video-unscaled", unscaled.into()), ("video-zoom", zoom.to_string()), ("panscan", panscan.to_string()), ("keepaspect", keepaspect.into())]
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

    pub fn options(&self) -> Vec<(&'static str, String)> {
        vec![
            ("video-scale-x", self.zoom_x.to_string()),
            ("video-scale-y", self.zoom_y.to_string()),
            ("video-pan-x", (self.pos_x - 0.5).to_string()),
            ("video-pan-y", (self.pos_y - 0.5).to_string()),
        ]
    }
}

/// MPC's next/previous audio or subtitle stream: steps through the tracks of that type, wrapping around
/// (unlike mpv's `cycle`, never to "no track"; subtitles are turned off with On/Off instead).
pub fn cycle_track(mpv: &Mpv, kind: &str, property: &str, dir: i64) {
    let n = prop::<i64>(mpv, "track-list/count").unwrap_or(0);
    let ids: Vec<i64> = (0..n)
        .filter(|i| prop::<String>(mpv, &format!("track-list/{i}/type")).as_deref() == Some(kind))
        .filter_map(|i| prop::<i64>(mpv, &format!("track-list/{i}/id")))
        .collect();
    if ids.is_empty() {
        return;
    }
    let cur = prop::<String>(mpv, property).and_then(|s| s.parse::<i64>().ok());
    let pos = cur.and_then(|c| ids.iter().position(|&i| i == c)).map_or(0, |p| (p as i64 + dir).rem_euclid(ids.len() as i64) as usize);
    let _ = mpv.set_property(property, ids[pos]);
    if let Some(title) = prop::<String>(mpv, &format!("current-tracks/{kind}/title")).or_else(|| prop::<String>(mpv, &format!("current-tracks/{kind}/lang"))) {
        osd(mpv, &format!("{}: {title}", if kind == "audio" { "Audio" } else { "Subtitle" }));
    }
}
