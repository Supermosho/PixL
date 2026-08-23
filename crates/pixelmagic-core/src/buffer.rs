//! CPU-side pixel storage.
//!
//! The GPU holds the working copy of every layer while a document is open;
//! these buffers are the authoritative CPU-side copy that gets uploaded, read
//! back for tools that need random access, and serialised on save.
//!
//! Storage is **8 bits per channel, straight (un-premultiplied) alpha**.
//! Straight alpha is the right choice here even though the renderer composites
//! premultiplied: it round-trips through PNG without loss, it keeps colour
//! information in fully transparent pixels (which matters when the user erases
//! and then reduces the eraser's effect), and premultiplying is a single cheap
//! step at upload time.
//!
//! A 16-bit variant belongs here eventually — Pixelmator Pro documents can be
//! 16-bit — but every operation would need a parallel implementation, so it is
//! deliberately deferred rather than half-built.

use crate::color::Rgba;
use crate::geom::Rect;
use serde::{Deserialize, Serialize};

/// An RGBA8 image with straight alpha.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelBuffer {
    width: u32,
    height: u32,
    /// `width * height * 4` bytes, row-major, top row first.
    data: Vec<u8>,
}

impl std::fmt::Debug for PixelBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PixelBuffer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.data.len())
            .finish()
    }
}

impl PixelBuffer {
    pub const CHANNELS: usize = 4;

    /// A fully transparent buffer.
    pub fn new(width: u32, height: u32) -> Self {
        let len = width as usize * height as usize * Self::CHANNELS;
        Self { width, height, data: vec![0; len] }
    }

    pub fn filled(width: u32, height: u32, color: Rgba) -> Self {
        let mut buf = Self::new(width, height);
        buf.fill(color);
        buf
    }

    /// Wrap existing RGBA8 bytes. Returns `None` if the length does not match
    /// the stated dimensions, rather than panicking on a malformed file.
    pub fn from_raw(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        if data.len() != width as usize * height as usize * Self::CHANNELS {
            return None;
        }
        Some(Self { width, height, data })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width as f32, self.height as f32)
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some((y as usize * self.width as usize + x as usize) * Self::CHANNELS)
    }

    pub fn get(&self, x: u32, y: u32) -> Option<Rgba> {
        let i = self.index(x, y)?;
        Some(Rgba::from_u8(self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3]))
    }

    pub fn set(&mut self, x: u32, y: u32, color: Rgba) -> bool {
        let Some(i) = self.index(x, y) else { return false };
        let px = color.to_u8();
        self.data[i..i + 4].copy_from_slice(&px);
        true
    }

    pub fn fill(&mut self, color: Rgba) {
        let px = color.to_u8();
        for chunk in self.data.chunks_exact_mut(Self::CHANNELS) {
            chunk.copy_from_slice(&px);
        }
    }

    pub fn clear(&mut self) {
        self.data.fill(0);
    }

    /// Fill only within `rect`, clipped to the buffer.
    pub fn fill_rect(&mut self, rect: Rect, color: Rgba) {
        let r = rect.round_out().intersection(self.bounds());
        if r.is_empty() {
            return;
        }
        let px = color.to_u8();
        let (x0, y0) = (r.x as u32, r.y as u32);
        let (x1, y1) = ((r.x + r.width) as u32, (r.y + r.height) as u32);
        for y in y0..y1 {
            let start = (y as usize * self.width as usize + x0 as usize) * Self::CHANNELS;
            let end = start + (x1 - x0) as usize * Self::CHANNELS;
            for chunk in self.data[start..end].chunks_exact_mut(Self::CHANNELS) {
                chunk.copy_from_slice(&px);
            }
        }
    }

    /// Copy `src` into this buffer at `(dx, dy)`, replacing destination pixels.
    /// Out-of-bounds regions are skipped.
    pub fn blit(&mut self, src: &PixelBuffer, dx: i32, dy: i32) {
        for sy in 0..src.height {
            let ty = sy as i64 + dy as i64;
            if ty < 0 || ty >= self.height as i64 {
                continue;
            }
            for sx in 0..src.width {
                let tx = sx as i64 + dx as i64;
                if tx < 0 || tx >= self.width as i64 {
                    continue;
                }
                let si = (sy as usize * src.width as usize + sx as usize) * Self::CHANNELS;
                let di = (ty as usize * self.width as usize + tx as usize) * Self::CHANNELS;
                self.data[di..di + 4].copy_from_slice(&src.data[si..si + 4]);
            }
        }
    }

    /// Extract a sub-rectangle. The result is clipped to the buffer, so it can
    /// be smaller than requested — and empty if the rectangle misses entirely.
    pub fn crop(&self, rect: Rect) -> PixelBuffer {
        let r = rect.round_out().intersection(self.bounds());
        if r.is_empty() {
            return PixelBuffer::new(0, 0);
        }
        let (w, h) = (r.width as u32, r.height as u32);
        let mut out = PixelBuffer::new(w, h);
        for y in 0..h {
            let si = ((r.y as u32 + y) as usize * self.width as usize + r.x as usize)
                * Self::CHANNELS;
            let di = (y as usize * w as usize) * Self::CHANNELS;
            let n = w as usize * Self::CHANNELS;
            out.data[di..di + n].copy_from_slice(&self.data[si..si + n]);
        }
        out
    }

    /// The tightest rectangle containing every non-transparent pixel.
    /// Empty when the buffer is fully transparent — this is what `Trim` and
    /// "crop to contents" are built on.
    pub fn opaque_bounds(&self) -> Rect {
        let (mut min_x, mut min_y) = (u32::MAX, u32::MAX);
        let (mut max_x, mut max_y) = (0u32, 0u32);
        let mut found = false;
        for y in 0..self.height {
            for x in 0..self.width {
                let i = (y as usize * self.width as usize + x as usize) * Self::CHANNELS;
                if self.data[i + 3] != 0 {
                    found = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        if !found {
            return Rect::ZERO;
        }
        Rect::new(
            min_x as f32,
            min_y as f32,
            (max_x - min_x + 1) as f32,
            (max_y - min_y + 1) as f32,
        )
    }
}

/// A single-channel coverage buffer: layer masks and selections.
///
/// 0 hides, 255 shows, and everything between is partial — which is exactly the
/// convention Pixelmator documents for mask thumbnails ("black hides, white
/// shows"), and it means a selection and a mask are the same kind of object
/// viewed through different UI.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskBuffer {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl std::fmt::Debug for MaskBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaskBuffer")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl MaskBuffer {
    /// A mask that hides everything.
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, data: vec![0; width as usize * height as usize] }
    }

    /// A mask that reveals everything — what `Add Mask` creates.
    pub fn revealed(width: u32, height: u32) -> Self {
        Self { width, height, data: vec![255; width as usize * height as usize] }
    }

    pub fn from_raw(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        if data.len() != width as usize * height as usize {
            return None;
        }
        Some(Self { width, height, data })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width as f32, self.height as f32)
    }

    pub fn get(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.data[y as usize * self.width as usize + x as usize]
    }

    pub fn set(&mut self, x: u32, y: u32, v: u8) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.data[y as usize * self.width as usize + x as usize] = v;
        true
    }

    pub fn fill(&mut self, v: u8) {
        self.data.fill(v);
    }

    pub fn invert(&mut self) {
        for v in &mut self.data {
            *v = 255 - *v;
        }
    }

    /// True when nothing is selected/revealed at all.
    pub fn is_empty(&self) -> bool {
        self.data.iter().all(|&v| v == 0)
    }

    /// True when everything is selected/revealed — the state where a selection
    /// can be dropped entirely rather than carried around.
    pub fn is_full(&self) -> bool {
        self.data.iter().all(|&v| v == 255)
    }

    /// Tightest rectangle containing every non-zero pixel.
    pub fn coverage_bounds(&self) -> Rect {
        let (mut min_x, mut min_y) = (u32::MAX, u32::MAX);
        let (mut max_x, mut max_y) = (0u32, 0u32);
        let mut found = false;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.data[y as usize * self.width as usize + x as usize] != 0 {
                    found = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        if !found {
            return Rect::ZERO;
        }
        Rect::new(
            min_x as f32,
            min_y as f32,
            (max_x - min_x + 1) as f32,
            (max_y - min_y + 1) as f32,
        )
    }

    /// Combine with another mask of the same size.
    pub fn combine(&mut self, other: &MaskBuffer, op: MaskOp) {
        if other.width != self.width || other.height != self.height {
            return;
        }
        for (a, &b) in self.data.iter_mut().zip(other.data.iter()) {
            *a = op.apply(*a, b);
        }
    }
}

/// How a new selection combines with the existing one (SPEC §5.14,
/// "Selection modes": new, add, subtract, intersect).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MaskOp {
    #[default]
    Replace,
    Add,
    Subtract,
    Intersect,
}

impl MaskOp {
    pub const ALL: [MaskOp; 4] =
        [MaskOp::Replace, MaskOp::Add, MaskOp::Subtract, MaskOp::Intersect];

    pub fn label(self) -> &'static str {
        match self {
            MaskOp::Replace => "New Selection",
            MaskOp::Add => "Add to Selection",
            MaskOp::Subtract => "Subtract from Selection",
            MaskOp::Intersect => "Intersect with Selection",
        }
    }

    /// Combine two coverage values. Add/subtract use max/complement rather than
    /// saturating arithmetic so that repeated strokes over the same area
    /// converge instead of creeping.
    pub fn apply(self, existing: u8, incoming: u8) -> u8 {
        match self {
            MaskOp::Replace => incoming,
            MaskOp::Add => existing.max(incoming),
            MaskOp::Subtract => existing.saturating_sub(incoming),
            MaskOp::Intersect => ((existing as u16 * incoming as u16) / 255) as u8,
        }
    }

    /// The mode implied by the modifier keys held during a drag: Shift adds,
    /// Option subtracts, Shift-Option intersects (SPEC §5.14).
    pub fn from_modifiers(shift: bool, alt: bool) -> Self {
        match (shift, alt) {
            (true, true) => MaskOp::Intersect,
            (true, false) => MaskOp::Add,
            (false, true) => MaskOp::Subtract,
            (false, false) => MaskOp::Replace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_transparent() {
        let b = PixelBuffer::new(4, 3);
        assert_eq!(b.width(), 4);
        assert_eq!(b.height(), 3);
        assert_eq!(b.get(0, 0), Some(Rgba::TRANSPARENT));
        assert_eq!(b.data().len(), 4 * 3 * 4);
    }

    #[test]
    fn get_set_round_trips_and_bounds_check() {
        let mut b = PixelBuffer::new(2, 2);
        assert!(b.set(1, 1, Rgba::rgb(1.0, 0.0, 0.0)));
        assert_eq!(b.get(1, 1).unwrap().to_u8(), [255, 0, 0, 255]);
        assert!(!b.set(2, 0, Rgba::WHITE));
        assert_eq!(b.get(9, 9), None);
    }

    #[test]
    fn from_raw_rejects_wrong_length() {
        assert!(PixelBuffer::from_raw(2, 2, vec![0; 15]).is_none());
        assert!(PixelBuffer::from_raw(2, 2, vec![0; 16]).is_some());
        assert!(MaskBuffer::from_raw(3, 3, vec![0; 8]).is_none());
    }

    #[test]
    fn fill_rect_clips_to_bounds() {
        let mut b = PixelBuffer::new(4, 4);
        b.fill_rect(Rect::new(2.0, 2.0, 100.0, 100.0), Rgba::WHITE);
        assert_eq!(b.get(3, 3).unwrap().to_u8(), [255, 255, 255, 255]);
        assert_eq!(b.get(1, 1).unwrap().a, 0.0);
    }

    #[test]
    fn fill_rect_entirely_outside_is_a_no_op() {
        let mut b = PixelBuffer::new(4, 4);
        b.fill_rect(Rect::new(50.0, 50.0, 10.0, 10.0), Rgba::WHITE);
        assert!(b.data().iter().all(|&v| v == 0));
    }

    #[test]
    fn opaque_bounds_finds_the_content() {
        let mut b = PixelBuffer::new(10, 10);
        assert!(b.opaque_bounds().is_empty());
        b.set(3, 4, Rgba::WHITE);
        b.set(6, 8, Rgba::WHITE);
        assert_eq!(b.opaque_bounds(), Rect::new(3.0, 4.0, 4.0, 5.0));
    }

    #[test]
    fn crop_extracts_the_right_pixels() {
        let mut b = PixelBuffer::new(4, 4);
        b.set(2, 2, Rgba::rgb(0.0, 1.0, 0.0));
        let c = b.crop(Rect::new(2.0, 2.0, 2.0, 2.0));
        assert_eq!(c.width(), 2);
        assert_eq!(c.get(0, 0).unwrap().to_u8(), [0, 255, 0, 255]);
    }

    #[test]
    fn crop_outside_yields_empty() {
        let b = PixelBuffer::new(4, 4);
        assert!(b.crop(Rect::new(90.0, 90.0, 4.0, 4.0)).is_empty());
    }

    #[test]
    fn blit_clips_at_the_edges() {
        let mut dst = PixelBuffer::new(4, 4);
        let src = PixelBuffer::filled(2, 2, Rgba::WHITE);
        dst.blit(&src, 3, 3);
        assert_eq!(dst.get(3, 3).unwrap().a, 1.0);
        // The other three source pixels fell off the edge; nothing panicked.
        assert_eq!(dst.get(0, 0).unwrap().a, 0.0);

        dst.blit(&src, -1, -1);
        assert_eq!(dst.get(0, 0).unwrap().a, 1.0);
    }

    #[test]
    fn mask_defaults_and_inversion() {
        let mut m = MaskBuffer::new(3, 3);
        assert!(m.is_empty());
        m.invert();
        assert!(m.is_full());

        let r = MaskBuffer::revealed(3, 3);
        assert!(r.is_full());
    }

    #[test]
    fn mask_ops_behave() {
        assert_eq!(MaskOp::Replace.apply(255, 10), 10);
        assert_eq!(MaskOp::Add.apply(100, 200), 200);
        assert_eq!(MaskOp::Add.apply(200, 100), 200);
        assert_eq!(MaskOp::Subtract.apply(100, 200), 0);
        assert_eq!(MaskOp::Subtract.apply(200, 100), 100);
        assert_eq!(MaskOp::Intersect.apply(255, 128), 128);
        assert_eq!(MaskOp::Intersect.apply(0, 255), 0);
    }

    #[test]
    fn repeated_adds_converge() {
        // Painting the same selection twice must not intensify it.
        let mut v = 0u8;
        for _ in 0..5 {
            v = MaskOp::Add.apply(v, 128);
        }
        assert_eq!(v, 128);
    }

    #[test]
    fn modifier_mapping() {
        assert_eq!(MaskOp::from_modifiers(false, false), MaskOp::Replace);
        assert_eq!(MaskOp::from_modifiers(true, false), MaskOp::Add);
        assert_eq!(MaskOp::from_modifiers(false, true), MaskOp::Subtract);
        assert_eq!(MaskOp::from_modifiers(true, true), MaskOp::Intersect);
    }

    #[test]
    fn combine_ignores_size_mismatch() {
        let mut a = MaskBuffer::revealed(4, 4);
        let b = MaskBuffer::new(2, 2);
        a.combine(&b, MaskOp::Replace);
        assert!(a.is_full(), "mismatched combine should be a no-op");
    }

    #[test]
    fn coverage_bounds_tracks_painted_area() {
        let mut m = MaskBuffer::new(8, 8);
        assert!(m.coverage_bounds().is_empty());
        m.set(2, 3, 255);
        m.set(5, 6, 1);
        assert_eq!(m.coverage_bounds(), Rect::new(2.0, 3.0, 4.0, 4.0));
    }
}
