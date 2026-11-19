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

> [!IMPORTANT]
> **mpc-revamp is not MPC-HC.** It's an independent, unofficial project with its own code base. It isn't
> affiliated with or endorsed by the MPC-HC developers.
>
> - **For MPC-HC users:** mpc-revamp copies MPC-HC's menus, keyboard shortcuts, Options pages and command
>   line switches so it feels familiar, but it isn't a drop-in replacement. Playback is done by
>   [libmpv](https://mpv.io/) instead of DirectShow/LAV Filters, so MPC-HC's renderers, filters,
>   skins and settings (registry or `.ini`) don't carry over, and some Windows-only features are
>   dropped (see "Dropped" in [TODO.md](TODO.md)).
> - **For MPC-HC developers:** none of MPC-HC's C++ code runs here. The MPC-HC source was used as a
>   reference, and some of its GPL assets are reused (see [Credits](#credits)). Please report mpc-revamp
>   problems in this repository, not to MPC-HC.
> - **Looking for MPC-HC itself on Windows?** See the original
>   [mpc-hc/mpc-hc](https://github.com/mpc-hc/mpc-hc) and its actively maintained continuation,
>   [clsid2/mpc-hc](https://github.com/clsid2/mpc-hc).

## Status

Linux, under active development. Done so far (details in [TODO.md](TODO.md)): playback with a
GTK4 interface, MPC's command table and remappable keys, playlist, settings and command line,
tracks/audio/subtitles with a subresync bar, video zoom/pan&scan/shaders, DVD/Blu-ray/audio CD,
capture devices, favorites, MPRIS and a JSON control socket, file associations, translations, and
DVB TV (untested). Still to do: the web remote, online subtitles and the Windows port.

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

## Credits

mpc-revamp follows MPC-HC's design closely, and reuses some of its GPL material:

- the command table: command IDs, names and default key bindings (`default_wmcmds`)
- the translations (MPC-HC's PO files, 42 languages)
- its pixel shaders, ported from HLSL to mpv's GLSL format
- setting names and defaults, menu layout, command line switches and the `MpcApi.h` command names
- its "321" icon, with a yellow "REVAMP" badge added so the two players can't be confused

The idle-screen logo shows that badged icon next to the program's name, mpc-rv,, a "RUST REVAMP" badge and an
"Unofficial Media Player Classic rewrite" line. `mpc-rv/res/make_art.py` builds both.

Thanks to the MPC-HC developers and translators.

## License

GPL-3.0-or-later, the same as MPC-HC. See [LICENSE](LICENSE).

"Media Player Classic" and "MPC-HC" are used only to describe what this project is inspired by.
mpc-revamp isn't affiliated with or endorsed by the MPC-HC project.
