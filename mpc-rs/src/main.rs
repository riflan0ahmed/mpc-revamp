mod cmdline;
mod commands;
mod config;
mod mpris;
mod options;
mod playlist;
mod playlistbar;
mod subresync;
mod subtitles;
mod ui;
mod video;

use gtk4::{self as gtk, gio, glib, prelude::*};
use libmpv2::Mpv;

fn main() -> glib::ExitCode {
    // Switches that change how the process starts are handled here; the rest go to the (maybe already
    // running) player through GApplication's command line forwarding.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    let local = match cmdline::parse(&args, &cwd) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}\n\n{}", cmdline::USAGE);
            return glib::ExitCode::FAILURE;
        }
    };
    for s in &local.ignored {
        eprintln!("{s} is not supported, ignored");
    }
    if local.help {
        println!("{}", cmdline::USAGE);
        return glib::ExitCode::SUCCESS;
    }
    if local.reset {
        config::save(&config::Config::default());
    }
    let mut flags = gio::ApplicationFlags::HANDLES_OPEN | gio::ApplicationFlags::HANDLES_COMMAND_LINE;
    if local.new || config::load().allow_multiple_instances {
        flags |= gio::ApplicationFlags::NON_UNIQUE;
    }
    // GApplication would try to parse "-x" / "--x" as its own options; pass every switch as "/x".
    let forwarded: Vec<String> = std::iter::once(std::env::args().next().unwrap_or_default())
        .chain(args.iter().map(|a| match a.strip_prefix("--").or_else(|| a.strip_prefix('-')) {
            Some(name) if !name.is_empty() && !a.starts_with("-/") => format!("/{name}"),
            _ => a.clone(),
        }))
        .collect();

    // ponytail: mpv lives for the whole process; leaked so RenderContext<'static> can live in GTK closures.
    let mpv: &'static Mpv = Box::leak(Box::new(
        Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            init.set_property("idle", "yes")?;
            // MPC "after playback: do nothing" = stay on the last frame.
            init.set_property("keep-open", "yes")?;
            init.set_property("volume-max", "100")?;
            // Room for MPC's volume boost (up to +300% = +12.04 dB).
            init.set_property("volume-gain-max", "13")?;
            // Auto-loaded subtitles are also looked for in these subfolders (MPC's default paths).
            init.set_property("sub-file-paths", "subtitles:subs")?;
            init.set_property("terminal", "yes")?;
            // Debug knob: MPC_RS_MPV_LOG=all=v (any mpv --msg-level value).
            init.set_property("msg-level", std::env::var("MPC_RS_MPV_LOG").unwrap_or("all=warn".into()))?;
            // ponytail: WSLg's llvmpipe renders black with mpv's default rgba16f FBO. Real GPUs keep the default.
            if std::env::var_os("WSL_DISTRO_NAME").is_some() {
                init.set_property("fbo-format", "rgba8")?;
            }
            Ok(())
        })
        .expect("failed to init mpv"),
    ));

    let app = gtk::Application::builder()
        .application_id("io.github.mpcrs.Player")
        .flags(flags)
        .build();

    app.connect_activate(move |app| ui::window(app, mpv));
    app.connect_open(move |app, files, _| {
        ui::window(app, mpv);
        ui::open_files(files, false);
    });

    app.connect_command_line(move |app, cl| {
        let args: Vec<String> = cl.arguments().iter().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
        let cwd = cl.cwd().unwrap_or_default();
        // Already checked (and warnings printed) by the launching process, so errors can't happen here.
        match cmdline::parse(&args, &cwd) {
            Ok(c) => {
                ui::command_line(app, mpv, c);
                glib::ExitCode::SUCCESS
            }
            Err(_) => glib::ExitCode::FAILURE,
        }
    });

    let code = app.run_with_args(&forwarded);
    // Render context was freed on GLArea unrealize, so mpv can be destroyed now.
    unsafe { drop(Box::from_raw(mpv as *const Mpv as *mut Mpv)) };
    code
}
