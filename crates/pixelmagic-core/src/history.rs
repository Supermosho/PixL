//! Undo and redo.
//!
//! Edits are command objects, not document snapshots. That choice is forced by
//! the data: a 6000×4000 pixel layer is 96 MB, so snapshotting the document per
//! step would blow through memory after a handful of brush strokes. Instead
//! each edit stores only what it needs to reverse itself — a property's old
//! value, or the pixels inside the rectangle a stroke actually touched.
//!
//! Consecutive edits that belong to one gesture are **coalesced**: dragging an
//! opacity slider fires a change per frame, and the user expects one undo step,
//! not sixty. Coalescing is keyed on `(kind, target)` and stops as soon as any
//! other edit is pushed.

use crate::blend::BlendMode;
use crate::buffer::PixelBuffer;
use crate::document::Document;
use crate::geom::{Rect, Transform};
use crate::layer::{ColorTag, LayerId, LayerKind};
use crate::{CoreError, Result};
use std::any::Any;

/// A reversible change to a document.
pub trait Edit: std::fmt::Debug + Send {
    /// Shown in the Edit menu as "Undo <label>".
    fn label(&self) -> String;

    fn undo(&mut self, doc: &mut Document) -> Result<()>;
    fn redo(&mut self, doc: &mut Document) -> Result<()>;

    /// Identity for coalescing. Two consecutive edits with the same `Some(key)`
    /// are merged into one undo step. `None` never coalesces.
    fn coalesce_key(&self) -> Option<String> {
        None
    }

    /// Absorb a later edit with the same key, keeping this edit's "before"
    /// state and adopting the other's "after". Returns false if the merge is
    /// not possible, in which case the caller pushes the edit separately.
    fn absorb(&mut self, _next: &dyn Any) -> bool {
        false
    }

    fn as_any(&self) -> &dyn Any;
}

// ---------------------------------------------------------------------------
// Layer properties
// ---------------------------------------------------------------------------

/// A single settable property of a layer. Bundling them into one enum means one
/// edit type covers the whole Style pane and the Layers sidebar.
#[derive(Debug, Clone, PartialEq)]
pub enum LayerProperty {
    Name(String),
    Visible(bool),
    Locked(bool),
    Opacity(f32),
    Blend(BlendMode),
    Transform(Transform),
    Clipping(bool),
    ColorTag(ColorTag),
}

impl LayerProperty {
    /// Short name used in the undo label and as part of the coalescing key.
    pub fn kind(&self) -> &'static str {
        match self {
            LayerProperty::Name(_) => "Rename Layer",
            LayerProperty::Visible(_) => "Toggle Visibility",
            LayerProperty::Locked(_) => "Lock Layer",
            LayerProperty::Opacity(_) => "Change Opacity",
            LayerProperty::Blend(_) => "Change Blend Mode",
            LayerProperty::Transform(_) => "Transform Layer",
            LayerProperty::Clipping(_) => "Clipping Mask",
            LayerProperty::ColorTag(_) => "Color Tag",
        }
    }

    fn apply(&self, doc: &mut Document, id: LayerId) -> Result<()> {
        let layer = doc.layers.try_get_mut(id)?;
        match self.clone() {
            LayerProperty::Name(v) => layer.name = v,
            LayerProperty::Visible(v) => layer.visible = v,
            LayerProperty::Locked(v) => layer.locked = v,
            LayerProperty::Opacity(v) => layer.opacity = v.clamp(0.0, 1.0),
            LayerProperty::Blend(v) => layer.blend_mode = v,
            LayerProperty::Transform(v) => layer.transform = v,
            LayerProperty::Clipping(v) => layer.clipping = v,
            LayerProperty::ColorTag(v) => layer.color_tag = v,
        }
        doc.dirty = true;
        Ok(())
    }

    /// Read the current value of the same property.
    fn capture(&self, doc: &Document, id: LayerId) -> Result<LayerProperty> {
        let layer = doc.layers.try_get(id)?;
        Ok(match self {
            LayerProperty::Name(_) => LayerProperty::Name(layer.name.clone()),
            LayerProperty::Visible(_) => LayerProperty::Visible(layer.visible),
            LayerProperty::Locked(_) => LayerProperty::Locked(layer.locked),
            LayerProperty::Opacity(_) => LayerProperty::Opacity(layer.opacity),
            LayerProperty::Blend(_) => LayerProperty::Blend(layer.blend_mode),
            LayerProperty::Transform(_) => LayerProperty::Transform(layer.transform),
            LayerProperty::Clipping(_) => LayerProperty::Clipping(layer.clipping),
            LayerProperty::ColorTag(_) => LayerProperty::ColorTag(layer.color_tag),
        })
    }
}

#[derive(Debug)]
pub struct SetLayerProperty {
    id: LayerId,
    before: LayerProperty,
    after: LayerProperty,
}

impl SetLayerProperty {
    /// Capture the current value and prepare to set a new one.
    pub fn new(doc: &Document, id: LayerId, after: LayerProperty) -> Result<Self> {
        let before = after.capture(doc, id)?;
        Ok(Self { id, before, after })
    }
}

impl Edit for SetLayerProperty {
    fn label(&self) -> String {
        self.after.kind().to_string()
    }

    fn undo(&mut self, doc: &mut Document) -> Result<()> {
        self.before.apply(doc, self.id)
    }

    fn redo(&mut self, doc: &mut Document) -> Result<()> {
        self.after.apply(doc, self.id)
    }

    fn coalesce_key(&self) -> Option<String> {
        // Only continuous properties are worth merging; a visibility toggle
        // should stay one step per click.
        match self.after {
            LayerProperty::Opacity(_)
            | LayerProperty::Transform(_)
            | LayerProperty::Name(_) => Some(format!("{}:{:?}", self.after.kind(), self.id)),
            _ => None,
        }
    }

    fn absorb(&mut self, next: &dyn Any) -> bool {
        match next.downcast_ref::<SetLayerProperty>() {
            Some(o) if o.id == self.id => {
                self.after = o.after.clone();
                true
            }
            _ => false,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Pixel edits
// ---------------------------------------------------------------------------

/// Reverses a change to part of a pixel layer by keeping the before and after
/// contents of just the affected rectangle.
///
/// This is what makes brush strokes affordable: a stroke across a corner of a
/// 24-megapixel image stores only the corner.
#[derive(Debug)]
pub struct PixelRegionEdit {
    id: LayerId,
    origin: (i32, i32),
    before: PixelBuffer,
    after: PixelBuffer,
    label: String,
}

impl PixelRegionEdit {
    /// Snapshot `rect` of the layer as the "before" state. Call before
    /// mutating, then [`PixelRegionEdit::finish`] after.
    pub fn begin(
        doc: &Document,
        id: LayerId,
        rect: Rect,
        label: impl Into<String>,
    ) -> Result<Self> {
        let layer = doc.layers.try_get(id)?;
        let LayerKind::Pixel { buffer } = &layer.kind else {
            return Err(CoreError::Invalid("not a pixel layer".into()));
        };
        let r = rect.round_out().intersection(buffer.bounds());
        Ok(Self {
            id,
            origin: (r.x as i32, r.y as i32),
            before: buffer.crop(r),
            after: PixelBuffer::new(0, 0),
            label: label.into(),
        })
    }

    /// Capture the "after" state from the now-modified layer.
    pub fn finish(&mut self, doc: &Document) -> Result<()> {
        let layer = doc.layers.try_get(self.id)?;
        let LayerKind::Pixel { buffer } = &layer.kind else {
            return Err(CoreError::Invalid("not a pixel layer".into()));
        };
        let r = Rect::new(
            self.origin.0 as f32,
            self.origin.1 as f32,
            self.before.width() as f32,
            self.before.height() as f32,
        );
        self.after = buffer.crop(r);
        Ok(())
    }

    /// True when nothing actually changed, so the edit can be dropped instead
    /// of cluttering the undo stack.
    pub fn is_empty(&self) -> bool {
        self.before.width() == 0 || self.before == self.after
    }

    fn restore(&self, doc: &mut Document, buf: &PixelBuffer) -> Result<()> {
        let layer = doc.layers.try_get_mut(self.id)?;
        let LayerKind::Pixel { buffer } = &mut layer.kind else {
            return Err(CoreError::Invalid("not a pixel layer".into()));
        };
        buffer.blit(buf, self.origin.0, self.origin.1);
        doc.dirty = true;
        Ok(())
    }
}

impl Edit for PixelRegionEdit {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn undo(&mut self, doc: &mut Document) -> Result<()> {
        let before = self.before.clone();
        self.restore(doc, &before)
    }

    fn redo(&mut self, doc: &mut Document) -> Result<()> {
        let after = self.after.clone();
        self.restore(doc, &after)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ---------------------------------------------------------------------------
// The stack
// ---------------------------------------------------------------------------

pub struct History {
    undo_stack: Vec<Box<dyn Edit>>,
    redo_stack: Vec<Box<dyn Edit>>,
    /// Maximum undo steps retained. Older steps are dropped from the bottom.
    limit: usize,
    /// Set while undoing or redoing, so edits applied as a side effect are not
    /// themselves recorded.
    replaying: bool,
    /// Set by [`History::break_coalescing`]; cleared by the next push.
    coalescing_blocked: bool,
}

impl std::fmt::Debug for History {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("History")
            .field("undo", &self.undo_stack.len())
            .field("redo", &self.redo_stack.len())
            .finish()
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new(200)
    }
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            limit: limit.max(1),
            replaying: false,
            coalescing_blocked: false,
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_label(&self) -> Option<String> {
        self.undo_stack.last().map(|e| e.label())
    }

    pub fn redo_label(&self) -> Option<String> {
        self.redo_stack.last().map(|e| e.label())
    }

    pub fn depth(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.coalescing_blocked = false;
    }

    /// Apply an edit and record it.
    ///
    /// Any redo history is discarded, since the timeline has branched.
    pub fn push(&mut self, doc: &mut Document, mut edit: Box<dyn Edit>) -> Result<()> {
        if self.replaying {
            return edit.redo(doc);
        }
        edit.redo(doc)?;
        self.redo_stack.clear();

        // Try to fold into the previous edit, unless a gesture just ended.
        let blocked = std::mem::take(&mut self.coalescing_blocked);
        if !blocked {
            if let (Some(key), Some(top)) = (edit.coalesce_key(), self.undo_stack.last_mut()) {
                if top.coalesce_key().as_deref() == Some(key.as_str())
                    && top.absorb(edit.as_any())
                {
                    return Ok(());
                }
            }
        }

        self.undo_stack.push(edit);
        if self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
        Ok(())
    }

    /// Record an edit that has already been applied to the document.
    pub fn push_applied(&mut self, edit: Box<dyn Edit>) {
        if self.replaying {
            return;
        }
        self.coalescing_blocked = false;
        self.redo_stack.clear();
        self.undo_stack.push(edit);
        if self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
    }

    /// Prevent the next [`History::push`] from coalescing with what is already
    /// on the stack. Call on mouse-up so the next drag starts a fresh step
    /// rather than merging into the one that just ended.
    pub fn break_coalescing(&mut self) {
        self.coalescing_blocked = true;
    }

    pub fn undo(&mut self, doc: &mut Document) -> Result<Option<String>> {
        let Some(mut edit) = self.undo_stack.pop() else {
            return Ok(None);
        };
        self.replaying = true;
        let result = edit.undo(doc);
        self.replaying = false;
        result?;
        let label = edit.label();
        self.redo_stack.push(edit);
        Ok(Some(label))
    }

    pub fn redo(&mut self, doc: &mut Document) -> Result<Option<String>> {
        let Some(mut edit) = self.redo_stack.pop() else {
            return Ok(None);
        };
        self.replaying = true;
        let result = edit.redo(doc);
        self.replaying = false;
        result?;
        let label = edit.label();
        self.undo_stack.push(edit);
        Ok(Some(label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    fn doc_with_layer() -> (Document, LayerId) {
        let doc = Document::new(16, 16);
        let id = doc.active[0];
        (doc, id)
    }

    #[test]
    fn property_edit_round_trips() {
        let (mut doc, id) = doc_with_layer();
        let mut h = History::default();

        let edit = SetLayerProperty::new(&doc, id, LayerProperty::Opacity(0.25)).unwrap();
        h.push(&mut doc, Box::new(edit)).unwrap();
        assert_eq!(doc.layers.get(id).unwrap().opacity, 0.25);

        assert_eq!(h.undo(&mut doc).unwrap().as_deref(), Some("Change Opacity"));
        assert_eq!(doc.layers.get(id).unwrap().opacity, 1.0);

        h.redo(&mut doc).unwrap();
        assert_eq!(doc.layers.get(id).unwrap().opacity, 0.25);
    }

    #[test]
    fn opacity_drag_coalesces_into_one_step() {
        let (mut doc, id) = doc_with_layer();
        let mut h = History::default();
        for v in [0.9, 0.8, 0.7, 0.6] {
            let e = SetLayerProperty::new(&doc, id, LayerProperty::Opacity(v)).unwrap();
            h.push(&mut doc, Box::new(e)).unwrap();
        }
        assert_eq!(h.depth(), 1, "a slider drag should be one undo step");
        h.undo(&mut doc).unwrap();
        assert_eq!(doc.layers.get(id).unwrap().opacity, 1.0, "undo goes to the start");
    }

    #[test]
    fn breaking_coalescing_starts_a_new_step() {
        let (mut doc, id) = doc_with_layer();
        let mut h = History::default();
        let e = SetLayerProperty::new(&doc, id, LayerProperty::Opacity(0.5)).unwrap();
        h.push(&mut doc, Box::new(e)).unwrap();
        h.break_coalescing();
        let e = SetLayerProperty::new(&doc, id, LayerProperty::Opacity(0.2)).unwrap();
        h.push(&mut doc, Box::new(e)).unwrap();
        assert_eq!(h.depth(), 2, "a new gesture should be its own undo step");
        h.undo(&mut doc).unwrap();
        assert_eq!(doc.layers.get(id).unwrap().opacity, 0.5);
    }

    #[test]
    fn discrete_properties_do_not_coalesce() {
        let (mut doc, id) = doc_with_layer();
        let mut h = History::default();
        for v in [false, true, false] {
            let e = SetLayerProperty::new(&doc, id, LayerProperty::Visible(v)).unwrap();
            h.push(&mut doc, Box::new(e)).unwrap();
        }
        assert_eq!(h.depth(), 3);
    }

    #[test]
    fn edits_to_different_layers_do_not_coalesce() {
        let mut doc = Document::new(16, 16);
        let a = doc.active[0];
        let b = doc.add_empty_layer();
        let mut h = History::default();
        for id in [a, b] {
            let e = SetLayerProperty::new(&doc, id, LayerProperty::Opacity(0.5)).unwrap();
            h.push(&mut doc, Box::new(e)).unwrap();
        }
        assert_eq!(h.depth(), 2);
    }

    #[test]
    fn new_edit_discards_the_redo_branch() {
        let (mut doc, id) = doc_with_layer();
        let mut h = History::default();
        let e = SetLayerProperty::new(&doc, id, LayerProperty::Visible(false)).unwrap();
        h.push(&mut doc, Box::new(e)).unwrap();
        h.undo(&mut doc).unwrap();
        assert!(h.can_redo());

        let e = SetLayerProperty::new(&doc, id, LayerProperty::Locked(true)).unwrap();
        h.push(&mut doc, Box::new(e)).unwrap();
        assert!(!h.can_redo(), "branching should drop the redo stack");
    }

    #[test]
    fn pixel_region_edit_restores_only_its_rectangle() {
        let (mut doc, id) = doc_with_layer();
        let mut h = History::default();

        let rect = Rect::new(2.0, 2.0, 4.0, 4.0);
        let mut edit = PixelRegionEdit::begin(&doc, id, rect, "Paint").unwrap();

        // Mutate inside and outside the recorded rectangle.
        {
            let LayerKind::Pixel { buffer } = &mut doc.layers.get_mut(id).unwrap().kind else {
                panic!();
            };
            buffer.fill_rect(rect, Rgba::WHITE);
            buffer.set(10, 10, Rgba::rgb(1.0, 0.0, 0.0));
        }
        edit.finish(&doc).unwrap();
        assert!(!edit.is_empty());
        h.push_applied(Box::new(edit));

        h.undo(&mut doc).unwrap();
        let LayerKind::Pixel { buffer } = &doc.active_layer().unwrap().kind else {
            panic!();
        };
        assert_eq!(buffer.get(3, 3).unwrap().a, 0.0, "inside the rect was restored");
        assert_eq!(
            buffer.get(10, 10).unwrap().to_u8(),
            [255, 0, 0, 255],
            "outside the rect was left alone"
        );
    }

    #[test]
    fn unchanged_pixel_edits_report_empty() {
        let (doc, id) = doc_with_layer();
        let mut edit =
            PixelRegionEdit::begin(&doc, id, Rect::new(0.0, 0.0, 4.0, 4.0), "Paint").unwrap();
        edit.finish(&doc).unwrap();
        assert!(edit.is_empty(), "a no-op stroke should not become an undo step");
    }

    #[test]
    fn stack_respects_its_limit() {
        let (mut doc, id) = doc_with_layer();
        let mut h = History::new(3);
        for v in [true, false, true, false, true] {
            let e = SetLayerProperty::new(&doc, id, LayerProperty::Visible(v)).unwrap();
            h.push(&mut doc, Box::new(e)).unwrap();
        }
        assert_eq!(h.depth(), 3);
    }

    #[test]
    fn undo_on_empty_history_is_harmless() {
        let (mut doc, _) = doc_with_layer();
        let mut h = History::default();
        assert!(h.undo(&mut doc).unwrap().is_none());
        assert!(h.redo(&mut doc).unwrap().is_none());
        assert!(!h.can_undo() && !h.can_redo());
    }

    #[test]
    fn labels_are_reported_for_the_menu() {
        let (mut doc, id) = doc_with_layer();
        let mut h = History::default();
        let e =
            SetLayerProperty::new(&doc, id, LayerProperty::Blend(BlendMode::Multiply)).unwrap();
        h.push(&mut doc, Box::new(e)).unwrap();
        assert_eq!(h.undo_label().as_deref(), Some("Change Blend Mode"));
        h.undo(&mut doc).unwrap();
        assert_eq!(h.redo_label().as_deref(), Some("Change Blend Mode"));
    }
}
