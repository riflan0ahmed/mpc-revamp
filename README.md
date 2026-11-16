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
