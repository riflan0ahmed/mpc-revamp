# mpc-rs

A Rust rewrite of MPC-HC (Media Player Classic - Home Cinema). **Linux first**, Windows later.
Goal: the same features as MPC-HC, except those tied to Windows itself (see "Dropped" in TODO.md).

## Layout

- `mpc-hc-develop/` — the original C++/MFC/DirectShow source. **Reference only. Never edit it.**
  It's gitignored (see README for how to get it). Its submodules (LAV Filters, MediaInfo, sanear, ...)
  are empty and not needed.
- `mpc-rs/` — the new Rust project (Cargo).
- `TODO.md` — the phase plan and feature checklist. Tick items off as they're done.

## Stack

- **Playback:** libmpv through the `libmpv2` crate. Use mpv for decoding, rendering, subtitles,
  audio filters, DVD/Blu-ray and capture. Don't reimplement anything mpv already does.
- **UI:** GTK4 (`gtk4-rs`). Video is drawn into a `GLArea` with mpv's OpenGL render API.
- **Config:** a TOML file in `$XDG_CONFIG_HOME/mpc-rs/` (via `serde` + `toml`).
- **IPC:** MPRIS/D-Bus on Linux, which replaces MPC's `WM_COPYDATA` API (`MpcApi.h`).
- **Web remote:** `tiny_http`, using the same routes as `src/mpc-hc/WebServer.cpp`.

## Dev environment

- Build and run inside **WSL Ubuntu**. WSLg displays the GUI windows. Rust isn't installed on Windows.
- The repo lives on the Windows drive, so WSL sees it under `/mnt/c/...`. Below, `$REPO` is that
  path (`wslpath` converts a Windows path). Point the build output at the Linux filesystem, because building on `/mnt/c` is slow:
  `export CARGO_TARGET_DIR=~/.cache/mpc-rs-target`
- Run commands from Windows like this: `wsl.exe -d Ubuntu -- bash -lc '<cmd>'`
- System packages: `build-essential pkg-config libmpv-dev libgtk-4-dev mpv`.
  sudo needs a password; install with `wsl.exe -d Ubuntu -u root -- apt-get install ...` instead.
- Build and run:
  `wsl.exe -d Ubuntu -- bash -lc 'source ~/.cargo/env; export CARGO_TARGET_DIR=~/.cache/mpc-rs-target; cd $REPO/mpc-rs && cargo run -- ~/media/test.mkv'`
  (`~/media/test.mkv` is a 20 s test pattern with a sine tone.)
- WSLg has no GPU here (llvmpipe, "COPY MODE"). For mpv to draw correctly it needs **desktop GL**
  (`GLArea::set_allowed_apis(GL)`) **and `fbo-format=rgba8`** (set only when `WSL_DISTRO_NAME` is set).
  GLES gives a green picture; the default `rgba16f` gives black.
- Checking the GUI: never capture the whole screen (it holds the user's private windows). Use
  `PrintWindow` on the window whose title contains `mpc-rs (Ubuntu)` (WSLg windows belong to
  `msrdc.exe`). Close the player with `WM_CLOSE` to the same window.

## Where to look in the original

| Topic | File |
|---|---|
| Main window, all command handlers, media open/close | `src/mpc-hc/MainFrm.cpp` (17k lines) |
| Settings and their defaults, default key bindings (`wmcmd` table) | `src/mpc-hc/AppSettings.cpp`, `SettingsDefines.h` |
| Command IDs | `src/mpc-hc/resource.h` |
| Playlist model and formats | `src/mpc-hc/Playlist.cpp`, `PlayerPlaylistBar.cpp` |
| Web remote routes and pages | `src/mpc-hc/WebServer.cpp`, `WebClientSocket.cpp`, `res/web/` |
| External control API | `src/mpc-hc/MpcApi.h` |
| Menus and dialogs | `src/mpc-hc/mpc-hc.rc` |
| Shaders (HLSL, to be ported to GLSL) | `src/mpc-hc/res/shaders/` |

## Rules

- Work one phase at a time, following TODO.md. Stop after each phase so the user can review and test.
- Every phase must leave a version that builds and runs.
- Don't commit unless the user asks. They review first.
- Commit messages: no `Co-Authored-By: Claude` trailer and no "Generated with Claude Code" lines.
- License is GPL-3.0-or-later (same as MPC-HC), so porting MPC-HC code and assets is allowed.
- Keep it simple: few crates, few files, no abstractions written "for later".
