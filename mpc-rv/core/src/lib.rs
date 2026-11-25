//! mpc-core: the parts of mpc-rv that don't depend on a UI toolkit, shared by every frontend
//! (GTK on Linux now, a native one on Windows later). Nothing here may use GTK or a platform API.

pub mod cmdline;
pub mod commands;
pub mod config;
pub mod i18n;
pub mod paths;
pub mod playlist;
pub mod subtitles;
