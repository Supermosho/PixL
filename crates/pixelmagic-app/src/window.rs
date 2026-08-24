//! The main window.
//!
//! ## Layout
//!
//! The original is a dark canvas with floating panels laid over it, and the
//! arrangement is specific: Layers on the **left**, the active tool's options on
//! the **right**, and the tool rail on the **far right**, outboard of them.
//! Pixelmagic originally had the rail on the left and layers on the right —
//! the GIMP/Photoshop convention, and not this one.
//!
//! Floating means a `GtkOverlay`, not a `GtkPaned`. Panes give you dividers and
//! flush edges; these panels sit *above* the canvas with a margin, rounded
//! corners and a shadow, and the canvas runs full-bleed underneath them.

use adw::prelude::*;
use gtk::gio;
use gtk::glib;
use pixelmagic_core::document::Document;
use pixelmagic_core::layer::LayerKind;
use pixelmagic_core::tool::Tool;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::canvas::Canvas;
use crate::state::EditorState;
use crate::style::metrics;

/// Height of the info strip, which the document should not sit under.
const STATUS_HEIGHT: i32 = 22;
use crate::ui::{
    build_adjustments_panel, build_arrange_panel, build_effects_panel,
    build_quick_select_panel, build_tool_options, LayersSidebar, ToolRail,
};

pub struct Window {
    pub window: adw::ApplicationWindow,
    state: Rc<RefCell<EditorState>>,
    canvas: Rc<Canvas>,
    layers: Rc<LayersSidebar>,
    rail: Rc<ToolRail>,
    /// Body of the right-hand panel, rebuilt when the active tool changes.
    options_body: gtk::Box,
    options_title: gtk::Label,
    layers_panel: gtk::Box,
    options_panel: gtk::Box,
    status: gtk::Label,
    title: gtk::Label,
    subtitle: gtk::Label,
    zoom: gtk::Scale,
    /// Guards the zoom slider against writing back the value we just set on it.
    syncing: std::cell::Cell<bool>,
}

impl Window {
    pub fn new(app: &adw::Application, document: Document) -> Rc<Self> {
        let state = EditorState::shared(document);
        let canvas = Canvas::new(state.clone());
        let layers = LayersSidebar::new(state.clone());
        let rail = ToolRail::new(state.clone(), {
            let state = state.clone();
            move |tool| state.borrow_mut().tool = tool
        });

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .default_width(1600)
            .default_height(1000)
            .title("Pixelmagic")
            .build();
        window.add_css_class("pixelmagic");

        // -- toolbar ---------------------------------------------------------
        let header = adw::HeaderBar::new();
        header.add_css_class("pm-toolbar");
        // Window controls on the left, as in the original. The buttons stay
        // GTK's own — drawing macOS traffic lights inside a GTK app would look
        // alien on a Linux desktop and would ignore the user's preference,
        // which belongs to their window manager.
        header.set_decoration_layout(Some("close,minimize,maximize:"));

        let title = gtk::Label::new(Some("Untitled"));
        title.add_css_class("pm-doc-title");
        let subtitle = gtk::Label::new(Some(""));
        subtitle.add_css_class("pm-doc-subtitle");
        let title_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        title_box.set_valign(gtk::Align::Center);
        title_box.append(&title);
        title_box.append(&subtitle);
        header.set_title_widget(Some(&title_box));

        let sidebar_toggle = gtk::Button::from_icon_name("sidebar-show-symbolic");
        sidebar_toggle.add_css_class("flat");
        sidebar_toggle.set_tooltip_text(Some("Show or hide the Layers panel"));
        header.pack_start(&sidebar_toggle);

        let (zoom_box, zoom) = build_zoom_control();
        header.pack_start(&zoom_box);

        let menu_button = gtk::MenuButton::new();
        menu_button.set_icon_name("view-more-symbolic");
        menu_button.add_css_class("flat");
        menu_button.set_menu_model(Some(&build_menu()));
        header.pack_end(&menu_button);

        let share = gtk::Button::from_icon_name("document-send-symbolic");
        share.add_css_class("flat");
        share.set_tooltip_text(Some("Export…  (Ctrl+E)"));
        share.set_action_name(Some("win.export"));
        header.pack_end(&share);

        let info = gtk::Button::from_icon_name("dialog-information-symbolic");
        info.add_css_class("flat");
        info.set_tooltip_text(Some("About Pixelmagic"));
        info.set_action_name(Some("win.about"));
        header.pack_end(&info);

        // -- floating panels -------------------------------------------------
        let layers_panel = panel_shell(&layers.widget);
        layers_panel.set_size_request(metrics::LAYERS_WIDTH, -1);
        layers_panel.set_halign(gtk::Align::Start);
        layers_panel.set_valign(gtk::Align::Fill);
        set_margins(&layers_panel, metrics::PANEL_MARGIN);

        let options_title = gtk::Label::new(Some("Arrange"));
        options_title.add_css_class("pm-panel-title");
        options_title.set_xalign(0.0);
        let options_body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        options_body.set_vexpand(true);

        let options_inner = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        head.add_css_class("pm-panel-header");
        head.append(&options_title);
        options_inner.append(&head);
        options_inner.append(&options_body);

        let options_panel = panel_shell(&options_inner);
        options_panel.set_size_request(metrics::OPTIONS_WIDTH, -1);
        options_panel.set_halign(gtk::Align::End);
        options_panel.set_valign(gtk::Align::Fill);
        set_margins(&options_panel, metrics::PANEL_MARGIN);
        // Sits inboard of the rail rather than underneath it.
        options_panel.set_margin_end(metrics::RAIL_WIDTH + metrics::PANEL_MARGIN * 2);

        rail.widget.set_halign(gtk::Align::End);
        rail.widget.set_valign(gtk::Align::Center);
        set_margins(&rail.widget, metrics::PANEL_MARGIN);

        let status = gtk::Label::new(Some(""));
        status.add_css_class("pm-status");
        status.set_xalign(0.0);
        status.set_halign(gtk::Align::Start);
        status.set_valign(gtk::Align::End);
        status.set_margin_start(metrics::LAYERS_WIDTH + metrics::PANEL_MARGIN * 2);

        let overlay = gtk::Overlay::new();
        overlay.set_vexpand(true);
        overlay.add_css_class("main-area");
        overlay.set_child(Some(&canvas.widget));
        overlay.add_overlay(&status);
        overlay.add_overlay(&layers_panel);
        overlay.add_overlay(&options_panel);
        overlay.add_overlay(&rail.widget);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&header);
        content.append(&overlay);
        window.set_content(Some(&content));

        let win = Rc::new(Window {
            window,
            state,
            canvas,
            layers,
            rail,
            options_body,
            options_title,
            layers_panel: layers_panel.clone(),
            options_panel: options_panel.clone(),
            status,
            title,
            subtitle,
            zoom,
            syncing: std::cell::Cell::new(false),
        });

        {
            let win = win.clone();
            sidebar_toggle.connect_clicked(move |_| {
                let visible = !win.layers_panel.is_visible();
                win.layers_panel.set_visible(visible);
                win.status.set_margin_start(if visible {
                    metrics::LAYERS_WIDTH + metrics::PANEL_MARGIN * 2
                } else {
                    metrics::PANEL_MARGIN * 2
                });
                win.sync_insets();
                win.sync_backdrops_soon();
            });
        }

        win.wire_up();
        win.refresh();
        win.sync_backdrops_soon();
        win
    }

    fn wire_up(self: &Rc<Self>) {
        {
            let win = self.clone();
            self.canvas.connect_changed(move || win.refresh());
        }
        {
            let win = self.clone();
            self.layers.connect_changed(move || win.refresh());
        }
        {
            let win = self.clone();
            self.zoom.connect_value_changed(move |s| {
                if win.syncing.get() {
                    return;
                }
                let zoom = 2f32.powf(s.value() as f32);
                win.canvas.set_zoom(zoom);
                win.refresh();
            });
        }
        {
            let win = self.clone();
            self.canvas.widget.connect_resize(move |_, _, _| win.sync_backdrops_soon());
        }
        self.install_actions();
        self.install_shortcuts();
    }

    /// Tell the canvas how much of it the floating panels cover.
    ///
    /// The canvas widget genuinely does extend underneath them — that is what
    /// makes them read as floating — so this is the only thing keeping the
    /// document from being centred half-behind the Layers panel.
    fn sync_insets(self: &Rc<Self>) {
        let m = metrics::PANEL_MARGIN as f32;
        let insets = crate::state::Insets {
            left: if self.layers_panel.is_visible() {
                metrics::LAYERS_WIDTH as f32 + m * 2.0
            } else {
                m
            },
            right: (metrics::OPTIONS_WIDTH + metrics::RAIL_WIDTH) as f32 + m * 3.0,
            top: m,
            bottom: m + STATUS_HEIGHT as f32,
        };
        self.canvas.set_insets(insets);
    }

    /// Hand the canvas the panel rectangles so it can frost the image behind
    /// them.
    ///
    /// Measured from the live widget allocations rather than recomputed from
    /// the metrics constants: the rail's height depends on how many tools it
    /// holds, and a frosted rectangle that disagrees with the panel drawn on
    /// top of it by even a pixel is immediately visible as a bright fringe.
    fn sync_backdrops(self: &Rc<Self>) {
        let canvas = &self.canvas.widget;
        let mut rects = Vec::new();
        for panel in self.backdrop_widgets() {
            if !panel.is_visible() {
                continue;
            }
            let Some(b) = panel.compute_bounds(canvas) else { continue };
            if b.width() < 1.0 || b.height() < 1.0 {
                continue;
            }
            // A panel wider than it asked for means some child's minimum width
            // won the negotiation, which pushes the panel out over the canvas.
            // The frosting follows it either way, so this is not a visual
            // glitch — it is a layout bug that would otherwise go unnoticed.
            if panel == self.options_panel {
                let want = metrics::OPTIONS_WIDTH as f32;
                if b.width() > want + 1.0 {
                    log::warn!(
                        "tool options panel is {:.0}px wide, {:.0}px over its budget — \
                         a control in it has a minimum width that does not fit",
                        b.width(),
                        b.width() - want,
                    );
                }
            }
            rects.push(pixelmagic_gpu::renderer::BackdropRect {
                x: b.x(),
                y: b.y(),
                width: b.width(),
                height: b.height(),
            });
        }
        self.canvas.set_backdrops(rects);
    }

    fn backdrop_widgets(&self) -> [gtk::Widget; 3] {
        [
            self.layers_panel.clone().upcast(),
            self.options_panel.clone().upcast(),
            self.rail.widget.clone().upcast(),
        ]
    }

    /// Re-measure the panels once GTK has finished laying them out.
    ///
    /// Allocations are not valid at the moment something asks for a change, so
    /// measuring immediately after a resize or a toggle reads the *previous*
    /// frame's geometry. One idle callback is enough to land after layout.
    fn sync_backdrops_soon(self: &Rc<Self>) {
        let win = self.clone();
        glib::idle_add_local_once(move || win.sync_backdrops());
    }

    /// Rebuild everything that mirrors document state.
    pub fn refresh(self: &Rc<Self>) {
        self.sync_insets();
        // The options panel is rebuilt below and its height — and, if a panel
        // misbehaves, its width — changes with it. Without this the frosted
        // rectangle keeps the previous panel's shape and leaves a blurred
        // ghost sitting on the canvas.
        self.sync_backdrops_soon();
        self.layers.refresh();
        self.rebuild_options();
        self.canvas.widget.queue_render();

        let st = self.state.borrow();
        self.title.set_text(&st.document.name);
        self.subtitle.set_text(if st.document.dirty { "Edited" } else { "" });

        let selection = if st.document.has_selection() {
            format!("  ·  selection {}", st.selection_bounds_label())
        } else {
            String::new()
        };
        self.status.set_text(&format!(
            "{} × {} px  ·  {} layers  ·  {:.0}%{selection}",
            st.document.width,
            st.document.height,
            st.document.layers.len(),
            st.view.zoom * 100.0,
        ));

        let zoom = st.view.zoom;
        let tool = st.tool;
        drop(st);

        self.syncing.set(true);
        self.zoom.set_value(zoom.max(0.01).log2() as f64);
        self.syncing.set(false);

        self.options_title.set_text(tool.label());
        self.rail.set_active_silently(tool);

        // Switching away from Quick Selection — or turning its preview off —
        // must take the yellow highlight with it. Left behind, it is
        // indistinguishable from a selection the user actually made.
        if crate::canvas::canvas_action(tool) != crate::canvas::CanvasAction::QuickSelect {
            self.canvas.clear_preview();
        }
    }

    fn rebuild_options(self: &Rc<Self>) {
        while let Some(child) = self.options_body.first_child() {
            self.options_body.remove(&child);
        }

        let on_change: Rc<dyn Fn()> = {
            let win = self.clone();
            Rc::new(move || {
                win.canvas.widget.queue_render();
                win.layers.refresh();
            })
        };

        let tool = self.state.borrow().tool;
        let panel = match tool {
            Tool::ColorAdjustments => {
                let histogram = self.canvas.histogram();
                build_adjustments_panel(self.state.clone(), on_change.clone(), histogram)
            }
            Tool::Effects => build_effects_panel(self.state.clone(), on_change.clone()),
            Tool::Arrange => build_arrange_panel(self.state.clone(), on_change.clone()),
            Tool::QuickSelection => {
                build_quick_select_panel(self.state.clone(), on_change.clone())
            }
            _ => build_tool_options(self.state.clone(), on_change.clone()),
        };
        self.options_body.append(&panel);
    }

    // -- actions ------------------------------------------------------------

    fn add_action(self: &Rc<Self>, name: &str, f: impl Fn(&Rc<Window>) + 'static) {
        let action = gio::SimpleAction::new(name, None);
        let win = self.clone();
        action.connect_activate(move |_, _| f(&win));
        self.window.add_action(&action);
    }

    fn install_actions(self: &Rc<Self>) {
        self.add_action("new", |w| w.action_new());
        self.add_action("open", |w| w.action_open());
        self.add_action("save", |w| w.action_save());
        self.add_action("export", |w| w.action_export());

        self.add_action("undo", |w| {
            let label = w.state.borrow_mut().undo();
            if let Some(l) = label {
                log::info!("undid {l}");
            }
            w.refresh();
        });
        self.add_action("redo", |w| {
            let label = w.state.borrow_mut().redo();
            if let Some(l) = label {
                log::info!("redid {l}");
            }
            w.refresh();
        });

        self.add_action("zoom-fit", |w| {
            w.canvas.zoom_to_fit();
            w.refresh();
        });
        self.add_action("zoom-actual", |w| {
            w.canvas.zoom_actual();
            w.refresh();
        });
        self.add_action("zoom-in", |w| {
            w.canvas.zoom_by(1.25);
            w.refresh();
        });
        self.add_action("zoom-out", |w| {
            w.canvas.zoom_by(1.0 / 1.25);
            w.refresh();
        });

        self.add_action("select-all", |w| {
            w.state.borrow_mut().select_all();
            w.refresh();
        });
        self.add_action("deselect", |w| {
            w.state.borrow_mut().deselect();
            w.refresh();
        });
        self.add_action("select-invert", |w| {
            let mut st = w.state.borrow_mut();
            let (width, height) = (st.document.width, st.document.height);
            let mut selection =
                st.document.selection.clone().unwrap_or_else(|| {
                    pixelmagic_core::selection::Selection::none(width, height)
                });
            selection.invert();
            st.set_selection(selection);
            drop(st);
            w.refresh();
        });

        self.add_action("layer-new", |w| {
            w.state.borrow_mut().document.add_empty_layer();
            w.refresh();
        });
        self.add_action("layer-duplicate", |w| {
            let mut st = w.state.borrow_mut();
            if let Some(id) = st.document.primary_active() {
                if let Ok(new_id) = st.document.layers.duplicate(id) {
                    st.document.set_active(vec![new_id]);
                    st.document.dirty = true;
                    st.needs_redraw = true;
                }
            }
            drop(st);
            w.refresh();
        });
        self.add_action("layer-delete", |w| {
            let mut st = w.state.borrow_mut();
            if let Some(id) = st.document.primary_active() {
                let _ = st.document.remove_layer(id);
            }
            drop(st);
            w.refresh();
        });
        self.add_action("layer-group", |w| {
            let mut st = w.state.borrow_mut();
            let ids = st.document.active.clone();
            if !ids.is_empty() {
                if let Ok(group) = st.document.layers.group(&ids, "Group") {
                    st.document.set_active(vec![group]);
                    st.document.dirty = true;
                }
            }
            drop(st);
            w.refresh();
        });
        self.add_action("layer-adjustments", |w| {
            w.state
                .borrow_mut()
                .document
                .add_layer("Color Adjustments", LayerKind::ColorAdjustments);
            w.state.borrow_mut().tool = Tool::ColorAdjustments;
            w.refresh();
        });
        self.add_action("layer-effects", |w| {
            w.state.borrow_mut().document.add_layer("Effects", LayerKind::Effects);
            w.state.borrow_mut().tool = Tool::Effects;
            w.refresh();
        });

        self.add_action("about", |w| w.action_about());
    }

    fn install_shortcuts(self: &Rc<Self>) {
        let controller = gtk::EventControllerKey::new();
        let win = self.clone();
        controller.connect_key_pressed(move |_, key, _code, modifiers| {
            let ctrl = modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);

            // Ctrl-anything belongs to the accelerators.
            if ctrl {
                return glib::Propagation::Proceed;
            }
            let Some(ch) = key.to_unicode() else {
                return glib::Propagation::Proceed;
            };

            if ch.is_ascii_alphabetic() {
                let target = if shift {
                    Tool::from_shortcut(ch.to_ascii_lowercase()).map(|t| t.cycle())
                } else {
                    Tool::from_shortcut(ch)
                };
                if let Some(tool) = target.filter(|t| t.is_implemented()) {
                    win.state.borrow_mut().tool = tool;
                    win.refresh();
                    return glib::Propagation::Stop;
                }
            }

            match ch {
                'x' | 'X' => {
                    win.state.borrow_mut().colors.swap();
                    win.refresh();
                    glib::Propagation::Stop
                }
                'd' | 'D' => {
                    win.state.borrow_mut().colors.reset();
                    win.refresh();
                    glib::Propagation::Stop
                }
                '[' => {
                    win.state.borrow_mut().brush.step_size(false);
                    win.refresh();
                    glib::Propagation::Stop
                }
                ']' => {
                    win.state.borrow_mut().brush.step_size(true);
                    win.refresh();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.window.add_controller(controller);
    }

    // -- file handling ------------------------------------------------------

    fn action_new(self: &Rc<Self>) {
        *self.state.borrow_mut() = EditorState::new(Document::new(1920, 1080));
        self.canvas.zoom_to_fit();
        self.refresh();
    }

    fn action_open(self: &Rc<Self>) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Images and Pixelmagic documents"));
        for ext in pixelmagic_io::OPENABLE_EXTENSIONS {
            filter.add_pattern(&format!("*.{ext}"));
        }
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);

        let dialog = gtk::FileDialog::builder().title("Open").filters(&filters).build();
        let win = self.clone();
        dialog.open(Some(&self.window), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return };
            let Some(path) = file.path() else { return };
            win.open_path(&path);
        });
    }

    pub fn open_path(self: &Rc<Self>, path: &std::path::Path) {
        let is_pxm = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case(pixelmagic_io::PXM_EXTENSION))
            .unwrap_or(false);

        let loaded = if is_pxm {
            pixelmagic_io::load_document(path).map_err(|e| e.to_string())
        } else {
            pixelmagic_io::load_image(path)
                .map(|buffer| {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Untitled")
                        .to_string();
                    Document::from_image(name, buffer)
                })
                .map_err(|e| e.to_string())
        };

        match loaded {
            Ok(mut doc) => {
                if !is_pxm {
                    doc.path = Some(path.to_path_buf());
                }
                *self.state.borrow_mut() = EditorState::new(doc);
                self.canvas.zoom_to_fit();
                self.refresh();
            }
            Err(e) => self.show_error("Could not open the file", &e),
        }
    }

    fn action_save(self: &Rc<Self>) {
        let existing = self.state.borrow().document.path.clone();
        match existing.filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case(pixelmagic_io::PXM_EXTENSION))
                .unwrap_or(false)
        }) {
            Some(path) => self.save_to(&path),
            None => self.save_as(),
        }
    }

    fn save_as(self: &Rc<Self>) {
        let dialog = gtk::FileDialog::builder()
            .title("Save As")
            .initial_name(self.state.borrow().file_name())
            .build();
        let win = self.clone();
        dialog.save(Some(&self.window), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return };
            let Some(mut path) = file.path() else { return };
            if path.extension().is_none() {
                path.set_extension(pixelmagic_io::PXM_EXTENSION);
            }
            win.save_to(&path);
        });
    }

    fn save_to(self: &Rc<Self>, path: &std::path::Path) {
        let result = {
            let st = self.state.borrow();
            pixelmagic_io::save_document(&st.document, path)
        };
        match result {
            Ok(()) => {
                self.state.borrow_mut().set_path(path.to_path_buf());
                self.refresh();
            }
            Err(e) => self.show_error("Could not save the document", &e.to_string()),
        }
    }

    fn action_export(self: &Rc<Self>) {
        let name = {
            let st = self.state.borrow();
            format!("{}.png", st.document.name)
        };
        let dialog = gtk::FileDialog::builder().title("Export").initial_name(name).build();
        let win = self.clone();
        dialog.save(Some(&self.window), gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else { return };
            let Some(path) = file.path() else { return };
            win.export_to(path);
        });
    }

    fn export_to(self: &Rc<Self>, path: PathBuf) {
        self.canvas.widget.make_current();
        match self.canvas.render_to_buffer() {
            Ok(buffer) => {
                let st = self.state.borrow();
                match pixelmagic_io::export_document(&st.document, &buffer, &path) {
                    Ok(()) => log::info!("exported to {}", path.display()),
                    Err(e) => {
                        drop(st);
                        self.show_error("Could not export the image", &e.to_string());
                    }
                }
            }
            Err(e) => self.show_error("Could not render for export", &e),
        }
    }

    fn action_about(self: &Rc<Self>) {
        let (tools_done, tools_total) = pixelmagic_core::tool::implemented_count();
        let (fx_done, fx_total) = pixelmagic_core::effect::implemented_count();

        let about = adw::AboutWindow::builder()
            .transient_for(&self.window)
            .application_name("Pixelmagic")
            .version(env!("CARGO_PKG_VERSION"))
            .comments(format!(
                "A GTK4-native, GPU-accelerated, non-destructive image editor.\n\n\
                 Tools implemented: {tools_done} of {tools_total}\n\
                 Effects implemented: {fx_done} of {fx_total}"
            ))
            .license_type(gtk::License::Gpl30)
            .website("https://github.com/Supermosho/Pixelmagic")
            .build();
        about.present();
    }

    fn show_error(self: &Rc<Self>, heading: &str, detail: &str) {
        log::error!("{heading}: {detail}");
        let dialog = adw::MessageDialog::new(Some(&self.window), Some(heading), Some(detail));
        dialog.add_response("ok", "OK");
        dialog.present();
    }
}

// ---------------------------------------------------------------------------
// Chrome helpers
// ---------------------------------------------------------------------------

/// Wrap content in a floating panel: rounded, translucent, shadowed.
fn panel_shell(content: &impl IsA<gtk::Widget>) -> gtk::Box {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add_css_class("pm-panel");
    panel.append(content);
    panel
}

fn set_margins(w: &impl IsA<gtk::Widget>, m: i32) {
    let w = w.as_ref();
    w.set_margin_top(m);
    w.set_margin_bottom(m);
    w.set_margin_start(m);
    w.set_margin_end(m);
}

/// The toolbar zoom control: minus, slider, plus.
///
/// The scale is in log2 units, so a fixed drag distance always changes the zoom
/// by a fixed *ratio*. On a linear scale everything below 100% is crammed into
/// the leftmost sliver, which is where most photo editing actually happens.
fn build_zoom_control() -> (gtk::Box, gtk::Scale) {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    container.set_valign(gtk::Align::Center);

    let out = gtk::Button::from_icon_name("zoom-out-symbolic");
    out.add_css_class("flat");
    out.set_action_name(Some("win.zoom-out"));
    out.set_tooltip_text(Some("Zoom out"));

    let scale = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        (crate::state::View::MIN_ZOOM as f64).log2(),
        (crate::state::View::MAX_ZOOM as f64).log2(),
        0.05,
    );
    scale.add_css_class("pm-zoom");
    scale.set_draw_value(false);
    scale.set_size_request(110, -1);
    scale.set_value(0.0);

    let inn = gtk::Button::from_icon_name("zoom-in-symbolic");
    inn.add_css_class("flat");
    inn.set_action_name(Some("win.zoom-in"));
    inn.set_tooltip_text(Some("Zoom in"));

    container.append(&out);
    container.append(&scale);
    container.append(&inn);
    (container, scale)
}

fn build_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let file = gio::Menu::new();
    file.append(Some("New"), Some("win.new"));
    file.append(Some("Open…"), Some("win.open"));
    file.append(Some("Save"), Some("win.save"));
    file.append(Some("Export…"), Some("win.export"));
    menu.append_section(Some("File"), &file);

    let edit = gio::Menu::new();
    edit.append(Some("Undo"), Some("win.undo"));
    edit.append(Some("Redo"), Some("win.redo"));
    menu.append_section(Some("Edit"), &edit);

    let layer = gio::Menu::new();
    layer.append(Some("New Layer"), Some("win.layer-new"));
    layer.append(Some("Duplicate Layer"), Some("win.layer-duplicate"));
    layer.append(Some("Delete Layer"), Some("win.layer-delete"));
    layer.append(Some("Group Layers"), Some("win.layer-group"));
    layer.append(Some("Color Adjustments Layer"), Some("win.layer-adjustments"));
    layer.append(Some("Effects Layer"), Some("win.layer-effects"));
    menu.append_section(Some("Layer"), &layer);

    let view = gio::Menu::new();
    view.append(Some("Zoom to Fit"), Some("win.zoom-fit"));
    view.append(Some("Actual Size"), Some("win.zoom-actual"));
    menu.append_section(Some("View"), &view);

    let select = gio::Menu::new();
    select.append(Some("Select All"), Some("win.select-all"));
    select.append(Some("Deselect"), Some("win.deselect"));
    select.append(Some("Invert Selection"), Some("win.select-invert"));
    menu.append_section(Some("Select"), &select);

    menu.append(Some("About Pixelmagic"), Some("win.about"));
    menu
}
