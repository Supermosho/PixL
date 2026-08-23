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
use pixelmagic_core::tool::{BrushSettings, Tool};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// How the canvas maps document space to widget space.
#[derive(Debug, Clone, Copy)]
pub struct View {
    pub zoom: f32,
    /// Pan offset in widget pixels.
    pub offset: glam::Vec2,
}

impl Default for View {
    fn default() -> Self {
        Self { zoom: 1.0, offset: glam::Vec2::ZERO }
    }
}

impl View {
    pub const MIN_ZOOM: f32 = 0.02;
    pub const MAX_ZOOM: f32 = 64.0;

    /// Zoom so the whole document fits, with a small margin.
    pub fn fit(doc_w: f32, doc_h: f32, widget_w: f32, widget_h: f32) -> Self {
        if doc_w <= 0.0 || doc_h <= 0.0 || widget_w <= 0.0 || widget_h <= 0.0 {
            return View::default();
        }
        let margin = 32.0;
        let zoom = ((widget_w - margin) / doc_w)
            .min((widget_h - margin) / doc_h)
            .clamp(Self::MIN_ZOOM, 1.0);
        View { zoom, offset: glam::Vec2::ZERO }
    }

    /// Widget-space rectangle the document occupies.
    pub fn document_rect(self, doc_w: f32, doc_h: f32, widget_w: f32, widget_h: f32) -> Rect {
        let w = doc_w * self.zoom;
        let h = doc_h * self.zoom;
        Rect::new(
            (widget_w - w) * 0.5 + self.offset.x,
            (widget_h - h) * 0.5 + self.offset.y,
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
    pub gesture: Gesture,
    /// Bumped per layer whenever its pixels change, so the renderer knows to
    /// re-upload without diffing megabytes.
    pub revisions: HashMap<LayerId, u64>,
    pub show_checkerboard: bool,
    /// Set when something has changed and the canvas needs a redraw.
    pub needs_redraw: bool,
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
            gesture: Gesture::None,
            revisions: HashMap::new(),
            show_checkerboard: true,
            needs_redraw: true,
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

    pub fn title(&self) -> String {
        let star = if self.document.dirty { " •" } else { "" };
        format!("{}{star}", self.document.name)
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
        self.needs_redraw = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_the_document_inside_the_widget() {
        let v = View::fit(1000.0, 500.0, 400.0, 400.0);
        assert!(v.zoom * 1000.0 <= 400.0);
        assert!(v.zoom > 0.0);
    }

    #[test]
    fn fit_never_upscales() {
        let v = View::fit(10.0, 10.0, 2000.0, 2000.0);
        assert_eq!(v.zoom, 1.0, "a tiny document should not be blown up to fill");
    }

    #[test]
    fn fit_handles_degenerate_input() {
        assert_eq!(View::fit(0.0, 0.0, 100.0, 100.0).zoom, 1.0);
        assert_eq!(View::fit(100.0, 100.0, 0.0, 0.0).zoom, 1.0);
    }

    #[test]
    fn document_coordinates_round_trip() {
        let v = View { zoom: 2.0, offset: glam::Vec2::new(10.0, -5.0) };
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
    fn title_marks_unsaved_changes() {
        let mut state = EditorState::new(Document::new(8, 8));
        assert!(!state.title().contains('•'));
        state.document.dirty = true;
        assert!(state.title().contains('•'));
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
