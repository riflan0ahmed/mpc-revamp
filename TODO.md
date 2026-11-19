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
- [ ] Dockable/side playlist panel; add, remove, reorder, sort, randomize
- [ ] Next/previous, shuffle; repeat file / playlist / forever
- [ ] Load and save `.mpcpl`, `.m3u`, `.m3u8`, `.pls`, `.asx`, `.cue`
- [ ] Auto-load subtitles and other files from the folder; "play next file in folder"
- [ ] Recent files; remember the playback position per file and per DVD

## Phase 5 — Settings
- [ ] TOML config in `$XDG_CONFIG_HOME/mpc-rs/config.toml`, with defaults matching MPC's
- [ ] Options dialog with a tree of pages, like `PPage*`: Player, Formats, Keys, Logo, Web Interface, Playback, Output, Shaders, Fullscreen, Sync, Audio, Subtitles, Subtitle style, Misc, Tweaks, Advanced
- [ ] Single instance; open in the running player or add to its playlist
- [ ] Command line switches (same as MPC's where they make sense: `/play`, `/close`, `/fullscreen`, `/start`, `/add`, `/sub`, ...)

## Phase 6 — Tracks, audio, subtitles
- [ ] Audio, video and subtitle track menus; preferred languages
- [ ] Volume boost, normalize, downmix, bs2b, audio delay (mpv `af`)
- [ ] Speed: increase / decrease / reset, with pitch correction
- [ ] Load and save subtitles; subtitle delay; show/hide subtitles
- [ ] Subtitle style overrides (font, colors, outline, position)
- [ ] Subresync bar (edit subtitle timings)
- [ ] Online subtitle download/upload (OpenSubtitles REST API)

## Phase 7 — Video
- [ ] Zoom 50/100/200%, auto fit, auto fit (larger only)
- [ ] Video frame: half/normal/double, stretch, touch window from inside/outside
- [ ] Aspect ratio: default, 4:3, 5:4, 16:9, 2.35:1, 1.85:1, square pixels
- [ ] Pan & scan: size, width, height, move, center, reset, presets
- [ ] Rotate/flip
- [ ] Shaders: port `res/shaders/*.hlsl` to GLSL; pre-/post-resize lists; shader editor
- [ ] Color management (ICC), output range 0-255 / 16-235
- [ ] VSync / display sync / interpolation options
- [ ] OSD messages; statistics overlay; show current time / file name
- [ ] Save Image, Save Thumbnails, copy frame to clipboard
- [ ] Deinterlace toggle

## Phase 8 — Discs, devices, info
- [ ] Open DVD/BD; DVD menus (title/root/subtitle/audio/angle/chapter); navigation
- [ ] Open Disc (auto-detect), Audio CD
- [ ] Capture devices via v4l2 (Open Device, capture bar)
- [ ] A-B repeat, Go To (time/frame), chapters
- [ ] Favorites: add/organize, for files, DVDs and devices
- [ ] Properties dialog: Details, Clip, MediaInfo tab (libmediainfo or mpv track info)
- [ ] Save a Copy

## Phase 9 — Control and integration
- [ ] Web remote: same routes as `WebServer.cpp` (`/`, `/controls.html`, `/command.html`, `/status.html`, `/variables.html`, `/info.html`, `/browser.html`, `/player.html`, `/snapshot.jpg`) and the same `wm_command` IDs
- [ ] MPRIS D-Bus service (play/pause, seek, metadata)
- [ ] JSON control socket as the replacement for the `MpcApi.h` commands
- [ ] `.desktop` file, MIME associations, icons
- [ ] After playback: nothing / next in folder / exit / suspend / shutdown / lock
- [ ] Translations (gettext; reuse the PO files in `mpcresources/PO`)

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
