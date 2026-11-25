//! MPC-RV for Windows (TODO.md Phase 11). For now a starting point: it plays the files from the
//! command line in mpv's own window, with the user's settings. The native Win32 window, menus and
//! dialogs replace mpv's window and controls from Phase 11.2 on.

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use libmpv2::events::Event;
    use mpc_core::{cmdline, config, player};
    use std::process::ExitCode;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    let c = match cmdline::parse(&args, &cwd) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}\n\n{}", cmdline::USAGE);
            return ExitCode::FAILURE;
        }
    };
    if c.help || c.files.is_empty() {
        println!("{}", cmdline::USAGE);
        return ExitCode::SUCCESS;
    }
    let mpv = match player::create("gpu-next") {
        Ok(m) => m,
        Err(e) => {
            eprintln!("mpv: {e}");
            return ExitCode::FAILURE;
        }
    };
    // mpv's own window, keys and on-screen controller stand in for the app's until Phase 11.2.
    for name in ["force-window", "input-default-bindings", "input-vo-keyboard", "osc"] {
        player::set(&mpv, name, "yes");
    }
    let cfg = config::load();
    player::apply(&mpv, &player::playback_options(&cfg));
    player::apply(&mpv, &player::audio_options(&cfg));
    player::apply(&mpv, &player::sub_style_options(&cfg));
    player::apply(&mpv, &player::video_options(&cfg, 0.0));
    player::apply(&mpv, &player::frame_options(cfg.video_frame));
    player::apply(&mpv, &[("volume", cfg.volume.to_string())]);
    for f in &c.files {
        player::cmd(&mpv, "loadfile", &[f, "append-play"]);
    }
    // Closing mpv's window quits mpv, which ends the program.
    loop {
        if let Some(Ok(Event::Shutdown)) = mpv.wait_event(-1.0) {
            return ExitCode::SUCCESS;
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("mpc-rv-windows is the Windows app. On Linux, run mpc-rv.");
    std::process::exit(1);
}
