//! mpc-rv: Media Player Classic - Home Cinema, rewritten in Rust (GTK4 + libmpv).
//!
//! The binary (`main.rs`) only starts the app; everything else lives in this library, so the
//! integration tests in `tests/` can use it. The UI-free modules (settings, playlist, command line,
//! ...) are in the `mpc-core` crate (`core/`), re-exported here under their old names.

pub use mpc_core::{cmdline, commands, config, i18n, player, playlist, subtitles};

pub mod dialogs;
pub mod dvb;
pub mod integration;
pub mod ui;
pub mod view;

mod mpris;
mod options;
mod playlistbar;
mod remote;
mod shaders;
mod subresync;
mod video;
