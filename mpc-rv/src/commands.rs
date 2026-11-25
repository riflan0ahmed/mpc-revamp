//! MPC-HC's command table (`default_wmcmds` in `AppSettings.cpp`) and the user's bindings to it.
//! Each command can have a key, a mouse action (windowed and fullscreen) and a media key (MPRIS).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Mouse actions on the video, same list as MPC's `wmcmd` enum.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Mouse {
    None,
    LDown,
    LUp,
    LDblClk,
    MDown,
    MUp,
    MDblClk,
    RDown,
    RUp,
    RDblClk,
    X1Down,
    X1Up,
    X1DblClk,
    X2Down,
    X2Up,
    X2DblClk,
    WUp,
    WDown,
}

impl Mouse {
    pub const ALL: [Mouse; 18] = [
        Mouse::None, Mouse::LDown, Mouse::LUp, Mouse::LDblClk, Mouse::MDown, Mouse::MUp, Mouse::MDblClk,
        Mouse::RDown, Mouse::RUp, Mouse::RDblClk, Mouse::X1Down, Mouse::X1Up, Mouse::X1DblClk,
        Mouse::X2Down, Mouse::X2Up, Mouse::X2DblClk, Mouse::WUp, Mouse::WDown,
    ];

    /// Labels from MPC's `MakeMouseButtonLabel`.
    pub fn label(self) -> &'static str {
        [
            "None", "Left Down", "Left Up", "Left DblClk", "Middle Down", "Middle Up", "Middle DblClk",
            "Right Down", "Right Up", "Right DblClk", "X1 Down", "X1 Up", "X1 DblClk",
            "X2 Down", "X2 Up", "X2 DblClk", "Wheel Up", "Wheel Down",
        ][self as usize]
    }

    /// Button press/release for a GDK button number (1 left, 2 middle, 3 right, 8/9 back/forward).
    pub fn button(button: u32, kind: Click) -> Mouse {
        let base = match button {
            1 => 1,
            2 => 4,
            3 => 7,
            8 => 10,
            9 => 13,
            _ => return Mouse::None,
        };
        Mouse::ALL[base + kind as usize]
    }
}

#[derive(Clone, Copy)]
pub enum Click {
    Down,
    Up,
    Double,
}

/// Media keys. MPC's "App Command" column (`APPCOMMAND_MEDIA_*`); on Linux they arrive as MPRIS calls.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Media {
    None,
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Previous,
}

impl Media {
    pub const ALL: [Media; 7] = [Media::None, Media::PlayPause, Media::Play, Media::Pause, Media::Stop, Media::Next, Media::Previous];

    pub fn label(self) -> &'static str {
        ["None", "Play/Pause", "Play", "Pause", "Stop", "Next", "Previous"][self as usize]
    }
}

pub struct Cmd {
    /// MPC's command ID (`resource.h`), also used by the web remote's `wm_command`.
    pub id: u16,
    /// GTK action name, without the `win.` prefix.
    pub action: &'static str,
    pub label: &'static str,
    key: &'static str,
    mouse: Mouse,
    mouse_fs: Mouse,
    media: Media,
}

impl Cmd {
    pub fn default_binding(&self) -> Binding {
        Binding { key: self.key.into(), mouse: self.mouse, mouse_fs: self.mouse_fs, media: self.media }
    }
}

/// What a command is bound to. `key` is a GTK accelerator string ("<Control>q"), empty for none.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Binding {
    pub key: String,
    pub mouse: Mouse,
    pub mouse_fs: Mouse,
    pub media: Media,
}

/// Bindings for every command (same order as `COMMANDS`), defaults overridden by the config's `[keys]`.
/// DVD/Blu-ray commands. Discs are rarely used, so these are hidden: they're not in the menus or
/// Options > Keys, and their keys are free for other commands.
pub fn is_dvd(action: &str) -> bool {
    action == "file-opendvdbd" || action.starts_with("dvd-") || (action.starts_with("navigate-") && action.contains("menu"))
}

pub fn load(overrides: &BTreeMap<String, Binding>) -> Vec<Binding> {
    COMMANDS.iter().map(|c| overrides.get(c.action).cloned().unwrap_or_else(|| c.default_binding())).collect()
}

/// The bindings that differ from MPC's defaults, keyed by action name (what the config stores).
pub fn overrides(bindings: &[Binding]) -> BTreeMap<String, Binding> {
    COMMANDS
        .iter()
        .zip(bindings)
        .filter(|(c, b)| **b != c.default_binding())
        .map(|(c, b)| (c.action.to_string(), b.clone()))
        .collect()
}

use Media::{Next, Pause, Play, PlayPause, Previous, Stop};
use Mouse::{LDblClk, LUp, RUp, WDown, WUp, X1Down, X2Down};

const fn c(id: u16, action: &'static str, label: &'static str, key: &'static str, mouse: Mouse, mouse_fs: Mouse, media: Media) -> Cmd {
    Cmd { id, action, label, key, mouse, mouse_fs, media }
}

const N: Mouse = Mouse::None;
const NM: Media = Media::None;

// Generated from `default_wmcmds`; keep the order (the first matching binding wins, as in MPC).
#[rustfmt::skip]
pub const COMMANDS: &[Cmd] = &[
    c(969, "file-openquick", "Quick Open File", "<Control>q", N, N, NM),
    c(800, "file-openmedia", "Open File", "<Control>o", N, N, NM),
    c(801, "file-opendvdbd", "Open DVD/BD", "<Control>d", N, N, NM),
    c(802, "file-opendevice", "Open Device", "<Control>v", N, N, NM),
    c(976, "file-reopen", "Reopen File", "<Control>e", N, N, NM),
    c(24044, "file-recycle", "Move to Recycle Bin", "", N, N, NM),
    c(805, "file-save-copy", "Save a Copy", "", N, N, NM),
    c(806, "file-save-image", "Save Image", "<Alt>i", N, N, NM),
    c(807, "file-save-image-auto", "Save Image (auto)", "F5", N, N, NM),
    c(808, "file-save-thumbnails", "Save thumbnails", "", N, N, NM),
    c(809, "file-subtitles-load", "Load Subtitles...", "<Control>l", N, N, NM),
    c(810, "file-subtitles-save", "Save Subtitles...", "<Control>s", N, N, NM),
    c(812, "file-subtitles-download", "Download Subtitles...", "d", N, N, NM),
    c(811, "file-subtitles-upload", "Upload Subtitles...", "u", N, N, NM),
    c(804, "file-close-and-restore", "Close", "<Control>c", N, N, NM),
    c(814, "file-properties", "Properties", "<Shift>F10", N, N, NM),
    c(816, "file-exit", "Exit", "<Alt>x", N, N, NM),
    c(889, "play-playpause", "Play/Pause", "space", LUp, LUp, PlayPause),
    c(887, "play-play", "Play", "", N, N, Play),
    c(888, "play-pause", "Pause", "", N, N, Pause),
    c(890, "play-stop", "Stop", "period", N, N, Stop),
    c(891, "play-framestep", "Frame-step", "<Control>Right", N, N, NM),
    c(892, "play-framestepcancel", "Frame-step back", "<Control>Left", N, N, NM),
    c(893, "navigate-goto", "Go To", "<Control>g", N, N, NM),
    c(895, "play-incrate", "Increase Rate", "<Control>Up", N, N, NM),
    c(894, "play-decrate", "Decrease Rate", "<Control>Down", N, N, NM),
    c(896, "play-resetrate", "Reset Rate", "<Control>r", N, N, NM),
    c(905, "play-incauddelay", "Audio Delay +10 ms", "KP_Add", N, N, NM),
    c(906, "play-decauddelay", "Audio Delay -10 ms", "KP_Subtract", N, N, NM),
    c(900, "play-seekforwardsmall", "Jump Forward (small)", "", N, N, NM),
    c(899, "play-seekbackwardsmall", "Jump Backward (small)", "", N, N, NM),
    c(902, "play-seekforwardmed", "Jump Forward (medium)", "Right", N, N, NM),
    c(901, "play-seekbackwardmed", "Jump Backward (medium)", "Left", N, N, NM),
    c(904, "play-seekforwardlarge", "Jump Forward (large)", "", N, N, NM),
    c(903, "play-seekbackwardlarge", "Jump Backward (large)", "", N, N, NM),
    c(898, "play-seekkeyforward", "Jump Forward (keyframe)", "<Shift>Right", N, N, NM),
    c(897, "play-seekkeybackward", "Jump Backward (keyframe)", "<Shift>Left", N, N, NM),
    c(996, "play-seekset", "Jump to Beginning", "Home", N, N, NM),
    c(33449, "play-repeat-forever", "Repeat Forever", "", N, N, NM),
    c(33450, "play-repeat-onefile", "Repeat Mode: File", "", N, N, NM),
    c(33451, "play-repeat-wholeplaylist", "Repeat Mode: Playlist", "", N, N, NM),
    c(922, "navigate-skipforward", "Next", "Page_Down", X2Down, X2Down, Next),
    c(921, "navigate-skipback", "Previous", "Page_Up", X1Down, X1Down, Previous),
    c(920, "navigate-skipforwardfile", "Next File", "<Control>Page_Down", N, N, NM),
    c(919, "navigate-skipbackfile", "Previous File", "<Control>Page_Up", N, N, NM),
    c(974, "navigate-tunerscan", "Tuner scan", "<Shift>t", N, N, NM),
    c(975, "favorites-quickaddfavorite", "Quick add favorite", "<Shift>q", N, N, NM),
    c(817, "view-captionmenu", "Toggle Caption&Menu", "<Control>0", N, N, NM),
    c(818, "view-seeker", "Toggle Seek Bar", "<Control>1", N, N, NM),
    c(819, "view-controls", "Toggle Controls", "<Control>2", N, N, NM),
    c(820, "view-information", "Toggle Information", "<Control>3", N, N, NM),
    c(821, "view-statistics", "Toggle Statistics", "<Control>4", N, N, NM),
    c(822, "view-status", "Toggle Status", "<Control>5", N, N, NM),
    c(823, "view-subresync", "Toggle Subresync Bar", "<Control>6", N, N, NM),
    c(824, "view-playlist", "Toggle Playlist Bar", "<Control>7", N, N, NM),
    c(825, "view-capture", "Toggle Capture Bar", "<Control>8", N, N, NM),
    c(33415, "view-navigation", "Toggle Navigation Bar", "<Control>9", N, N, NM),
    c(826, "view-debugshaders", "Toggle Debug Shaders", "", N, N, NM),
    c(827, "view-presets-minimal", "View Minimal", "1", N, N, NM),
    c(828, "view-presets-compact", "View Compact", "2", N, N, NM),
    c(829, "view-presets-normal", "View Normal", "3", N, N, NM),
    c(830, "view-fullscreen", "Fullscreen", "<Alt>Return", LDblClk, LDblClk, NM),
    c(831, "view-fullscreen-secondary", "Fullscreen (w/o res.change)", "F11", N, N, NM),
    c(832, "view-zoom-50", "Zoom 50%", "<Alt>1", N, N, NM),
    c(833, "view-zoom-100", "Zoom 100%", "<Alt>2", N, N, NM),
    c(834, "view-zoom-200", "Zoom 200%", "<Alt>3", N, N, NM),
    c(968, "view-zoom-autofit", "Zoom Auto Fit", "<Alt>4", N, N, NM),
    c(4900, "view-zoom-autofit-larger", "Zoom Auto Fit (Larger Only)", "<Alt>5", N, N, NM),
    c(859, "aspectratio-next", "Next AR Preset", "", N, N, NM),
    c(835, "view-vf-half", "VidFrm Half", "", N, N, NM),
    c(836, "view-vf-normal", "VidFrm Normal", "", N, N, NM),
    c(837, "view-vf-double", "VidFrm Double", "", N, N, NM),
    c(838, "view-vf-stretch", "VidFrm Stretch", "", N, N, NM),
    c(839, "view-vf-frominside", "VidFrm Inside", "", N, N, NM),
    c(841, "view-vf-zoom1", "VidFrm Zoom 1", "", N, N, NM),
    c(842, "view-vf-zoom2", "VidFrm Zoom 2", "", N, N, NM),
    c(840, "view-vf-fromoutside", "VidFrm Outside", "", N, N, NM),
    c(843, "view-vf-switchzoom", "VidFrm Switch Zoom", "", N, N, NM),
    c(884, "ontop-always", "Always On Top", "<Control>a", N, N, NM),
    c(861, "view-reset", "PnS Reset", "KP_5", N, N, NM),
    c(862, "view-incsize", "PnS Inc Size", "KP_9", N, N, NM),
    c(864, "view-incwidth", "PnS Inc Width", "KP_6", N, N, NM),
    c(866, "view-incheight", "PnS Inc Height", "KP_8", N, N, NM),
    c(863, "view-decsize", "PnS Dec Size", "KP_1", N, N, NM),
    c(865, "view-decwidth", "PnS Dec Width", "KP_4", N, N, NM),
    c(867, "view-decheight", "PnS Dec Height", "KP_2", N, N, NM),
    c(876, "panscan-center", "PnS Center", "<Control>KP_5", N, N, NM),
    c(868, "panscan-moveleft", "PnS Left", "<Control>KP_4", N, N, NM),
    c(869, "panscan-moveright", "PnS Right", "<Control>KP_6", N, N, NM),
    c(870, "panscan-moveup", "PnS Up", "<Control>KP_8", N, N, NM),
    c(871, "panscan-movedown", "PnS Down", "<Control>KP_2", N, N, NM),
    c(872, "panscan-moveupleft", "PnS Up/Left", "<Control>KP_7", N, N, NM),
    c(873, "panscan-moveupright", "PnS Up/Right", "<Control>KP_9", N, N, NM),
    c(874, "panscan-movedownleft", "PnS Down/Left", "<Control>KP_1", N, N, NM),
    c(875, "panscan-movedownright", "PnS Down/Right", "<Control>KP_3", N, N, NM),
    c(877, "panscan-rotatexp", "PnS Rotate X+", "<Alt>KP_8", N, N, NM),
    c(878, "panscan-rotatexm", "PnS Rotate X-", "<Alt>KP_2", N, N, NM),
    c(879, "panscan-rotateyp", "PnS Rotate Y+", "<Alt>KP_4", N, N, NM),
    c(880, "panscan-rotateym", "PnS Rotate Y-", "<Alt>KP_6", N, N, NM),
    c(881, "panscan-rotatezp", "PnS Rotate Z+", "<Alt>KP_1", N, N, NM),
    c(882, "panscan-rotatezm", "PnS Rotate Z-", "<Alt>KP_3", N, N, NM),
    c(907, "volume-up", "Volume Up", "Up", WUp, WUp, NM),
    c(908, "volume-down", "Volume Down", "Down", WDown, WDown, NM),
    c(909, "volume-mute", "Volume Mute", "<Control>m", N, N, NM),
    c(970, "volume-boost-inc", "Volume boost increase", "", N, N, NM),
    c(971, "volume-boost-dec", "Volume boost decrease", "", N, N, NM),
    c(972, "volume-boost-min", "Volume boost Min", "", N, N, NM),
    c(973, "volume-boost-max", "Volume boost Max", "", N, N, NM),
    c(993, "custom-channel-mapping", "Toggle custom channel mapping", "", N, N, NM),
    c(994, "normalize", "Toggle normalization", "", N, N, NM),
    c(995, "regain-volume", "Toggle regain volume", "", N, N, NM),
    c(984, "color-brightness-inc", "Brightness increase", "", N, N, NM),
    c(985, "color-brightness-dec", "Brightness decrease", "", N, N, NM),
    c(986, "color-contrast-inc", "Contrast increase", "", N, N, NM),
    c(987, "color-contrast-dec", "Contrast decrease", "", N, N, NM),
    c(988, "color-hue-inc", "Hue increase", "", N, N, NM),
    c(989, "color-hue-dec", "Hue decrease", "", N, N, NM),
    c(990, "color-saturation-inc", "Saturation increase", "", N, N, NM),
    c(991, "color-saturation-dec", "Saturation decrease", "", N, N, NM),
    c(992, "color-reset", "Reset color settings", "", N, N, NM),
    c(923, "navigate-titlemenu", "DVD Title Menu", "<Alt>t", N, N, NM),
    c(924, "navigate-rootmenu", "DVD Root Menu", "<Alt>r", N, N, NM),
    c(925, "navigate-subpicturemenu", "DVD Subtitle Menu", "", N, N, NM),
    c(926, "navigate-audiomenu", "DVD Audio Menu", "", N, N, NM),
    c(927, "navigate-anglemenu", "DVD Angle Menu", "", N, N, NM),
    c(928, "navigate-chaptermenu", "DVD Chapter Menu", "", N, N, NM),
    c(929, "navigate-menu-left", "DVD Menu Left", "<Alt>Left", N, N, NM),
    c(930, "navigate-menu-right", "DVD Menu Right", "<Alt>Right", N, N, NM),
    c(931, "navigate-menu-up", "DVD Menu Up", "<Alt>Up", N, N, NM),
    c(932, "navigate-menu-down", "DVD Menu Down", "<Alt>Down", N, N, NM),
    c(933, "navigate-menu-activate", "DVD Menu Activate", "", N, N, NM),
    c(934, "navigate-menu-back", "DVD Menu Back", "", N, N, NM),
    c(935, "navigate-menu-leave", "DVD Menu Leave", "", N, N, NM),
    c(944, "boss", "Boss key", "b", N, N, NM),
    c(949, "menu-player-short", "Player Menu", "Menu", RUp, RUp, NM),
    c(950, "menu-player-long", "Player Menu (full)", "", N, N, NM),
    c(951, "menu-filters", "Filters Menu", "", N, N, NM),
    c(815, "view-options", "Options", "o", N, N, NM),
    c(952, "stream-audio-next", "Next Audio Track", "a", N, N, NM),
    c(953, "stream-audio-prev", "Prev Audio Track", "<Shift>a", N, N, NM),
    c(954, "stream-sub-next", "Next Subtitle Track", "s", N, N, NM),
    c(955, "stream-sub-prev", "Prev Subtitle Track", "<Shift>s", N, N, NM),
    c(956, "stream-sub-onoff", "On/Off Subtitle", "w", N, N, NM),
    c(2302, "subtitles-subitem-2", "Reload Subtitles", "", N, N, NM),
    c(961, "dvd-angle-next", "Next Angle (DVD)", "", N, N, NM),
    c(962, "dvd-angle-prev", "Prev Angle (DVD)", "", N, N, NM),
    c(963, "dvd-audio-next", "Next Audio Track (DVD)", "", N, N, NM),
    c(964, "dvd-audio-prev", "Prev Audio Track (DVD)", "", N, N, NM),
    c(965, "dvd-sub-next", "Next Subtitle Track (DVD)", "", N, N, NM),
    c(966, "dvd-sub-prev", "Prev Subtitle Track (DVD)", "", N, N, NM),
    c(967, "dvd-sub-onoff", "On/Off Subtitle (DVD)", "", N, N, NM),
    c(32769, "view-tearing-test", "Tearing Test", "<Control>t", N, N, NM),
    c(32778, "view-osd-display-time", "OSD: Display Current Time", "<Control>i", N, N, NM),
    c(32777, "view-osd-show-filename", "OSD: Show File Name", "n", N, N, NM),
    c(57382, "shaders-preset-next", "Next Shader Preset", "", N, N, NM),
    c(57384, "shaders-preset-prev", "Prev Shader Preset", "", N, N, NM),
    c(32779, "d3dfullscreen-toggle", "Toggle Direct3D fullscreen", "", N, N, NM),
    c(32780, "goto-prev-sub", "Goto Prev Subtitle", "y", N, N, NM),
    c(32781, "goto-next-sub", "Goto Next Subtitle", "u", N, N, NM),
    c(32782, "shift-sub-down", "Shift Subtitle Left", "<Alt>Page_Down", N, N, NM),
    c(32783, "shift-sub-up", "Shift Subtitle Right", "<Alt>Page_Up", N, N, NM),
    c(32784, "view-display-renderer-stats", "OSD: Display Renderer Statistics", "<Control>j", N, N, NM),
    c(32785, "view-reset-renderer-stats", "OSD: Reset Renderer Statistics", "<Control><Alt>r", N, N, NM),
    c(33243, "view-vsync", "VSync", "v", N, N, NM),
    c(33265, "view-enableframetimecorrection", "Enable Frame Time Correction", "", N, N, NM),
    c(33260, "view-vsyncaccurate", "Accurate VSync", "<Control><Alt>v", N, N, NM),
    c(33246, "view-vsyncoffset-decrease", "Decrease VSync Offset", "<Control><Alt>Up", N, N, NM),
    c(33247, "view-vsyncoffset-increase", "Increase VSync Offset", "<Control><Alt>Down", N, N, NM),
    c(24000, "sub-delay-down", "Subtitle Delay -", "F1", N, N, NM),
    c(24001, "sub-delay-up", "Subtitle Delay +", "F2", N, N, NM),
    c(948, "afterplayback-donothing", "After Playback: Do nothing", "", N, N, NM),
    c(947, "afterplayback-playnext", "After Playback: Play next file in the folder", "", N, N, NM),
    c(918, "afterplayback-monitoroff", "After Playback: Turn off the monitor", "", N, N, NM),
    c(912, "afterplayback-exit", "After Playback: Exit", "", N, N, NM),
    c(913, "afterplayback-standby", "After Playback: Stand By", "", N, N, NM),
    c(914, "afterplayback-hibernate", "After Playback: Hibernate", "", N, N, NM),
    c(915, "afterplayback-shutdown", "After Playback: Shutdown", "", N, N, NM),
    c(916, "afterplayback-logoff", "After Playback: Log Off", "", N, N, NM),
    c(917, "afterplayback-lock", "After Playback: Lock", "", N, N, NM),
    c(846, "view-editlisteditor", "Toggle EDL window", "", N, N, NM),
    c(847, "edl-in", "EDL set In", "", N, N, NM),
    c(848, "edl-out", "EDL set Out", "", N, N, NM),
    c(849, "edl-newclip", "EDL new clip", "", N, N, NM),
    c(860, "edl-save", "EDL save", "", N, N, NM),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert_eq!(COMMANDS.len(), 184);
        let mut ids: Vec<u16> = COMMANDS.iter().map(|c| c.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), COMMANDS.len(), "duplicate command id");
        let mut names: Vec<&str> = COMMANDS.iter().map(|c| c.action).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), COMMANDS.len(), "duplicate action name");
        for c in COMMANDS {
            if !c.key.is_empty() {
                let name = c.key.rsplit('>').next().unwrap();
                assert!(gtk4::gdk::Key::from_name(name).is_some(), "bad key {} for {}", c.key, c.action);
            }
        }
        assert_eq!(Mouse::button(1, Click::Double), Mouse::LDblClk);
        assert_eq!(Mouse::button(9, Click::Up), Mouse::X2Up);
        assert_eq!(Mouse::button(3, Click::Down), Mouse::RDown);
    }

}
