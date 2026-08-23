//! The document: canvas, layer tree, selection and view-independent state.

use crate::buffer::PixelBuffer;
use crate::color::{BitDepth, ColorSpace, Rgba};
use crate::geom::{Rect, Size};
use crate::layer::{Layer, LayerId, LayerKind, LayerTree};
use crate::selection::Selection;
use crate::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A ruler guide.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub horizontal: bool,
    /// Position along the perpendicular axis, in canvas pixels.
    pub position: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridSettings {
    pub visible: bool,
    pub spacing: f32,
    pub subdivisions: u32,
    pub snap: bool,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self { visible: false, spacing: 50.0, subdivisions: 5, snap: false }
    }
}

/// Units for the rulers and the image-size dialog (SPEC §6.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Unit {
    #[default]
    Pixels,
    Inches,
    Centimeters,
}

impl Unit {
    pub const ALL: [Unit; 3] = [Unit::Pixels, Unit::Inches, Unit::Centimeters];

    pub fn label(self) -> &'static str {
        match self {
            Unit::Pixels => "Pixels",
            Unit::Inches => "Inches",
            Unit::Centimeters => "Centimeters",
        }
    }

    pub fn from_pixels(self, px: f32, dpi: f32) -> f32 {
        match self {
            Unit::Pixels => px,
            Unit::Inches => px / dpi,
            Unit::Centimeters => px / dpi * 2.54,
        }
    }

    pub fn to_pixels(self, value: f32, dpi: f32) -> f32 {
        match self {
            Unit::Pixels => value,
            Unit::Inches => value * dpi,
            Unit::Centimeters => value / 2.54 * dpi,
        }
    }
}

/// Resampling algorithms offered by Image Size (SPEC §6.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Resampling {
    #[default]
    Bilinear,
    Lanczos,
    NearestNeighbor,
    /// ML upscaling. Requires a model to be available; the UI disables the
    /// option when one is not.
    SuperResolution,
}

impl Resampling {
    pub const ALL: [Resampling; 4] = [
        Resampling::Bilinear,
        Resampling::Lanczos,
        Resampling::NearestNeighbor,
        Resampling::SuperResolution,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Resampling::Bilinear => "Bilinear",
            Resampling::Lanczos => "Lanczos",
            Resampling::NearestNeighbor => "Nearest Neighbor",
            Resampling::SuperResolution => "Super Resolution",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Resampling::Bilinear => "Good for resizing images in most use cases",
            Resampling::Lanczos => "Good for resizing images with small details",
            Resampling::NearestNeighbor => {
                "Copies the color of the nearest pixel when resizing, \
                 resulting in a blocky, pixellated look"
            }
            Resampling::SuperResolution => {
                "Preserves sharpness and details intelligently. \
                 Ideal for making an image larger"
            }
        }
    }
}

/// Which corner or edge stays fixed when the canvas is resized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// Normalised offset of the anchor within the frame, 0..1 on each axis.
    pub fn factors(self) -> (f32, f32) {
        let col = match self {
            Anchor::TopLeft | Anchor::Left | Anchor::BottomLeft => 0.0,
            Anchor::Top | Anchor::Center | Anchor::Bottom => 0.5,
            _ => 1.0,
        };
        let row = match self {
            Anchor::TopLeft | Anchor::Top | Anchor::TopRight => 0.0,
            Anchor::Left | Anchor::Center | Anchor::Right => 0.5,
            _ => 1.0,
        };
        (col, row)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    pub path: Option<PathBuf>,
    /// Canvas size in pixels.
    pub width: u32,
    pub height: u32,
    /// Pixels per inch, used for print output and the ruler units.
    pub resolution: f32,
    pub color_space: ColorSpace,
    pub bit_depth: BitDepth,
    pub layers: LayerTree,
    pub selection: Option<Selection>,
    /// Layers the user has selected in the sidebar, front-most first.
    pub active: Vec<LayerId>,
    pub guides: Vec<Guide>,
    pub grid: GridSettings,
    pub unit: Unit,
    /// Set on every mutation, cleared on save.
    pub dirty: bool,
}

impl Document {
    /// A new document with a single transparent pixel layer, matching what
    /// `Command-N` produces.
    pub fn new(width: u32, height: u32) -> Self {
        let mut doc = Self::empty(width, height);
        let id = doc.layers.insert(
            "Background",
            LayerKind::Pixel { buffer: PixelBuffer::new(width, height) },
            None,
        );
        doc.active = vec![id];
        doc
    }

    /// A document with no layers at all.
    pub fn empty(width: u32, height: u32) -> Self {
        Self {
            name: "Untitled".into(),
            path: None,
            width: width.max(1),
            height: height.max(1),
            resolution: 72.0,
            color_space: ColorSpace::default(),
            bit_depth: BitDepth::default(),
            layers: LayerTree::new(),
            selection: None,
            active: Vec::new(),
            guides: Vec::new(),
            grid: GridSettings::default(),
            unit: Unit::default(),
            dirty: false,
        }
    }

    /// A document wrapping a single imported image.
    pub fn from_image(name: impl Into<String>, buffer: PixelBuffer) -> Self {
        let (w, h) = (buffer.width(), buffer.height());
        let mut doc = Self::empty(w, h);
        doc.name = name.into();
        let id = doc.layers.insert("Image", LayerKind::Pixel { buffer }, None);
        doc.active = vec![id];
        doc
    }

    pub fn size(&self) -> Size {
        Size::new(self.width as f32, self.height as f32)
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width as f32, self.height as f32)
    }

    /// The single active layer, or `None` when the selection is empty or spans
    /// several layers.
    pub fn active_layer(&self) -> Option<&Layer> {
        match self.active.as_slice() {
            [one] => self.layers.get(*one),
            _ => None,
        }
    }

    pub fn active_layer_mut(&mut self) -> Option<&mut Layer> {
        match self.active.as_slice() {
            [one] => {
                let id = *one;
                self.layers.get_mut(id)
            }
            _ => None,
        }
    }

    /// The front-most active layer, which is what single-target commands act
    /// on when several layers are selected.
    pub fn primary_active(&self) -> Option<LayerId> {
        self.active.first().copied()
    }

    pub fn set_active(&mut self, ids: Vec<LayerId>) {
        self.active = ids.into_iter().filter(|id| self.layers.get(*id).is_some()).collect();
    }

    /// Drop any active ids that no longer exist. Call after removing layers.
    pub fn prune_active(&mut self) {
        self.active.retain(|id| self.layers.get(*id).is_some());
    }

    /// The effective selection, treating "no selection" as "everything".
    pub fn selection_bounds(&self) -> Rect {
        match &self.selection {
            Some(s) if !s.is_empty() => s.bounds(),
            _ => self.bounds(),
        }
    }

    pub fn has_selection(&self) -> bool {
        self.selection.as_ref().is_some_and(|s| !s.is_empty() && !s.is_everything())
    }

    /// `Command-D`.
    pub fn deselect(&mut self) {
        self.selection = None;
    }

    /// `Command-A`.
    pub fn select_all(&mut self) {
        self.selection = Some(Selection::all(self.width, self.height));
    }

    /// Change the canvas size without resampling, anchoring the existing
    /// content. Returns the offset applied to layer positions.
    pub fn resize_canvas(&mut self, width: u32, height: u32, anchor: Anchor) -> glam::Vec2 {
        let (fx, fy) = anchor.factors();
        let dx = (width as f32 - self.width as f32) * fx;
        let dy = (height as f32 - self.height as f32) * fy;
        self.width = width.max(1);
        self.height = height.max(1);

        let offset = glam::Vec2::new(dx, dy);
        if offset != glam::Vec2::ZERO {
            let roots: Vec<LayerId> = self.layers.roots().to_vec();
            for id in roots {
                if let Some(layer) = self.layers.get_mut(id) {
                    layer.transform =
                        layer.transform.then(&crate::geom::Transform::translate(offset));
                }
            }
        }
        // A selection sized for the old canvas is meaningless on the new one.
        self.selection = None;
        self.dirty = true;
        offset
    }

    /// Tight bounds of every visible layer — what `Trim` and "crop to
    /// contents" use.
    pub fn content_bounds(&self) -> Rect {
        let mut r = Rect::ZERO;
        for &root in self.layers.roots() {
            if self.layers.get(root).is_some_and(|l| l.is_hidden()) {
                continue;
            }
            r = r.union(self.layers.bounds_of(root));
        }
        r
    }

    /// Add a layer above the front-most active layer, or at the top of the
    /// stack when nothing is active.
    pub fn add_layer(&mut self, name: impl Into<String>, kind: LayerKind) -> LayerId {
        let id = match self.primary_active() {
            Some(sibling) => self
                .layers
                .insert_above(name, kind, sibling)
                .unwrap_or_else(|_| unreachable!("active id was just validated")),
            None => self.layers.insert(name, kind, None),
        };
        self.active = vec![id];
        self.dirty = true;
        id
    }

    /// Add an empty pixel layer the size of the canvas — `Shift-Command-N`.
    pub fn add_empty_layer(&mut self) -> LayerId {
        self.add_layer(
            "Layer",
            LayerKind::Pixel { buffer: PixelBuffer::new(self.width, self.height) },
        )
    }

    pub fn remove_layer(&mut self, id: LayerId) -> Result<()> {
        if self.layers.get(id).is_none() {
            return Err(CoreError::NoSuchLayer(id));
        }
        if self.layers.get(id).is_some_and(|l| l.locked) {
            return Err(CoreError::LayerLocked);
        }
        self.layers.remove(id);
        self.prune_active();
        self.dirty = true;
        Ok(())
    }

    /// Fill the current selection (or the whole active layer) with a colour.
    pub fn fill_selection(&mut self, color: Rgba) -> Result<()> {
        let id = self.primary_active().ok_or(CoreError::Invalid("no active layer".into()))?;
        let selection = self.selection.clone();
        let layer = self.layers.try_get_mut(id)?;
        if layer.locked {
            return Err(CoreError::LayerLocked);
        }
        let LayerKind::Pixel { buffer } = &mut layer.kind else {
            return Err(CoreError::Invalid("can only fill a pixel layer".into()));
        };

        match selection {
            Some(sel) if !sel.is_everything() => {
                for y in 0..buffer.height() {
                    for x in 0..buffer.width() {
                        let cov = sel.coverage_at(x, y);
                        if cov <= 0.0 {
                            continue;
                        }
                        let existing = buffer.get(x, y).unwrap_or(Rgba::TRANSPARENT);
                        buffer.set(x, y, existing.lerp(color, cov));
                    }
                }
            }
            _ => buffer.fill(color),
        }
        self.dirty = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_has_one_active_layer() {
        let doc = Document::new(800, 600);
        assert_eq!(doc.layers.len(), 1);
        assert_eq!(doc.active.len(), 1);
        assert!(doc.active_layer().is_some());
        assert_eq!(doc.bounds(), Rect::new(0.0, 0.0, 800.0, 600.0));
    }

    #[test]
    fn zero_sized_documents_are_clamped() {
        let doc = Document::empty(0, 0);
        assert_eq!((doc.width, doc.height), (1, 1));
    }

    #[test]
    fn adding_a_layer_puts_it_above_the_active_one() {
        let mut doc = Document::new(64, 64);
        let first = doc.active[0];
        let second = doc.add_empty_layer();
        assert_eq!(doc.layers.roots(), &[second, first]);
        assert_eq!(doc.active, vec![second]);
    }

    #[test]
    fn removing_a_layer_prunes_the_active_set() {
        let mut doc = Document::new(64, 64);
        let id = doc.active[0];
        doc.remove_layer(id).unwrap();
        assert!(doc.active.is_empty());
        assert!(doc.remove_layer(id).is_err());
    }

    #[test]
    fn locked_layers_refuse_removal() {
        let mut doc = Document::new(64, 64);
        let id = doc.active[0];
        doc.layers.get_mut(id).unwrap().locked = true;
        assert!(matches!(doc.remove_layer(id), Err(CoreError::LayerLocked)));
    }

    #[test]
    fn select_all_then_deselect() {
        let mut doc = Document::new(32, 32);
        assert!(!doc.has_selection());
        doc.select_all();
        // Selecting everything is equivalent to no selection for tool purposes.
        assert!(!doc.has_selection());
        assert_eq!(doc.selection_bounds(), doc.bounds());
        doc.deselect();
        assert!(doc.selection.is_none());
    }

    #[test]
    fn fill_covers_the_whole_layer_without_a_selection() {
        let mut doc = Document::new(4, 4);
        doc.fill_selection(Rgba::rgb(1.0, 0.0, 0.0)).unwrap();
        let LayerKind::Pixel { buffer } = &doc.active_layer().unwrap().kind else {
            panic!("expected a pixel layer");
        };
        assert_eq!(buffer.get(2, 2).unwrap().to_u8(), [255, 0, 0, 255]);
    }

    #[test]
    fn fill_respects_the_selection() {
        use crate::selection::{Selection, SelectionOptions};
        let mut doc = Document::new(8, 8);
        doc.selection = Some(Selection::from_mask(Selection::rectangle(
            8,
            8,
            Rect::new(0.0, 0.0, 4.0, 8.0),
            SelectionOptions { antialias: false, feather: 0.0 },
        )));
        doc.fill_selection(Rgba::rgb(0.0, 1.0, 0.0)).unwrap();

        let LayerKind::Pixel { buffer } = &doc.active_layer().unwrap().kind else {
            panic!("expected a pixel layer");
        };
        assert_eq!(buffer.get(1, 1).unwrap().to_u8(), [0, 255, 0, 255]);
        assert_eq!(buffer.get(6, 1).unwrap().a, 0.0, "outside the selection");
    }

    #[test]
    fn fill_refuses_non_pixel_layers() {
        let mut doc = Document::empty(8, 8);
        let id = doc.layers.insert("G", LayerKind::Group, None);
        doc.set_active(vec![id]);
        assert!(doc.fill_selection(Rgba::WHITE).is_err());
    }

    #[test]
    fn canvas_resize_anchors_content() {
        let mut doc = Document::new(100, 100);
        let id = doc.active[0];
        let offset = doc.resize_canvas(200, 100, Anchor::Center);
        assert_eq!(offset, glam::Vec2::new(50.0, 0.0));
        let t = doc.layers.get(id).unwrap().transform;
        assert_eq!(t.apply(glam::Vec2::ZERO), glam::Vec2::new(50.0, 0.0));

        let mut doc2 = Document::new(100, 100);
        assert_eq!(doc2.resize_canvas(200, 200, Anchor::TopLeft), glam::Vec2::ZERO);
    }

    #[test]
    fn unit_conversion_round_trips() {
        for unit in Unit::ALL {
            let px = unit.to_pixels(unit.from_pixels(300.0, 150.0), 150.0);
            assert!((px - 300.0).abs() < 1e-3, "{unit:?} failed to round-trip");
        }
    }

    #[test]
    fn anchor_factors_are_sane() {
        assert_eq!(Anchor::TopLeft.factors(), (0.0, 0.0));
        assert_eq!(Anchor::Center.factors(), (0.5, 0.5));
        assert_eq!(Anchor::BottomRight.factors(), (1.0, 1.0));
    }

    #[test]
    fn setting_active_filters_dead_ids() {
        let mut doc = Document::new(8, 8);
        let real = doc.active[0];
        doc.remove_layer(real).unwrap();
        doc.set_active(vec![real]);
        assert!(doc.active.is_empty());
    }
}
