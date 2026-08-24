//! Editor state shared across the UI.
//!
//! GTK widgets are reference-counted and their callbacks outlive the scope that
//! created them, so the state they mutate has to be shared. `Rc<RefCell<..>>`
//! is the idiomatic answer in gtk-rs, with one rule that has to be respected
//! everywhere: **never hold a borrow across a call that can re-enter the UI**.
//! Every handler below takes the borrow, does its work, drops it, and only then
//! asks widgets to refresh.

use pixelmagic_core::buffer::MaskOp;
use pixelmagic_core::color::Rgba;
use pixelmagic_core::document::Document;
use pixelmagic_core::geom::Rect;
use pixelmagic_core::history::{Edit, History};
use pixelmagic_core::layer::{LayerId, LayerKind};
use pixelmagic_core::selection::{Selection, SelectionOptions};
use pixelmagic_core::tool::{BrushSettings, QuickSelectSettings, Tool};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// Widget-space margins that the floating panels occupy.
///
/// The canvas runs full-bleed *underneath* the panels — that is what makes them
/// look like they float — but the document must not be centred underneath one,
/// or half the image is behind the Layers panel and unreachable. So the visible
/// area is the widget minus these insets, and everything that positions the
/// document works in that rectangle instead of the whole widget.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Insets {
    /// The free rectangle inside a widget of this size, never collapsing to a
    /// negative extent on a window narrower than the panels themselves.
    fn free(self, widget_w: f32, widget_h: f32) -> (f32, f32, f32, f32) {
        let w = (widget_w - self.left - self.right).max(1.0);
        let h = (widget_h - self.top - self.bottom).max(1.0);
        (self.left, self.top, w, h)
    }
}

/// How the canvas maps document space to widget space.
#[derive(Debug, Clone, Copy)]
pub struct View {
    pub zoom: f32,
    /// Pan offset in widget pixels.
    pub offset: glam::Vec2,
    /// Space taken by the floating panels, so the document centres in what the
    /// user can actually see.
    pub insets: Insets,
}

impl Default for View {
    fn default() -> Self {
        Self { zoom: 1.0, offset: glam::Vec2::ZERO, insets: Insets::default() }
    }
}

impl View {
    pub const MIN_ZOOM: f32 = 0.02;
    pub const MAX_ZOOM: f32 = 64.0;

    /// [`View::fit_with`] against the whole widget. Tests only — the
    /// application always has panels.
    #[cfg(test)]
    fn fit_no_insets(doc_w: f32, doc_h: f32, widget_w: f32, widget_h: f32) -> Self {
        Self::fit_with(doc_w, doc_h, widget_w, widget_h, Insets::default())
    }

    /// Zoom so the whole document fits inside the area the panels leave free,
    /// with a small margin.
    pub fn fit_with(
        doc_w: f32,
        doc_h: f32,
        widget_w: f32,
        widget_h: f32,
        insets: Insets,
    ) -> Self {
        if doc_w <= 0.0 || doc_h <= 0.0 || widget_w <= 0.0 || widget_h <= 0.0 {
            return View { insets, ..View::default() };
        }
        let (_, _, free_w, free_h) = insets.free(widget_w, widget_h);
        let margin = 32.0;
        let zoom = ((free_w - margin) / doc_w)
            .min((free_h - margin) / doc_h)
            .clamp(Self::MIN_ZOOM, 1.0);
        View { zoom, offset: glam::Vec2::ZERO, insets }
    }

    /// Widget-space rectangle the document occupies.
    pub fn document_rect(self, doc_w: f32, doc_h: f32, widget_w: f32, widget_h: f32) -> Rect {
        let w = doc_w * self.zoom;
        let h = doc_h * self.zoom;
        let (fx, fy, fw, fh) = self.insets.free(widget_w, widget_h);
        Rect::new(
            fx + (fw - w) * 0.5 + self.offset.x,
            fy + (fh - h) * 0.5 + self.offset.y,
            w,
            h,
        )
    }

    /// Widget point to document point.
    pub fn to_document(
        self,
        p: glam::Vec2,
        doc_w: f32,
        doc_h: f32,
        widget_w: f32,
        widget_h: f32,
    ) -> glam::Vec2 {
        let r = self.document_rect(doc_w, doc_h, widget_w, widget_h);
        glam::Vec2::new((p.x - r.x) / self.zoom, (p.y - r.y) / self.zoom)
    }

    /// Zoom about a fixed widget point, so the pixel under the cursor stays
    /// under the cursor.
    pub fn zoom_about(
        &mut self,
        factor: f32,
        anchor: glam::Vec2,
        doc_w: f32,
        doc_h: f32,
        widget_w: f32,
        widget_h: f32,
    ) {
        let before = self.to_document(anchor, doc_w, doc_h, widget_w, widget_h);
        self.zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let after = self.to_document(anchor, doc_w, doc_h, widget_w, widget_h);
        self.offset += (after - before) * self.zoom;
    }
}

/// Which colours the painting tools use. `D` resets, `X` swaps.
#[derive(Debug, Clone, Copy)]
pub struct ColorPair {
    pub foreground: Rgba,
    pub background: Rgba,
}

impl Default for ColorPair {
    fn default() -> Self {
        Self { foreground: Rgba::BLACK, background: Rgba::WHITE }
    }
}

impl ColorPair {
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.foreground, &mut self.background);
    }

    pub fn reset(&mut self) {
        *self = ColorPair::default();
    }
}

/// A drag in progress, so motion events know what they are continuing.
#[derive(Debug, Clone)]
pub enum Gesture {
    None,
    Pan { last: glam::Vec2 },
    Paint { last: glam::Vec2, erasing: bool },
    Marquee { origin: glam::Vec2, op: MaskOp },
    MoveLayer { last: glam::Vec2 },
}

pub struct EditorState {
    pub document: Document,
    pub history: History,
    pub view: View,
    pub tool: Tool,
    pub brush: BrushSettings,
    pub colors: ColorPair,
    pub selection_options: SelectionOptions,
    pub quick_select: QuickSelectSettings,
    pub gesture: Gesture,
    /// The last non-empty selection, so Reselect can bring it back after a
    /// deselect. Pixelmator's Reselect does exactly this and nothing more.
    pub last_selection: Option<Selection>,
    /// Bumped per layer whenever its pixels change, so the renderer knows to
    /// re-upload without diffing megabytes.
    pub revisions: HashMap<LayerId, u64>,
    pub show_checkerboard: bool,
    /// Set when something has changed and the canvas needs a redraw.
    pub needs_redraw: bool,
    /// Bumped whenever the selection changes, so the canvas knows to re-upload
    /// the mask texture the overlay draws from. Comparing the masks themselves
    /// would mean hashing megabytes on every frame.
    pub selection_revision: u64,
}

impl EditorState {
    pub fn new(document: Document) -> Self {
        Self {
            document,
            history: History::default(),
            view: View::default(),
            tool: Tool::Arrange,
            brush: BrushSettings::default(),
            colors: ColorPair::default(),
            selection_options: SelectionOptions::default(),
            quick_select: QuickSelectSettings::default(),
            gesture: Gesture::None,
            last_selection: None,
            revisions: HashMap::new(),
            show_checkerboard: true,
            needs_redraw: true,
            selection_revision: 0,
        }
    }

    pub fn shared(document: Document) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self::new(document)))
    }

    /// Mark a layer's pixels as changed.
    pub fn touch(&mut self, id: LayerId) {
        *self.revisions.entry(id).or_insert(0) += 1;
        self.document.dirty = true;
        self.needs_redraw = true;
    }

    pub fn apply(&mut self, edit: Box<dyn Edit>) {
        // Split the borrow: `History::push` needs `&mut Document`, and both
        // live in `self`.
        let Self { document, history, .. } = self;
        if let Err(e) = history.push(document, edit) {
            log::warn!("edit failed: {e}");
        }
        self.needs_redraw = true;
    }

    pub fn undo(&mut self) -> Option<String> {
        let Self { document, history, .. } = self;
        let label = history.undo(document).ok().flatten();
        if label.is_some() {
            self.invalidate_all();
        }
        label
    }

    pub fn redo(&mut self) -> Option<String> {
        let Self { document, history, .. } = self;
        let label = history.redo(document).ok().flatten();
        if label.is_some() {
            self.invalidate_all();
        }
        label
    }

    /// An undo can change any layer's pixels, and the command objects do not
    /// report which — so bump everything rather than risk a stale texture.
    fn invalidate_all(&mut self) {
        for (_, r) in self.revisions.iter_mut() {
            *r += 1;
        }
        let ids: Vec<LayerId> =
            self.document.layers.iter_depth_first().into_iter().map(|(id, _)| id).collect();
        for id in ids {
            self.revisions.entry(id).or_insert(0);
        }
        self.needs_redraw = true;
    }

    pub fn file_name(&self) -> String {
        self.document
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{}.pxm", self.document.name))
    }

    pub fn set_path(&mut self, path: PathBuf) {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            self.document.name = stem.to_string();
        }
        self.document.path = Some(path);
        self.document.dirty = false;
    }

    /// The active layer, if it is a pixel layer the tools can draw into.
    pub fn paintable_layer(&self) -> Option<LayerId> {
        let id = self.document.primary_active()?;
        let layer = self.document.layers.get(id)?;
        if layer.locked {
            return None;
        }
        matches!(layer.kind, LayerKind::Pixel { .. }).then_some(id)
    }

    /// A short description of the selection for the info bar.
    pub fn selection_bounds_label(&self) -> String {
        let b = self.document.selection_bounds();
        format!("{}×{} at {},{}", b.width as i32, b.height as i32, b.x as i32, b.y as i32)
    }

    /// Replace the selection, dropping it entirely when it covers everything —
    /// tools take a faster path when there is nothing to clip against.
    pub fn set_selection(&mut self, selection: Selection) {
        self.document.selection = if selection.is_everything() || selection.is_empty() {
            None
        } else {
            Some(selection)
        };
        self.touch_selection();
    }

    /// `Command-A`. Unlike [`EditorState::set_selection`] this keeps the
    /// full-canvas mask rather than collapsing it to `None`, so the overlay
    /// can draw ants around the whole canvas — which is what tells the user
    /// the command did anything at all.
    pub fn select_all(&mut self) {
        self.document.select_all();
        self.touch_selection();
    }

    pub fn deselect(&mut self) {
        // Remember it first — that is the whole point of Reselect, and doing
        // it here rather than in the action means every route to a deselect
        // is covered, including the ones added later.
        if let Some(sel) = self.document.selection.take() {
            if !sel.is_empty() {
                self.last_selection = Some(sel);
            }
        }
        self.document.deselect();
        self.touch_selection();
    }

    /// Bring back the selection that was last thrown away. Returns false when
    /// there is nothing to restore, so the caller can leave the menu item and
    /// the button insensitive rather than offering a no-op.
    pub fn reselect(&mut self) -> bool {
        match self.last_selection.take() {
            Some(sel) => {
                self.document.selection = Some(sel);
                self.touch_selection();
                true
            }
            None => false,
        }
    }

    pub fn can_reselect(&self) -> bool {
        self.last_selection.is_some()
    }

    /// Mark the selection as changed. Every path that alters
    /// `document.selection` must go through here or the overlay will keep
    /// drawing the previous one.
    pub fn touch_selection(&mut self) {
        self.selection_revision = self.selection_revision.wrapping_add(1);
        self.needs_redraw = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insets_centre_the_document_in_the_free_area_not_the_widget() {
        // Panels covering 300px on the left and 100px on the right leave the
        // free area centred at x = 300 + (1000-300-100)/2 = 600, not 500.
        let insets = Insets { left: 300.0, right: 100.0, top: 0.0, bottom: 0.0 };
        let view = View { zoom: 1.0, offset: glam::Vec2::ZERO, insets };
        let r = view.document_rect(200.0, 100.0, 1000.0, 400.0);
        assert_eq!(r.x + r.width * 0.5, 600.0);
        assert_eq!(r.y + r.height * 0.5, 200.0);

        // And with no insets it is the plain centre, so nothing else changes.
        let plain = View::default().document_rect(200.0, 100.0, 1000.0, 400.0);
        assert_eq!(plain.x + plain.width * 0.5, 500.0);
    }

    #[test]
    fn fit_with_insets_fits_the_free_area() {
        // A 900-wide document in a 1000-wide widget fits at 1:1 with no
        // panels, but must shrink once 400px of it is covered.
        let none = View::fit_no_insets(900.0, 100.0, 1000.0, 1000.0);
        let some = View::fit_with(
            900.0,
            100.0,
            1000.0,
            1000.0,
            Insets { left: 300.0, right: 100.0, top: 0.0, bottom: 0.0 },
        );
        assert!(some.zoom < none.zoom, "{} should be under {}", some.zoom, none.zoom);
        // The scaled document plus the fit margin stays within the free width.
        assert!(900.0 * some.zoom <= 600.0);
    }

    #[test]
    fn insets_wider_than_the_window_do_not_invert_the_free_area() {
        let insets = Insets { left: 600.0, right: 600.0, top: 0.0, bottom: 0.0 };
        let view = View::fit_with(100.0, 100.0, 800.0, 800.0, insets);
        assert!(view.zoom > 0.0 && view.zoom.is_finite());
        let r = view.document_rect(100.0, 100.0, 800.0, 800.0);
        assert!(r.width > 0.0 && r.x.is_finite());
    }

    #[test]
    fn fit_keeps_the_document_inside_the_widget() {
        let v = View::fit_no_insets(1000.0, 500.0, 400.0, 400.0);
        assert!(v.zoom * 1000.0 <= 400.0);
        assert!(v.zoom > 0.0);
    }

    #[test]
    fn fit_never_upscales() {
        let v = View::fit_no_insets(10.0, 10.0, 2000.0, 2000.0);
        assert_eq!(v.zoom, 1.0, "a tiny document should not be blown up to fill");
    }

    #[test]
    fn fit_handles_degenerate_input() {
        assert_eq!(View::fit_no_insets(0.0, 0.0, 100.0, 100.0).zoom, 1.0);
        assert_eq!(View::fit_no_insets(100.0, 100.0, 0.0, 0.0).zoom, 1.0);
    }

    #[test]
    fn document_coordinates_round_trip() {
        let v =
            View { zoom: 2.0, offset: glam::Vec2::new(10.0, -5.0), insets: Insets::default() };
        let r = v.document_rect(100.0, 50.0, 400.0, 300.0);
        let widget = glam::Vec2::new(r.x + 2.0 * 3.0, r.y + 2.0 * 7.0);
        let doc = v.to_document(widget, 100.0, 50.0, 400.0, 300.0);
        assert!((doc.x - 3.0).abs() < 1e-3 && (doc.y - 7.0).abs() < 1e-3);
    }

    #[test]
    fn zoom_about_keeps_the_anchor_pixel_still() {
        let mut v = View::default();
        let anchor = glam::Vec2::new(120.0, 90.0);
        let before = v.to_document(anchor, 200.0, 200.0, 400.0, 300.0);
        v.zoom_about(2.0, anchor, 200.0, 200.0, 400.0, 300.0);
        let after = v.to_document(anchor, 200.0, 200.0, 400.0, 300.0);
        assert!(
            (before - after).length() < 0.01,
            "anchor drifted from {before:?} to {after:?}"
        );
    }

    #[test]
    fn zoom_is_clamped() {
        let mut v = View::default();
        for _ in 0..50 {
            v.zoom_about(2.0, glam::Vec2::ZERO, 100.0, 100.0, 400.0, 400.0);
        }
        assert_eq!(v.zoom, View::MAX_ZOOM);
        for _ in 0..100 {
            v.zoom_about(0.5, glam::Vec2::ZERO, 100.0, 100.0, 400.0, 400.0);
        }
        assert_eq!(v.zoom, View::MIN_ZOOM);
    }

    #[test]
    fn colour_pair_swap_and_reset() {
        let mut c = ColorPair::default();
        c.swap();
        assert_eq!(c.foreground, Rgba::WHITE);
        c.reset();
        assert_eq!(c.foreground, Rgba::BLACK);
    }

    #[test]
    fn touching_a_layer_bumps_its_revision() {
        let doc = Document::new(8, 8);
        let id = doc.active[0];
        let mut state = EditorState::new(doc);
        assert_eq!(state.revisions.get(&id), None);
        state.touch(id);
        state.touch(id);
        assert_eq!(state.revisions.get(&id), Some(&2));
        assert!(state.document.dirty);
    }

    #[test]
    fn paintable_layer_respects_kind_and_lock() {
        let doc = Document::new(8, 8);
        let id = doc.active[0];
        let mut state = EditorState::new(doc);
        assert_eq!(state.paintable_layer(), Some(id));

        state.document.layers.get_mut(id).unwrap().locked = true;
        assert_eq!(state.paintable_layer(), None);

        state.document.layers.get_mut(id).unwrap().locked = false;
        state.document.layers.get_mut(id).unwrap().kind = LayerKind::Group;
        assert_eq!(state.paintable_layer(), None);
    }

    #[test]
    fn a_full_selection_is_stored_as_none() {
        let mut state = EditorState::new(Document::new(8, 8));
        state.set_selection(Selection::all(8, 8));
        assert!(state.document.selection.is_none());

        state.set_selection(Selection::none(8, 8));
        assert!(state.document.selection.is_none());
    }

    #[test]
    fn setting_a_path_renames_the_document() {
        let mut state = EditorState::new(Document::new(8, 8));
        state.document.dirty = true;
        state.set_path(PathBuf::from("/tmp/Poster.pxm"));
        assert_eq!(state.document.name, "Poster");
        assert!(!state.document.dirty);
        assert_eq!(state.file_name(), "Poster.pxm");
    }
}
