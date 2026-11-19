# mpc-rs TODO

Feature parity with MPC-HC, Linux first. `[ ]` = todo, `[x]` = done.
Each phase ends with a build that runs, then stops for review.

## Phase 0 — Environment (WSL Ubuntu)
- [x] Install system packages: `build-essential pkg-config libmpv-dev libgtk-4-dev mpv`
- [x] Install Rust with rustup (1.100)
- [x] Check that a WSLg window opens and `mpv` plays a test file with audio (`~/media/test.mkv`)
- [x] Check that Ubuntu's libmpv (0.37) was built with DVD (dvdnav), Blu-ray, v4l2 and DVB support (all present)

## Phase 1 — Skeleton
- [x] `cargo new mpc-rs`; add the gtk4 (0.11, `v4_12`) and libmpv2 (7.0) crates
- [x] GTK4 window with a `GLArea`; mpv's OpenGL render API draws into it
- [x] Play a file passed on the command line (a second launch hands its files to the running player)
- [x] Close cleanly (mpv and GL context torn down)

## Phase 2 — Core playback UI
- [x] Seek bar (click/drag, hover time tooltip, chapter marks)
- [x] Toolbar: play/pause, stop, previous/next, frame step, volume slider, mute
- [x] Status bar: state, elapsed / total time
- [x] Menu bar: File / View / Play / Navigate / Help (Favorites comes in phase 8)
- [x] Open File, Quick Open, Open Directory, Open URL dialogs
- [x] Drag and drop files, folders and subtitles
- [x] Fullscreen (double-click, keyboard); hide controls and cursor in fullscreen
- [x] View presets: Minimal / Compact / Normal; toggle Caption & Menu, Seek Bar, Controls, Info, Statistics, Status
- [ ] Stays on top: Default / Always / While Playing / While Playing Video (GTK4 has no API for this on Wayland; X11 only, or drop it)
- [x] Logo shown when idle

## Phase 3 — Command system
- [x] Command table ported from `wmcmd` in `AppSettings.cpp` (IDs, names, default keys); commands
      not implemented yet are listed but dimmed
- [x] Key, mouse button and mouse wheel bindings, all remappable (saved to `config.toml` `[keys]`)
- [x] Hotkey editor in Options (Keys page); right-click player menu (`ID_MENU_PLAYER_SHORT`)
- [x] Media keys through MPRIS (Play/Pause/Stop/Next/Previous, bindable like MPC's App Command column)

## Phase 4 — Playlist
- [x] Side playlist panel (View > Playlist, Ctrl+7); add, remove, drag to reorder, sort, randomize,
      restore, copy, add containing folder, hide on fullscreen; restored on the next start (`default.mpcpl`)
- [x] Next/previous, shuffle; repeat file / playlist / forever
- [x] Load `.mpcpl`, `.m3u`, `.m3u8`, `.pls`, `.asx`, `.cue` (a cue sheet plays as one item with chapters);
      save `.mpcpl`, `.m3u`, `.m3u8`, `.pls`, `.asx`
- [x] Auto-load subtitles and audio files from the folder (mpv `sub-auto`/`audio-file-auto`); next/previous
      file in the folder when the playlist has one item
- [x] Recent files; remember the playback position per file (off by default, as in MPC:
      `remember_file_pos` in `config.toml` until the Options page exists)

## Phase 5 — Settings
- [x] TOML config in `$XDG_CONFIG_HOME/mpc-rs/config.toml`, with defaults matching MPC's
- [x] Options dialog with a tree of pages, like `PPage*`: Player, Keys, Logo, Playback, Fullscreen, Tweaks,
      Miscellaneous (color controls, reset, export). The other pages come with their features:
      Formats and Web Interface (phase 9), Output, Shaders and Sync (phase 7), Audio, Subtitles and
      Subtitle style (phase 6), Advanced (once there are settings without a page)
- [x] Single instance; open in the running player or add to its playlist ("Open a new player for each
      media file" and `/new` start a separate one)
- [x] Command line switches (same as MPC's where they make sense: `/open`, `/play`, `/close`, `/playnext`,
      `/add`, `/new`, `/fullscreen`, `/minimized`, `/viewpreset`, `/randomize`, `/start`, `/startpos`, `/sub`,
      `/dub`, `/volume`, `/mute`, `/fixedsize`, `/nofocus`, `/reset`, `/help`); also as `-x` / `--x`
- [x] After Playback setting: do nothing / play next file in folder / rewind / close / exit
- Not possible on Wayland: remember window position, fullscreen monitor and display mode switching

## Phase 6 — Tracks, audio, subtitles
- [x] Audio, video and subtitle track menus (Play > Audio/Subtitle/Video Track); next/previous track
      (`a`/`s`), subtitles on/off (`w`), reload; preferred languages (phase 5)
- [x] Volume boost, normalize, downmix, bs2b crossfeed, down-sample to 44100 Hz, audio delay and time
      shift (mpv `volume-gain`, `af`); Options > Audio
- [x] Speed: increase / decrease / reset (MPC's speed step, 0 = double/halve), with pitch correction
- [x] Load and save subtitles (save: external SRT/WebVTT/ASS, with delay and resync applied); subtitle
      delay (F1/F2); previous/next subtitle line (`y`/`u`); show/hide subtitles
- [x] Subtitle style overrides (font, size, bold/italic, colors and transparency, outline, shadow,
      opaque box, margin, vertical position; optionally forced on ASS); Options > Subtitles, Default Style
- [x] Subresync bar (View > Subresync, Ctrl+6): line list, set start / set to current time, MPC's
      anchors (stretch between, shift outside), shift current line (Alt+PgUp/PgDn). External text
      subtitles only: mpv can't hand back embedded subtitle lines
- Moved to phase 9: online subtitle download/upload (needs an HTTPS client crate and an API key)

## Phase 7 — Video
- [x] Zoom 50/100/200%, auto fit, auto fit (larger only); auto-zoom on open (Options > Playback)
- [x] Video frame: half/normal/double, stretch, touch window from inside/outside, zoom 1/2, switch zoom
- [x] Aspect ratio: default, 4:3, 5:4, 16:9, 2.35:1, 1.85:1, square pixels; next aspect ratio
- [x] Pan & scan: size, width, height, move (8 directions), center, reset, presets (MPC's three defaults)
- [x] Rotate 90° both ways; flip / mirror (MPC's X/Y "rotate -"). MPC's 3D tilt (X/Y "rotate +") has no
      mpv equivalent
- [x] Shaders: all 22 of `res/shaders/external/*.hlsl` ported to mpv GLSL; pre-/post-resize lists
      (Options > Shaders); shader editor with mpv's compile errors; Play > Shaders > Enabled. Shader
      presets not done yet
- [x] Color management (display ICC profile, rendering intent), output range 0-255 / 16-235;
      hardware decoding (Options > Output)
- [x] Sync video to display (display-resample) and frame interpolation (Options > Sync, `V`)
- [x] OSD messages (mpv's OSD; Show OSD, OSD font); statistics overlay (Ctrl+J); show current time
      (Ctrl+I) / file name (N)
- [x] Save Image, Save Image (auto, F5), Save Thumbnails, copy image to clipboard
- [x] Deinterlace toggle (View > Video Frame)

## Phase 8 — Discs, devices, info
- [x] Open DVD/BD (folder with VIDEO_TS/BDMV, or an .iso); titles (Navigate > Titles), angles, DVD audio/
      subtitle commands; remember DVD position (with its title). Encrypted DVDs need libdvdcss
- [x] Open Disc (mounted DVDs/Blu-rays, audio CDs, drives; refreshed as discs come and go); Audio CD
      plays as one item with a chapter per track
- [x] Capture devices via v4l2 (Open Device: device, size, default audio input; capture bar with Record)
- [x] A-B repeat (Play > A-B Repeat), Go To (time/frame), chapters (Navigate > Chapters)
- [x] Favorites: add (with position), quick add, organize (rename, delete, reorder), for files, discs and
      devices
- [x] Properties dialog: Details, Clip, MediaInfo tab (from mpv's track info and metadata)
- [x] Save a Copy (anything gio can read, so local files and http streams; with progress)
- Not possible with mpv: DVD/Blu-ray menus (title/root/subtitle/audio/angle/chapter menus and menu
  navigation). mpv plays disc titles directly

## Phase 9 — Control and integration
- [ ] Web remote: same routes as `WebServer.cpp` (`/`, `/controls.html`, `/command.html`, `/status.html`,
      `/variables.html`, `/info.html`, `/browser.html`, `/player.html`, `/snapshot.jpg`) and the same
      `wm_command` IDs. Built once, removed for now (not needed yet)
- [x] MPRIS D-Bus service: play state, metadata, position, Seek/SetPosition (Seeked signal), volume,
      rate, loop status, shuffle, OpenUri
- [x] JSON control socket (`$XDG_RUNTIME_DIR/mpc-rs.sock`, one JSON object per line) with `MpcApi.h`'s
      commands and events (STATE, NOWPLAYING, NOTIFYSEEK, NOTIFYENDOFSTREAM); see `remote.rs`
- [x] `.desktop` entry, MPC's icon, MIME associations: Options > Formats and `/regvid` `/regaud`
      `/regpl` `/regall` `/unregall` (installs into `~/.local/share`, defaults in `mimeapps.list`)
- [x] Play > After Playback menu (one-shot): do nothing, play next, turn off the monitor, exit,
      stand by, hibernate, shutdown, log off, lock (systemd-logind / GNOME over D-Bus)
- [x] Translations from MPC's PO files (42 languages, `res/po`): menus, Options and the Keys page.
      Options > Player > Language (system default unless set); takes effect at the next start

## Phase 10 — DVB TV
- [ ] Tuner setup and channel scan (`dvb://`)
- [ ] Channel list, switching, EPG / now-next info

## Phase 11 — Windows port
- [ ] Build on Windows (MSVC or MSYS2 GTK4), bundle the GTK and libmpv DLLs
- [ ] Single instance / IPC on Windows; optionally the `WM_COPYDATA` API for compatibility
- [ ] File associations, installer

## Dropped (Windows-only or obsolete)
- DirectShow internal/external filter management, filter property pages
- EVR/VMR/madVR/D3D renderer specifics (mpv's renderer covers this)
- QuickTime, RealMedia, Shockwave Flash graphs
- Logitech LCD, Skype mood message, WinLIRC/uICE
- DrDump crash reporter, update checker

## Last — Online subtitles
- [ ] Online subtitle download/upload (OpenSubtitles REST API; decide on the HTTPS crate and API key first)
