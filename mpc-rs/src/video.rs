//! The video area: a GTK GLArea that mpv's OpenGL render API draws into.

use gtk4::{self as gtk, glib, prelude::*};
use libmpv2::{
    Mpv,
    render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType},
};
use std::{
    cell::RefCell,
    ffi::{CString, c_char, c_void},
    rc::Rc,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

// ponytail: EGL only (GTK 4.14 uses EGL on both Wayland and X11). Add glXGetProcAddress if a GLX-only setup shows up.
#[link(name = "EGL")]
unsafe extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
}

fn gl_proc(_: &(), name: &str) -> *mut c_void {
    let name = CString::new(name).unwrap();
    unsafe { eglGetProcAddress(name.as_ptr()) }
}

/// GTK renders GLArea into its own FBO, so ask GL which one is bound.
fn current_fbo() -> i32 {
    static GET_INTEGERV: OnceLock<unsafe extern "C" fn(u32, *mut i32)> = OnceLock::new();
    let f = GET_INTEGERV.get_or_init(|| unsafe { std::mem::transmute(gl_proc(&(), "glGetIntegerv")) });
    let mut fbo = 0;
    unsafe { f(0x8CA6 /* GL_FRAMEBUFFER_BINDING */, &mut fbo) };
    fbo
}

pub fn new(mpv: &'static Mpv) -> gtk::GLArea {
    let area = gtk::GLArea::new();
    area.set_auto_render(false);
    // Desktop GL: mpv's GLES path loses chroma on llvmpipe, and desktop GL is mpv's best-tested path anyway.
    area.set_allowed_apis(gtk::gdk::GLAPI::GL);
    area.set_hexpand(true);
    area.set_vexpand(true);
    let render_ctx: Rc<RefCell<Option<RenderContext<'static>>>> = Rc::default();
    let dirty = Arc::new(AtomicBool::new(false));

    area.connect_realize({
        let (render_ctx, dirty) = (render_ctx.clone(), dirty.clone());
        move |area| {
            area.make_current();
            if let Some(e) = area.error() {
                eprintln!("GL context error: {e}");
                return;
            }
            let mut ctx = mpv
                .create_render_context(vec![
                    RenderParam::ApiType(RenderParamApiType::OpenGl),
                    RenderParam::InitParams(OpenGLInitParams { get_proc_address: gl_proc, ctx: () }),
                ])
                .expect("failed to create mpv render context");
            let dirty = dirty.clone();
            ctx.set_update_callback(move || dirty.store(true, Ordering::Relaxed));
            *render_ctx.borrow_mut() = Some(ctx);
        }
    });

    area.connect_unrealize({
        let render_ctx = render_ctx.clone();
        move |area| {
            area.make_current();
            render_ctx.borrow_mut().take();
        }
    });

    area.connect_render(move |area, _| {
        if let Some(ctx) = &*render_ctx.borrow() {
            let s = area.scale_factor();
            if let Err(e) = ctx.render::<()>(current_fbo(), area.width() * s, area.height() * s, true) {
                eprintln!("mpv render: {e}");
            }
        }
        glib::Propagation::Stop
    });

    // mpv's update callback runs on its own thread; pick the flag up on the GTK frame clock.
    // ponytail: tick callback keeps the frame clock running even when idle; switch to a glib channel if idle CPU matters.
    area.add_tick_callback(move |area, _| {
        if dirty.swap(false, Ordering::Relaxed) {
            area.queue_render();
        }
        glib::ControlFlow::Continue
    });

    area
}
