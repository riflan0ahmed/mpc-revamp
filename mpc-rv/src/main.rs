use mpc_rv::{cmdline, config, i18n, integration, player, ui};
use gtk4::{self as gtk, gio, glib, prelude::*};
use libmpv2::Mpv;

fn main() -> glib::ExitCode {
    // Switches that change how the process starts are handled here; the rest go to the (maybe already
    // running) player through GApplication's command line forwarding.
    config::migrate_old_dir();
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
    // File associations, like MPC: done here, then the player only starts if there's something to play.
    let (v, a, p) = local.register;
    if v || a || p || local.unregister {
        use integration::Kind;
        if local.unregister {
            println!("Removed {} file associations.", integration::unassociate());
        }
        let kinds: Vec<Kind> = [(v, Kind::Video), (a, Kind::Audio), (p, Kind::Playlist)].into_iter().filter(|(on, _)| *on).map(|(_, k)| k).collect();
        if !kinds.is_empty() {
            match integration::associate(&kinds) {
                Ok(n) => println!("MPC-RV now opens {n} file types."),
                Err(e) => eprintln!("File associations: {e}"),
            }
        }
        if local.files.is_empty() {
            return glib::ExitCode::SUCCESS;
        }
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
    let mpv: &'static Mpv = Box::leak(Box::new(player::create("libmpv").expect("failed to init mpv")));

    i18n::init(&config::load().language);
    let app = gtk::Application::builder()
        .application_id(integration::APP_ID)
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
