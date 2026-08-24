//! Sidebars and parameter panels.
//!
//! The parameter panels are **generated** from [`ParamSpec`] rather than
//! hand-built. Pixelmagic catalogues ~75 effects and 16 adjustments; writing a
//! panel for each would be thousands of lines that drift out of step with the
//! shaders the moment anyone adds a slider. Generating them means a new effect
//! is a table row plus a shader, and the UI follows for free.

use adw::prelude::*;
use pixelmagic_core::adjust::AdjustmentKind;
use pixelmagic_core::blend::{BlendGroup, BlendMode};
use pixelmagic_core::effect::{EffectCategory, EFFECTS};
use pixelmagic_core::layer::{LayerId, LayerKind};
use pixelmagic_core::param::{ParamKind, ParamSpec, ParamValue};
use pixelmagic_core::tool::{tools_in, Tool, ToolCategory};
use std::cell::RefCell;
use std::rc::Rc;

use crate::state::EditorState;

/// Build a control for one parameter.
///
/// `get` reads the current value; `set` is called on every change. Returning a
/// widget rather than mutating shared state directly keeps this reusable for
/// adjustments, effects, tool options and layer styles alike.
pub fn build_param_row(
    spec: &ParamSpec,
    get: impl Fn() -> Option<ParamValue> + 'static,
    set: impl Fn(ParamValue) + 'static,
) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("param-row");

    let label = gtk::Label::new(Some(spec.label));
    label.set_xalign(0.0);
    label.set_width_chars(11);
    label.set_max_width_chars(11);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_tooltip_text(Some(spec.label));
    row.append(&label);

    match spec.kind {
        ParamKind::Slider { min, max, soft_min, soft_max, default, percent, unit } => {
            let current = get().and_then(|v| v.as_f32()).unwrap_or(default);
            // The slider tracks the *soft* range, which is where the useful
            // values are; the spin button below can still reach the hard
            // limits, which is how Pixelmator's Option-drag behaves.
            let scale = gtk::Scale::with_range(
                gtk::Orientation::Horizontal,
                soft_min as f64,
                soft_max as f64,
                ((soft_max - soft_min) / 200.0).max(1e-4) as f64,
            );
            scale.set_value(current.clamp(soft_min, soft_max) as f64);
            scale.set_hexpand(true);
            scale.set_draw_value(false);

            let entry = gtk::SpinButton::with_range(
                min as f64,
                max as f64,
                if percent { 0.01 } else { 0.1 },
            );
            entry.set_digits(if percent { 0 } else { 2 });
            entry.set_width_chars(6);
            entry.set_value(if percent { (current * 100.0) as f64 } else { current as f64 });
            if !unit.is_empty() {
                entry.set_tooltip_text(Some(unit));
            }

            let set = Rc::new(set);
            let syncing = Rc::new(std::cell::Cell::new(false));

            {
                let entry = entry.clone();
                let set = set.clone();
                let syncing = syncing.clone();
                scale.connect_value_changed(move |s| {
                    if syncing.get() {
                        return;
                    }
                    syncing.set(true);
                    let v = s.value() as f32;
                    entry.set_value(if percent { (v * 100.0) as f64 } else { v as f64 });
                    set(ParamValue::Float(v));
                    syncing.set(false);
                });
            }
            {
                let scale = scale.clone();
                let set = set.clone();
                let syncing = syncing.clone();
                entry.connect_value_changed(move |e| {
                    if syncing.get() {
                        return;
                    }
                    syncing.set(true);
                    let v = if percent { e.value() as f32 / 100.0 } else { e.value() as f32 };
                    scale.set_value(v.clamp(soft_min, soft_max) as f64);
                    set(ParamValue::Float(v));
                    syncing.set(false);
                });
            }

            row.append(&scale);
            row.append(&entry);
        }
        ParamKind::Angle { default } => {
            let current = get().and_then(|v| v.as_f32()).unwrap_or(default);
            let spin = gtk::SpinButton::with_range(0.0, 360.0, 1.0);
            spin.set_wrap(true);
            spin.set_value(current as f64);
            spin.set_hexpand(true);
            spin.connect_value_changed(move |s| set(ParamValue::Float(s.value() as f32)));
            row.append(&spin);
        }
        ParamKind::Toggle { default } => {
            let current = get().and_then(|v| v.as_bool()).unwrap_or(default);
            let sw = gtk::Switch::new();
            sw.set_active(current);
            sw.set_halign(gtk::Align::Start);
            sw.set_hexpand(true);
            sw.connect_state_set(move |_, on| {
                set(ParamValue::Bool(on));
                gtk::glib::Propagation::Proceed
            });
            row.append(&sw);
        }
        ParamKind::Color { default } => {
            let current = get().and_then(|v| v.as_color()).unwrap_or(default);
            let button = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::new()));
            button.set_rgba(&to_gdk(current));
            button.set_hexpand(true);
            button.connect_rgba_notify(move |b| {
                set(ParamValue::Color(from_gdk(b.rgba())));
            });
            row.append(&button);
        }
        ParamKind::Choice { options, default } => {
            let current = get().and_then(|v| v.as_index()).unwrap_or(default);
            let model = gtk::StringList::new(options);
            let drop = gtk::DropDown::new(Some(model), gtk::Expression::NONE);
            drop.set_selected(current);
            drop.set_hexpand(true);
            drop.connect_selected_notify(move |d| {
                set(ParamValue::Index(d.selected()));
            });
            row.append(&drop);
        }
        ParamKind::Point { default } => {
            // On-canvas "effect ropes" are the right interface for these; until
            // they exist, two spin buttons in normalised coordinates at least
            // make the parameter reachable.
            let current = get().and_then(|v| v.as_point()).unwrap_or(default);
            let sx = gtk::SpinButton::with_range(0.0, 1.0, 0.01);
            let sy = gtk::SpinButton::with_range(0.0, 1.0, 0.01);
            sx.set_digits(2);
            sy.set_digits(2);
            sx.set_value(current.x as f64);
            sy.set_value(current.y as f64);
            sx.set_hexpand(true);
            sy.set_hexpand(true);

            let set = Rc::new(set);
            {
                let sy2 = sy.clone();
                let set = set.clone();
                sx.connect_value_changed(move |s| {
                    set(ParamValue::Point(glam::Vec2::new(
                        s.value() as f32,
                        sy2.value() as f32,
                    )));
                });
            }
            {
                let sx2 = sx.clone();
                sy.connect_value_changed(move |s| {
                    set(ParamValue::Point(glam::Vec2::new(
                        sx2.value() as f32,
                        s.value() as f32,
                    )));
                });
            }
            row.append(&sx);
            row.append(&sy);
        }
        ParamKind::Curve => {
            let placeholder = gtk::Label::new(Some("Curve editor"));
            placeholder.set_sensitive(false);
            placeholder.set_hexpand(true);
            placeholder.set_xalign(0.0);
            row.append(&placeholder);
        }
    }

    row.upcast()
}

pub fn to_gdk(c: pixelmagic_core::color::Rgba) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(c.r, c.g, c.b, c.a)
}

pub fn from_gdk(c: gtk::gdk::RGBA) -> pixelmagic_core::color::Rgba {
    pixelmagic_core::color::Rgba::new(c.red(), c.green(), c.blue(), c.alpha())
}

// ---------------------------------------------------------------------------
// Tool rail
// ---------------------------------------------------------------------------

/// The vertical strip of tool buttons.
///
/// In the original this sits on the **far right**, outboard of the tool's own
/// options panel — not on the left where Photoshop and GIMP put it.
pub struct ToolRail {
    pub widget: gtk::Box,
    buttons: RefCell<Vec<(Tool, gtk::ToggleButton)>>,
    /// Set while syncing to the current tool, so the `toggled` signals that
    /// causes are not mistaken for user clicks.
    updating: std::cell::Cell<bool>,
}

impl ToolRail {
    pub fn new(
        state: Rc<RefCell<EditorState>>,
        on_select: impl Fn(Tool) + 'static,
    ) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.add_css_class("pm-rail");
        widget.set_width_request(crate::style::metrics::RAIL_WIDTH);

        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.set_margin_top(2);
        list.set_margin_bottom(2);

        let rail = Rc::new(ToolRail {
            widget: widget.clone(),
            buttons: RefCell::new(Vec::new()),
            updating: std::cell::Cell::new(false),
        });
        let on_select = Rc::new(on_select);

        for (i, category) in ToolCategory::ALL.iter().enumerate() {
            if i > 0 {
                let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
                sep.set_margin_top(3);
                sep.set_margin_bottom(3);
                sep.set_margin_start(9);
                sep.set_margin_end(9);
                list.append(&sep);
            }
            for info in tools_in(*category) {
                let button = gtk::ToggleButton::new();
                button.add_css_class("tool-button");
                button.set_child(Some(&gtk::Label::new(Some(&tool_glyph(info.tool)))));

                let shortcut = info
                    .shortcut
                    .map(|c| format!("  ({})", c.to_ascii_uppercase()))
                    .unwrap_or_default();
                let status = if info.implemented { "" } else { "\n\nNot implemented yet" };
                button.set_tooltip_text(Some(&format!(
                    "{}{shortcut}\n{}{status}",
                    info.label, info.description
                )));
                button.set_sensitive(info.implemented);

                let tool = info.tool;
                let on_select = on_select.clone();
                let rail_weak = Rc::downgrade(&rail);
                button.connect_toggled(move |b| {
                    let Some(rail) = rail_weak.upgrade() else { return };
                    if rail.updating.get() || !b.is_active() {
                        return;
                    }
                    rail.set_active_silently(tool);
                    on_select(tool);
                });

                rail.buttons.borrow_mut().push((tool, button.clone()));
                list.append(&button);
            }
        }

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_propagate_natural_height(true);
        scroller.set_child(Some(&list));
        widget.append(&scroller);

        let current = state.borrow().tool;
        rail.set_active_silently(current);
        rail
    }

    /// Update which button looks active without re-firing `on_select`.
    pub fn set_active_silently(&self, tool: Tool) {
        let was = self.updating.replace(true);
        for (t, b) in self.buttons.borrow().iter() {
            let should = *t == tool;
            if b.is_active() != should {
                b.set_active(should);
            }
        }
        self.updating.set(was);
    }
}

/// A short glyph for each tool.
///
/// Typographic rather than iconographic, and deliberately so: Pixelmagic does
/// not ship Apple's icon set, and inventing fifty pictograms is a design
/// project of its own. Letters and symbols are honest placeholders that stay
/// legible at rail size.
fn tool_glyph(tool: Tool) -> String {
    use Tool::*;
    // Strictly monochrome. Anything in the emoji planes (a hand, a paintbrush,
    // a bucket) renders as a full-colour glyph, which looks badly out of place
    // in a grey rail and cannot be tinted to show the active state.
    match tool {
        Style => "◆",
        Arrange => "➤",
        ColorAdjustments => "◐",
        Effects => "✦",
        Crop => "⌗",
        ExportForWeb => "⤓",
        ColorPicker => "⌖",
        Zoom => "⌕",
        Hand => "⎈",

        RectangularSelection => "▭",
        OvalSelection => "◯",
        RowSelection => "▬",
        ColumnSelection => "▮",
        FreeSelection => "◌",
        PolygonalSelection => "⬠",
        MagneticSelection => "⌁",
        ColorSelection => "◍",
        QuickSelection => "⌾",

        Paint => "✏",
        PixelPaint => "▦",
        ColorFill => "◧",
        GradientFill => "▤",
        Erase => "◻",
        SmartErase => "⌫",

        Repair => "✚",
        Clone => "⎘",
        Sharpen => "△",
        Soften => "◠",
        Smudge => "≈",
        Lighten => "⊕",
        Darken => "⊖",
        Saturate => "◉",
        Desaturate => "○",
        Distort => "∿",
        Bump => "◗",
        Pinch => "◖",
        Twirl => "⊚",

        Shape => "⬟",
        Pen => "✒",
        FreeformPen => "✎",
        Rectangle => "▢",
        RoundedRectangle => "▣",
        Oval => "⬭",
        Polygon => "⬡",
        Star => "★",
        Line => "╱",

        Type => "T",
        CircularType => "◜",
        PathType => "⌇",
        FreeformType => "≀",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Layers panel
// ---------------------------------------------------------------------------

pub struct LayersSidebar {
    pub widget: gtk::Box,
    list: gtk::ListBox,
    opacity: gtk::Scale,
    opacity_value: gtk::Label,
    blend: gtk::DropDown,
    search: gtk::SearchEntry,
    state: Rc<RefCell<EditorState>>,
    on_change: RefCell<Vec<Rc<dyn Fn()>>>,
    /// Set while rebuilding, so programmatic changes are not mistaken for user
    /// input and written back into the document.
    updating: std::cell::Cell<bool>,
}

impl LayersSidebar {
    pub fn new(state: Rc<RefCell<EditorState>>) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // -- header: title and the three action buttons ----------------------
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        header.add_css_class("pm-panel-header");
        let title = gtk::Label::new(Some("Layers"));
        title.add_css_class("pm-panel-title");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        header.append(&title);

        for (icon, tooltip, action) in [
            ("list-add-symbolic", "New layer", "win.layer-new"),
            ("edit-copy-symbolic", "Duplicate layer", "win.layer-duplicate"),
        ] {
            let b = gtk::Button::from_icon_name(icon);
            b.add_css_class("flat");
            b.set_tooltip_text(Some(tooltip));
            b.set_action_name(Some(action));
            header.append(&b);
        }
        let more = gtk::MenuButton::new();
        more.set_icon_name("view-more-symbolic");
        more.add_css_class("flat");
        let more_menu = gtk::gio::Menu::new();
        more_menu.append(Some("Group Layers"), Some("win.layer-group"));
        more_menu.append(Some("Delete Layer"), Some("win.layer-delete"));
        more_menu.append(Some("Color Adjustments Layer"), Some("win.layer-adjustments"));
        more_menu.append(Some("Effects Layer"), Some("win.layer-effects"));
        more.set_menu_model(Some(&more_menu));
        header.append(&more);
        widget.append(&header);

        // -- blend mode and opacity, applying to the selected layer ----------
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        controls.set_margin_start(10);
        controls.set_margin_end(10);
        controls.set_margin_bottom(6);

        let blend_model = gtk::StringList::new(&[]);
        for group in BlendGroup::ALL {
            for mode in BlendMode::ALL.iter().filter(|m| m.group() == group) {
                blend_model.append(mode.label());
            }
        }
        let blend = gtk::DropDown::new(Some(blend_model), gtk::Expression::NONE);
        blend.set_size_request(112, -1);
        blend.set_valign(gtk::Align::Center);
        controls.append(&blend);

        let opacity_col = gtk::Box::new(gtk::Orientation::Vertical, 0);
        opacity_col.set_hexpand(true);
        let opacity_head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let opacity_label = gtk::Label::new(Some("Opacity"));
        opacity_label.set_xalign(0.0);
        opacity_label.set_hexpand(true);
        let opacity_value = gtk::Label::new(Some("100%"));
        opacity_value.set_xalign(1.0);
        opacity_head.append(&opacity_label);
        opacity_head.append(&opacity_value);

        let opacity = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        opacity.add_css_class("pm-opacity");
        opacity.set_value(100.0);
        opacity.set_draw_value(false);

        opacity_col.append(&opacity_head);
        opacity_col.append(&opacity);
        controls.append(&opacity_col);
        widget.append(&controls);

        // -- the list --------------------------------------------------------
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        // Deliberately *not* `.background`: that libadwaita class paints the
        // opaque window colour, which covers the frosted backdrop the renderer
        // draws under this panel and turns the bottom two-thirds of it into a
        // black rectangle. The stylesheet makes it transparent instead.
        list.add_css_class("pm-layer-list");

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&list));
        widget.append(&scroller);

        // -- search, along the bottom edge ------------------------------------
        let search = gtk::SearchEntry::new();
        search.add_css_class("pm-search");
        search.set_placeholder_text(Some("Search"));
        widget.append(&search);

        let sidebar = Rc::new(LayersSidebar {
            widget,
            list: list.clone(),
            opacity: opacity.clone(),
            opacity_value,
            blend: blend.clone(),
            search: search.clone(),
            state: state.clone(),
            on_change: RefCell::new(Vec::new()),
            updating: std::cell::Cell::new(false),
        });

        {
            let s = sidebar.clone();
            list.connect_row_selected(move |_, row| {
                if s.updating.get() {
                    return;
                }
                let Some(row) = row else { return };
                let Some(id) = s.visible_layers().get(row.index() as usize).copied() else {
                    return;
                };
                s.state.borrow_mut().document.set_active(vec![id]);
                s.notify();
            });
        }

        {
            let s = sidebar.clone();
            search.connect_search_changed(move |_| {
                if s.updating.get() {
                    return;
                }
                s.refresh();
            });
        }

        {
            let s = sidebar.clone();
            opacity.connect_value_changed(move |sc| {
                if s.updating.get() {
                    return;
                }
                let value = (sc.value() / 100.0) as f32;
                let st = s.state.borrow();
                let edit = st.document.primary_active().and_then(|id| {
                    pixelmagic_core::history::SetLayerProperty::new(
                        &st.document,
                        id,
                        pixelmagic_core::history::LayerProperty::Opacity(value),
                    )
                    .ok()
                });
                drop(st);
                if let Some(edit) = edit {
                    s.state.borrow_mut().apply(Box::new(edit));
                }
                s.notify();
            });
        }

        {
            let s = sidebar.clone();
            blend.connect_selected_notify(move |d| {
                if s.updating.get() {
                    return;
                }
                let Some(mode) = BlendMode::ALL.get(d.selected() as usize).copied() else {
                    return;
                };
                let st = s.state.borrow();
                let edit = st.document.primary_active().and_then(|id| {
                    pixelmagic_core::history::SetLayerProperty::new(
                        &st.document,
                        id,
                        pixelmagic_core::history::LayerProperty::Blend(mode),
                    )
                    .ok()
                });
                drop(st);
                if let Some(edit) = edit {
                    s.state.borrow_mut().apply(Box::new(edit));
                }
                s.notify();
            });
        }

        sidebar.refresh();
        sidebar
    }

    pub fn connect_changed<F: Fn() + 'static>(&self, f: F) {
        self.on_change.borrow_mut().push(Rc::new(f));
    }

    fn notify(&self) {
        let handlers: Vec<Rc<dyn Fn()>> = self.on_change.borrow().clone();
        for f in handlers {
            f();
        }
    }

    /// Layer ids currently shown, honouring the search filter — the mapping
    /// from row index back to layer.
    fn visible_layers(&self) -> Vec<LayerId> {
        let needle = self.search.text().to_lowercase();
        let st = self.state.borrow();
        st.document
            .layers
            .iter_depth_first()
            .into_iter()
            .filter(|(id, _)| {
                needle.is_empty()
                    || st
                        .document
                        .layers
                        .get(*id)
                        .map(|l| l.name.to_lowercase().contains(&needle))
                        .unwrap_or(false)
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// Rebuild the list from the document.
    pub fn refresh(&self) {
        self.updating.set(true);

        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        let visible = self.visible_layers();
        let st = self.state.borrow();
        let depths: std::collections::HashMap<LayerId, usize> =
            st.document.layers.iter_depth_first().into_iter().collect();
        let active = st.document.primary_active();
        let mut active_index = None;

        for (index, id) in visible.iter().enumerate() {
            let Some(layer) = st.document.layers.get(*id) else { continue };
            let depth = depths.get(id).copied().unwrap_or(0);

            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.add_css_class("pm-layer-row");
            row.set_margin_start(6 + depth as i32 * 14);

            row.append(&build_thumbnail(layer));

            let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
            text.set_valign(gtk::Align::Center);
            text.set_hexpand(true);
            let name = gtk::Label::new(Some(&layer.name));
            name.add_css_class("pm-layer-name");
            name.set_xalign(0.0);
            name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            text.append(&name);

            let meta = gtk::Label::new(Some(&layer_meta(layer)));
            meta.add_css_class("pm-layer-meta");
            meta.set_xalign(0.0);
            meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text.append(&meta);
            row.append(&text);

            if let Some(tag) = layer.color_tag.color() {
                let swatch = gtk::DrawingArea::new();
                swatch.set_size_request(8, 8);
                swatch.set_valign(gtk::Align::Center);
                swatch.set_draw_func(move |_, cr, w, h| {
                    cr.set_source_rgb(tag.r as f64, tag.g as f64, tag.b as f64);
                    cr.arc(w as f64 / 2.0, h as f64 / 2.0, 4.0, 0.0, std::f64::consts::TAU);
                    let _ = cr.fill();
                });
                row.append(&swatch);
            }
            if layer.locked {
                let lock = gtk::Label::new(Some("🔒"));
                lock.set_valign(gtk::Align::Center);
                row.append(&lock);
            }

            let eye = gtk::CheckButton::new();
            eye.add_css_class("pm-visible");
            eye.set_active(layer.visible);
            eye.set_valign(gtk::Align::Center);
            eye.set_tooltip_text(Some("Show or hide this layer"));
            {
                let state = self.state.clone();
                let id = *id;
                eye.connect_toggled(move |b| {
                    let mut st = state.borrow_mut();
                    if let Some(l) = st.document.layers.get_mut(id) {
                        if l.visible != b.is_active() {
                            l.visible = b.is_active();
                            st.document.dirty = true;
                            st.needs_redraw = true;
                        }
                    }
                });
            }
            row.append(&eye);

            let list_row = gtk::ListBoxRow::new();
            list_row.set_child(Some(&row));
            self.list.append(&list_row);

            if Some(*id) == active {
                active_index = Some(index);
            }
        }

        if let Some(i) = active_index {
            if let Some(row) = self.list.row_at_index(i as i32) {
                self.list.select_row(Some(&row));
            }
        }

        if let Some(layer) = st.document.active_layer() {
            self.opacity.set_value((layer.opacity * 100.0) as f64);
            self.opacity_value.set_text(&format!("{:.0}%", layer.opacity * 100.0));
            self.blend.set_selected(layer.blend_mode.shader_index());
            self.opacity.set_sensitive(true);
            self.blend.set_sensitive(true);
        } else {
            self.opacity.set_sensitive(false);
            self.blend.set_sensitive(false);
            self.opacity_value.set_text("—");
        }

        drop(st);
        self.updating.set(false);
    }
}

/// The line under a layer's name: its pixel dimensions, or its kind for the
/// layers that have none.
fn layer_meta(layer: &pixelmagic_core::layer::Layer) -> String {
    match &layer.kind {
        LayerKind::Pixel { buffer } if !buffer.is_empty() => {
            format!("{} × {} px", buffer.width(), buffer.height())
        }
        other => other.type_label().to_string(),
    }
}

/// A small preview of the layer's content.
///
/// Built by point-sampling the layer's buffer rather than filtering it. A
/// thumbnail is 32 px; the difference between a box filter and nearest
/// neighbour is invisible at that size, and point sampling costs nothing even
/// for a 24-megapixel layer.
fn build_thumbnail(layer: &pixelmagic_core::layer::Layer) -> gtk::Widget {
    const SIZE: u32 = 32;

    let picture = gtk::Picture::new();
    picture.set_size_request(SIZE as i32, SIZE as i32);
    picture.add_css_class("pm-thumb");
    picture.set_valign(gtk::Align::Center);
    picture.set_content_fit(gtk::ContentFit::Contain);

    if let LayerKind::Pixel { buffer } = &layer.kind {
        if !buffer.is_empty() {
            let (sw, sh) = (buffer.width(), buffer.height());
            let scale = (sw.max(sh) as f32 / SIZE as f32).max(1.0);
            let (tw, th) = (
                ((sw as f32 / scale).round() as u32).max(1),
                ((sh as f32 / scale).round() as u32).max(1),
            );

            let mut data = Vec::with_capacity((tw * th * 4) as usize);
            for y in 0..th {
                for x in 0..tw {
                    let sx = ((x as f32 + 0.5) * scale) as u32;
                    let sy = ((y as f32 + 0.5) * scale) as u32;
                    let c = buffer
                        .get(sx.min(sw - 1), sy.min(sh - 1))
                        .unwrap_or(pixelmagic_core::color::Rgba::TRANSPARENT);
                    data.extend_from_slice(&c.to_u8());
                }
            }

            let texture = gtk::gdk::MemoryTexture::new(
                tw as i32,
                th as i32,
                gtk::gdk::MemoryFormat::R8g8b8a8,
                &gtk::glib::Bytes::from_owned(data),
                (tw * 4) as usize,
            );
            picture.set_paintable(Some(&texture));
            return picture.upcast();
        }
    }

    // Non-pixel layers get their type glyph instead of an image.
    let label = gtk::Label::new(Some(kind_glyph(&layer.kind)));
    label.add_css_class("pm-thumb");
    label.set_size_request(SIZE as i32, SIZE as i32);
    label.set_valign(gtk::Align::Center);
    label.upcast()
}

fn kind_glyph(kind: &LayerKind) -> &'static str {
    match kind {
        LayerKind::Group => "🗀",
        LayerKind::Pixel { .. } => "▦",
        LayerKind::Shape { .. } => "⬟",
        LayerKind::Text { .. } => "T",
        LayerKind::ColorAdjustments => "◐",
        LayerKind::Effects => "✦",
        LayerKind::Video { .. } => "▶",
    }
}

// ---------------------------------------------------------------------------
// Arrange panel
// ---------------------------------------------------------------------------

/// The Arrange tool's options: ordering, alignment, size, position, rotation
/// and the layer-level commands.
///
/// This is the densest panel in the original and the one that makes the window
/// look like an image editor rather than a viewer. Controls that are not backed
/// by working code are present but insensitive, with a tooltip saying so —
/// leaving them out entirely would misrepresent the shape of the tool, and
/// leaving them live would misrepresent what it does.
pub fn build_arrange_panel(
    state: Rc<RefCell<EditorState>>,
    on_change: Rc<dyn Fn()>,
) -> gtk::Widget {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer.set_margin_start(8);
    outer.set_margin_end(8);
    outer.set_margin_bottom(10);
    outer.set_vexpand(true);

    let has_layer = state.borrow().document.primary_active().is_some();

    // -- stacking order ------------------------------------------------------
    //
    // Two rows of two rather than one row of four. A horizontal `GtkBox`
    // reports the *sum* of its children's minimum widths as its own, and
    // `set_propagate_natural_width(false)` on the enclosing scroller does not
    // suppress that — so "Back Front Backward Forward" on one line silently
    // widened the whole options panel past its 272px and shoved it over the
    // canvas. Anything added here has to survive that constraint.
    let mut order_row: Option<gtk::Box> = None;
    for (index, (label, delta)) in
        [("Back", i32::MAX), ("Front", i32::MIN), ("Backward", 1), ("Forward", -1)]
            .into_iter()
            .enumerate()
    {
        let row = match (index % 2, order_row.clone()) {
            (0, _) | (_, None) => {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                row.set_homogeneous(true);
                row.set_margin_bottom(4);
                outer.append(&row);
                order_row = Some(row.clone());
                row
            }
            (_, Some(row)) => row,
        };

        let b = gtk::Button::with_label(label);
        b.set_sensitive(has_layer);
        let state = state.clone();
        let on_change = on_change.clone();
        b.connect_clicked(move |_| {
            {
                let mut st = state.borrow_mut();
                if let Some(id) = st.document.primary_active() {
                    // Root order is front-most first, so "Front" means index 0.
                    let _ = st.document.layers.reorder(id, delta as isize);
                    st.document.dirty = true;
                    st.needs_redraw = true;
                }
            }
            on_change();
        });
        row.append(&b);
    }
    if let Some(row) = order_row {
        row.set_margin_bottom(8);
    }

    // -- alignment, against the canvas ---------------------------------------
    outer.append(&section_label("Align"));
    for row_spec in [
        [("⤒", Align::Top), ("⇕", Align::VCenter), ("⤓", Align::Bottom)],
        [("⇤", Align::Left), ("⇔", Align::HCenter), ("⇥", Align::Right)],
    ] {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        row.set_homogeneous(true);
        row.set_margin_bottom(4);
        for (glyph, align) in row_spec {
            let b = gtk::Button::with_label(glyph);
            b.set_tooltip_text(Some(align.tooltip()));
            b.set_sensitive(has_layer);
            let state = state.clone();
            let on_change = on_change.clone();
            b.connect_clicked(move |_| {
                align_active_layer(&state, align);
                on_change();
            });
            row.append(&b);
        }
        outer.append(&row);
    }

    // -- size ----------------------------------------------------------------
    let geom = active_geometry(&state);
    outer.append(&section_label("Size"));

    let constrain = gtk::CheckButton::with_label("Constrain proportions");
    constrain.set_active(true);
    let constrain_flag = Rc::new(std::cell::Cell::new(true));
    {
        let flag = constrain_flag.clone();
        constrain.connect_toggled(move |b| flag.set(b.is_active()));
    }

    let (size_row, width_spin, height_spin) = paired_spins(
        "W:",
        "H:",
        geom.map(|g| g.width).unwrap_or(0.0),
        geom.map(|g| g.height).unwrap_or(0.0),
        1.0,
        20000.0,
    );
    size_row.set_sensitive(has_layer);
    outer.append(&size_row);
    outer.append(&constrain);

    let original = gtk::Button::with_label("Original Size");
    original.set_margin_top(4);
    original.set_sensitive(has_layer);
    {
        let state = state.clone();
        let on_change = on_change.clone();
        original.connect_clicked(move |_| {
            with_active_transform(&state, |d| {
                d.scale = glam::Vec2::new(d.scale.x.signum(), d.scale.y.signum());
            });
            on_change();
        });
    }
    outer.append(&original);

    // Wiring the two spins after both exist, so each can read the other for the
    // constrain-proportions case.
    {
        let (state, on_change) = (state.clone(), on_change.clone());
        let (flag, other) = (constrain_flag.clone(), height_spin.clone());
        let base = geom;
        width_spin.connect_value_changed(move |sb| {
            let Some(base) = base else { return };
            let ratio = sb.value() as f32 / base.width.max(1e-3);
            with_active_transform(&state, |d| {
                d.scale.x = ratio * d.scale.x.signum() * base.scale.x.abs()
                    / base.scale.x.abs().max(1e-6);
                d.scale.x = ratio * base.scale.x.signum();
                if flag.get() {
                    d.scale.y = ratio * base.scale.y.signum();
                }
            });
            if flag.get() {
                other.set_value((base.height * ratio) as f64);
            }
            on_change();
        });
    }
    {
        let (state, on_change) = (state.clone(), on_change.clone());
        let (flag, other) = (constrain_flag.clone(), width_spin.clone());
        let base = geom;
        height_spin.connect_value_changed(move |sb| {
            let Some(base) = base else { return };
            let ratio = sb.value() as f32 / base.height.max(1e-3);
            with_active_transform(&state, |d| {
                d.scale.y = ratio * base.scale.y.signum();
                if flag.get() {
                    d.scale.x = ratio * base.scale.x.signum();
                }
            });
            if flag.get() {
                other.set_value((base.width * ratio) as f64);
            }
            on_change();
        });
    }

    // -- position ------------------------------------------------------------
    outer.append(&section_label("Position"));
    let (pos_row, x_spin, y_spin) = paired_spins(
        "X:",
        "Y:",
        geom.map(|g| g.x).unwrap_or(0.0),
        geom.map(|g| g.y).unwrap_or(0.0),
        -20000.0,
        20000.0,
    );
    pos_row.set_sensitive(has_layer);
    outer.append(&pos_row);
    {
        let (state, on_change) = (state.clone(), on_change.clone());
        x_spin.connect_value_changed(move |sb| {
            let v = sb.value() as f32;
            with_active_transform(&state, |d| d.translation.x = v);
            on_change();
        });
    }
    {
        let (state, on_change) = (state.clone(), on_change.clone());
        y_spin.connect_value_changed(move |sb| {
            let v = sb.value() as f32;
            with_active_transform(&state, |d| d.translation.y = v);
            on_change();
        });
    }

    // -- rotate and flip -----------------------------------------------------
    outer.append(&section_label("Rotate"));
    let rotate_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    rotate_row.set_sensitive(has_layer);

    let angle = gtk::SpinButton::with_range(-360.0, 360.0, 1.0);
    angle.set_hexpand(true);
    angle.set_value(geom.map(|g| g.rotation_degrees as f64).unwrap_or(0.0));
    {
        let (state, on_change) = (state.clone(), on_change.clone());
        angle.connect_value_changed(move |sb| {
            let radians = (sb.value() as f32).to_radians();
            with_active_transform(&state, |d| d.rotation = radians);
            on_change();
        });
    }
    rotate_row.append(&angle);

    for (glyph, tip, horizontal) in
        [("⇋", "Flip horizontally", true), ("⇅", "Flip vertically", false)]
    {
        let b = gtk::Button::with_label(glyph);
        b.set_tooltip_text(Some(tip));
        let state = state.clone();
        let on_change = on_change.clone();
        b.connect_clicked(move |_| {
            with_active_transform(&state, |d| {
                if horizontal {
                    d.scale.x = -d.scale.x;
                } else {
                    d.scale.y = -d.scale.y;
                }
            });
            on_change();
        });
        rotate_row.append(&b);
    }
    outer.append(&rotate_row);

    // -- layer commands ------------------------------------------------------
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_margin_top(10);
    outer.append(&spacer);

    let locked = state.borrow().document.active_layer().map(|l| l.locked).unwrap_or(false);
    let visible = state.borrow().document.active_layer().map(|l| l.visible).unwrap_or(true);

    outer.append(&button_pair(
        ("Lock", has_layer && !locked, {
            let (state, on_change) = (state.clone(), on_change.clone());
            Box::new(move || {
                set_layer_locked(&state, true);
                on_change();
            }) as Box<dyn Fn()>
        }),
        ("Unlock", has_layer && locked, {
            let (state, on_change) = (state.clone(), on_change.clone());
            Box::new(move || {
                set_layer_locked(&state, false);
                on_change();
            })
        }),
    ));

    outer.append(&button_pair(
        ("Hide", has_layer && visible, {
            let (state, on_change) = (state.clone(), on_change.clone());
            Box::new(move || {
                set_layer_visible(&state, false);
                on_change();
            }) as Box<dyn Fn()>
        }),
        ("Show", has_layer && !visible, {
            let (state, on_change) = (state.clone(), on_change.clone());
            Box::new(move || {
                set_layer_visible(&state, true);
                on_change();
            })
        }),
    ));

    let group_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    group_row.set_homogeneous(true);
    group_row.set_margin_top(8);
    let group = gtk::Button::with_label("Group");
    group.set_action_name(Some("win.layer-group"));
    group.set_sensitive(has_layer);
    let ungroup = gtk::Button::with_label("Ungroup");
    ungroup.set_sensitive(
        state.borrow().document.active_layer().map(|l| l.is_group()).unwrap_or(false),
    );
    {
        let (state, on_change) = (state.clone(), on_change.clone());
        ungroup.connect_clicked(move |_| {
            {
                let mut st = state.borrow_mut();
                if let Some(id) = st.document.primary_active() {
                    let _ = st.document.layers.ungroup(id);
                    st.document.prune_active();
                    st.document.dirty = true;
                    st.needs_redraw = true;
                }
            }
            on_change();
        });
    }
    group_row.append(&group);
    group_row.append(&ungroup);
    outer.append(&group_row);

    for (label, why) in [
        ("Merge Layers", "Merging is not implemented yet"),
        ("Transform…", "Free transform is not implemented yet"),
        ("Warp…", "Warp is not implemented yet"),
    ] {
        let b = gtk::Button::with_label(label);
        b.set_margin_top(6);
        b.set_sensitive(false);
        b.set_tooltip_text(Some(why));
        outer.append(&b);
    }

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_width(false);
    scroller.set_vexpand(true);
    scroller.set_child(Some(&outer));
    scroller.upcast()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    HCenter,
    Right,
    Top,
    VCenter,
    Bottom,
}

impl Align {
    fn tooltip(self) -> &'static str {
        match self {
            Align::Left => "Align left edge to the canvas",
            Align::HCenter => "Centre horizontally in the canvas",
            Align::Right => "Align right edge to the canvas",
            Align::Top => "Align top edge to the canvas",
            Align::VCenter => "Centre vertically in the canvas",
            Align::Bottom => "Align bottom edge to the canvas",
        }
    }
}

/// The active layer's placement, as the panel displays it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Geometry {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    rotation_degrees: f32,
    scale: glam::Vec2,
}

fn active_geometry(state: &Rc<RefCell<EditorState>>) -> Option<Geometry> {
    let st = state.borrow();
    let layer = st.document.active_layer()?;
    let local = layer.local_bounds();
    let d = layer.transform.decompose();
    Some(Geometry {
        x: d.translation.x,
        y: d.translation.y,
        width: local.width * d.scale.x.abs(),
        height: local.height * d.scale.y.abs(),
        rotation_degrees: d.rotation.to_degrees(),
        scale: d.scale,
    })
}

/// Edit the active layer's transform through its decomposition.
fn with_active_transform(
    state: &Rc<RefCell<EditorState>>,
    f: impl FnOnce(&mut pixelmagic_core::geom::Decomposed),
) {
    let mut st = state.borrow_mut();
    let Some(id) = st.document.primary_active() else { return };
    let Some(layer) = st.document.layers.get_mut(id) else { return };
    if layer.locked {
        return;
    }
    let mut d = layer.transform.decompose();
    f(&mut d);
    layer.transform = pixelmagic_core::geom::Transform::compose(d);
    st.document.dirty = true;
    st.needs_redraw = true;
}

fn align_active_layer(state: &Rc<RefCell<EditorState>>, align: Align) {
    let (canvas_w, canvas_h) = {
        let st = state.borrow();
        (st.document.width as f32, st.document.height as f32)
    };
    let Some(g) = active_geometry(state) else { return };
    with_active_transform(state, |d| match align {
        Align::Left => d.translation.x = 0.0,
        Align::HCenter => d.translation.x = (canvas_w - g.width) * 0.5,
        Align::Right => d.translation.x = canvas_w - g.width,
        Align::Top => d.translation.y = 0.0,
        Align::VCenter => d.translation.y = (canvas_h - g.height) * 0.5,
        Align::Bottom => d.translation.y = canvas_h - g.height,
    });
}

fn set_layer_locked(state: &Rc<RefCell<EditorState>>, locked: bool) {
    let st = state.borrow();
    let edit = st.document.primary_active().and_then(|id| {
        pixelmagic_core::history::SetLayerProperty::new(
            &st.document,
            id,
            pixelmagic_core::history::LayerProperty::Locked(locked),
        )
        .ok()
    });
    drop(st);
    if let Some(edit) = edit {
        state.borrow_mut().apply(Box::new(edit));
    }
}

fn set_layer_visible(state: &Rc<RefCell<EditorState>>, visible: bool) {
    let st = state.borrow();
    let edit = st.document.primary_active().and_then(|id| {
        pixelmagic_core::history::SetLayerProperty::new(
            &st.document,
            id,
            pixelmagic_core::history::LayerProperty::Visible(visible),
        )
        .ok()
    });
    drop(st);
    if let Some(edit) = edit {
        state.borrow_mut().apply(Box::new(edit));
    }
}

fn section_label(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("pm-section");
    l.set_xalign(0.0);
    l.set_margin_top(10);
    l.set_margin_bottom(4);
    l
}

/// Two labelled spin buttons side by side, as in the Size and Position rows.
fn paired_spins(
    a_label: &str,
    b_label: &str,
    a_value: f32,
    b_value: f32,
    min: f64,
    max: f64,
) -> (gtk::Box, gtk::SpinButton, gtk::SpinButton) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let mut spins = Vec::new();
    for (label, value) in [(a_label, a_value), (b_label, b_value)] {
        let cell = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        cell.set_hexpand(true);
        let l = gtk::Label::new(Some(label));
        l.set_xalign(0.0);
        let spin = gtk::SpinButton::with_range(min, max, 1.0);
        spin.set_hexpand(true);
        // A `GtkSpinButton`'s minimum width is its character count plus the
        // stepper buttons, and two of these side by side set the minimum width
        // of the whole options panel: at six characters each the panel came out
        // 44px over budget and sat on top of the canvas. Three is the floor;
        // `max_width_chars` lets it show a five-digit dimension whenever there
        // is room, which at the panel's real width there is.
        spin.set_width_chars(3);
        spin.set_max_width_chars(7);
        // Value set before the handler is connected, so populating the panel
        // does not look like the user typing into it.
        spin.set_value(value as f64);
        cell.append(&l);
        cell.append(&spin);
        row.append(&cell);
        spins.push(spin);
    }
    (row, spins[0].clone(), spins[1].clone())
}

fn button_pair(a: (&str, bool, Box<dyn Fn()>), b: (&str, bool, Box<dyn Fn()>)) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.set_homogeneous(true);
    row.set_margin_top(4);
    for (label, sensitive, action) in [a, b] {
        let button = gtk::Button::with_label(label);
        button.set_sensitive(sensitive);
        button.connect_clicked(move |_| action());
        row.append(&button);
    }
    row
}

// ---------------------------------------------------------------------------
// Histogram
// ---------------------------------------------------------------------------

/// The histogram that sits at the top of the Color Adjustments pane
/// (`docs/SPEC.md` §3.1).
///
/// Drawn as three additively-blended channel fills, which is the conventional
/// look and is genuinely informative: where all three overlap you get white,
/// so neutral tones read as grey and a colour cast shows up as a coloured
/// fringe. A single luma curve would hide exactly the thing you open a
/// histogram to see.
pub fn build_histogram(hist: Option<pixelmagic_gpu::renderer::Histogram>) -> gtk::DrawingArea {
    use pixelmagic_gpu::renderer::Histogram;

    let area = gtk::DrawingArea::new();
    area.set_content_height(96);
    area.set_hexpand(true);
    area.add_css_class("pm-histogram");

    area.set_draw_func(move |_, cr, w, h| {
        let (w, h) = (w as f64, h as f64);

        cr.set_source_rgb(0.11, 0.11, 0.12);
        let _ = cr.paint();

        // Quarter-tone guides, so the eye can place shadows and highlights.
        cr.set_line_width(1.0);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.08);
        for i in 1..4 {
            let x = (w * i as f64 / 4.0).floor() + 0.5;
            cr.move_to(x, 0.0);
            cr.line_to(x, h);
        }
        let _ = cr.stroke();

        let Some(hist) = hist.as_ref() else {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.35);
            cr.move_to(8.0, h / 2.0);
            let _ = cr.show_text("No histogram yet");
            return;
        };
        if hist.is_empty() {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.35);
            cr.move_to(8.0, h / 2.0);
            let _ = cr.show_text("Empty document");
            return;
        }

        // Scaling to the tallest bin is the obvious choice and the wrong one.
        // A single flat region — a solid background, a clipped highlight, a
        // channel that only takes two values — produces a spike orders of
        // magnitude above everything else, and dividing by it flattens the
        // actual distribution into an invisible smear along the bottom.
        //
        // So: sort every bin across the three channels and scale to roughly the
        // 98th percentile, letting the handful of spikes clip off the top. The
        // shape of the distribution is what a histogram is read for; the exact
        // height of a spike is not.
        let mut sorted: Vec<u32> = [Histogram::RED, Histogram::GREEN, Histogram::BLUE]
            .iter()
            .flat_map(|c| hist.bins[*c].iter().copied())
            .filter(|v| *v > 0)
            .collect();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        let ceiling = sorted
            .get(sorted.len() / 50)
            .copied()
            .or_else(|| sorted.first().copied())
            .unwrap_or(1)
            .max(1) as f64;

        cr.set_operator(gtk::cairo::Operator::Add);
        for (channel, rgb) in [
            (Histogram::RED, (0.90, 0.20, 0.20)),
            (Histogram::GREEN, (0.20, 0.85, 0.30)),
            (Histogram::BLUE, (0.25, 0.45, 0.95)),
        ] {
            cr.move_to(0.0, h);
            for bin in 0..256 {
                let x = w * bin as f64 / 255.0;
                let v = (hist.bins[channel][bin] as f64 / ceiling).min(1.0);
                cr.line_to(x, h - v * (h - 2.0));
            }
            cr.line_to(w, h);
            cr.close_path();
            cr.set_source_rgba(rgb.0, rgb.1, rgb.2, 0.55);
            let _ = cr.fill();
        }
        cr.set_operator(gtk::cairo::Operator::Over);
    });

    area
}

// ---------------------------------------------------------------------------
// Adjustments and effects panels
// ---------------------------------------------------------------------------

/// The Color Adjustments pane: one expander per adjustment, each generated
/// from its parameter specs.
pub fn build_adjustments_panel(
    state: Rc<RefCell<EditorState>>,
    on_change: Rc<dyn Fn()>,
    histogram: Option<pixelmagic_gpu::renderer::Histogram>,
) -> gtk::Widget {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 6);
    outer.set_margin_top(8);
    outer.set_margin_bottom(8);
    outer.set_margin_start(8);
    outer.set_margin_end(8);

    let pixels = histogram.as_ref().map(|h| h.total).unwrap_or(0);
    outer.append(&build_histogram(histogram));
    if pixels > 0 {
        let caption = gtk::Label::new(Some(&format!("{pixels} px")));
        caption.add_css_class("dim-label");
        caption.add_css_class("caption");
        caption.set_xalign(1.0);
        outer.append(&caption);
    }

    for kind in AdjustmentKind::ALL {
        let expander = gtk::Expander::new(Some(kind.label()));
        let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
        body.set_margin_start(8);
        body.set_margin_top(4);
        body.set_margin_bottom(4);

        let probe = kind.new();
        let specs = probe.specs();

        if specs.is_empty() {
            // Levels, Curves, the colour wheels and the mixer need bespoke
            // editors; say so rather than showing an empty box.
            let note = gtk::Label::new(Some("Needs a dedicated editor — not built yet"));
            note.set_xalign(0.0);
            note.set_wrap(true);
            note.set_sensitive(false);
            body.append(&note);
        } else {
            for spec in &specs {
                let row = build_param_row(
                    spec,
                    {
                        let state = state.clone();
                        let key = spec.key;
                        move || {
                            let st = state.borrow();
                            let layer = st.document.active_layer()?;
                            layer
                                .adjustments
                                .iter()
                                .find(|a| a.adjustment.kind() == kind)
                                .and_then(|a| a.adjustment.get(key))
                        }
                    },
                    {
                        let state = state.clone();
                        let on_change = on_change.clone();
                        let key = spec.key;
                        move |value| {
                            {
                                let mut st = state.borrow_mut();
                                let Some(id) = st.document.primary_active() else { return };
                                let Some(layer) = st.document.layers.get_mut(id) else {
                                    return;
                                };
                                // Attach the adjustment on first use, so the
                                // pane behaves like Pixelmator's: dragging a
                                // slider is what creates it.
                                if !layer
                                    .adjustments
                                    .iter()
                                    .any(|a| a.adjustment.kind() == kind)
                                {
                                    layer.adjustments.push(
                                        pixelmagic_core::adjust::AdjustmentInstance::new(kind),
                                    );
                                }
                                if let Some(a) = layer
                                    .adjustments
                                    .iter_mut()
                                    .find(|a| a.adjustment.kind() == kind)
                                {
                                    a.adjustment.set(key, value);
                                }
                                st.document.dirty = true;
                                st.needs_redraw = true;
                            }
                            on_change();
                        }
                    },
                );
                body.append(&row);
            }
        }

        expander.set_child(Some(&body));
        outer.append(&expander);
    }

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_width(false);
    scroller.set_child(Some(&outer));
    scroller.set_vexpand(true);
    scroller.upcast()
}

/// The Effects browser: categories, each listing its effects.
pub fn build_effects_panel(
    state: Rc<RefCell<EditorState>>,
    on_change: Rc<dyn Fn()>,
) -> gtk::Widget {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 6);
    outer.set_margin_top(8);
    outer.set_margin_bottom(8);
    outer.set_margin_start(8);
    outer.set_margin_end(8);

    let (done, total) = pixelmagic_core::effect::implemented_count();
    let status = gtk::Label::new(Some(&format!("{done} of {total} effects implemented")));
    status.set_xalign(0.0);
    status.add_css_class("dim-label");
    outer.append(&status);

    for category in EffectCategory::ALL {
        let expander = gtk::Expander::new(Some(category.label()));
        let body = gtk::Box::new(gtk::Orientation::Vertical, 2);
        body.set_margin_start(8);

        for descriptor in EFFECTS.iter().filter(|d| d.category == category) {
            let button = gtk::Button::with_label(descriptor.label);
            button.add_css_class("flat");
            button.set_halign(gtk::Align::Fill);
            button.set_sensitive(descriptor.implemented);
            if !descriptor.implemented {
                button.set_tooltip_text(Some("Catalogued, but no shader yet"));
            }

            let id = descriptor.id;
            let state = state.clone();
            let on_change = on_change.clone();
            button.connect_clicked(move |_| {
                {
                    let mut st = state.borrow_mut();
                    let Some(layer_id) = st.document.primary_active() else { return };
                    let Some(effect) = pixelmagic_core::effect::Effect::new(id) else { return };
                    if let Some(layer) = st.document.layers.get_mut(layer_id) {
                        layer.effects.push(effect);
                        st.document.dirty = true;
                        st.needs_redraw = true;
                    }
                }
                on_change();
            });
            body.append(&button);
        }

        expander.set_child(Some(&body));
        outer.append(&expander);
    }

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_width(false);
    scroller.set_child(Some(&outer));
    scroller.set_vexpand(true);
    scroller.upcast()
}

/// Options for the current tool: brush settings, selection settings, and so on.
pub fn build_tool_options(
    state: Rc<RefCell<EditorState>>,
    on_change: Rc<dyn Fn()>,
) -> gtk::Widget {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 6);
    outer.set_margin_top(8);
    outer.set_margin_bottom(8);
    outer.set_margin_start(8);
    outer.set_margin_end(8);

    let tool = state.borrow().tool;
    let description = gtk::Label::new(Some(tool.info().description));
    description.set_xalign(0.0);
    description.set_wrap(true);
    description.add_css_class("dim-label");
    outer.append(&description);

    if tool.needs_pixel_layer() {
        for (label, min, max, get, set) in brush_controls() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let l = gtk::Label::new(Some(label));
            l.set_xalign(0.0);
            l.set_width_chars(9);
            row.append(&l);

            let scale = gtk::Scale::with_range(
                gtk::Orientation::Horizontal,
                min,
                max,
                (max - min) / 100.0,
            );
            scale.set_hexpand(true);
            scale.set_draw_value(true);
            scale.set_value(get(&state.borrow().brush));

            let state = state.clone();
            let on_change = on_change.clone();
            scale.connect_value_changed(move |s| {
                set(&mut state.borrow_mut().brush, s.value());
                on_change();
            });
            row.append(&scale);
            outer.append(&row);
        }
    }

    if tool.is_selection() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let l = gtk::Label::new(Some("Feather"));
        l.set_xalign(0.0);
        l.set_width_chars(9);
        row.append(&l);

        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        scale.set_hexpand(true);
        scale.set_draw_value(true);
        scale.set_value(state.borrow().selection_options.feather as f64);
        {
            let state = state.clone();
            scale.connect_value_changed(move |s| {
                state.borrow_mut().selection_options.feather = s.value() as f32;
            });
        }
        row.append(&scale);
        outer.append(&row);

        let antialias = gtk::CheckButton::with_label("Anti-alias");
        antialias.set_active(state.borrow().selection_options.antialias);
        {
            let state = state.clone();
            antialias.connect_toggled(move |b| {
                state.borrow_mut().selection_options.antialias = b.is_active();
            });
        }
        outer.append(&antialias);
    }

    // Wrap in a scroller. A bare `Box` propagates its children's natural width
    // to its parent, and a row of sliders wants to be very wide indeed — which
    // is what was squeezing the canvas out of the window.
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_width(false);
    scroller.set_child(Some(&outer));
    scroller.set_vexpand(true);
    scroller.upcast()
}

type BrushControl = (
    &'static str,
    f64,
    f64,
    fn(&pixelmagic_core::tool::BrushSettings) -> f64,
    fn(&mut pixelmagic_core::tool::BrushSettings, f64),
);

fn brush_controls() -> Vec<BrushControl> {
    vec![
        ("Size", 1.0, 500.0, |b| b.size as f64, |b, v| b.size = v as f32),
        ("Softness", 0.0, 1.0, |b| b.softness as f64, |b, v| b.softness = v as f32),
        ("Opacity", 0.0, 1.0, |b| b.opacity as f64, |b, v| b.opacity = v as f32),
        ("Flow", 0.0, 1.0, |b| b.flow as f64, |b, v| b.flow = v as f32),
        ("Spacing", 0.01, 1.0, |b| b.spacing as f64, |b, v| b.spacing = v as f32),
    ]
}

// ---------------------------------------------------------------------------
// Quick Selection
// ---------------------------------------------------------------------------

/// The Quick Selection panel.
///
/// Laid out after the original: the four combine modes as radio buttons with
/// a line of explanation each, then the brush size, then the checkboxes, then
/// the command buttons. Two of those commands are not implemented and say so
/// rather than being left out — a panel missing "Select Subject" would suggest
/// the tool is complete, and it is not.
pub fn build_quick_select_panel(
    state: Rc<RefCell<EditorState>>,
    on_change: Rc<dyn Fn()>,
) -> gtk::Widget {
    use pixelmagic_core::buffer::MaskOp;

    let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    outer.set_margin_start(8);
    outer.set_margin_end(8);
    outer.set_margin_bottom(10);
    outer.set_vexpand(true);

    // -- combine mode --------------------------------------------------------
    let current_mode = state.borrow().quick_select.mode;
    let mut first_radio: Option<gtk::CheckButton> = None;

    for (mode, blurb) in [
        (MaskOp::Replace, "Creates a new selection."),
        (MaskOp::Add, "Allows you to add areas to your existing selection."),
        (MaskOp::Subtract, "Allows you to subtract areas from your existing selection."),
        (
            MaskOp::Intersect,
            "Constrains the bounds of the new selection to the existing selection.",
        ),
    ] {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_margin_top(6);

        let radio = gtk::CheckButton::new();
        radio.set_valign(gtk::Align::Start);
        radio.add_css_class("pm-radio");
        match &first_radio {
            None => first_radio = Some(radio.clone()),
            Some(f) => radio.set_group(Some(f)),
        }
        radio.set_active(mode == current_mode);

        let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // "New" rather than "Replace": the model's name for the operation is
        // not the user's name for the button.
        let name = gtk::Label::new(Some(quick_select_mode_label(mode)));
        name.set_xalign(0.0);
        let detail = gtk::Label::new(Some(blurb));
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        detail.add_css_class("dim-label");
        detail.add_css_class("caption");
        text.append(&name);
        text.append(&detail);

        row.append(&radio);
        row.append(&text);
        outer.append(&row);

        let state = state.clone();
        let on_change = on_change.clone();
        radio.connect_toggled(move |r| {
            if r.is_active() {
                state.borrow_mut().quick_select.mode = mode;
                on_change();
            }
        });
    }

    // -- brush size ----------------------------------------------------------
    let size_head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    size_head.set_margin_top(12);
    let size_label = gtk::Label::new(Some("Brush Size"));
    size_label.set_xalign(0.0);
    size_label.set_hexpand(true);
    let size_value = gtk::Label::new(None);
    size_value.add_css_class("dim-label");
    size_head.append(&size_label);
    size_head.append(&size_value);
    outer.append(&size_head);

    let size = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    size.add_css_class("pm-opacity");
    size.set_draw_value(false);
    size.set_value((state.borrow().quick_select.reach_percent() * 100.0) as f64);
    size_value.set_text(&format!("{:.0}%", size.value()));
    outer.append(&size);
    {
        let state = state.clone();
        let size_value = size_value.clone();
        let on_change = on_change.clone();
        size.connect_value_changed(move |s| {
            state.borrow_mut().quick_select.set_reach_percent(s.value() as f32 / 100.0);
            size_value.set_text(&format!("{:.0}%", s.value()));
            on_change();
        });
    }

    // -- tolerance -----------------------------------------------------------
    //
    // Not in the original's panel. Ours is a colour flood fill rather than a
    // trained segmentation, so this is the control that decides whether the
    // tool works at all on a given image; hiding it would be cargo-culting the
    // interface without the machinery that lets the original get away with it.
    let tol_head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    tol_head.set_margin_top(8);
    let tol_label = gtk::Label::new(Some("Tolerance"));
    tol_label.set_xalign(0.0);
    tol_label.set_hexpand(true);
    tol_label.set_tooltip_text(Some(
        "How different a neighbouring pixel may be before the region stops growing.",
    ));
    let tol_value = gtk::Label::new(None);
    tol_value.add_css_class("dim-label");
    tol_head.append(&tol_label);
    tol_head.append(&tol_value);
    outer.append(&tol_head);

    let tol = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 100.0, 1.0);
    tol.add_css_class("pm-opacity");
    tol.set_draw_value(false);
    tol.set_value((state.borrow().quick_select.tolerance * 100.0) as f64);
    tol_value.set_text(&format!("{:.0}%", tol.value()));
    outer.append(&tol);
    {
        let state = state.clone();
        let tol_value = tol_value.clone();
        let on_change = on_change.clone();
        tol.connect_value_changed(move |s| {
            state.borrow_mut().quick_select.tolerance = s.value() as f32 / 100.0;
            tol_value.set_text(&format!("{:.0}%", s.value()));
            on_change();
        });
    }

    // -- checkboxes ----------------------------------------------------------
    for (label, tooltip, get, set) in [
        (
            "Sample all layers",
            "Grow the region from the composited image rather than the active layer alone.",
            Box::new(|s: &EditorState| s.quick_select.sample_all_layers)
                as Box<dyn Fn(&EditorState) -> bool>,
            Box::new(|s: &mut EditorState, v: bool| s.quick_select.sample_all_layers = v)
                as Box<dyn Fn(&mut EditorState, bool)>,
        ),
        (
            "Show selection preview",
            "Highlight in yellow what would be selected under the pointer.",
            Box::new(|s: &EditorState| s.quick_select.show_preview),
            Box::new(|s: &mut EditorState, v: bool| s.quick_select.show_preview = v),
        ),
    ] {
        let check = gtk::CheckButton::with_label(label);
        check.set_margin_top(8);
        check.set_tooltip_text(Some(tooltip));
        check.add_css_class("pm-visible");
        check.set_active(get(&state.borrow()));
        let state = state.clone();
        let on_change = on_change.clone();
        check.connect_toggled(move |c| {
            set(&mut state.borrow_mut(), c.is_active());
            on_change();
        });
        outer.append(&check);
    }

    // -- commands ------------------------------------------------------------
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_size_request(-1, 8);
    outer.append(&spacer);

    for (label, why) in [
        (
            "Select Subject",
            "Not implemented: finding the subject of a photograph needs a trained \
             segmentation model, which Pixelmagic does not ship yet.",
        ),
        (
            "Select and Mask…",
            "Not implemented: this opens a dedicated edge-refinement workspace.",
        ),
    ] {
        let b = gtk::Button::with_label(label);
        b.set_margin_top(6);
        b.set_sensitive(false);
        b.set_tooltip_text(Some(why));
        outer.append(&b);
    }

    let invert = gtk::Button::with_label("Invert Selection");
    invert.set_margin_top(6);
    invert.set_action_name(Some("win.select-invert"));
    outer.append(&invert);

    let reselect = gtk::Button::with_label("Reselect");
    reselect.set_margin_top(6);
    reselect.set_sensitive(state.borrow().can_reselect());
    reselect.set_tooltip_text(Some("Bring back the selection you last dismissed."));
    {
        let state = state.clone();
        let on_change = on_change.clone();
        reselect.connect_clicked(move |_| {
            let restored = state.borrow_mut().reselect();
            if restored {
                on_change();
            }
        });
    }
    outer.append(&reselect);

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_width(false);
    scroller.set_vexpand(true);
    scroller.set_child(Some(&outer));
    scroller.upcast()
}

/// The panel's name for a combine mode, which is not the model's name for it:
/// the operation is `Replace`, the button is "New".
fn quick_select_mode_label(op: pixelmagic_core::buffer::MaskOp) -> &'static str {
    use pixelmagic_core::buffer::MaskOp;
    match op {
        MaskOp::Replace => "New",
        MaskOp::Add => "Add",
        MaskOp::Subtract => "Subtract",
        MaskOp::Intersect => "Intersect",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_glyph() {
        for info in pixelmagic_core::tool::TOOLS {
            let g = tool_glyph(info.tool);
            assert!(!g.is_empty(), "{} has no glyph", info.id);
        }
    }

    #[test]
    fn brush_controls_cover_the_main_settings() {
        let names: Vec<&str> = brush_controls().iter().map(|c| c.0).collect();
        assert!(names.contains(&"Size"));
        assert!(names.contains(&"Softness"));
        assert!(names.contains(&"Opacity"));
    }

    #[test]
    fn brush_control_getters_and_setters_agree() {
        let mut b = pixelmagic_core::tool::BrushSettings::default();
        for (name, min, max, get, set) in brush_controls() {
            set(&mut b, max);
            assert!((get(&b) - max).abs() < 1e-6, "{name} did not round-trip its max");
            set(&mut b, min);
            assert!((get(&b) - min).abs() < 1e-6, "{name} did not round-trip its min");
        }
    }
}
