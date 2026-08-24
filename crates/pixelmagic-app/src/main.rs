//! Pixelmagic — a GTK4-native image editor for Linux.

mod brush;
mod canvas;
mod state;
mod ui;
mod window;

use adw::prelude::*;
use gtk::gio;
use gtk::glib;
use pixelmagic_core::document::Document;
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "dev.pixelmagic.Pixelmagic";

/// Minimal styling. Everything else comes from libadwaita, so Pixelmagic
/// follows the user's system theme instead of imposing its own — which is the
/// point of being GTK-native rather than a port.
const CSS: &str = "
.tool-button {
    min-width: 40px;
    min-height: 34px;
    padding: 0;
    font-size: 15px;
}
.tools-sidebar {
    background: @sidebar_bg_color;
}
.layers-sidebar {
    background: @sidebar_bg_color;
}
.param-row {
    margin: 2px 0;
}
.histogram {
    border-radius: 6px;
    margin: 4px 0;
}
";

fn main() -> glib::ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("pixelmagic=info,warn"),
    )
    .init();

    // `--check-shaders` compiles the whole shader library against a headless
    // context and exits. It is what CI runs: a GLSL typo then fails the build
    // rather than surfacing as a blank panel for a user.
    if std::env::args().any(|a| a == "--check-shaders") {
        return check_shaders();
    }

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    // Accelerators. The action exists whether or not a key is bound to it, so
    // forgetting this table is a silent failure: the menu item works and the
    // shortcut quietly does nothing. Bindings follow SPEC §5's Mac shortcuts
    // with Command mapped to Control, which is what a Linux user expects.
    const ACCELS: &[(&str, &[&str])] = &[
        ("win.new", &["<Control>n"]),
        ("win.open", &["<Control>o"]),
        ("win.save", &["<Control>s"]),
        ("win.export", &["<Control>e"]),
        ("win.undo", &["<Control>z"]),
        ("win.redo", &["<Control><Shift>z", "<Control>y"]),
        ("win.zoom-fit", &["<Control>0"]),
        ("win.zoom-actual", &["<Control>1"]),
        ("win.zoom-in", &["<Control>plus", "<Control>equal"]),
        ("win.zoom-out", &["<Control>minus"]),
        ("win.select-all", &["<Control>a"]),
        ("win.deselect", &["<Control>d"]),
        ("win.select-invert", &["<Control><Shift>i"]),
        ("win.layer-new", &["<Control><Shift>n"]),
        ("win.layer-duplicate", &["<Control>j"]),
        ("win.layer-group", &["<Control>g"]),
    ];

    app.connect_startup(|_| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(CSS);
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });

    let opened: Rc<RefCell<Option<Rc<window::Window>>>> = Rc::new(RefCell::new(None));

    for (action, keys) in ACCELS {
        app.set_accels_for_action(action, keys);
    }

    {
        let opened = opened.clone();
        app.connect_activate(move |app| {
            if let Some(w) = opened.borrow().as_ref() {
                w.window.present();
                return;
            }
            let w = window::Window::new(app, Document::new(1920, 1080));
            w.window.present();
            *opened.borrow_mut() = Some(w);
        });
    }

    {
        let opened = opened.clone();
        app.connect_open(move |app, files, _hint| {
            let w = match opened.borrow().as_ref() {
                Some(w) => w.clone(),
                None => {
                    let w = window::Window::new(app, Document::new(1920, 1080));
                    w.window.present();
                    w
                }
            };
            *opened.borrow_mut() = Some(w.clone());
            if let Some(path) = files.first().and_then(|f| f.path()) {
                w.open_path(&path);
            }
            w.window.present();
        });
    }

    app.run()
}

fn check_shaders() -> glib::ExitCode {
    match pixelmagic_gpu::headless::HeadlessContext::new() {
        Ok(ctx) => {
            println!("GL context: {}", ctx.describe());
            if let Ok(r) = pixelmagic_gpu::Renderer::new(ctx.gl.clone(), ctx.flavor) {
                println!("capabilities: {}", r.capabilities().describe());
            }
            match pixelmagic_gpu::Renderer::new(ctx.gl.clone(), ctx.flavor)
                .and_then(|mut r| r.precompile())
            {
                Ok(n) => {
                    println!("all {n} shaders compiled (fragment + compute)");
                    glib::ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("shader compilation failed:\n{e}");
                    glib::ExitCode::FAILURE
                }
            }
        }
        Err(e) => {
            eprintln!("no headless GL context available: {e}");
            glib::ExitCode::FAILURE
        }
    }
}
