//! Layers and the layer tree.
//!
//! Layers live in an arena keyed by [`LayerId`], with parent/child links rather
//! than nested ownership. The alternative — `Vec<Layer>` nested by group — makes
//! "find layer 47" a full traversal and reparenting an exercise in fighting the
//! borrow checker. An arena makes both trivial, at the cost of having to keep
//! the links consistent, which is what [`LayerTree`]'s API exists to guarantee.

use crate::adjust::AdjustmentInstance;
use crate::blend::BlendMode;
use crate::buffer::{MaskBuffer, PixelBuffer};
use crate::color::Rgba;
use crate::effect::Effect;
use crate::geom::{Rect, Transform};
use crate::style::LayerStyle;
use crate::text::TextContent;
use crate::vector::{Path, ShapeGeometry};
use crate::{CoreError, Result};
use serde::{Deserialize, Serialize};
use slotmap::{new_key_type, SlotMap};
use std::path::PathBuf;

new_key_type! {
    pub struct LayerId;
}

/// A colour tag, as offered by the Layers sidebar's context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColorTag {
    #[default]
    None,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Gray,
}

impl ColorTag {
    pub const ALL: [ColorTag; 8] = [
        ColorTag::None,
        ColorTag::Red,
        ColorTag::Orange,
        ColorTag::Yellow,
        ColorTag::Green,
        ColorTag::Blue,
        ColorTag::Purple,
        ColorTag::Gray,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ColorTag::None => "None",
            ColorTag::Red => "Red",
            ColorTag::Orange => "Orange",
            ColorTag::Yellow => "Yellow",
            ColorTag::Green => "Green",
            ColorTag::Blue => "Blue",
            ColorTag::Purple => "Purple",
            ColorTag::Gray => "Gray",
        }
    }

    pub fn color(self) -> Option<Rgba> {
        Some(match self {
            ColorTag::None => return None,
            ColorTag::Red => Rgba::rgb(0.90, 0.25, 0.22),
            ColorTag::Orange => Rgba::rgb(0.95, 0.58, 0.16),
            ColorTag::Yellow => Rgba::rgb(0.95, 0.80, 0.20),
            ColorTag::Green => Rgba::rgb(0.35, 0.75, 0.35),
            ColorTag::Blue => Rgba::rgb(0.25, 0.55, 0.92),
            ColorTag::Purple => Rgba::rgb(0.62, 0.40, 0.85),
            ColorTag::Gray => Rgba::rgb(0.55, 0.55, 0.58),
        })
    }
}

/// A layer's mask. Bitmap and vector masks are alternatives, not a stack —
/// which matches the original, where `Refine Mask` on a vector mask converts it
/// to a bitmap one rather than adding a second mask.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Mask {
    Bitmap {
        buffer: MaskBuffer,
        /// Offset of the mask's top-left corner in document space.
        offset: glam::Vec2,
        inverted: bool,
        opacity: f32,
        density: f32,
        feather: f32,
    },
    Vector {
        path: Path,
        inverted: bool,
        opacity: f32,
        density: f32,
        feather: f32,
    },
}

impl Mask {
    pub fn bitmap(width: u32, height: u32) -> Self {
        Mask::Bitmap {
            buffer: MaskBuffer::revealed(width, height),
            offset: glam::Vec2::ZERO,
            inverted: false,
            opacity: 1.0,
            density: 1.0,
            feather: 0.0,
        }
    }

    pub fn vector(path: Path) -> Self {
        Mask::Vector { path, inverted: false, opacity: 1.0, density: 1.0, feather: 0.0 }
    }

    pub fn is_vector(&self) -> bool {
        matches!(self, Mask::Vector { .. })
    }

    pub fn inverted(&self) -> bool {
        match self {
            Mask::Bitmap { inverted, .. } | Mask::Vector { inverted, .. } => *inverted,
        }
    }

    pub fn set_inverted(&mut self, v: bool) {
        match self {
            Mask::Bitmap { inverted, .. } | Mask::Vector { inverted, .. } => *inverted = v,
        }
    }
}

/// What a layer actually contains.
///
/// Mirrors SPEC §6.1. `Mask` is not a variant here because masks hang off a
/// layer rather than being layers themselves, even though the sidebar shows
/// them nested like one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayerKind {
    /// A container. Its children composite together before the group's own
    /// opacity, blend mode, style and mask apply.
    Group,
    /// Raster content. An "empty layer" is this with a fully transparent
    /// buffer.
    Pixel {
        buffer: PixelBuffer,
    },
    Shape {
        geometry: ShapeGeometry,
    },
    Text {
        content: Box<TextContent>,
    },
    /// A standalone adjustments layer: affects everything below it.
    ColorAdjustments,
    /// A standalone effects layer: affects everything below it.
    Effects,
    /// A video layer. Decoding is not implemented yet; the variant exists so
    /// documents that reference video round-trip instead of losing the layer.
    Video {
        path: PathBuf,
        frame: u32,
    },
}

impl LayerKind {
    pub fn type_label(&self) -> &'static str {
        match self {
            LayerKind::Group => "Group",
            LayerKind::Pixel { .. } => "Image",
            LayerKind::Shape { .. } => "Shape",
            LayerKind::Text { .. } => "Text",
            LayerKind::ColorAdjustments => "Color Adjustments",
            LayerKind::Effects => "Effects",
            LayerKind::Video { .. } => "Video",
        }
    }

    /// Whether this kind composites its own pixels. Adjustment and effects
    /// layers do not — they modify what is already beneath them.
    pub fn has_content(&self) -> bool {
        !matches!(self, LayerKind::ColorAdjustments | LayerKind::Effects)
    }

    /// SPEC §4: layer styles cannot be applied to effect layers, colour
    /// adjustment layers, or empty layers.
    pub fn accepts_styles(&self) -> bool {
        match self {
            LayerKind::ColorAdjustments | LayerKind::Effects => false,
            LayerKind::Pixel { buffer } => !buffer.opaque_bounds().is_empty(),
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub kind: LayerKind,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    /// Placement of the layer's content in document space.
    pub transform: Transform,
    /// Clip to the layer below — Pixelmator's "clipping set".
    pub clipping: bool,
    pub color_tag: ColorTag,
    pub style: LayerStyle,
    pub mask: Option<Mask>,
    /// Non-destructive adjustments attached to this layer.
    pub adjustments: Vec<AdjustmentInstance>,
    /// Non-destructive effects attached to this layer.
    pub effects: Vec<Effect>,
    pub parent: Option<LayerId>,
    /// Children, front-most first — the same order the sidebar shows.
    pub children: Vec<LayerId>,
}

impl Layer {
    fn bare(id: LayerId, name: impl Into<String>, kind: LayerKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            visible: true,
            locked: false,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            transform: Transform::IDENTITY,
            clipping: false,
            color_tag: ColorTag::None,
            style: LayerStyle::default(),
            mask: None,
            adjustments: Vec::new(),
            effects: Vec::new(),
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn is_group(&self) -> bool {
        matches!(self.kind, LayerKind::Group)
    }

    /// Bounds of the layer's own content in its local space, before
    /// `transform`. Groups return an empty rect; their extent comes from their
    /// children and is computed by [`LayerTree::bounds_of`].
    pub fn local_bounds(&self) -> Rect {
        match &self.kind {
            LayerKind::Pixel { buffer } => buffer.bounds(),
            LayerKind::Shape { geometry } => geometry.bounds(),
            LayerKind::Text { content } => {
                // Without a layout engine we can only estimate. The UI replaces
                // this with Pango's measured extents once the layer is laid out.
                let lines = content.text.lines().count().max(1) as f32;
                Rect::new(
                    0.0,
                    0.0,
                    content.width,
                    lines * content.base.size * content.base.line_height,
                )
            }
            LayerKind::Video { .. } => Rect::ZERO,
            LayerKind::Group | LayerKind::ColorAdjustments | LayerKind::Effects => Rect::ZERO,
        }
    }

    /// True when the layer contributes nothing and the renderer can skip it.
    pub fn is_hidden(&self) -> bool {
        !self.visible || self.opacity <= 0.0
    }

    /// Whether anything about this layer requires a render pass of its own
    /// rather than being drawn straight into the parent's accumulator.
    pub fn needs_offscreen(&self) -> bool {
        self.mask.is_some()
            || !self.style.is_empty()
            || self.adjustments.iter().any(|a| !a.is_noop())
            || self.effects.iter().any(|e| !e.is_noop())
            || self.is_group()
    }
}

/// The layer arena plus the root ordering.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LayerTree {
    layers: SlotMap<LayerId, Layer>,
    /// Top-level layers, front-most first.
    roots: Vec<LayerId>,
}

/// Compares trees by structure and content rather than by arena layout, so a
/// tree that has had layers removed and re-added still equals an identical one
/// built from scratch. Slot keys are an implementation detail; two documents
/// that render the same should compare equal.
impl PartialEq for LayerTree {
    fn eq(&self, other: &Self) -> bool {
        if self.layers.len() != other.layers.len() || self.roots.len() != other.roots.len() {
            return false;
        }
        let mine = self.iter_depth_first();
        let theirs = other.iter_depth_first();
        if mine.len() != theirs.len() {
            return false;
        }
        mine.iter().zip(theirs.iter()).all(|(&(a, da), &(b, db))| {
            da == db
                && match (self.layers.get(a), other.layers.get(b)) {
                    (Some(x), Some(y)) => {
                        x.name == y.name
                            && x.kind == y.kind
                            && x.visible == y.visible
                            && x.locked == y.locked
                            && x.opacity == y.opacity
                            && x.blend_mode == y.blend_mode
                            && x.transform == y.transform
                            && x.clipping == y.clipping
                            && x.color_tag == y.color_tag
                            && x.style == y.style
                            && x.mask == y.mask
                            && x.adjustments == y.adjustments
                            && x.effects == y.effects
                    }
                    _ => false,
                }
        })
    }
}

impl LayerTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn roots(&self) -> &[LayerId] {
        &self.roots
    }

    pub fn get(&self, id: LayerId) -> Option<&Layer> {
        self.layers.get(id)
    }

    pub fn get_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.get_mut(id)
    }

    pub fn try_get(&self, id: LayerId) -> Result<&Layer> {
        self.layers.get(id).ok_or(CoreError::NoSuchLayer(id))
    }

    pub fn try_get_mut(&mut self, id: LayerId) -> Result<&mut Layer> {
        self.layers.get_mut(id).ok_or(CoreError::NoSuchLayer(id))
    }

    /// Insert a new layer at the front of `parent`'s children (or of the roots).
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        kind: LayerKind,
        parent: Option<LayerId>,
    ) -> LayerId {
        let id = self.layers.insert_with_key(|id| Layer::bare(id, name, kind));
        self.layers[id].parent = parent;
        match parent {
            Some(p) => {
                if let Some(pl) = self.layers.get_mut(p) {
                    pl.children.insert(0, id);
                } else {
                    self.layers[id].parent = None;
                    self.roots.insert(0, id);
                }
            }
            None => self.roots.insert(0, id),
        }
        id
    }

    /// Insert directly above `sibling`, in the same parent.
    pub fn insert_above(
        &mut self,
        name: impl Into<String>,
        kind: LayerKind,
        sibling: LayerId,
    ) -> Result<LayerId> {
        let parent = self.try_get(sibling)?.parent;
        let id = self.layers.insert_with_key(|id| Layer::bare(id, name, kind));
        self.layers[id].parent = parent;
        let siblings = self.siblings_mut(parent);
        let pos = siblings.iter().position(|&s| s == sibling).unwrap_or(0);
        siblings.insert(pos, id);
        Ok(id)
    }

    fn siblings_mut(&mut self, parent: Option<LayerId>) -> &mut Vec<LayerId> {
        match parent {
            Some(p) => &mut self.layers[p].children,
            None => &mut self.roots,
        }
    }

    /// Remove a layer and everything under it. Returns the removed ids,
    /// deepest first.
    pub fn remove(&mut self, id: LayerId) -> Vec<LayerId> {
        if !self.layers.contains_key(id) {
            return Vec::new();
        }
        let parent = self.layers[id].parent;
        self.siblings_mut(parent).retain(|&c| c != id);

        let mut removed = Vec::new();
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if let Some(layer) = self.layers.get(cur) {
                stack.extend(layer.children.iter().copied());
            }
            removed.push(cur);
        }
        for &r in removed.iter().rev() {
            self.layers.remove(r);
        }
        removed
    }

    /// Every ancestor of `id`, nearest first.
    pub fn ancestors(&self, id: LayerId) -> Vec<LayerId> {
        let mut out = Vec::new();
        let mut cur = self.layers.get(id).and_then(|l| l.parent);
        while let Some(p) = cur {
            out.push(p);
            cur = self.layers.get(p).and_then(|l| l.parent);
        }
        out
    }

    pub fn is_ancestor_of(&self, ancestor: LayerId, descendant: LayerId) -> bool {
        self.ancestors(descendant).contains(&ancestor)
    }

    /// Move `id` into `new_parent` at `index`.
    ///
    /// Refuses to move a layer into its own subtree, which would detach that
    /// subtree from the roots and leak it.
    pub fn reparent(
        &mut self,
        id: LayerId,
        new_parent: Option<LayerId>,
        index: usize,
    ) -> Result<()> {
        self.try_get(id)?;
        if let Some(p) = new_parent {
            self.try_get(p)?;
            if p == id || self.is_ancestor_of(id, p) {
                return Err(CoreError::CyclicReparent);
            }
        }

        let old_parent = self.layers[id].parent;
        self.siblings_mut(old_parent).retain(|&c| c != id);
        self.layers[id].parent = new_parent;
        let siblings = self.siblings_mut(new_parent);
        let index = index.min(siblings.len());
        siblings.insert(index, id);
        Ok(())
    }

    /// Move within the current parent. Negative `delta` moves towards the
    /// front (up in the sidebar).
    pub fn reorder(&mut self, id: LayerId, delta: isize) -> Result<()> {
        let parent = self.try_get(id)?.parent;
        let siblings = self.siblings_mut(parent);
        let Some(pos) = siblings.iter().position(|&c| c == id) else {
            return Err(CoreError::NoSuchLayer(id));
        };
        let new = (pos as isize + delta).clamp(0, siblings.len() as isize - 1) as usize;
        let v = siblings.remove(pos);
        siblings.insert(new, v);
        Ok(())
    }

    /// Depth-first walk, front-most first, yielding `(id, depth)`.
    pub fn iter_depth_first(&self) -> Vec<(LayerId, usize)> {
        let mut out = Vec::with_capacity(self.layers.len());
        for &root in &self.roots {
            self.walk(root, 0, &mut out);
        }
        out
    }

    fn walk(&self, id: LayerId, depth: usize, out: &mut Vec<(LayerId, usize)>) {
        out.push((id, depth));
        if let Some(layer) = self.layers.get(id) {
            for &child in &layer.children {
                self.walk(child, depth + 1, out);
            }
        }
    }

    /// The children of `parent` in render order — back to front, i.e. the
    /// reverse of sidebar order.
    pub fn render_order(&self, parent: Option<LayerId>) -> Vec<LayerId> {
        let list = match parent {
            Some(p) => self.layers.get(p).map(|l| l.children.clone()).unwrap_or_default(),
            None => self.roots.clone(),
        };
        list.into_iter().rev().collect()
    }

    /// Bounds of a layer in document space, including its descendants.
    pub fn bounds_of(&self, id: LayerId) -> Rect {
        let Some(layer) = self.layers.get(id) else { return Rect::ZERO };
        let mut r = layer.local_bounds();
        if !r.is_empty() {
            r = r.transformed_bounds(&layer.transform);
        }
        for &child in &layer.children {
            let cb = self.bounds_of(child);
            if !cb.is_empty() {
                r = r.union(cb.transformed_bounds(&layer.transform));
            }
        }
        let expansion = layer.style.bounds_expansion();
        if expansion > 0.0 && !r.is_empty() {
            r = r.inset(expansion);
        }
        r
    }

    /// Wrap `ids` in a new group, inserted where the front-most of them was.
    /// All the layers must share a parent, which is what the UI enforces by
    /// only enabling Group for a same-level selection.
    pub fn group(&mut self, ids: &[LayerId], name: impl Into<String>) -> Result<LayerId> {
        if ids.is_empty() {
            return Err(CoreError::Invalid("nothing to group".into()));
        }
        let parent = self.try_get(ids[0])?.parent;
        for &id in ids {
            if self.try_get(id)?.parent != parent {
                return Err(CoreError::Invalid(
                    "can only group layers that share a parent".into(),
                ));
            }
        }

        let siblings = self.siblings_mut(parent);
        let index = ids
            .iter()
            .filter_map(|id| siblings.iter().position(|s| s == id))
            .min()
            .unwrap_or(0);

        let group = self.layers.insert_with_key(|id| Layer::bare(id, name, LayerKind::Group));
        self.layers[group].parent = parent;
        let siblings = self.siblings_mut(parent);
        siblings.retain(|s| !ids.contains(s));
        let index = index.min(siblings.len());
        siblings.insert(index, group);

        // Preserve relative order: reparent front-most last so it ends up
        // front-most inside the group.
        for &id in ids.iter().rev() {
            self.layers[id].parent = Some(group);
            self.layers[group].children.insert(0, id);
        }
        Ok(group)
    }

    /// Dissolve a group, splicing its children into its place.
    pub fn ungroup(&mut self, id: LayerId) -> Result<Vec<LayerId>> {
        let layer = self.try_get(id)?;
        if !layer.is_group() {
            return Err(CoreError::Invalid("not a group".into()));
        }
        let parent = layer.parent;
        let children = layer.children.clone();

        let siblings = self.siblings_mut(parent);
        let index = siblings.iter().position(|&s| s == id).unwrap_or(0);
        siblings.remove(index);
        for (i, &child) in children.iter().enumerate() {
            self.siblings_mut(parent).insert(index + i, child);
            self.layers[child].parent = parent;
        }
        self.layers[id].children.clear();
        self.layers.remove(id);
        Ok(children)
    }

    /// Deep-copy a layer and its subtree, inserting the copy above the
    /// original.
    pub fn duplicate(&mut self, id: LayerId) -> Result<LayerId> {
        let src = self.try_get(id)?.clone();
        let new_id = self.insert_above(format!("{} copy", src.name), src.kind.clone(), id)?;
        {
            let dst = &mut self.layers[new_id];
            dst.visible = src.visible;
            dst.locked = src.locked;
            dst.opacity = src.opacity;
            dst.blend_mode = src.blend_mode;
            dst.transform = src.transform;
            dst.clipping = src.clipping;
            dst.color_tag = src.color_tag;
            dst.style = src.style.clone();
            dst.mask = src.mask.clone();
            dst.adjustments = src.adjustments.clone();
            dst.effects = src.effects.clone();
        }
        for &child in src.children.iter().rev() {
            self.duplicate_into(child, new_id)?;
        }
        Ok(new_id)
    }

    fn duplicate_into(&mut self, id: LayerId, parent: LayerId) -> Result<LayerId> {
        let src = self.try_get(id)?.clone();
        let new_id = self.insert(src.name.clone(), src.kind.clone(), Some(parent));
        {
            let dst = &mut self.layers[new_id];
            dst.visible = src.visible;
            dst.locked = src.locked;
            dst.opacity = src.opacity;
            dst.blend_mode = src.blend_mode;
            dst.transform = src.transform;
            dst.clipping = src.clipping;
            dst.color_tag = src.color_tag;
            dst.style = src.style.clone();
            dst.mask = src.mask.clone();
            dst.adjustments = src.adjustments.clone();
            dst.effects = src.effects.clone();
        }
        for &child in src.children.iter().rev() {
            self.duplicate_into(child, new_id)?;
        }
        Ok(new_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(w: u32, h: u32) -> LayerKind {
        LayerKind::Pixel { buffer: PixelBuffer::new(w, h) }
    }

    #[test]
    fn equality_ignores_arena_slot_reuse() {
        let mut a = LayerTree::new();
        let tmp = a.insert("Temp", pixel(2, 2), None);
        a.remove(tmp);
        a.insert("Real", pixel(2, 2), None);

        let mut b = LayerTree::new();
        b.insert("Real", pixel(2, 2), None);

        assert_eq!(a, b, "slot reuse must not affect equality");

        b.get_mut(b.roots()[0]).unwrap().opacity = 0.5;
        assert_ne!(a, b, "a real difference must still compare unequal");
    }

    #[test]
    fn insert_puts_new_layers_in_front() {
        let mut t = LayerTree::new();
        let a = t.insert("A", pixel(4, 4), None);
        let b = t.insert("B", pixel(4, 4), None);
        assert_eq!(t.roots(), &[b, a], "newest layer should be front-most");
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn render_order_is_back_to_front() {
        let mut t = LayerTree::new();
        let a = t.insert("A", pixel(4, 4), None);
        let b = t.insert("B", pixel(4, 4), None);
        assert_eq!(t.render_order(None), vec![a, b]);
    }

    #[test]
    fn removing_a_group_removes_its_subtree() {
        let mut t = LayerTree::new();
        let g = t.insert("G", LayerKind::Group, None);
        let a = t.insert("A", pixel(4, 4), Some(g));
        let b = t.insert("B", pixel(4, 4), Some(a));
        let removed = t.remove(g);
        assert_eq!(removed.len(), 3);
        assert!(t.is_empty());
        assert!(t.get(b).is_none());
    }

    #[test]
    fn removing_an_unknown_layer_is_harmless() {
        let mut t = LayerTree::new();
        let a = t.insert("A", pixel(1, 1), None);
        t.remove(a);
        assert!(t.remove(a).is_empty());
    }

    #[test]
    fn reparent_rejects_cycles() {
        let mut t = LayerTree::new();
        let g = t.insert("G", LayerKind::Group, None);
        let inner = t.insert("Inner", LayerKind::Group, Some(g));
        assert!(matches!(t.reparent(g, Some(inner), 0), Err(CoreError::CyclicReparent)));
        assert!(matches!(t.reparent(g, Some(g), 0), Err(CoreError::CyclicReparent)));
        // The tree survived the rejected moves.
        assert_eq!(t.get(inner).unwrap().parent, Some(g));
    }

    #[test]
    fn reparent_moves_between_parents() {
        let mut t = LayerTree::new();
        let g1 = t.insert("G1", LayerKind::Group, None);
        let g2 = t.insert("G2", LayerKind::Group, None);
        let a = t.insert("A", pixel(4, 4), Some(g1));
        t.reparent(a, Some(g2), 0).unwrap();
        assert_eq!(t.get(a).unwrap().parent, Some(g2));
        assert!(t.get(g1).unwrap().children.is_empty());
        assert_eq!(t.get(g2).unwrap().children, vec![a]);
    }

    #[test]
    fn reparent_clamps_out_of_range_index() {
        let mut t = LayerTree::new();
        let g = t.insert("G", LayerKind::Group, None);
        let a = t.insert("A", pixel(4, 4), None);
        t.reparent(a, Some(g), 999).unwrap();
        assert_eq!(t.get(g).unwrap().children, vec![a]);
    }

    #[test]
    fn reorder_clamps_at_the_ends() {
        let mut t = LayerTree::new();
        let a = t.insert("A", pixel(4, 4), None);
        let b = t.insert("B", pixel(4, 4), None);
        let c = t.insert("C", pixel(4, 4), None);
        assert_eq!(t.roots(), &[c, b, a]);
        t.reorder(c, -5).unwrap();
        assert_eq!(t.roots(), &[c, b, a], "already front-most");
        t.reorder(c, 99).unwrap();
        assert_eq!(t.roots(), &[b, a, c]);
    }

    #[test]
    fn grouping_preserves_order_and_position() {
        let mut t = LayerTree::new();
        let a = t.insert("A", pixel(4, 4), None);
        let b = t.insert("B", pixel(4, 4), None);
        let c = t.insert("C", pixel(4, 4), None);
        assert_eq!(t.roots(), &[c, b, a]);

        let g = t.group(&[c, b], "Group").unwrap();
        assert_eq!(t.roots(), &[g, a]);
        assert_eq!(t.get(g).unwrap().children, vec![c, b]);
        assert_eq!(t.get(c).unwrap().parent, Some(g));
    }

    #[test]
    fn grouping_across_parents_is_refused() {
        let mut t = LayerTree::new();
        let g = t.insert("G", LayerKind::Group, None);
        let a = t.insert("A", pixel(4, 4), None);
        let b = t.insert("B", pixel(4, 4), Some(g));
        assert!(t.group(&[a, b], "X").is_err());
        assert!(t.group(&[], "X").is_err());
    }

    #[test]
    fn ungroup_splices_children_back() {
        let mut t = LayerTree::new();
        let a = t.insert("A", pixel(4, 4), None);
        let b = t.insert("B", pixel(4, 4), None);
        let z = t.insert("Z", pixel(4, 4), None);
        let g = t.group(&[b, a], "G").unwrap();
        assert_eq!(t.roots(), &[z, g]);

        let kids = t.ungroup(g).unwrap();
        assert_eq!(kids, vec![b, a]);
        assert_eq!(t.roots(), &[z, b, a]);
        assert_eq!(t.get(b).unwrap().parent, None);
        assert!(t.get(g).is_none());
    }

    #[test]
    fn ungroup_rejects_non_groups() {
        let mut t = LayerTree::new();
        let a = t.insert("A", pixel(4, 4), None);
        assert!(t.ungroup(a).is_err());
    }

    #[test]
    fn duplicate_copies_the_subtree_and_settings() {
        let mut t = LayerTree::new();
        let g = t.insert("G", LayerKind::Group, None);
        let child = t.insert("Child", pixel(4, 4), Some(g));
        t.get_mut(g).unwrap().opacity = 0.42;
        t.get_mut(g).unwrap().blend_mode = BlendMode::Multiply;
        t.get_mut(child).unwrap().name = "Kid".into();

        let copy = t.duplicate(g).unwrap();
        let copied = t.get(copy).unwrap();
        assert_eq!(copied.name, "G copy");
        assert_eq!(copied.opacity, 0.42);
        assert_eq!(copied.blend_mode, BlendMode::Multiply);
        assert_eq!(copied.children.len(), 1);
        assert_eq!(t.get(copied.children[0]).unwrap().name, "Kid");
        // Sits directly above the original.
        assert_eq!(t.roots(), &[copy, g]);
    }

    #[test]
    fn depth_first_walk_reports_depth() {
        let mut t = LayerTree::new();
        let g = t.insert("G", LayerKind::Group, None);
        let inner = t.insert("Inner", LayerKind::Group, Some(g));
        t.insert("Deep", pixel(1, 1), Some(inner));
        let depths: Vec<usize> = t.iter_depth_first().iter().map(|(_, d)| *d).collect();
        assert_eq!(depths, vec![0, 1, 2]);
    }

    #[test]
    fn bounds_include_children_and_style_growth() {
        let mut t = LayerTree::new();
        let g = t.insert("G", LayerKind::Group, None);
        let a = t.insert("A", pixel(10, 10), Some(g));
        t.get_mut(a).unwrap().transform = Transform::translate(glam::Vec2::new(20.0, 20.0));
        let b = t.bounds_of(g);
        assert_eq!(b, Rect::new(20.0, 20.0, 10.0, 10.0));
    }

    #[test]
    fn adjustment_layers_reject_styles() {
        assert!(!LayerKind::ColorAdjustments.accepts_styles());
        assert!(!LayerKind::Effects.accepts_styles());
        assert!(!pixel(4, 4).accepts_styles(), "an empty layer takes no styles");
        assert!(LayerKind::Group.accepts_styles());
    }

    #[test]
    fn hidden_layers_are_detected() {
        let mut t = LayerTree::new();
        let a = t.insert("A", pixel(4, 4), None);
        assert!(!t.get(a).unwrap().is_hidden());
        t.get_mut(a).unwrap().opacity = 0.0;
        assert!(t.get(a).unwrap().is_hidden());
        t.get_mut(a).unwrap().opacity = 1.0;
        t.get_mut(a).unwrap().visible = false;
        assert!(t.get(a).unwrap().is_hidden());
    }

    #[test]
    fn plain_layers_avoid_an_offscreen_pass() {
        let mut t = LayerTree::new();
        let a = t.insert("A", pixel(4, 4), None);
        assert!(!t.get(a).unwrap().needs_offscreen());
        t.get_mut(a).unwrap().mask = Some(Mask::bitmap(4, 4));
        assert!(t.get(a).unwrap().needs_offscreen());
    }
}
