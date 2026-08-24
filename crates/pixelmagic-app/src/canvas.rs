//! The canvas: a `GLArea` that renders the document and handles tool input.

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use pixelmagic_core::buffer::{MaskBuffer, MaskOp, PixelBuffer};
use pixelmagic_core::geom::Rect;
use pixelmagic_core::history::PixelRegionEdit;
use pixelmagic_core::layer::LayerKind;
use pixelmagic_core::quickselect;
use pixelmagic_core::selection::Selection;
use pixelmagic_core::tool::{Tool, ToolCategory};
use pixelmagic_gpu::renderer::{BackdropRect, BackdropStyle, SelectionOverlayStyle};
use pixelmagic_gpu::texture::{Filter, Format, Texture, Wrap};
use pixelmagic_gpu::Renderer;
use std::cell::RefCell;
use std::rc::Rc;

use crate::brush::{self, BrushMode};
use crate::state::{EditorState, Gesture, View};

/// Owns the GL side of the canvas. Created on `realize`, destroyed on
/// `unrealize` — GL objects belong to the context and must not outlive it.
struct CanvasGl {
    renderer: Renderer,
    /// The selection mask on the GPU, with the revision it was built from.
    /// Re-uploading a canvas-sized mask every frame would cost more than the
    /// rest of the overlay put together, and the ants animate every frame.
    selection: Option<(Texture, u64)>,
    /// The Quick Selection hover preview, likewise cached by revision.
    preview: Option<(Texture, u64)>,
}

impl CanvasGl {
    /// Upload `mask` into `slot` unless the slot already holds this revision
    /// at this size, and hand back the texture to draw with.
    fn mask_texture<'a>(
        gl: &Rc<glow::Context>,
        slot: &'a mut Option<(Texture, u64)>,
        mask: &MaskBuffer,
        revision: u64,
    ) -> Option<&'a Texture> {
        let stale = match slot {
            Some((tex, rev)) => {
                *rev != revision || tex.width != mask.width() || tex.height != mask.height()
            }
            None => true,
        };
        if stale {
            let tex = Texture::new(
                gl.clone(),
                mask.width(),
                mask.height(),
                Format::R8,
                // Linear, so at high zoom the mask's edge ramps across the
                // magnified texel instead of stepping, and the outline the
                // shader derives from it follows the shape rather than the
                // texel grid.
                Filter::Linear,
                Wrap::Clamp,
            )
            .ok()?;
            tex.upload_raw(mask.data()).ok()?;
            *slot = Some((tex, revision));
        }
        slot.as_ref().map(|(tex, _)| tex)
    }
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
    /// Where the floating panels sit, in widget-logical pixels, so the
    /// renderer can frost the canvas underneath them. Empty means no frosting.
    backdrops: Rc<RefCell<Vec<BackdropRect>>>,
    backdrop_style: Rc<std::cell::Cell<BackdropStyle>>,
    /// What Quick Selection would select if you clicked where the pointer is.
    /// Transient hover state, so it lives here rather than in the document.
    preview_mask: Rc<RefCell<Option<MaskBuffer>>>,
    preview_revision: Rc<std::cell::Cell<u64>>,
    /// Marching-ants animation phase, in device pixels.
    ants_phase: Rc<std::cell::Cell<f32>>,
    /// Pixels Quick Selection samples, with the key they were built from.
    /// Shared by `Rc` so the hover path can use it without cloning a canvas.
    sample_cache: RefCell<Option<(String, Rc<PixelBuffer>)>>,
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
            preview_mask: Rc::new(RefCell::new(None)),
            preview_revision: Rc::new(std::cell::Cell::new(0)),
            ants_phase: Rc::new(std::cell::Cell::new(0.0)),
            sample_cache: RefCell::new(None),
            backdrops: Rc::new(RefCell::new(Vec::new())),
            backdrop_style: Rc::new(std::cell::Cell::new(BackdropStyle {
                corner: crate::style::metrics::PANEL_CORNER,
                ..BackdropStyle::default()
            })),
        });

        canvas.connect_gl();
        canvas.connect_input();
        canvas.start_ant_animation();
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
                Ok(r) => {
                    *gl_cell.borrow_mut() =
                        Some(CanvasGl { renderer: r, selection: None, preview: None })
                }
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
        let backdrops = self.backdrops.clone();
        let backdrop_style = self.backdrop_style.clone();
        let preview_mask = self.preview_mask.clone();
        let preview_revision = self.preview_revision.clone();
        let ants_phase = self.ants_phase.clone();
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

                    // Selection chrome, drawn onto the same rectangle the
                    // document just went to. Before the panel frosting, so a
                    // selection that runs under a panel is blurred along with
                    // the image rather than sitting sharply on top of it.
                    let phase = ants_phase.get();
                    let context = gl.renderer.context();

                    let selection = state.borrow().document.selection.clone();
                    if let Some(sel) = selection.as_ref() {
                        let revision = state.borrow().selection_revision;
                        if let Some(tex) = CanvasGl::mask_texture(
                            &context,
                            &mut gl.selection,
                            sel.mask(),
                            revision,
                        ) {
                            if let Err(e) = gl.renderer.draw_selection_overlay(
                                tex,
                                vp,
                                SelectionOverlayStyle::ants(phase),
                                target,
                            ) {
                                log::warn!("selection overlay failed: {e}");
                            }
                        }
                    }

                    // The Quick Selection hover preview goes on top of the
                    // committed selection: it is what would be *added*, so it
                    // has to be legible over what is already there.
                    let preview = preview_mask.borrow();
                    if let Some(mask) = preview.as_ref() {
                        if let Some(tex) = CanvasGl::mask_texture(
                            &context,
                            &mut gl.preview,
                            mask,
                            preview_revision.get(),
                        ) {
                            if let Err(e) = gl.renderer.draw_selection_overlay(
                                tex,
                                vp,
                                SelectionOverlayStyle::preview(phase),
                                target,
                            ) {
                                log::warn!("preview overlay failed: {e}");
                            }
                        }
                    }
                }
                Err(e) => log::error!("render failed: {e}"),
            }

            // Frost the canvas under the floating panels. This has to come
            // after `present` — it blurs what is already in the framebuffer —
            // and before GTK draws the panel widgets over the top.
            let rects: Vec<_> = backdrops
                .borrow()
                .iter()
                .map(|r| BackdropRect {
                    x: r.x * scale as f32,
                    y: r.y * scale as f32,
                    width: r.width * scale as f32,
                    height: r.height * scale as f32,
                })
                .collect();
            if !rects.is_empty() {
                let mut style = backdrop_style.get();
                style.radius *= scale as f32;
                style.corner *= scale as f32;
                if let Err(e) =
                    gl.renderer.blur_backdrop((width, height), &rects, style, target)
                {
                    // Not fatal: the panels are still legible against their own
                    // tint, so log once and carry on rather than killing the
                    // frame.
                    log::warn!("backdrop blur failed: {e}");
                }
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
        self.connect_hover();
    }

    /// Pointer motion with no button held, which is what drives the Quick
    /// Selection preview.
    ///
    /// `GtkGestureDrag` does not report these — it only starts once a button
    /// goes down — so this needs its own controller. It also has to clear the
    /// preview on leave, or the yellow region stays painted on the canvas
    /// after the pointer has gone somewhere else entirely.
    fn connect_hover(self: &Rc<Self>) {
        let motion = gtk::EventControllerMotion::new();

        let this = self.clone();
        motion.connect_motion(move |_, x, y| this.hover_quick_select(x, y));

        let this = self.clone();
        motion.connect_leave(move |_| this.clear_preview());

        self.widget.add_controller(motion);
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
            CanvasAction::QuickSelect => {
                // Shift and Alt override the panel's mode for one click, the
                // same convention the marquees use.
                let op = if shift || alt {
                    MaskOp::from_modifiers(shift, alt)
                } else {
                    self.state.borrow().quick_select.mode
                };
                self.commit_quick_select(p, op);
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

    // -- quick selection ----------------------------------------------------

    /// The pixels Quick Selection judges similarity against.
    ///
    /// With "Sample all layers" off this is the active layer's own buffer;
    /// with it on, the composited image, which has to come back off the GPU.
    /// That readback is why the result is cached: the hover preview runs on
    /// every pointer move, and reading a full canvas back per move would make
    /// the tool unusable on anything larger than a thumbnail.
    fn quick_select_source(&self) -> Option<Rc<PixelBuffer>> {
        let (all_layers, key) = {
            let st = self.state.borrow();
            let all = st.quick_select.sample_all_layers;
            // A cache key that changes whenever the pixels might have: which
            // layer is active, how many times any layer has been edited, and
            // which of the two sources we are reading.
            let edits: u64 =
                st.revisions.values().copied().fold(0u64, |a, b| a.wrapping_add(b));
            let active =
                st.document.primary_active().map(|id| format!("{id:?}")).unwrap_or_default();
            (all, format!("{all}|{edits}|{active}|{}", st.document.layers.len()))
        };

        if let Some((cached_key, buffer)) = &*self.sample_cache.borrow() {
            if *cached_key == key {
                return Some(buffer.clone());
            }
        }

        let buffer = if all_layers {
            self.widget.make_current();
            match self.render_to_buffer() {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("quick selection could not read the composite: {e}");
                    return None;
                }
            }
        } else {
            let st = self.state.borrow();
            let id = st.document.primary_active()?;
            match st.document.layers.get(id).map(|l| &l.kind) {
                Some(LayerKind::Pixel { buffer }) => buffer.clone(),
                // An adjustment or effects layer has no pixels of its own.
                // Falling back to the composite is more useful than refusing,
                // and matches what the user is looking at.
                _ => {
                    drop(st);
                    self.widget.make_current();
                    self.render_to_buffer().ok()?
                }
            }
        };

        let buffer = Rc::new(buffer);
        *self.sample_cache.borrow_mut() = Some((key, buffer.clone()));
        Some(buffer)
    }

    /// The region that would be selected by clicking at `p`.
    fn quick_select_region(&self, p: glam::Vec2) -> Option<MaskBuffer> {
        if p.x < 0.0 || p.y < 0.0 {
            return None;
        }
        let (tolerance, reach) = {
            let st = self.state.borrow();
            (st.quick_select.tolerance, st.quick_select.reach)
        };
        let source = self.quick_select_source()?;
        let (x, y) = (p.x as u32, p.y as u32);
        if x >= source.width() || y >= source.height() {
            return None;
        }
        Some(quickselect::grow(
            &source,
            (x, y),
            quickselect::GrowOptions::preview(tolerance, reach),
        ))
    }

    /// Update the yellow hover preview for a pointer at `p`.
    pub fn hover_quick_select(&self, x: f64, y: f64) {
        let show = {
            let st = self.state.borrow();
            canvas_action(st.tool) == CanvasAction::QuickSelect && st.quick_select.show_preview
        };
        if !show {
            self.set_preview_mask(None);
            return;
        }
        let p = self.to_doc(x, y);
        self.set_preview_mask(self.quick_select_region(p));
    }

    /// Clear the hover preview — on tool change, on leaving the canvas, and
    /// after committing. A preview left on screen reads as a real selection.
    pub fn clear_preview(&self) {
        self.set_preview_mask(None);
    }

    fn commit_quick_select(&self, p: glam::Vec2, op: MaskOp) {
        let Some(region) = self.quick_select_region(p) else { return };
        {
            let mut st = self.state.borrow_mut();
            let (w, h) = (st.document.width, st.document.height);
            let mut selection =
                st.document.selection.clone().unwrap_or_else(|| Selection::none(w, h));
            // Replace starts from nothing, so "New" does not accumulate.
            if op == MaskOp::Replace {
                selection = Selection::none(w, h);
            }
            selection.combine(&region, if op == MaskOp::Replace { MaskOp::Add } else { op });

            let feather = st.selection_options.feather;
            if feather > 0.0 {
                let mut mask = selection.mask().clone();
                pixelmagic_core::selection::feather(&mut mask, feather);
                selection = Selection::from_mask(mask);
            }
            st.set_selection(selection);
        }
        self.clear_preview();
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

    /// Drive the marching ants.
    ///
    /// A `GtkWidget` tick callback would be the obvious mechanism and is the
    /// wrong one: it holds the frame clock open, so the app repaints at the
    /// display's refresh rate forever, selection or no selection. A timer that
    /// only asks for a redraw when there is actually an overlay on screen
    /// costs one closure call every 60ms when idle.
    ///
    /// 60ms — about 16fps — is deliberate. Ants are a slow crawl; running them
    /// at 60fps spends four times the redraws to look no different, and each
    /// redraw re-renders the whole document.
    fn start_ant_animation(self: &Rc<Self>) {
        const INTERVAL_MS: u64 = 60;
        /// Device pixels per tick. One dash pair is 8px, so a dash takes about
        /// half a second to travel its own length.
        const STEP: f32 = 1.0;

        let this = Rc::downgrade(self);
        glib::timeout_add_local(std::time::Duration::from_millis(INTERVAL_MS), move || {
            // Weak, so the timer does not keep the canvas — and through it the
            // whole document — alive after the window is gone.
            let Some(canvas) = this.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !canvas.has_overlay() || !canvas.widget.is_mapped() {
                return glib::ControlFlow::Continue;
            }
            // Wrap at a multiple of the dash period so the pattern is
            // continuous across the wrap and the ants do not visibly jump.
            let next = (canvas.ants_phase.get() + STEP) % 1024.0;
            canvas.ants_phase.set(next);
            canvas.widget.queue_render();
            glib::ControlFlow::Continue
        });
    }

    /// Whether anything is on screen that needs animating.
    fn has_overlay(&self) -> bool {
        self.state.borrow().document.selection.is_some() || self.preview_mask.borrow().is_some()
    }

    /// Show what Quick Selection would take if the user clicked now.
    ///
    /// Passing `None` clears it — which every path out of the tool must do, or
    /// a stale yellow region is left painted on the canvas.
    pub fn set_preview_mask(&self, mask: Option<MaskBuffer>) {
        let had = self.preview_mask.borrow().is_some();
        let has = mask.is_some();
        // Cheap identity check first: recomputing the same region on every
        // pointer move within one shape is the common case, and re-uploading
        // it would make the hover stutter on a large fill.
        let unchanged = match (&*self.preview_mask.borrow(), &mask) {
            (Some(a), Some(b)) => a.width() == b.width() && a.data() == b.data(),
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            return;
        }
        *self.preview_mask.borrow_mut() = mask;
        self.preview_revision.set(self.preview_revision.get().wrapping_add(1));
        if had || has {
            self.widget.queue_render();
        }
    }

    /// Tell the canvas where the floating panels are, so it can frost the
    /// image behind them.
    ///
    /// Rectangles are in widget-logical pixels with a top-left origin — the
    /// same space GTK allocates widgets in — so a caller can hand over an
    /// allocation directly.
    pub fn set_backdrops(&self, rects: Vec<BackdropRect>) {
        let changed = *self.backdrops.borrow() != rects;
        *self.backdrops.borrow_mut() = rects;
        if changed {
            self.widget.queue_render();
        }
    }

    /// Tell the canvas how much of it the floating panels cover, so the
    /// document centres in what is actually visible.
    pub fn set_insets(&self, insets: crate::state::Insets) {
        let changed = {
            let mut st = self.state.borrow_mut();
            let changed = st.view.insets != insets;
            st.view.insets = insets;
            changed
        };
        if changed {
            self.queue_redraw();
        }
    }

    /// Fit the document to the widget — `Command-0`.
    pub fn zoom_to_fit(&self) {
        let (w, h) = self.widget_size();
        let mut st = self.state.borrow_mut();
        let (dw, dh) = (st.document.width as f32, st.document.height as f32);
        let insets = st.view.insets;
        st.view = View::fit_with(dw, dh, w, h, insets);
        drop(st);
        self.queue_redraw();
    }

    pub fn zoom_actual(&self) {
        let mut st = self.state.borrow_mut();
        let insets = st.view.insets;
        st.view = View { insets, ..View::default() };
        drop(st);
        self.queue_redraw();
    }

    /// Centre of the area the panels leave free — the natural anchor for a
    /// zoom that did not come from the pointer.
    fn free_centre(&self, w: f32, h: f32, insets: crate::state::Insets) -> glam::Vec2 {
        glam::Vec2::new(
            (insets.left + (w - insets.right)) * 0.5,
            (insets.top + (h - insets.bottom)) * 0.5,
        )
    }

    /// Set an absolute zoom, keeping the centre of the view fixed.
    pub fn set_zoom(&self, zoom: f32) {
        let (w, h) = self.widget_size();
        let mut st = self.state.borrow_mut();
        let (dw, dh) = (st.document.width as f32, st.document.height as f32);
        let anchor = self.free_centre(w, h, st.view.insets);
        let factor = zoom / st.view.zoom.max(1e-6);
        st.view.zoom_about(factor, anchor, dw, dh, w, h);
        drop(st);
        self.queue_redraw();
    }

    pub fn zoom_by(&self, factor: f32) {
        let (w, h) = self.widget_size();
        let mut st = self.state.borrow_mut();
        let (dw, dh) = (st.document.width as f32, st.document.height as f32);
        let anchor = self.free_centre(w, h, st.view.insets);
        st.view.zoom_about(factor, anchor, dw, dh, w, h);
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
    /// Grow a region from the pointer: previews on hover, commits on click.
    QuickSelect,
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

        QuickSelection => CanvasAction::QuickSelect,

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
