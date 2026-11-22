<p align="center">
  <img src="mpc-rv/res/icons/128.png" width="96" alt="mpc-revamp icon">
</p>

<h1 align="center">mpc-revamp</h1>

<p align="center">
  <img src="https://img.shields.io/badge/status-unofficial%20rewrite-e8622a" alt="Unofficial rewrite">
  <img src="https://img.shields.io/badge/platform-Linux%20first-555" alt="Linux first">
  <img src="https://img.shields.io/badge/built%20with-Rust%20%2B%20libmpv%20%2B%20GTK4-b7410e" alt="Rust, libmpv, GTK4">
  <img src="https://img.shields.io/badge/license-GPL--3.0--or--later-blue" alt="GPL-3.0-or-later">
</p>

<p align="center">
  A media player for Linux in the spirit of Media Player Classic - Home Cinema,<br>
  rewritten from scratch in Rust. The program is called <code>mpc-rv</code>.
</p>

> [!NOTE]
> **mpc-revamp is a vibe-coded revamp of MPC-HC, written in Rust.** It gives you almost the same
> experience as MPC-HC, with some small UI improvements. It's an unofficial project, not made by
> the MPC-HC developers.
>
> Its code was written with an AI coding assistant, guided and tested by its developer. Bug
> reports, fixes and reviews from people who know MPC-HC, GTK or mpv are especially welcome.
>
> Thanks to the MPC-HC developers for the original player.
>
> **Looking for MPC-HC itself on Windows?** Get it from [clsid2/mpc-hc](https://github.com/clsid2/mpc-hc),
> the actively maintained version.

## Status

Version 1.0.0 for Linux, under active development. See [TODO.md](TODO.md) for what's done and
what's planned.

## Install (Linux)

| Package | Distributions | Status |
|---|---|---|
| Flatpak | Any Linux distribution | Ready (1.0.0) |
| AUR (`mpc-rv`) | Arch Linux, Manjaro, EndeavourOS | Coming soon (build it from this repo now) |
| `.deb` | Debian, Ubuntu, Linux Mint | Not ready yet |
| AppImage | Any Linux distribution | Not ready yet |

Until the `.deb` and AppImage are ready, use the Flatpak on Debian, Ubuntu and Mint, or
[build from source](#building-linux).

### Flatpak

Download `mpc-rv-<version>-x86_64.flatpak` from the
[latest release](https://github.com/riflan0ahmed/mpc-revamp/releases/latest), then install and run it:

```sh
flatpak install --user mpc-rv-1.0.0-x86_64.flatpak
flatpak run io.github.mpcrv.Player /path/to/video.mkv
```

Flatpak fetches the GNOME runtime from Flathub if you don't have it yet. After that, mpc-rv
shows up in your applications menu. To update, install the newer file the same way. To remove it:
`flatpak uninstall io.github.mpcrv.Player`.

> [!TIP]
> If you just installed Flatpak for the first time, log out and back in (or restart). Until then,
> mpc-rv doesn't show up in your applications menu and has no icon.

Differences in the Flatpak version:

- Settings are kept in `~/.var/app/io.github.mpcrv.Player/config/mpc-rv/`, not `~/.config/mpc-rv/`.
- The JSON control socket is created inside the sandbox, so programs outside it can't reach it.
  Use MPRIS (media keys, `playerctl`) instead.
- Options > Formats and `/regall` set file associations inside the sandbox only. The package
  already registers mpc-rv for video and audio files, so use your desktop's "Open With" to make
  it the default player.
- Navigate > Tuner Scan can't run your system's `dvbv5-scan`. Run the scan in a terminal
  instead (`dvbv5-scan -o channels.conf -O ZAP <scan table>`) and copy the resulting
  `channels.conf` into the settings folder above.

### Arch Linux (AUR)

The `mpc-rv` package isn't on the AUR yet. Until it is, build and install it from the PKGBUILD
in this repository:

```sh
git clone https://github.com/riflan0ahmed/mpc-revamp.git
cd mpc-revamp/packaging/aur
makepkg -si
```

It builds from the release tag and uses Arch's own GTK4 and mpv, so none of the Flatpak
differences above apply.

## Building (Linux)

Install the dependencies (Debian/Ubuntu shown; GTK 4.12 or newer is required):

```sh
sudo apt install build-essential pkg-config libmpv-dev libgtk-4-dev
curl https://sh.rustup.rs -sSf | sh
```

Then build and run:

```sh
cd mpc-rv
cargo run --release -- /path/to/video.mkv
```

If the player is already running, opening another file sends it to that window.

### Building the Flatpak package

Install `flatpak` and `flatpak-builder`, then run from the repository root:

```sh
packaging/build-flatpak.sh
```

The script builds `dist/mpc-rv-<version>-x86_64.flatpak`. It bundles mpc-rv with its own libmpv
and FFmpeg. The first run downloads the GNOME SDK (a few GB) and compiles FFmpeg and libmpv. Later
runs reuse that work.

## Supported formats

Playback is done by mpv/FFmpeg, so mpc-revamp plays almost anything FFmpeg can decode. The extensions
below are the ones it recognizes when it opens a folder, picks the next file in a folder, or sets
up file associations (Options > Formats). A file with another extension still plays when you open
it directly or drop it on the window.

**Video:** `.3g2` `.3gp` `.asf` `.avi` `.divx` `.dv` `.evo` `.f4v` `.flv` `.h264` `.h265` `.hevc`
`.m1v` `.m2t` `.m2ts` `.m2v` `.m4v` `.mkv` `.mov` `.mp4` `.mpe` `.mpeg` `.mpg` `.mts` `.mxf` `.ogm`
`.ogv` `.rm` `.rmvb` `.ts` `.vob` `.webm` `.wmv` `.y4m`

**Audio:** `.aac` `.ac3` `.aif` `.aiff` `.alac` `.amr` `.ape` `.au` `.dts` `.flac` `.m4a` `.m4b`
`.mid` `.midi` `.mka` `.mp3` `.mpc` `.oga` `.opus` `.ra` `.ram` `.tak` `.tta` `.wav` `.wma` `.wv`

**Video or audio:** `.mp2` `.mpa` `.ogg`

**Playlists:** `.mpcpl` `.m3u` `.m3u8` `.pls` `.asx` `.cue` (a cue sheet plays as one item with a
chapter per track)

**Subtitles:** `.srt` `.ass` `.ssa` `.sub` `.idx` `.sup` `.vtt` `.smi` `.usf`. Subtitles named like
the video are loaded automatically, also from `subs/` and `subtitles/` folders next to it.

**Also:** DVD and Blu-ray folders (`VIDEO_TS` / `BDMV`) and `.iso` images, audio CDs, capture
devices (v4l2), and network streams (`http://`, `https://` and anything else mpv understands).
Encrypted DVDs need `libdvdcss`. DVD and Blu-ray menus aren't supported (mpv plays the titles
directly).

## Tests

```sh
cd mpc-rv
cargo test
```

Unit tests sit next to the code they test (`#[cfg(test)] mod tests` at the bottom of a file) and cover
private details. Integration tests in `mpc-rv/tests/` use the crate's public API, the way other code
would: playlists and their file formats, subtitles, settings and history, the command line, key
bindings, discs and TV channels, desktop integration. The app itself is a thin `main.rs` over the
`mpc_rv` library (`src/lib.rs`), so both kinds of tests can reach the code.

## Repository layout

| Path | Contents |
|---|---|
| `mpc-rv/` | The Rust project (the `mpc-rv` program) |
| `packaging/` | The Flatpak recipe and build script, the AUR `PKGBUILD`, desktop entry and AppStream metadata |
| `TODO.md` | Roadmap and feature checklist, phase by phase |
| `CLAUDE.md` | Project notes and conventions |

The original MPC-HC source is used as a reference while porting, but it isn't part of this repo.
To look at it the same way, clone it into `mpc-hc-develop/` (that folder is gitignored):

```sh
git clone --depth 1 -b develop https://github.com/mpc-hc/mpc-hc.git mpc-hc-develop
```

## Contributing

Developers are warmly welcome. You can help in many ways:

- **Support:** answer questions and help other users in
  [Issues](https://github.com/riflan0ahmed/mpc-revamp/issues).
- **Bug fixes:** pick a reported bug, or report one you found, with steps to reproduce it.
- **New features:** pick an unchecked item from [TODO.md](TODO.md), or open an issue to suggest one.
- **Packaging:** the `.deb` and AppImage packages aren't ready yet, and help with them is welcome.

How to send a change:

1. Branch off the current `release/<version>` branch (for example `release/1.0.1`), not `main`.
   Name your branch `feature/<name>` for new work or `fix/<name>` for a bug fix.
2. Open a pull request against that `release/<version>` branch.
3. The maintainer reviews it and merges it. When a release is ready, it's merged into `main`
   and published.

Small, focused changes are the easiest to review.

## Credits

mpc-revamp follows MPC-HC's design closely, and reuses some of its GPL material:

- the command table: command IDs, names and default key bindings (`default_wmcmds`)
- the translations (MPC-HC's PO files, 42 languages)
- its pixel shaders, ported from HLSL to mpv's GLSL format
- setting names and defaults, menu layout, command line switches and the `MpcApi.h` command names
- its "321" icon, with a yellow "REVAMP" badge added so the two players can't be confused

The idle-screen logo shows that badged icon next to the program's name, mpc-rv, a "RUST REVAMP" badge and an
"Unofficial Media Player Classic rewrite" line. `mpc-rv/res/make_art.py` builds both.

Thanks to the MPC-HC developers and translators.

## License

GPL-3.0-or-later, the same as MPC-HC. See [LICENSE](LICENSE).

"Media Player Classic" and "MPC-HC" are used only to describe what this project is inspired by.
mpc-revamp isn't affiliated with or endorsed by the MPC-HC project.
