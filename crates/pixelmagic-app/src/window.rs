//! The main window: layout, actions and shortcuts.

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
use crate::ui::{
    build_adjustments_panel, build_effects_panel, build_tool_options, LayersSidebar,
    ToolsSidebar,
};

pub struct Window {
    pub window: adw::ApplicationWindow,
    state: Rc<RefCell<EditorState>>,
    canvas: Rc<Canvas>,
    layers: Rc<LayersSidebar>,
    tools: Rc<ToolsSidebar>,
    /// The right-hand pane, rebuilt when the active tool changes.
    inspector: gtk::Box,
    status: gtk::Label,
    title: adw::WindowTitle,
}

impl Window {
    pub fn new(app: &adw::Application, document: Document) -> Rc<Self> {
        let state = EditorState::shared(document);
        let canvas = Canvas::new(state.clone());
        let layers = LayersSidebar::new(state.clone());

        let title = adw::WindowTitle::new("Pixelmagic", "");
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&title));

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .default_width(1440)
            .default_height(900)
            .title("Pixelmagic")
            .build();

        let inspector = gtk::Box::new(gtk::Orientation::Vertical, 0);
        inspector.set_size_request(320, -1);
        // Without this the inspector grows to whatever its widest panel wants
        // and steals the canvas's space.
        inspector.set_hexpand(false);

        let tools_holder: Rc<RefCell<Option<Rc<ToolsSidebar>>>> = Rc::new(RefCell::new(None));
        let tools = ToolsSidebar::new(state.clone(), {
            let state = state.clone();
            move |tool| {
                state.borrow_mut().tool = tool;
            }
        });
        *tools_holder.borrow_mut() = Some(tools.clone());

        let status = gtk::Label::new(Some(""));
        status.set_xalign(0.0);
        status.add_css_class("dim-label");
        status.set_margin_start(10);
        status.set_margin_end(10);
        status.set_margin_top(3);
        status.set_margin_bottom(3);

        // Left rail, canvas, right inspector; a slim info bar underneath.
        let centre = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        centre.append(&tools.widget);
        centre.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        centre.append(&canvas.widget);
        centre.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        centre.append(&inspector);

        let right_split = gtk::Paned::new(gtk::Orientation::Vertical);
        right_split.set_start_child(Some(&centre));
        right_split.set_resize_start_child(true);
        right_split.set_shrink_start_child(false);

        let bottom = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bottom.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        bottom.append(&status);
        right_split.set_end_child(Some(&bottom));
        right_split.set_resize_end_child(false);
        right_split.set_shrink_end_child(false);

        // The layers sidebar is a separate pane so it can be resized.
        let main_split = gtk::Paned::new(gtk::Orientation::Horizontal);
        main_split.set_start_child(Some(&right_split));
        main_split.set_end_child(Some(&layers.widget));
        main_split.set_resize_start_child(true);
        main_split.set_shrink_start_child(false);
        main_split.set_resize_end_child(false);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&header);
        content.append(&main_split);
        window.set_content(Some(&content));

        let win = Rc::new(Window {
            window: window.clone(),
            state,
            canvas: canvas.clone(),
            layers: layers.clone(),
            tools,
            inspector,
            status,
            title,
        });

        win.build_header(&header);
        win.wire_up();
        win.refresh();
        win
    }

    fn build_header(self: &Rc<Self>, header: &adw::HeaderBar) {
        let new_button = gtk::Button::from_icon_name("document-new-symbolic");
        new_button.set_tooltip_text(Some("New document  (Ctrl+N)"));
        new_button.set_action_name(Some("win.new"));
        header.pack_start(&new_button);

        let open_button = gtk::Button::from_icon_name("document-open-symbolic");
        open_button.set_tooltip_text(Some("Open  (Ctrl+O)"));
        open_button.set_action_name(Some("win.open"));
        header.pack_start(&open_button);

        let save_button = gtk::Button::from_icon_name("document-save-symbolic");
        save_button.set_tooltip_text(Some("Save  (Ctrl+S)"));
        save_button.set_action_name(Some("win.save"));
        header.pack_start(&save_button);

        let undo = gtk::Button::from_icon_name("edit-undo-symbolic");
        undo.set_tooltip_text(Some("Undo  (Ctrl+Z)"));
        undo.set_action_name(Some("win.undo"));
        header.pack_start(&undo);

        let redo = gtk::Button::from_icon_name("edit-redo-symbolic");
        redo.set_tooltip_text(Some("Redo  (Ctrl+Shift+Z)"));
        redo.set_action_name(Some("win.redo"));
        header.pack_start(&redo);

        let export = gtk::Button::with_label("Export…");
        export.set_action_name(Some("win.export"));
        header.pack_end(&export);

        let menu = gio::Menu::new();

        let layer_section = gio::Menu::new();
        layer_section.append(Some("New Layer"), Some("win.layer-new"));
        layer_section.append(Some("Duplicate Layer"), Some("win.layer-duplicate"));
        layer_section.append(Some("Delete Layer"), Some("win.layer-delete"));
        layer_section.append(Some("Group Layers"), Some("win.layer-group"));
        menu.append_section(Some("Layer"), &layer_section);

        let add_section = gio::Menu::new();
        add_section.append(Some("Color Adjustments Layer"), Some("win.layer-adjustments"));
        add_section.append(Some("Effects Layer"), Some("win.layer-effects"));
        menu.append_section(Some("Add"), &add_section);

        let view_section = gio::Menu::new();
        view_section.append(Some("Zoom to Fit"), Some("win.zoom-fit"));
        view_section.append(Some("Actual Size"), Some("win.zoom-actual"));
        view_section.append(Some("Zoom In"), Some("win.zoom-in"));
        view_section.append(Some("Zoom Out"), Some("win.zoom-out"));
        menu.append_section(Some("View"), &view_section);

        let select_section = gio::Menu::new();
        select_section.append(Some("Select All"), Some("win.select-all"));
        select_section.append(Some("Deselect"), Some("win.deselect"));
        select_section.append(Some("Invert Selection"), Some("win.select-invert"));
        menu.append_section(Some("Select"), &select_section);

        menu.append(Some("About Pixelmagic"), Some("win.about"));

        let button = gtk::MenuButton::new();
        button.set_icon_name("open-menu-symbolic");
        button.set_menu_model(Some(&menu));
        header.pack_end(&button);
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
        self.install_actions();
        self.install_shortcuts();
    }

    /// Rebuild everything that mirrors document state.
    pub fn refresh(self: &Rc<Self>) {
        self.layers.refresh();
        self.rebuild_inspector();
        self.canvas.widget.queue_render();

        let st = self.state.borrow();
        self.title.set_title(&st.title());
        self.title.set_subtitle(&format!(
            "{} × {} · {:.0}%",
            st.document.width,
            st.document.height,
            st.view.zoom * 100.0
        ));

        let selection = if st.document.has_selection() {
            let b = st.selection_bounds_label();
            format!(" · selection {b}")
        } else {
            String::new()
        };
        let undo =
            st.history.undo_label().map(|l| format!(" · next undo: {l}")).unwrap_or_default();
        self.status.set_text(&format!(
            "{} · {} layers{selection}{undo}",
            st.tool.label(),
            st.document.layers.len()
        ));
        drop(st);

        // Read the tool into a local *before* calling into the sidebar. Written
        // as `set_active_silently(self.state.borrow().tool)` the temporary
        // borrow would live until the end of the statement — i.e. across
        // `set_active`, which synchronously fires `toggled`, whose handler
        // takes `borrow_mut`. That is an instant panic, and it is invisible
        // until something actually toggles a button.
        let tool = self.state.borrow().tool;
        self.tools.set_active_silently(tool);
    }

    fn rebuild_inspector(self: &Rc<Self>) {
        while let Some(child) = self.inspector.first_child() {
            self.inspector.remove(&child);
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
                build_adjustments_panel(self.state.clone(), on_change.clone())
            }
            Tool::Effects => build_effects_panel(self.state.clone(), on_change.clone()),
            _ => build_tool_options(self.state.clone(), on_change.clone()),
        };
        self.inspector.append(&panel);
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
            w.state.borrow_mut().document.select_all();
            w.refresh();
        });
        self.add_action("deselect", |w| {
            w.state.borrow_mut().document.deselect();
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

            // Bare letters select tools, exactly as documented in SPEC §5.1 —
            // but only when no modifier is held, or Ctrl+S would pick the Style
            // tool instead of saving.
            if !ctrl {
                if let Some(ch) = key.to_unicode() {
                    if ch.is_ascii_alphabetic() && !shift {
                        if let Some(tool) = Tool::from_shortcut(ch) {
                            if tool.is_implemented() {
                                win.state.borrow_mut().tool = tool;
                                win.refresh();
                                return glib::Propagation::Stop;
                            }
                        }
                    }
                    if shift {
                        // Shift + a tool's letter cycles its group.
                        if let Some(tool) = Tool::from_shortcut(ch.to_ascii_lowercase()) {
                            let next = tool.cycle();
                            if next != tool && next.is_implemented() {
                                win.state.borrow_mut().tool = next;
                                win.refresh();
                                return glib::Propagation::Stop;
                            }
                        }
                    }
                    match ch {
                        'x' | 'X' => {
                            win.state.borrow_mut().colors.swap();
                            win.refresh();
                            return glib::Propagation::Stop;
                        }
                        'd' | 'D' => {
                            win.state.borrow_mut().colors.reset();
                            win.refresh();
                            return glib::Propagation::Stop;
                        }
                        '[' => {
                            win.state.borrow_mut().brush.step_size(false);
                            win.refresh();
                            return glib::Propagation::Stop;
                        }
                        ']' => {
                            win.state.borrow_mut().brush.step_size(true);
                            win.refresh();
                            return glib::Propagation::Stop;
                        }
                        _ => {}
                    }
                }
            }
            glib::Propagation::Proceed
        });
        self.window.add_controller(controller);
    }

    // -- file handling ------------------------------------------------------

    fn action_new(self: &Rc<Self>) {
        let mut st = self.state.borrow_mut();
        *st = EditorState::new(Document::new(1920, 1080));
        drop(st);
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

    /// Export the flattened document.
    ///
    /// Renders through the same pipeline the canvas uses rather than a separate
    /// CPU path, so what is exported is exactly what was on screen. That is
    /// worth the awkwardness of needing the GL context current.
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
