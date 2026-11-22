# mpc-revamp

A Rust rewrite of MPC-HC (Media Player Classic - Home Cinema). **Linux first**, Windows later.
Goal: the same features as MPC-HC, except those tied to Windows itself (see "Dropped" in TODO.md).

## Layout

- `mpc-hc-develop/` — the original C++/MFC/DirectShow source. **Reference only. Never edit it.**
  It's gitignored (see README for how to get it). Its submodules (LAV Filters, MediaInfo, sanear, ...)
  are empty and not needed.
- `mpc-rv/` — the new Rust project (Cargo); the program is `mpc-rv`.
- `TODO.md` — the phase plan and feature checklist. Tick items off as they're done.
- Tests: unit tests inline (`#[cfg(test)] mod tests`) for private details; integration tests in
  `mpc-rv/tests/` for anything reachable through the `mpc_rv` library (`src/lib.rs`; `main.rs` stays thin).

## Stack

- **Playback:** libmpv through the `libmpv2` crate. Use mpv for decoding, rendering, subtitles,
  audio filters, DVD/Blu-ray and capture. Don't reimplement anything mpv already does.
- **UI:** GTK4 (`gtk4-rs`). Video is drawn into a `GLArea` with mpv's OpenGL render API.
- **Config:** a TOML file in `$XDG_CONFIG_HOME/mpc-rv/` (via `serde` + `toml`).
- **IPC:** MPRIS/D-Bus on Linux, which replaces MPC's `WM_COPYDATA` API (`MpcApi.h`).
- **Web remote:** `tiny_http`, using the same routes as `src/mpc-hc/WebServer.cpp`.

## Dev environment

- Native **Ubuntu 26.04**, GNOME on **Wayland**, NVIDIA GPU. Run everything directly from the repo:
  `cd mpc-rv && cargo run -- ../media/test-movie.mp4`
- System packages: `build-essential pkg-config libmpv-dev libgtk-4-dev mpv`. sudo needs a password,
  so ask the user to install packages.
- **Test video:** always test with `media/test-movie.mp4` (a 720p x264/AAC feature film). `/media/` is
  gitignored. Never commit it.
- GTK 4.22 draws with **GLES** here. Don't force desktop GL on the `GLArea`, because GTK then can't create
  its context ("Unable to create a GL context") and the video stays blank while audio plays.
- Logs: libmpv prints only to a tty, so wrap runs in a pty to capture them:
  `MPC_RS_MPV_LOG=all=v script -qfec "cargo run -q -- ../media/test-movie.mp4" /dev/null`
  (`GDK_DEBUG=opengl` shows GTK's GL context choice.)
- Checking the GUI: never capture the whole screen (it holds the user's private windows). GNOME on
  Wayland has no per-window capture, so run with `GDK_BACKEND=x11`, find the player window with
  `xwininfo -root -tree` (skip the 1x1 leader window), and grab only that window id with `XGetImage`
  (Python ctypes + PIL; `xwd`/`xdotool` aren't installed). Never send synthetic input, because it would
  go to whatever window has focus.
- WSL (older setup, still handled in code): WSLg's llvmpipe needs desktop GL and `fbo-format=rgba8`.
  Both apply only when `WSL_DISTRO_NAME` is set.

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

## Branches and releases

- `main` holds released versions only. Each upcoming version has a release branch, `release/<version>`
  (e.g. `release/1.0.1`), made from `main`.
- All work happens on a sub-branch made from the current release branch: `feature/<name>` for new work,
  `fix/<name>` for bug fixes.
- Only the admin (the repository owner) merges into `release/*` and `main`, after checking the work.
  Contributors open pull requests against the current `release/*` branch, never `main`.
- Merges use merge commits (`git merge --no-ff`), so each sub-branch and each release shows as one unit.
- Releasing: bump the version on the release branch (`mpc-rv/Cargo.toml`, `Cargo.lock`, the metainfo
  `<release>`, `packaging/aur/PKGBUILD` and `.SRCINFO`), merge it into `main` and push the tag
  `v<version>` on `main`. The "Release Flatpak" GitHub Action (`.github/workflows/release-flatpak.yml`)
  then builds the Flatpak and attaches it to that tag's GitHub release (creating the release if needed).
  `packaging/build-flatpak.sh` builds the same bundle locally.
- Ask before every commit, merge, tag and push.
