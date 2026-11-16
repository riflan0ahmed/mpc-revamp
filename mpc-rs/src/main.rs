mod ui;
mod video;

use gtk4::{self as gtk, gio, glib, prelude::*};
use libmpv2::Mpv;

fn main() -> glib::ExitCode {
    // ponytail: mpv lives for the whole process; leaked so RenderContext<'static> can live in GTK closures.
    let mpv: &'static Mpv = Box::leak(Box::new(
        Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            init.set_property("idle", "yes")?;
            // MPC "after playback: do nothing" = stay on the last frame.
            init.set_property("keep-open", "yes")?;
            init.set_property("volume-max", "100")?;
            init.set_property("terminal", "yes")?;
            // Debug knob: MPC_RS_MPV_LOG=all=v (any mpv --msg-level value).
            init.set_property("msg-level", std::env::var("MPC_RS_MPV_LOG").unwrap_or("all=warn".into()))?;
            // ponytail: WSLg's llvmpipe renders black with mpv's default rgba16f FBO. Real GPUs keep the default.
            if std::env::var_os("WSL_DISTRO_NAME").is_some() {
                init.set_property("fbo-format", "rgba8")?;            }
            Ok(())
        })
        .expect("failed to init mpv"),
    ));

    let app = gtk::Application::builder()
        .application_id("io.github.mpcrs.Player")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_activate(move |app| ui::window(app, mpv));
    app.connect_open(move |app, files, _| {
        ui::window(app, mpv);
        ui::open_files(mpv, files, false);
    });

    let code = app.run();
    // Render context was freed on GLArea unrealize, so mpv can be destroyed now.
    unsafe { drop(Box::from_raw(mpv as *const Mpv as *mut Mpv)) };
    code
}
