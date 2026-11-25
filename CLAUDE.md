# MPC-RV

A Rust rewrite of MPC-HC (Media Player Classic - Home Cinema). **Linux first**, Windows later.
Goal: the same features as MPC-HC, except those tied to Windows itself (see "Dropped" in TODO.md).

## Layout

- `mpc-hc-develop/` — the original C++/MFC/DirectShow source. **Reference only. Never edit it.**
  It's gitignored (see README for how to get it). Its submodules (LAV Filters, MediaInfo, sanear, ...)
  are empty and not needed.
- `mpc-rv/` — the new Rust project, a monorepo-style Cargo workspace (the app's name, as users see it,
  is **MPC-RV**). The root `Cargo.toml` only lists the members and holds the version.
  - `mpc-rv/core/` — the `mpc-core` crate, shared by both apps: everything with no UI (playlist,
    commands, config, command line, subtitles, i18n, paths, and `player`: starting mpv and the settings
    as mpv options). No GTK, no Win32 and no window code here (mpv is fine). Where Linux and Windows
    differ, keep it to a small, tested `cfg!(windows)`.
  - `mpc-rv/linux/` — the `mpc-rv` crate: the Linux app (GTK4), the program `mpc-rv`. It re-exports
    the core's modules (`mpc_rv::config` etc.).
  - `mpc-rv/windows/` — the `mpc-rv-windows` crate: the Windows app. On Linux it builds as a stub.
  - `mpc-rv/res/` — icons, logo, translations (PO), shaders; shared.
  - `cargo build`/`run`/`test` in `mpc-rv/` cover the core and the Linux app (`default-members`).
- `installers/` — one folder per way to install the app: `flatpak/`, `aur/`, `windows/` (later `debian/`,
  `appimage/`).
- `website/` — the landing page (plain HTML, no build step), published to GitHub Pages at
  https://riflan0ahmed.github.io/mpc-revamp by `.github/workflows/website.yml` when it changes on `main`. Keep its
  feature list and install steps in line with the app, and its claims true (check them in the code).
  When a page changes, update its `<lastmod>` in `website/sitemap.xml` (and, for the home page, the
  "Page updated" date in its footer). `website/llms.txt` summarizes the app for AI assistants: keep it true too. `website/keyboard-shortcuts/` is
  generated: after changing `core/src/commands.rs` or adding a command to the app, run
  `python3 website/make_shortcuts.py` from the repo root.
- `TODO.md` — the phase plan and feature checklist. Tick items off as they're done.
- `ARCHITECTURE.md` — diagrams of how the crates and files connect, for readers new to Rust. Update it
  when a source file is added, removed or moved, or when one starts or stops using another.
- Tests: unit tests inline (`#[cfg(test)] mod tests`) for private details; integration tests in
  `mpc-rv/core/tests/` for the `mpc_core` library and `mpc-rv/linux/tests/` for the `mpc_rv` library
  (`linux/src/lib.rs`; `main.rs` stays thin). Run them all with `cargo test --workspace`.
- A core change must keep both apps building: check Linux with `cargo clippy --workspace --all-targets`
  and Windows with `cargo clippy -p mpc-core -p mpc-rv-windows --all-targets --target x86_64-pc-windows-msvc`;
  CI (`.github/workflows/windows.yml`) runs the core's tests on Windows.

## Stack

- **Playback:** libmpv through the `libmpv2` crate. Use mpv for decoding, rendering, subtitles,
  audio filters, DVD/Blu-ray and capture. Don't reimplement anything mpv already does.
- **UI (Linux):** GTK4 (`gtk4-rs`). Video is drawn into a `GLArea` with mpv's OpenGL render API.
- **UI (Windows):** a native Win32 app through the `windows` crate (Windows 11 look: dark mode,
  Mica), in `mpc-rv/windows/` on top of `mpc-core`; mpv draws straight into the window (`wid`).
  Not GTK on Windows. See Phase 11 in TODO.md. Toolchain: Rust's MSVC target, linked against a pinned
  libmpv build (`installers/windows/get-mpv.ps1`). From Linux, the clippy command above catches Windows
  compile errors (it can't link or run there; CI does that).
- **Config:** a TOML file in `$XDG_CONFIG_HOME/mpc-rv/` (via `serde` + `toml`). User folders come from
  `mpc_core::paths` (same results as GLib on Linux), not GLib, so the core stays GTK-free.
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
- Releasing: bump the version on the release branch (`mpc-rv/Cargo.toml` `[workspace.package]`, `Cargo.lock`, the metainfo
  `<release>`, `installers/aur/PKGBUILD` and `.SRCINFO`), then merge it into `main` (pull request or
  local merge + push). The "Release Flatpak" GitHub Action (`.github/workflows/release-flatpak.yml`) runs
  on every push to `main`: it creates the tag `v<version>` and its GitHub release if they don't exist
  yet, builds the Flatpak and attaches it. No tag needs to be pushed by hand. A push to `main` without a
  version bump releases nothing.
  `installers/build-flatpak.sh` builds the same bundle locally.
- Ask before every commit, merge, tag and push.
