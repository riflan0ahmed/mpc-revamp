//! mpc-rs: Media Player Classic - Home Cinema, rewritten in Rust (GTK4 + libmpv).
//!
//! The binary (`main.rs`) only starts the app; everything else lives in this library, so the
//! integration tests in `tests/` can use it. The public modules are the parts with no UI of their own
//! (formats, settings, command line, ...) plus `ui`, which `main` starts.

pub mod cmdline;
pub mod commands;
pub mod config;
pub mod dialogs;
pub mod dvb;
pub mod i18n;
pub mod integration;
pub mod playlist;
pub mod subtitles;
pub mod ui;
pub mod view;

mod mpris;
mod options;
mod playlistbar;
mod remote;
mod shaders;
mod subresync;
mod video;
