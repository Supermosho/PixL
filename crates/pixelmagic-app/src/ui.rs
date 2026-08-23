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
// Tools sidebar
// ---------------------------------------------------------------------------

pub struct ToolsSidebar {
    pub widget: gtk::Box,
    buttons: RefCell<Vec<(Tool, gtk::ToggleButton)>>,
    /// Set while the sidebar is syncing itself to the current tool, so the
    /// `toggled` signals that causes do not read as user clicks and bounce
    /// straight back into the state that produced them.
    updating: std::cell::Cell<bool>,
}

impl ToolsSidebar {
    pub fn new(
        state: Rc<RefCell<EditorState>>,
        on_select: impl Fn(Tool) + 'static,
    ) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.add_css_class("tools-sidebar");
        widget.set_width_request(56);

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_vexpand(true);

        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        list.set_margin_top(6);
        list.set_margin_bottom(6);

        let sidebar = Rc::new(ToolsSidebar {
            widget: widget.clone(),
            buttons: RefCell::new(Vec::new()),
            updating: std::cell::Cell::new(false),
        });
        let on_select = Rc::new(on_select);

        for (i, category) in ToolCategory::ALL.iter().enumerate() {
            if i > 0 {
                let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
                sep.set_margin_top(4);
                sep.set_margin_bottom(4);
                sep.set_margin_start(10);
                sep.set_margin_end(10);
                list.append(&sep);
            }
            for info in tools_in(*category) {
                let button = gtk::ToggleButton::new();
                button.add_css_class("flat");
                button.add_css_class("tool-button");
                button.set_child(Some(&gtk::Label::new(Some(&tool_glyph(info.tool)))));

                let shortcut = info
                    .shortcut
                    .map(|c| format!("  ({})", c.to_ascii_uppercase()))
                    .unwrap_or_default();
                let status = if info.implemented { "" } else { "  — not implemented yet" };
                button.set_tooltip_text(Some(&format!(
                    "{}{shortcut}\n{}{status}",
                    info.label, info.description
                )));
                // Tools without an implementation stay visible but inert, so the
                // roster reads as a roadmap rather than a set of traps.
                button.set_sensitive(info.implemented);

                let tool = info.tool;
                let on_select = on_select.clone();
                let sidebar_weak = Rc::downgrade(&sidebar);
                button.connect_toggled(move |b| {
                    let Some(sidebar) = sidebar_weak.upgrade() else { return };
                    if sidebar.updating.get() || !b.is_active() {
                        return;
                    }
                    sidebar.set_active_silently(tool);
                    on_select(tool);
                });

                sidebar.buttons.borrow_mut().push((tool, button.clone()));
                list.append(&button);
            }
        }

        scroller.set_child(Some(&list));
        widget.append(&scroller);

        let current = state.borrow().tool;
        sidebar.set_active_silently(current);
        sidebar
    }

    /// Update which button looks active without re-firing `on_select`.
    pub fn set_active_silently(&self, tool: Tool) {
        // `set_active` emits `toggled` synchronously; the guard flag is what
        // stops that turning into an infinite ping-pong between the sidebar and
        // the editor state.
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
/// Deliberately typographic rather than iconographic: Pixelmagic does not ship
/// Apple's icon set, and inventing 50 pictograms is a design project of its
/// own. Letters and symbols are honest placeholders that stay legible.
fn tool_glyph(tool: Tool) -> String {
    use Tool::*;
    match tool {
        Style => "◆",
        Arrange => "✥",
        ColorAdjustments => "◐",
        Effects => "✦",
        Crop => "⌗",
        ExportForWeb => "⤓",
        ColorPicker => "⌖",
        Zoom => "⌕",
        Hand => "✋",
        RectangularSelection => "▭",
        OvalSelection => "◯",
        RowSelection => "▬",
        ColumnSelection => "▮",
        FreeSelection => "◌",
        PolygonalSelection => "⬠",
        MagneticSelection => "⌁",
        ColorSelection => "◍",
        QuickSelection => "⌾",
        Paint => "🖌",
        PixelPaint => "▦",
        ColorFill => "🪣",
        GradientFill => "▤",
        Erase => "◻",
        SmartErase => "⌫",
        Repair => "✚",
        Clone => "⎘",
        Sharpen => "△",
        Soften => "◠",
        Smudge => "≈",
        Lighten => "☀",
        Darken => "☾",
        Saturate => "◉",
        Desaturate => "○",
        Distort => "∿",
        Bump => "◗",
        Pinch => "◖",
        Twirl => "🌀",
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
        FreeformType => "⌁",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Layers sidebar
// ---------------------------------------------------------------------------

pub struct LayersSidebar {
    pub widget: gtk::Box,
    list: gtk::ListBox,
    opacity: gtk::Scale,
    blend: gtk::DropDown,
    state: Rc<RefCell<EditorState>>,
    on_change: RefCell<Vec<Rc<dyn Fn()>>>,
    /// Set while rebuilding, so programmatic changes do not look like user
    /// input and write themselves back into the document.
    updating: std::cell::Cell<bool>,
}

impl LayersSidebar {
    pub fn new(state: Rc<RefCell<EditorState>>) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 6);
        widget.add_css_class("layers-sidebar");
        widget.set_width_request(260);
        widget.set_margin_top(6);
        widget.set_margin_bottom(6);
        widget.set_margin_start(6);
        widget.set_margin_end(6);

        // Blend mode and opacity sit above the list, as they do in the
        // original: they apply to the selected layer, not to the document.
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let blend_model = gtk::StringList::new(&[]);
        for group in BlendGroup::ALL {
            for mode in BlendMode::ALL.iter().filter(|m| m.group() == group) {
                blend_model.append(mode.label());
            }
        }
        let blend = gtk::DropDown::new(Some(blend_model), gtk::Expression::NONE);
        blend.set_hexpand(true);
        header.append(&blend);

        let opacity = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        opacity.set_value(100.0);
        opacity.set_hexpand(true);
        opacity.set_draw_value(true);
        opacity.set_value_pos(gtk::PositionType::Right);

        let opacity_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let opacity_label = gtk::Label::new(Some("Opacity"));
        opacity_label.set_xalign(0.0);
        opacity_row.append(&opacity_label);
        opacity_row.append(&opacity);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.add_css_class("navigation-sidebar");

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&list));

        widget.append(&header);
        widget.append(&opacity_row);
        widget.append(&scroller);

        let sidebar = Rc::new(LayersSidebar {
            widget,
            list: list.clone(),
            opacity: opacity.clone(),
            blend: blend.clone(),
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
                let index = row.index();
                let ids: Vec<LayerId> = s
                    .state
                    .borrow()
                    .document
                    .layers
                    .iter_depth_first()
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect();
                if let Some(id) = ids.get(index as usize) {
                    s.state.borrow_mut().document.set_active(vec![*id]);
                }
                s.notify();
            });
        }

        {
            let s = sidebar.clone();
            opacity.connect_value_changed(move |sc| {
                if s.updating.get() {
                    return;
                }
                let value = (sc.value() / 100.0) as f32;
                let st = s.state.borrow_mut();
                if let Some(id) = st.document.primary_active() {
                    let edit = pixelmagic_core::history::SetLayerProperty::new(
                        &st.document,
                        id,
                        pixelmagic_core::history::LayerProperty::Opacity(value),
                    );
                    if let Ok(edit) = edit {
                        drop(st);
                        s.state.borrow_mut().apply(Box::new(edit));
                    }
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
                let st = s.state.borrow_mut();
                if let Some(id) = st.document.primary_active() {
                    let edit = pixelmagic_core::history::SetLayerProperty::new(
                        &st.document,
                        id,
                        pixelmagic_core::history::LayerProperty::Blend(mode),
                    );
                    if let Ok(edit) = edit {
                        drop(st);
                        s.state.borrow_mut().apply(Box::new(edit));
                    }
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

    /// Rebuild the list from the document.
    pub fn refresh(&self) {
        self.updating.set(true);

        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        let st = self.state.borrow();
        let entries = st.document.layers.iter_depth_first();
        let active = st.document.primary_active();
        let mut active_index = None;

        for (index, (id, depth)) in entries.iter().enumerate() {
            let Some(layer) = st.document.layers.get(*id) else { continue };
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            row.set_margin_start(6 + *depth as i32 * 14);
            row.set_margin_end(6);
            row.set_margin_top(3);
            row.set_margin_bottom(3);

            let eye = gtk::CheckButton::new();
            eye.set_active(layer.visible);
            eye.set_tooltip_text(Some("Show or hide this layer"));
            {
                let state = self.state.clone();
                let id = *id;
                eye.connect_toggled(move |b| {
                    let mut st = state.borrow_mut();
                    if let Some(l) = st.document.layers.get_mut(id) {
                        l.visible = b.is_active();
                        st.document.dirty = true;
                        st.needs_redraw = true;
                    }
                });
            }
            row.append(&eye);

            let kind = gtk::Label::new(Some(kind_glyph(&layer.kind)));
            kind.set_tooltip_text(Some(layer.kind.type_label()));
            row.append(&kind);

            let name = gtk::Label::new(Some(&layer.name));
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            row.append(&name);

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
                row.append(&gtk::Label::new(Some("🔒")));
            }

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

        // Reflect the active layer's own settings in the header controls.
        if let Some(layer) = st.document.active_layer() {
            self.opacity.set_value((layer.opacity * 100.0) as f64);
            self.blend.set_selected(layer.blend_mode.shader_index());
            self.opacity.set_sensitive(true);
            self.blend.set_sensitive(true);
        } else {
            self.opacity.set_sensitive(false);
            self.blend.set_sensitive(false);
        }

        drop(st);
        self.updating.set(false);
    }
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
// Adjustments and effects panels
// ---------------------------------------------------------------------------

/// The Color Adjustments pane: one expander per adjustment, each generated
/// from its parameter specs.
pub fn build_adjustments_panel(
    state: Rc<RefCell<EditorState>>,
    on_change: Rc<dyn Fn()>,
) -> gtk::Widget {
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 6);
    outer.set_margin_top(8);
    outer.set_margin_bottom(8);
    outer.set_margin_start(8);
    outer.set_margin_end(8);

    let heading = gtk::Label::new(Some("Color Adjustments"));
    heading.add_css_class("title-4");
    heading.set_xalign(0.0);
    outer.append(&heading);

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

    let heading = gtk::Label::new(Some("Effects"));
    heading.add_css_class("title-4");
    heading.set_xalign(0.0);
    outer.append(&heading);

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
    let heading = gtk::Label::new(Some(tool.label()));
    heading.add_css_class("title-4");
    heading.set_xalign(0.0);
    outer.append(&heading);

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
