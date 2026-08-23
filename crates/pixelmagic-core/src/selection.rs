//! Selections.
//!
//! A selection is a coverage mask over the canvas — the same representation as
//! a layer mask, which is what makes "convert a selection to a mask" a move
//! rather than a conversion. Anti-aliased edges are represented by intermediate
//! coverage values rather than by keeping the geometry around, so every tool
//! sees selections through one interface no matter how they were made.

use crate::buffer::{MaskBuffer, MaskOp};
use crate::geom::Rect;
use crate::vector::Path;
use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Options shared by the geometric and freehand selection tools (SPEC §5.14).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SelectionOptions {
    pub antialias: bool,
    /// Feather radius in pixels.
    pub feather: f32,
}

impl Default for SelectionOptions {
    fn default() -> Self {
        Self { antialias: true, feather: 0.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    mask: MaskBuffer,
    /// Cached tight bounds; `None` means "recompute on next query".
    bounds: Option<Rect>,
}

impl Selection {
    /// An empty selection over a canvas of the given size.
    pub fn none(width: u32, height: u32) -> Self {
        Self { mask: MaskBuffer::new(width, height), bounds: Some(Rect::ZERO) }
    }

    /// Select everything — `Command-A`.
    pub fn all(width: u32, height: u32) -> Self {
        Self {
            mask: MaskBuffer::revealed(width, height),
            bounds: Some(Rect::new(0.0, 0.0, width as f32, height as f32)),
        }
    }

    pub fn from_mask(mask: MaskBuffer) -> Self {
        Self { mask, bounds: None }
    }

    pub fn mask(&self) -> &MaskBuffer {
        &self.mask
    }

    pub fn width(&self) -> u32 {
        self.mask.width()
    }

    pub fn height(&self) -> u32 {
        self.mask.height()
    }

    pub fn is_empty(&self) -> bool {
        self.bounds().is_empty()
    }

    /// True when the whole canvas is selected, which is equivalent to having no
    /// selection at all and lets tools take their unclipped fast path.
    pub fn is_everything(&self) -> bool {
        self.mask.is_full()
    }

    /// Tight bounds of the selected area, cached between edits.
    pub fn bounds(&self) -> Rect {
        self.bounds.unwrap_or_else(|| self.mask.coverage_bounds())
    }

    /// Recompute the cached bounds. Call after mutating the mask directly.
    pub fn invalidate_bounds(&mut self) {
        self.bounds = None;
    }

    pub fn coverage_at(&self, x: u32, y: u32) -> f32 {
        self.mask.get(x, y) as f32 / 255.0
    }

    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= 0.0 && p.y >= 0.0 && self.mask.get(p.x as u32, p.y as u32) > 127
    }

    pub fn invert(&mut self) {
        self.mask.invert();
        self.bounds = None;
    }

    pub fn clear(&mut self) {
        self.mask.fill(0);
        self.bounds = Some(Rect::ZERO);
    }

    /// Combine another coverage mask into this selection.
    pub fn combine(&mut self, other: &MaskBuffer, op: MaskOp) {
        self.mask.combine(other, op);
        self.bounds = None;
    }

    // -- constructors for the geometric tools ------------------------------

    /// Rectangular selection, with anti-aliased edges when requested.
    pub fn rectangle(
        width: u32,
        height: u32,
        rect: Rect,
        options: SelectionOptions,
    ) -> MaskBuffer {
        let mut m = MaskBuffer::new(width, height);
        let r = rect.intersection(Rect::new(0.0, 0.0, width as f32, height as f32));
        if r.is_empty() {
            return m;
        }
        let (x0, y0) = (r.x, r.y);
        let (x1, y1) = (r.x + r.width, r.y + r.height);
        for y in r.y.floor() as u32..(r.y + r.height).ceil().min(height as f32) as u32 {
            for x in r.x.floor() as u32..(r.x + r.width).ceil().min(width as f32) as u32 {
                // Coverage of this pixel by the rectangle, computed exactly as
                // the overlap area — which is what makes a half-pixel-offset
                // marquee look right rather than jagged.
                let cov = if options.antialias {
                    let ox = (x1.min(x as f32 + 1.0) - x0.max(x as f32)).clamp(0.0, 1.0);
                    let oy = (y1.min(y as f32 + 1.0) - y0.max(y as f32)).clamp(0.0, 1.0);
                    ox * oy
                } else {
                    let cx = x as f32 + 0.5;
                    let cy = y as f32 + 0.5;
                    if cx >= x0 && cx < x1 && cy >= y0 && cy < y1 {
                        1.0
                    } else {
                        0.0
                    }
                };
                m.set(x, y, (cov * 255.0).round() as u8);
            }
        }
        if options.feather > 0.0 {
            feather(&mut m, options.feather);
        }
        m
    }

    /// Elliptical selection inscribed in `rect`.
    pub fn ellipse(
        width: u32,
        height: u32,
        rect: Rect,
        options: SelectionOptions,
    ) -> MaskBuffer {
        let mut m = MaskBuffer::new(width, height);
        if rect.is_empty() {
            return m;
        }
        let c = rect.center();
        let (rx, ry) = (rect.width * 0.5, rect.height * 0.5);
        if rx <= 0.0 || ry <= 0.0 {
            return m;
        }
        let clip =
            rect.inset(1.0).intersection(Rect::new(0.0, 0.0, width as f32, height as f32));
        if clip.is_empty() {
            return m;
        }
        for y in clip.y as u32..(clip.y + clip.height).ceil().min(height as f32) as u32 {
            for x in clip.x as u32..(clip.x + clip.width).ceil().min(width as f32) as u32 {
                let dx = (x as f32 + 0.5 - c.x) / rx;
                let dy = (y as f32 + 0.5 - c.y) / ry;
                let d = (dx * dx + dy * dy).sqrt();
                let cov = if options.antialias {
                    // Width of the transition band in normalised units, so the
                    // edge stays about one pixel wide whatever the radius.
                    let band = 1.0 / rx.min(ry).max(1.0);
                    ((1.0 - d) / band + 0.5).clamp(0.0, 1.0)
                } else if d <= 1.0 {
                    1.0
                } else {
                    0.0
                };
                m.set(x, y, (cov * 255.0).round() as u8);
            }
        }
        if options.feather > 0.0 {
            feather(&mut m, options.feather);
        }
        m
    }

    /// Selection from a vector path — used by the Polygonal, Free and Magnetic
    /// tools once their outline is closed, and by "load layer outline".
    pub fn from_path(
        width: u32,
        height: u32,
        path: &Path,
        options: SelectionOptions,
    ) -> MaskBuffer {
        let mut m = MaskBuffer::new(width, height);
        let b = path.bounds().round_out().intersection(Rect::new(
            0.0,
            0.0,
            width as f32,
            height as f32,
        ));
        if b.is_empty() {
            return m;
        }
        // 2×2 supersampling when anti-aliasing: cheap, and enough to hide
        // stair-stepping on the near-horizontal edges that give it away.
        let samples: &[(f32, f32)] = if options.antialias {
            &[(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
        } else {
            &[(0.5, 0.5)]
        };
        for y in b.y as u32..(b.y + b.height) as u32 {
            for x in b.x as u32..(b.x + b.width) as u32 {
                let hits = samples
                    .iter()
                    .filter(|(sx, sy)| path.contains(Vec2::new(x as f32 + sx, y as f32 + sy)))
                    .count();
                m.set(x, y, (hits * 255 / samples.len()) as u8);
            }
        }
        if options.feather > 0.0 {
            feather(&mut m, options.feather);
        }
        m
    }
}

/// Feather a coverage mask with a separable box blur, iterated three times.
///
/// Three box passes approximate a Gaussian closely enough that the difference
/// is invisible at selection-edge scales, and cost O(n) per pass instead of
/// O(n·r).
pub fn feather(mask: &mut MaskBuffer, radius: f32) {
    let r = radius.round() as i32;
    if r < 1 {
        return;
    }
    for _ in 0..3 {
        box_blur_axis(mask, r, true);
        box_blur_axis(mask, r, false);
    }
}

fn box_blur_axis(mask: &mut MaskBuffer, radius: i32, horizontal: bool) {
    let (w, h) = (mask.width() as i32, mask.height() as i32);
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    let src = mask.data().to_vec();
    let data = mask.data_mut();
    let window = (radius * 2 + 1) as u32;

    for o in 0..outer {
        let at = |i: i32| -> usize {
            let (x, y) = if horizontal { (i, o) } else { (o, i) };
            (y * w + x) as usize
        };
        // Running sum, seeded with the clamped left edge.
        let mut sum: u32 = 0;
        for k in -radius..=radius {
            sum += src[at(k.clamp(0, inner - 1))] as u32;
        }
        for i in 0..inner {
            data[at(i)] = (sum / window) as u8;
            let out = src[at((i - radius).clamp(0, inner - 1))] as u32;
            let inn = src[at((i + radius + 1).clamp(0, inner - 1))] as u32;
            sum = sum + inn - out;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_and_none() {
        let a = Selection::all(8, 8);
        assert!(a.is_everything());
        assert!(!a.is_empty());
        assert_eq!(a.bounds(), Rect::new(0.0, 0.0, 8.0, 8.0));

        let n = Selection::none(8, 8);
        assert!(n.is_empty());
        assert!(!n.is_everything());
    }

    #[test]
    fn inverting_all_gives_nothing() {
        let mut s = Selection::all(8, 8);
        s.invert();
        assert!(s.is_empty());
        s.invert();
        assert!(s.is_everything());
    }

    #[test]
    fn rectangle_selection_covers_the_right_pixels() {
        let m = Selection::rectangle(
            16,
            16,
            Rect::new(4.0, 4.0, 8.0, 8.0),
            SelectionOptions { antialias: false, feather: 0.0 },
        );
        assert_eq!(m.get(4, 4), 255);
        assert_eq!(m.get(11, 11), 255);
        assert_eq!(m.get(12, 12), 0);
        assert_eq!(m.get(3, 3), 0);
        assert_eq!(m.coverage_bounds(), Rect::new(4.0, 4.0, 8.0, 8.0));
    }

    #[test]
    fn antialiased_rectangle_has_partial_edges() {
        let m = Selection::rectangle(
            16,
            16,
            Rect::new(4.5, 4.0, 8.0, 8.0),
            SelectionOptions { antialias: true, feather: 0.0 },
        );
        let edge = m.get(4, 5);
        assert!(edge > 0 && edge < 255, "expected a partial edge pixel, got {edge}");
    }

    #[test]
    fn rectangle_outside_the_canvas_selects_nothing() {
        let m = Selection::rectangle(
            8,
            8,
            Rect::new(50.0, 50.0, 4.0, 4.0),
            SelectionOptions::default(),
        );
        assert!(m.is_empty());
    }

    #[test]
    fn ellipse_selects_its_centre_but_not_its_corners() {
        let m = Selection::ellipse(
            32,
            32,
            Rect::new(0.0, 0.0, 32.0, 32.0),
            SelectionOptions::default(),
        );
        assert_eq!(m.get(16, 16), 255);
        assert_eq!(m.get(0, 0), 0, "corner is outside the inscribed ellipse");
        assert_eq!(m.get(31, 31), 0);
    }

    #[test]
    fn degenerate_ellipse_is_empty() {
        let m = Selection::ellipse(
            8,
            8,
            Rect::new(1.0, 1.0, 0.0, 5.0),
            SelectionOptions::default(),
        );
        assert!(m.is_empty());
    }

    #[test]
    fn path_selection_matches_its_shape() {
        let path = Path::rect(Rect::new(2.0, 2.0, 6.0, 6.0));
        let m = Selection::from_path(
            16,
            16,
            &path,
            SelectionOptions { antialias: false, feather: 0.0 },
        );
        assert_eq!(m.get(4, 4), 255);
        assert_eq!(m.get(10, 10), 0);
    }

    #[test]
    fn combining_respects_the_mask_op() {
        let mut s = Selection::from_mask(Selection::rectangle(
            16,
            16,
            Rect::new(0.0, 0.0, 8.0, 16.0),
            SelectionOptions { antialias: false, feather: 0.0 },
        ));
        let right = Selection::rectangle(
            16,
            16,
            Rect::new(8.0, 0.0, 8.0, 16.0),
            SelectionOptions { antialias: false, feather: 0.0 },
        );
        s.combine(&right, MaskOp::Add);
        assert!(s.is_everything());

        s.combine(&right, MaskOp::Subtract);
        assert_eq!(s.bounds(), Rect::new(0.0, 0.0, 8.0, 16.0));
    }

    #[test]
    fn feathering_softens_the_edge_without_moving_the_centre() {
        let mut m = Selection::rectangle(
            64,
            64,
            Rect::new(16.0, 16.0, 32.0, 32.0),
            SelectionOptions { antialias: false, feather: 0.0 },
        );
        assert_eq!(m.get(16, 32), 255);
        feather(&mut m, 4.0);
        assert!(m.get(32, 32) > 240, "centre should stay selected");
        let edge = m.get(16, 32);
        assert!(edge > 0 && edge < 250, "edge should be partial, got {edge}");
        // Coverage bleeds outside the original rectangle.
        assert!(m.get(13, 32) > 0);
    }

    #[test]
    fn zero_radius_feather_is_a_no_op() {
        let mut m = Selection::rectangle(
            16,
            16,
            Rect::new(4.0, 4.0, 8.0, 8.0),
            SelectionOptions { antialias: false, feather: 0.0 },
        );
        let before = m.data().to_vec();
        feather(&mut m, 0.4);
        assert_eq!(m.data(), &before[..]);
    }

    #[test]
    fn bounds_cache_invalidates_on_edit() {
        let mut s = Selection::all(8, 8);
        s.clear();
        assert!(s.bounds().is_empty());
    }
}
