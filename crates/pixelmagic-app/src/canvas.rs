//! The canvas: a `GLArea` that renders the document and handles tool input.

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use pixelmagic_core::buffer::{MaskOp, PixelBuffer};
use pixelmagic_core::geom::Rect;
use pixelmagic_core::history::PixelRegionEdit;
use pixelmagic_core::layer::LayerKind;
use pixelmagic_core::selection::Selection;
use pixelmagic_core::tool::{Tool, ToolCategory};
use pixelmagic_gpu::Renderer;
use std::cell::RefCell;
use std::rc::Rc;

use crate::brush::{self, BrushMode};
use crate::state::{EditorState, Gesture, View};

/// Owns the GL side of the canvas. Created on `realize`, destroyed on
/// `unrealize` — GL objects belong to the context and must not outlive it.
struct CanvasGl {
    renderer: Renderer,
}

pub struct Canvas {
    pub widget: gtk::GLArea,
    gl: Rc<RefCell<Option<CanvasGl>>>,
    state: Rc<RefCell<EditorState>>,
    /// Notified after any change the rest of the UI should react to.
    on_change: RefCell<Vec<Rc<dyn Fn()>>>,
    /// The undo record for a stroke in progress.
    ///
    /// Lives here rather than in `EditorState` because it is only meaningful
    /// between drag-begin and drag-end; putting a transient like this in the
    /// shared state invites it being left set after a cancelled gesture.
    pending_edit: RefCell<Option<PixelRegionEdit>>,
}

impl Canvas {
    pub fn new(state: Rc<RefCell<EditorState>>) -> Rc<Self> {
        let widget = gtk::GLArea::builder()
            .hexpand(true)
            .vexpand(true)
            .has_depth_buffer(false)
            .has_stencil_buffer(false)
            .can_focus(true)
            .focusable(true)
            .build();
        let canvas = Rc::new(Canvas {
            widget: widget.clone(),
            gl: Rc::new(RefCell::new(None)),
            state,
            on_change: RefCell::new(Vec::new()),
            pending_edit: RefCell::new(None),
        });

        canvas.connect_gl();
        canvas.connect_input();
        canvas
    }

    pub fn connect_changed<F: Fn() + 'static>(&self, f: F) {
        self.on_change.borrow_mut().push(Rc::new(f));
    }

    fn notify(&self) {
        // Clone the handler list before calling any of them. A handler may well
        // register another one — or re-enter something that does — and holding
        // the `RefCell` borrow across those calls would panic at runtime.
        let handlers: Vec<Rc<dyn Fn()>> = self.on_change.borrow().clone();
        for f in handlers {
            f();
        }
    }

    pub fn queue_redraw(&self) {
        self.state.borrow_mut().needs_redraw = true;
        self.widget.queue_render();
    }

    fn connect_gl(self: &Rc<Self>) {
        let gl_cell = self.gl.clone();
        self.widget.connect_realize(move |area| {
            area.make_current();
            if let Some(e) = area.error() {
                log::error!("GLArea failed to initialise: {e}");
                return;
            }
            let context = match unsafe { pixelmagic_gpu::context_from_epoxy() } {
                Ok(c) => Rc::new(c),
                Err(e) => {
                    log::error!("could not load GL entry points: {e}");
                    return;
                }
            };
            let flavor = pixelmagic_gpu::detect_flavor(&context);
            log::info!("GL flavour: {flavor:?}");
            match Renderer::new(context, flavor) {
                Ok(r) => *gl_cell.borrow_mut() = Some(CanvasGl { renderer: r }),
                Err(e) => log::error!("renderer init failed: {e}"),
            }
        });

        let gl_cell = self.gl.clone();
        self.widget.connect_unrealize(move |area| {
            area.make_current();
            // Dropping the renderer here, while the context is still current,
            // is what makes its GL object destructors valid.
            *gl_cell.borrow_mut() = None;
        });

        let gl_cell = self.gl.clone();
        let state = self.state.clone();
        self.widget.connect_render(move |area, _ctx| {
            let mut gl_ref = gl_cell.borrow_mut();
            let Some(gl) = gl_ref.as_mut() else { return glib::Propagation::Proceed };

            let scale = area.scale_factor().max(1);
            let width = area.width() * scale;
            let height = area.height() * scale;
            if width <= 0 || height <= 0 {
                return glib::Propagation::Proceed;
            }

            // GTK renders the GLArea into its own framebuffer, not zero.
            let target = gl.renderer.current_framebuffer();

            let st = state.borrow();
            let result = gl.renderer.render_document(&st.document, &st.revisions);
            let checker = st.show_checkerboard;
            let doc_w = st.document.width as f32;
            let doc_h = st.document.height as f32;
            let view = st.view;
            drop(st);

            match result {
                Ok(image) => {
                    let r = view.document_rect(doc_w, doc_h, width as f32, height as f32);
                    // GL's viewport origin is bottom-left; the view rectangle
                    // is in top-left space.
                    let vp = (
                        r.x.round() as i32,
                        (height as f32 - r.y - r.height).round() as i32,
                        r.width.round().max(1.0) as i32,
                        r.height.round().max(1.0) as i32,
                    );
                    if let Err(e) = gl.renderer.present(&image, vp, checker, target) {
                        log::error!("present failed: {e}");
                    }
                    gl.renderer.release(image);
                }
                Err(e) => log::error!("render failed: {e}"),
            }
            glib::Propagation::Stop
        });
    }

    // -- input ------------------------------------------------------------

    fn widget_size(&self) -> (f32, f32) {
        (self.widget.width() as f32, self.widget.height() as f32)
    }

    /// Convert a widget-space point to document space.
    fn to_doc(&self, x: f64, y: f64) -> glam::Vec2 {
        let st = self.state.borrow();
        let (w, h) = self.widget_size();
        st.view.to_document(
            glam::Vec2::new(x as f32, y as f32),
            st.document.width as f32,
            st.document.height as f32,
            w,
            h,
        )
    }

    fn connect_input(self: &Rc<Self>) {
        self.connect_drag();
        self.connect_scroll();
        self.connect_middle_drag();
    }

    fn connect_drag(self: &Rc<Self>) {
        let drag = gtk::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);

        let this = self.clone();
        drag.connect_drag_begin(move |g, x, y| {
            this.widget.grab_focus();
            let modifiers = g.current_event_state();
            let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
            let alt = modifiers.contains(gdk::ModifierType::ALT_MASK);
            let space = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
            this.begin_gesture(x, y, shift, alt, space);
        });

        let this = self.clone();
        drag.connect_drag_update(move |g, dx, dy| {
            if let Some((sx, sy)) = g.start_point() {
                this.update_gesture(sx + dx, sy + dy);
            }
        });

        let this = self.clone();
        drag.connect_drag_end(move |g, dx, dy| {
            if let Some((sx, sy)) = g.start_point() {
                this.end_gesture(sx + dx, sy + dy);
            }
        });

        self.widget.add_controller(drag);
    }

    /// Middle-drag pans, whatever tool is active — the one navigation gesture
    /// that should never be modal.
    fn connect_middle_drag(self: &Rc<Self>) {
        let drag = gtk::GestureDrag::new();
        drag.set_button(gdk::BUTTON_MIDDLE);

        let this = self.clone();
        drag.connect_drag_begin(move |_, x, y| {
            this.state.borrow_mut().gesture =
                Gesture::Pan { last: glam::Vec2::new(x as f32, y as f32) };
        });

        let this = self.clone();
        drag.connect_drag_update(move |g, dx, dy| {
            if let Some((sx, sy)) = g.start_point() {
                this.update_gesture(sx + dx, sy + dy);
            }
        });

        let this = self.clone();
        drag.connect_drag_end(move |_, _, _| {
            this.state.borrow_mut().gesture = Gesture::None;
        });

        self.widget.add_controller(drag);
    }

    fn connect_scroll(self: &Rc<Self>) {
        let scroll = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::BOTH_AXES
                | gtk::EventControllerScrollFlags::DISCRETE,
        );
        let this = self.clone();
        scroll.connect_scroll(move |c, _dx, dy| {
            let modifiers = c.current_event_state();
            let (w, h) = this.widget_size();
            let mut st = this.state.borrow_mut();
            let (dw, dh) = (st.document.width as f32, st.document.height as f32);

            if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
                // Ctrl-scroll zooms about the pointer.
                let factor = if dy < 0.0 { 1.1 } else { 1.0 / 1.1 };
                let anchor = glam::Vec2::new(w * 0.5, h * 0.5);
                st.view.zoom_about(factor, anchor, dw, dh, w, h);
            } else {
                st.view.offset.y -= dy as f32 * 48.0;
            }
            drop(st);
            this.queue_redraw();
            glib::Propagation::Stop
        });
        self.widget.add_controller(scroll);
    }

    fn begin_gesture(&self, x: f64, y: f64, shift: bool, alt: bool, force_pan: bool) {
        let p = self.to_doc(x, y);
        let widget_p = glam::Vec2::new(x as f32, y as f32);
        let tool = self.state.borrow().tool;

        if force_pan || tool == Tool::Hand {
            self.state.borrow_mut().gesture = Gesture::Pan { last: widget_p };
            return;
        }

        match canvas_action(tool) {
            CanvasAction::Zoom => {
                let (w, h) = self.widget_size();
                let mut st = self.state.borrow_mut();
                let (dw, dh) = (st.document.width as f32, st.document.height as f32);
                let factor = if alt { 1.0 / 1.5 } else { 1.5 };
                st.view.zoom_about(factor, widget_p, dw, dh, w, h);
                drop(st);
                self.queue_redraw();
            }
            CanvasAction::PickColor => {
                self.pick_color(p);
            }
            CanvasAction::MoveLayer => {
                self.state.borrow_mut().gesture = Gesture::MoveLayer { last: p };
            }
            CanvasAction::Marquee => {
                self.state.borrow_mut().gesture =
                    Gesture::Marquee { origin: p, op: MaskOp::from_modifiers(shift, alt) };
            }
            CanvasAction::Brush => {
                self.begin_paint(p, alt);
            }
            CanvasAction::Pan | CanvasAction::PanelOnly | CanvasAction::None => {}
        }
    }

    fn begin_paint(&self, p: glam::Vec2, alt: bool) {
        let (tool, erasing) = {
            let st = self.state.borrow();
            (st.tool, matches!(st.tool, Tool::Erase) || alt)
        };
        if !tool.is_implemented() {
            return;
        }
        let Some(id) = self.state.borrow().paintable_layer() else { return };

        // Snapshot the whole layer before the stroke starts. A per-dab
        // snapshot would be tighter, but then one stroke becomes hundreds of
        // undo steps unless they are merged, and merging pixel edits correctly
        // is fiddlier than paying for one layer-sized snapshot per stroke.
        let bounds = {
            let st = self.state.borrow();
            match &st.document.layers.get(id).map(|l| &l.kind) {
                Some(LayerKind::Pixel { buffer }) => buffer.bounds(),
                _ => Rect::ZERO,
            }
        };
        let edit = {
            let st = self.state.borrow();
            PixelRegionEdit::begin(&st.document, id, bounds, stroke_label(tool)).ok()
        };
        *self.pending_edit.borrow_mut() = edit;

        self.state.borrow_mut().gesture = Gesture::Paint { last: p, erasing };
        self.paint_to(p);
    }

    fn update_gesture(&self, x: f64, y: f64) {
        let p = self.to_doc(x, y);
        let widget_p = glam::Vec2::new(x as f32, y as f32);
        let gesture = self.state.borrow().gesture.clone();

        match gesture {
            Gesture::Pan { last } => {
                let mut st = self.state.borrow_mut();
                st.view.offset += widget_p - last;
                st.gesture = Gesture::Pan { last: widget_p };
                drop(st);
                self.queue_redraw();
            }
            Gesture::Paint { .. } => self.paint_to(p),
            Gesture::Marquee { origin, op, .. } => {
                self.update_marquee(origin, p, op);
            }
            Gesture::MoveLayer { last } => {
                let delta = p - last;
                let mut st = self.state.borrow_mut();
                if let Some(id) = st.document.primary_active() {
                    if let Some(layer) = st.document.layers.get_mut(id) {
                        if !layer.locked {
                            layer.transform = layer
                                .transform
                                .then(&pixelmagic_core::geom::Transform::translate(delta));
                            st.document.dirty = true;
                        }
                    }
                }
                st.gesture = Gesture::MoveLayer { last: p };
                drop(st);
                self.queue_redraw();
            }
            Gesture::None => {}
        }
    }

    fn end_gesture(&self, _x: f64, _y: f64) {
        let gesture = self.state.borrow().gesture.clone();
        self.state.borrow_mut().gesture = Gesture::None;

        if matches!(gesture, Gesture::Paint { .. }) {
            if let Some(mut edit) = self.pending_edit.borrow_mut().take() {
                let mut st = self.state.borrow_mut();
                if edit.finish(&st.document).is_ok() && !edit.is_empty() {
                    st.history.push_applied(Box::new(edit));
                }
            }
        }
        self.state.borrow_mut().history.break_coalescing();
        self.notify();
    }

    fn paint_to(&self, p: glam::Vec2) {
        let Some(id) = self.state.borrow().paintable_layer() else { return };

        let (settings, color, mode, selection) = {
            let st = self.state.borrow();
            let erasing = matches!(st.gesture, Gesture::Paint { erasing: true, .. });
            let mode = if erasing { BrushMode::Erase } else { tool_brush_mode(st.tool) };
            (st.brush.clone(), st.colors.foreground, mode, st.document.selection.clone())
        };

        let last = match self.state.borrow().gesture {
            Gesture::Paint { last, .. } => last,
            _ => p,
        };

        {
            let mut st = self.state.borrow_mut();
            let sel: Option<&Selection> = selection.as_ref();
            if let Some(layer) = st.document.layers.get_mut(id) {
                if let LayerKind::Pixel { buffer } = &mut layer.kind {
                    brush::stroke(buffer, last, p, &settings, color, mode, sel);
                }
            }
            if let Gesture::Paint { erasing, .. } = st.gesture {
                st.gesture = Gesture::Paint { last: p, erasing };
            }
        }

        self.state.borrow_mut().touch(id);
        self.widget.queue_render();
    }

    fn update_marquee(&self, origin: glam::Vec2, current: glam::Vec2, op: MaskOp) {
        let (tool, options, w, h, existing) = {
            let st = self.state.borrow();
            (
                st.tool,
                st.selection_options,
                st.document.width,
                st.document.height,
                st.document.selection.clone(),
            )
        };

        let rect = Rect::from_corners(origin, current);
        let mask = match tool {
            Tool::OvalSelection => Selection::ellipse(w, h, rect, options),
            Tool::RowSelection => Selection::rectangle(
                w,
                h,
                Rect::new(0.0, rect.y, w as f32, rect.height.max(1.0)),
                options,
            ),
            Tool::ColumnSelection => Selection::rectangle(
                w,
                h,
                Rect::new(rect.x, 0.0, rect.width.max(1.0), h as f32),
                options,
            ),
            _ => Selection::rectangle(w, h, rect, options),
        };

        let mut selection = match (op, existing) {
            (MaskOp::Replace, _) | (_, None) => Selection::none(w, h),
            (_, Some(s)) => s,
        };
        selection.combine(&mask, if matches!(op, MaskOp::Replace) { MaskOp::Add } else { op });

        self.state.borrow_mut().set_selection(selection);
        self.widget.queue_render();
    }

    fn pick_color(&self, p: glam::Vec2) {
        if p.x < 0.0 || p.y < 0.0 {
            return;
        }
        let mut st = self.state.borrow_mut();
        let Some(id) = st.document.primary_active() else { return };
        let picked = match st.document.layers.get(id).map(|l| &l.kind) {
            Some(LayerKind::Pixel { buffer }) => buffer.get(p.x as u32, p.y as u32),
            _ => None,
        };
        if let Some(c) = picked {
            st.colors.foreground = c.with_alpha(1.0);
        }
        drop(st);
        self.notify();
    }

    /// Render the document and read it back as a flat image.
    ///
    /// Goes through the same renderer the canvas uses, so an export is exactly
    /// what was on screen — no second code path to drift out of agreement.
    /// The caller must have made the GLArea's context current first.
    pub fn render_to_buffer(&self) -> std::result::Result<PixelBuffer, String> {
        let mut gl_ref = self.gl.borrow_mut();
        let gl = gl_ref.as_mut().ok_or("the canvas has no GL context yet")?;

        let st = self.state.borrow();
        let target = gl
            .renderer
            .render_document(&st.document, &st.revisions)
            .map_err(|e| e.to_string())?;
        let (w, h) = (st.document.width, st.document.height);
        drop(st);

        let pixels = gl.renderer.read_image(&target).map_err(|e| e.to_string());
        gl.renderer.release(target);
        let pixels = pixels?;

        PixelBuffer::from_raw(w, h, pixels)
            .ok_or_else(|| "readback produced the wrong number of bytes".to_string())
    }

    /// Render the document and compute its histogram.
    ///
    /// Returns `None` before the GLArea has been realised — the Color
    /// Adjustments panel is built during window construction, which happens
    /// before there is any GL context to render with.
    pub fn histogram(&self) -> Option<pixelmagic_gpu::renderer::Histogram> {
        self.widget.make_current();
        let mut gl_ref = self.gl.borrow_mut();
        let gl = gl_ref.as_mut()?;

        let st = self.state.borrow();
        let target = gl.renderer.render_document(&st.document, &st.revisions).ok()?;
        drop(st);

        let hist = gl.renderer.histogram(&target).ok();
        gl.renderer.release(target);
        hist
    }

    /// Fit the document to the widget — `Command-0`.
    pub fn zoom_to_fit(&self) {
        let (w, h) = self.widget_size();
        let mut st = self.state.borrow_mut();
        let (dw, dh) = (st.document.width as f32, st.document.height as f32);
        st.view = View::fit(dw, dh, w, h);
        drop(st);
        self.queue_redraw();
    }

    pub fn zoom_actual(&self) {
        let mut st = self.state.borrow_mut();
        st.view = View::default();
        drop(st);
        self.queue_redraw();
    }

    pub fn zoom_by(&self, factor: f32) {
        let (w, h) = self.widget_size();
        let mut st = self.state.borrow_mut();
        let (dw, dh) = (st.document.width as f32, st.document.height as f32);
        st.view.zoom_about(factor, glam::Vec2::new(w * 0.5, h * 0.5), dw, dh, w, h);
        drop(st);
        self.queue_redraw();
    }
}

/// What a tool actually does when you drag on the canvas.
///
/// This is the single source of truth for "does this tool work", and it is
/// deliberately not a restatement of `ToolInfo::implemented` — it *defines*
/// it. The two are checked against each other by a test, so a tool cannot be
/// advertised as working without a dispatch arm here, and a tool cannot gain a
/// dispatch arm without the rail lighting it up.
///
/// The distinction that matters: a tool whose gesture does the *wrong* thing
/// belongs in `None`, not in a nearby arm. Free Selection routed to the marquee
/// would drag out a rectangle, which is worse than being greyed out — the user
/// assumes they are holding it wrong rather than that it is unfinished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasAction {
    Pan,
    Zoom,
    PickColor,
    MoveLayer,
    /// Drag out a geometric selection.
    Marquee,
    /// Stroke with the brush engine.
    Brush,
    /// Drives the inspector, with no canvas gesture of its own.
    PanelOnly,
    /// Not implemented yet.
    None,
}

pub fn canvas_action(tool: Tool) -> CanvasAction {
    use Tool::*;
    match tool {
        Hand => CanvasAction::Pan,
        Zoom => CanvasAction::Zoom,
        ColorPicker => CanvasAction::PickColor,
        Arrange => CanvasAction::MoveLayer,

        // Only the geometric marquees. The freehand, polygonal, magnetic,
        // colour and quick selections all need their own gesture and would
        // otherwise silently draw a rectangle.
        RectangularSelection | OvalSelection | RowSelection | ColumnSelection => {
            CanvasAction::Marquee
        }

        // Only the brush modes `tool_brush_mode` genuinely implements. Color
        // Fill, Gradient Fill, Smart Erase, Pixel Paint, Smudge, Repair and
        // Clone would all fall through to an ordinary paint stroke.
        Paint | Erase | Sharpen | Soften | Lighten | Darken | Saturate | Desaturate => {
            CanvasAction::Brush
        }

        ColorAdjustments | Effects => CanvasAction::PanelOnly,

        _ => CanvasAction::None,
    }
}

fn tool_brush_mode(tool: Tool) -> BrushMode {
    match tool {
        Tool::Erase => BrushMode::Erase,
        Tool::Soften => BrushMode::Soften,
        Tool::Sharpen => BrushMode::Sharpen,
        Tool::Lighten => BrushMode::Lighten,
        Tool::Darken => BrushMode::Darken,
        Tool::Saturate => BrushMode::Saturate,
        Tool::Desaturate => BrushMode::Desaturate,
        Tool::Clone => BrushMode::Clone { offset: glam::Vec2::new(32.0, 32.0) },
        _ => BrushMode::Paint,
    }
}

fn stroke_label(tool: Tool) -> String {
    match tool.category() {
        ToolCategory::Painting => format!("{} Stroke", tool.label()),
        _ => tool.label().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixelmagic_core::tool::Tool;

    /// Ties the roster's `implemented` flag to the canvas's actual dispatch.
    ///
    /// This is the test that stops the app from lying about itself. Marking a
    /// tool implemented without wiring it up fails here, and so does wiring one
    /// up without lighting it in the rail.
    #[test]
    fn implemented_flag_matches_real_canvas_behaviour() {
        for info in pixelmagic_core::tool::TOOLS {
            let has_behaviour = canvas_action(info.tool) != CanvasAction::None;
            assert_eq!(
                info.implemented,
                has_behaviour,
                "`{}` is marked implemented={} but its canvas action is {:?}",
                info.label,
                info.implemented,
                canvas_action(info.tool),
            );
        }
    }

    #[test]
    fn wrong_behaviour_counts_as_unimplemented() {
        // Guards the specific trap: these have a plausible-looking nearby
        // dispatch arm, and routing them to it would be worse than nothing.
        for tool in [
            Tool::FreeSelection,
            Tool::PolygonalSelection,
            Tool::ColorSelection,
            Tool::ColorFill,
            Tool::GradientFill,
            Tool::PixelPaint,
            Tool::Smudge,
        ] {
            assert_eq!(canvas_action(tool), CanvasAction::None, "{}", tool.label());
            assert!(!tool.is_implemented(), "{}", tool.label());
        }
    }

    #[test]
    fn brush_modes_follow_the_tool() {
        assert_eq!(tool_brush_mode(Tool::Paint), BrushMode::Paint);
        assert_eq!(tool_brush_mode(Tool::Erase), BrushMode::Erase);
        assert_eq!(tool_brush_mode(Tool::Lighten), BrushMode::Lighten);
        assert!(matches!(tool_brush_mode(Tool::Clone), BrushMode::Clone { .. }));
    }

    #[test]
    fn stroke_labels_read_naturally_in_the_undo_menu() {
        assert_eq!(stroke_label(Tool::Paint), "Paint Stroke");
        assert_eq!(stroke_label(Tool::Smudge), "Smudge");
    }
}
