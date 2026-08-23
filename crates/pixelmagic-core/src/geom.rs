//! Geometry primitives shared by the document model, the renderer and the UI.
//!
//! Everything is in *document space*: the origin is the top-left of the canvas,
//! +x runs right and +y runs down, and one unit is one canvas pixel. The view
//! transform that maps document space to widget space lives in the UI layer.

use glam::{Mat3, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size { width: 0.0, height: 0.0 };

    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    pub fn area(self) -> f32 {
        self.width * self.height
    }

    pub fn aspect(self) -> f32 {
        if self.height == 0.0 {
            1.0
        } else {
            self.width / self.height
        }
    }

    /// Pixel dimensions, rounded up, clamped to at least 1×1. Used when
    /// allocating textures for a region.
    pub fn to_pixels(self) -> (u32, u32) {
        (
            (self.width.ceil() as i64).clamp(1, u32::MAX as i64) as u32,
            (self.height.ceil() as i64).clamp(1, u32::MAX as i64) as u32,
        )
    }

    /// Scale to fit inside `bounds` without cropping, preserving aspect ratio.
    pub fn fit_within(self, bounds: Size) -> f32 {
        if self.is_empty() || bounds.is_empty() {
            return 1.0;
        }
        (bounds.width / self.width).min(bounds.height / self.height)
    }
}

/// An axis-aligned rectangle, stored as origin + size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };

    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn from_size(size: Size) -> Self {
        Self::new(0.0, 0.0, size.width, size.height)
    }

    pub fn from_corners(a: Vec2, b: Vec2) -> Self {
        let min = a.min(b);
        let max = a.max(b);
        Self::new(min.x, min.y, max.x - min.x, max.y - min.y)
    }

    pub fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn min(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    pub fn max(self) -> Vec2 {
        Vec2::new(self.x + self.width, self.y + self.height)
    }

    pub fn center(self) -> Vec2 {
        Vec2::new(self.x + self.width * 0.5, self.y + self.height * 0.5)
    }

    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    pub fn contains(self, p: Vec2) -> bool {
        p.x >= self.x
            && p.y >= self.y
            && p.x < self.x + self.width
            && p.y < self.y + self.height
    }

    pub fn intersects(self, other: Rect) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }

    pub fn intersection(self, other: Rect) -> Rect {
        let min = self.min().max(other.min());
        let max = self.max().min(other.max());
        if max.x <= min.x || max.y <= min.y {
            Rect::ZERO
        } else {
            Rect::new(min.x, min.y, max.x - min.x, max.y - min.y)
        }
    }

    pub fn union(self, other: Rect) -> Rect {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let min = self.min().min(other.min());
        let max = self.max().max(other.max());
        Rect::new(min.x, min.y, max.x - min.x, max.y - min.y)
    }

    /// Grow (or, with a negative amount, shrink) on every side.
    pub fn inset(self, amount: f32) -> Rect {
        Rect::new(
            self.x - amount,
            self.y - amount,
            self.width + amount * 2.0,
            self.height + amount * 2.0,
        )
    }

    /// Smallest pixel-aligned rectangle containing this one. Effects that read
    /// neighbouring pixels (blurs, and anything with a radius) need integral
    /// bounds before they can allocate.
    pub fn round_out(self) -> Rect {
        let min = self.min().floor();
        let max = self.max().ceil();
        Rect::new(min.x, min.y, max.x - min.x, max.y - min.y)
    }

    pub fn corners(self) -> [Vec2; 4] {
        [
            Vec2::new(self.x, self.y),
            Vec2::new(self.x + self.width, self.y),
            Vec2::new(self.x + self.width, self.y + self.height),
            Vec2::new(self.x, self.y + self.height),
        ]
    }

    /// Axis-aligned bounds of this rectangle after `t` is applied. Because the
    /// transform may rotate or skew, the result is generally larger than the
    /// transformed rectangle itself.
    pub fn transformed_bounds(self, t: &Transform) -> Rect {
        let pts = self.corners().map(|p| t.apply(p));
        let mut min = pts[0];
        let mut max = pts[0];
        for p in &pts[1..] {
            min = min.min(*p);
            max = max.max(*p);
        }
        Rect::new(min.x, min.y, max.x - min.x, max.y - min.y)
    }
}

/// A 2-D affine transform, stored as a 3×3 matrix with an implicit
/// `[0, 0, 1]` bottom row.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform(pub Mat3);

impl Default for Transform {
    fn default() -> Self {
        Transform(Mat3::IDENTITY)
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform(Mat3::IDENTITY);

    pub fn translate(offset: Vec2) -> Self {
        Transform(Mat3::from_translation(offset))
    }

    pub fn scale(factor: Vec2) -> Self {
        Transform(Mat3::from_scale(factor))
    }

    pub fn rotate(radians: f32) -> Self {
        Transform(Mat3::from_angle(radians))
    }

    /// Rotation about an arbitrary pivot rather than the origin — what every
    /// on-canvas rotation handle actually needs.
    pub fn rotate_about(radians: f32, pivot: Vec2) -> Self {
        // Move the pivot to the origin, rotate, move it back. `then` applies
        // `self` first, so the un-translate has to come first in the chain.
        Transform::translate(-pivot)
            .then(&Transform::rotate(radians))
            .then(&Transform::translate(pivot))
    }

    pub fn scale_about(factor: Vec2, pivot: Vec2) -> Self {
        Transform::translate(-pivot)
            .then(&Transform::scale(factor))
            .then(&Transform::translate(pivot))
    }

    /// Shear, as used by the Arrange tool's skew mode.
    pub fn skew(sx: f32, sy: f32) -> Self {
        Transform(Mat3::from_cols_array(&[1.0, sy, 0.0, sx, 1.0, 0.0, 0.0, 0.0, 1.0]))
    }

    pub fn apply(&self, p: Vec2) -> Vec2 {
        self.0.transform_point2(p)
    }

    /// Apply to a direction, ignoring translation.
    pub fn apply_vector(&self, v: Vec2) -> Vec2 {
        self.0.transform_vector2(v)
    }

    /// `self` first, then `other`.
    pub fn then(&self, other: &Transform) -> Transform {
        Transform(other.0 * self.0)
    }

    pub fn inverse(&self) -> Transform {
        Transform(self.0.inverse())
    }

    pub fn is_identity(&self) -> bool {
        self.0.abs_diff_eq(Mat3::IDENTITY, 1e-6)
    }

    /// True when the transform is a pure translation by whole pixels — the case
    /// where sampling can skip interpolation entirely.
    pub fn is_integer_translation(&self) -> bool {
        let m = self.0;
        let linear_is_identity = (m.x_axis.x - 1.0).abs() < 1e-6
            && m.x_axis.y.abs() < 1e-6
            && m.y_axis.x.abs() < 1e-6
            && (m.y_axis.y - 1.0).abs() < 1e-6;
        linear_is_identity
            && (m.z_axis.x - m.z_axis.x.round()).abs() < 1e-6
            && (m.z_axis.y - m.z_axis.y.round()).abs() < 1e-6
    }

    /// Column-major 3×3, ready for `glUniformMatrix3fv`.
    pub fn to_cols_array(&self) -> [f32; 9] {
        self.0.to_cols_array()
    }
}

/// How a resize should treat the source aspect ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ResizeMode {
    #[default]
    Stretch,
    Fit,
    Fill,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_intersection_and_union() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.intersection(b), Rect::new(5.0, 5.0, 5.0, 5.0));
        assert_eq!(a.union(b), Rect::new(0.0, 0.0, 15.0, 15.0));

        let disjoint = Rect::new(100.0, 100.0, 1.0, 1.0);
        assert!(a.intersection(disjoint).is_empty());
        assert!(!a.intersects(disjoint));
    }

    #[test]
    fn union_ignores_empty_rects() {
        let a = Rect::new(2.0, 3.0, 4.0, 5.0);
        assert_eq!(a.union(Rect::ZERO), a);
        assert_eq!(Rect::ZERO.union(a), a);
    }

    #[test]
    fn round_out_expands_to_pixel_grid() {
        let r = Rect::new(0.4, 0.6, 1.2, 1.1).round_out();
        assert_eq!(r, Rect::new(0.0, 0.0, 2.0, 2.0));
    }

    #[test]
    fn rotate_about_keeps_pivot_fixed() {
        let pivot = Vec2::new(50.0, 25.0);
        let t = Transform::rotate_about(std::f32::consts::FRAC_PI_3, pivot);
        assert!((t.apply(pivot) - pivot).length() < 1e-3);
    }

    #[test]
    fn scale_about_keeps_pivot_fixed() {
        let pivot = Vec2::new(12.0, -30.0);
        let t = Transform::scale_about(Vec2::new(3.0, 0.5), pivot);
        assert!((t.apply(pivot) - pivot).length() < 1e-3);
        // A point one unit right of the pivot ends up three units right.
        let p = pivot + Vec2::new(1.0, 0.0);
        assert!((t.apply(p) - (pivot + Vec2::new(3.0, 0.0))).length() < 1e-3);
    }

    #[test]
    fn then_applies_self_before_other() {
        // Scale-then-translate must not scale the translation.
        let t = Transform::scale(Vec2::splat(2.0))
            .then(&Transform::translate(Vec2::new(10.0, 0.0)));
        assert!((t.apply(Vec2::new(1.0, 0.0)) - Vec2::new(12.0, 0.0)).length() < 1e-4);
    }

    #[test]
    fn transform_inverse_round_trips() {
        let t = Transform::translate(Vec2::new(10.0, -4.0))
            .then(&Transform::rotate(0.7))
            .then(&Transform::scale(Vec2::new(2.0, 3.0)));
        let p = Vec2::new(3.0, 9.0);
        assert!((t.inverse().apply(t.apply(p)) - p).length() < 1e-3);
    }

    #[test]
    fn integer_translation_detection() {
        assert!(Transform::translate(Vec2::new(3.0, -7.0)).is_integer_translation());
        assert!(!Transform::translate(Vec2::new(3.5, 0.0)).is_integer_translation());
        assert!(!Transform::rotate(0.1).is_integer_translation());
    }

    #[test]
    fn rotated_bounds_grow() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = r.transformed_bounds(&Transform::rotate(std::f32::consts::FRAC_PI_4));
        assert!(b.width > 14.0 && b.width < 14.3);
    }
}
