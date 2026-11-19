# mpc-rs

A rewrite of [MPC-HC](https://github.com/mpc-hc/mpc-hc) (Media Player Classic - Home Cinema) in Rust.
It targets **Linux first**, with Windows support planned later.

The goal is MPC-HC's feature set and feel on Linux: the classic menus and controls, playlist,
key bindings, subtitles, DVD/Blu-ray, capture devices, DVB, the web remote and more. Playback is
handled by [libmpv](https://mpv.io/) and the UI by [GTK4](https://gtk-rs.org/).

> **Status: early development.** Right now it opens a window and plays a file passed on the
> command line. See [TODO.md](TODO.md) for the roadmap and progress.

## Building (Linux)

Install the dependencies (Debian/Ubuntu shown; GTK 4.12 or newer is required):

```sh
sudo apt install build-essential pkg-config libmpv-dev libgtk-4-dev
curl https://sh.rustup.rs -sSf | sh
```

Then build and run:

```sh
cd mpc-rs
cargo run --release -- /path/to/video.mkv
```

If the player is already running, opening another file sends it to that window.

## Supported formats

Playback is done by mpv/FFmpeg, so mpc-rs plays almost anything FFmpeg can decode. The extensions
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
cd mpc-rs
cargo test
```

Unit tests sit next to the code they test (`#[cfg(test)] mod tests` at the bottom of a file) and cover
private details. Integration tests in `mpc-rs/tests/` use the crate's public API, the way other code
would: playlists and their file formats, subtitles, settings and history, the command line, key
bindings, discs and TV channels, desktop integration. The app itself is a thin `main.rs` over the
`mpc_rs` library (`src/lib.rs`), so both kinds of tests can reach the code.

## Repository layout

| Path | Contents |
|---|---|
| `mpc-rs/` | The Rust project |
| `TODO.md` | Roadmap and feature checklist, phase by phase |
| `CLAUDE.md` | Project notes and conventions (also used by Claude Code) |

The original MPC-HC source is used as a reference while porting, but it isn't part of this repo.
To look at it the same way, clone it into `mpc-hc-develop/` (that folder is gitignored):

```sh
git clone --depth 1 -b develop https://github.com/mpc-hc/mpc-hc.git mpc-hc-develop
```

## Contributing

Pick an unchecked item from [TODO.md](TODO.md), ideally in the current phase, and open a pull request.
Keep changes small and focused.

## License

GPL-3.0-or-later, the same as MPC-HC. See [LICENSE](LICENSE).

mpc-rs isn't affiliated with the MPC-HC project.
