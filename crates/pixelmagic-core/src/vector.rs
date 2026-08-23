//! Vector paths and shape primitives.
//!
//! Backs shape layers, vector masks, the Pen and Freeform Pen tools, and text
//! converted to outlines. Paths are cubic Bézier, which is what SVG and PDF use
//! and what every rasteriser we might target speaks natively.

use crate::geom::{Rect, Transform};
use glam::Vec2;
use serde::{Deserialize, Serialize};

/// One segment's worth of path data. The start point is the previous
/// segment's end (or the subpath's `start`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Segment {
    Line {
        to: Vec2,
    },
    /// Cubic Bézier with two control points.
    Cubic {
        c1: Vec2,
        c2: Vec2,
        to: Vec2,
    },
}

impl Segment {
    pub fn end(&self) -> Vec2 {
        match self {
            Segment::Line { to } | Segment::Cubic { to, .. } => *to,
        }
    }

    /// Evaluate at parameter `t` in 0..=1, given the segment's start point.
    pub fn eval(&self, from: Vec2, t: f32) -> Vec2 {
        match *self {
            Segment::Line { to } => from.lerp(to, t),
            Segment::Cubic { c1, c2, to } => {
                let u = 1.0 - t;
                from * (u * u * u)
                    + c1 * (3.0 * u * u * t)
                    + c2 * (3.0 * u * t * t)
                    + to * (t * t * t)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubPath {
    pub start: Vec2,
    pub segments: Vec<Segment>,
    pub closed: bool,
}

impl SubPath {
    pub fn new(start: Vec2) -> Self {
        Self { start, segments: Vec::new(), closed: false }
    }

    pub fn line_to(&mut self, to: Vec2) -> &mut Self {
        self.segments.push(Segment::Line { to });
        self
    }

    pub fn cubic_to(&mut self, c1: Vec2, c2: Vec2, to: Vec2) -> &mut Self {
        self.segments.push(Segment::Cubic { c1, c2, to });
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.closed = true;
        self
    }

    /// Every on-curve point, in order.
    pub fn anchors(&self) -> Vec<Vec2> {
        let mut v = vec![self.start];
        v.extend(self.segments.iter().map(|s| s.end()));
        v
    }

    /// Flatten to a polyline. `tolerance` is the maximum deviation in the same
    /// units as the points; smaller values subdivide more.
    pub fn flatten(&self, tolerance: f32) -> Vec<Vec2> {
        let mut out = vec![self.start];
        let mut cursor = self.start;
        for seg in &self.segments {
            match *seg {
                Segment::Line { to } => out.push(to),
                Segment::Cubic { c1, c2, to } => {
                    // Steps from the control polygon's length: cheap, and it
                    // over-subdivides rather than under-subdividing.
                    let poly = cursor.distance(c1) + c1.distance(c2) + c2.distance(to);
                    let steps =
                        ((poly / tolerance.max(1e-3)).sqrt().ceil() as usize).clamp(2, 256);
                    for i in 1..=steps {
                        out.push(seg.eval(cursor, i as f32 / steps as f32));
                    }
                }
            }
            cursor = seg.end();
        }
        if self.closed && out.last() != Some(&self.start) {
            out.push(self.start);
        }
        out
    }
}

/// Fill rule for self-intersecting and multi-subpath shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Path {
    pub subpaths: Vec<SubPath>,
    pub fill_rule: FillRule,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, sub: SubPath) -> &mut Self {
        self.subpaths.push(sub);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.subpaths.iter().all(|s| s.segments.is_empty())
    }

    /// Tight-ish bounds. Computed from the flattened polyline, so it is exact
    /// for lines and slightly generous for curves — which is the safe
    /// direction for allocating a render target.
    pub fn bounds(&self) -> Rect {
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for sub in &self.subpaths {
            for p in sub.flatten(0.25) {
                min = min.min(p);
                max = max.max(p);
            }
        }
        if !min.x.is_finite() {
            return Rect::ZERO;
        }
        Rect::new(min.x, min.y, max.x - min.x, max.y - min.y)
    }

    pub fn transform(&mut self, t: &Transform) {
        for sub in &mut self.subpaths {
            sub.start = t.apply(sub.start);
            for seg in &mut sub.segments {
                *seg = match *seg {
                    Segment::Line { to } => Segment::Line { to: t.apply(to) },
                    Segment::Cubic { c1, c2, to } => {
                        Segment::Cubic { c1: t.apply(c1), c2: t.apply(c2), to: t.apply(to) }
                    }
                };
            }
        }
    }

    /// Winding-rule hit test against the flattened outline.
    pub fn contains(&self, p: Vec2) -> bool {
        let mut winding = 0i32;
        let mut crossings = 0u32;
        for sub in &self.subpaths {
            let pts = sub.flatten(0.25);
            for w in pts.windows(2) {
                let (a, b) = (w[0], w[1]);
                if (a.y <= p.y) != (b.y <= p.y) {
                    let dy = b.y - a.y;
                    if dy.abs() < 1e-9 {
                        continue;
                    }
                    let x = a.x + (p.y - a.y) / dy * (b.x - a.x);
                    if x > p.x {
                        crossings += 1;
                        winding += if b.y > a.y { 1 } else { -1 };
                    }
                }
            }
        }
        match self.fill_rule {
            FillRule::NonZero => winding != 0,
            FillRule::EvenOdd => crossings % 2 == 1,
        }
    }

    // -- primitive constructors -------------------------------------------

    pub fn rect(r: Rect) -> Self {
        let mut sub = SubPath::new(Vec2::new(r.x, r.y));
        sub.line_to(Vec2::new(r.x + r.width, r.y))
            .line_to(Vec2::new(r.x + r.width, r.y + r.height))
            .line_to(Vec2::new(r.x, r.y + r.height))
            .close();
        let mut p = Path::new();
        p.push(sub);
        p
    }

    /// Circular-arc corners approximated with the standard Bézier constant.
    pub fn rounded_rect(r: Rect, radius: f32) -> Self {
        const K: f32 = 0.552_284_8;
        let rad = radius.min(r.width * 0.5).min(r.height * 0.5).max(0.0);
        if rad <= 0.0 {
            return Path::rect(r);
        }
        let (x0, y0) = (r.x, r.y);
        let (x1, y1) = (r.x + r.width, r.y + r.height);
        let o = rad * K;

        let mut sub = SubPath::new(Vec2::new(x0 + rad, y0));
        sub.line_to(Vec2::new(x1 - rad, y0))
            .cubic_to(
                Vec2::new(x1 - rad + o, y0),
                Vec2::new(x1, y0 + rad - o),
                Vec2::new(x1, y0 + rad),
            )
            .line_to(Vec2::new(x1, y1 - rad))
            .cubic_to(
                Vec2::new(x1, y1 - rad + o),
                Vec2::new(x1 - rad + o, y1),
                Vec2::new(x1 - rad, y1),
            )
            .line_to(Vec2::new(x0 + rad, y1))
            .cubic_to(
                Vec2::new(x0 + rad - o, y1),
                Vec2::new(x0, y1 - rad + o),
                Vec2::new(x0, y1 - rad),
            )
            .line_to(Vec2::new(x0, y0 + rad))
            .cubic_to(
                Vec2::new(x0, y0 + rad - o),
                Vec2::new(x0 + rad - o, y0),
                Vec2::new(x0 + rad, y0),
            )
            .close();
        let mut p = Path::new();
        p.push(sub);
        p
    }

    pub fn ellipse(r: Rect) -> Self {
        const K: f32 = 0.552_284_8;
        let c = r.center();
        let (rx, ry) = (r.width * 0.5, r.height * 0.5);
        let (ox, oy) = (rx * K, ry * K);

        let mut sub = SubPath::new(Vec2::new(c.x, c.y - ry));
        sub.cubic_to(
            Vec2::new(c.x + ox, c.y - ry),
            Vec2::new(c.x + rx, c.y - oy),
            Vec2::new(c.x + rx, c.y),
        )
        .cubic_to(
            Vec2::new(c.x + rx, c.y + oy),
            Vec2::new(c.x + ox, c.y + ry),
            Vec2::new(c.x, c.y + ry),
        )
        .cubic_to(
            Vec2::new(c.x - ox, c.y + ry),
            Vec2::new(c.x - rx, c.y + oy),
            Vec2::new(c.x - rx, c.y),
        )
        .cubic_to(
            Vec2::new(c.x - rx, c.y - oy),
            Vec2::new(c.x - ox, c.y - ry),
            Vec2::new(c.x, c.y - ry),
        )
        .close();
        let mut p = Path::new();
        p.push(sub);
        p
    }

    pub fn polygon(r: Rect, sides: u32) -> Self {
        let sides = sides.max(3);
        let c = r.center();
        let (rx, ry) = (r.width * 0.5, r.height * 0.5);
        let angle = |i: u32| {
            -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / sides as f32
        };
        let pt = |i: u32| {
            let a = angle(i);
            Vec2::new(c.x + rx * a.cos(), c.y + ry * a.sin())
        };
        let mut sub = SubPath::new(pt(0));
        for i in 1..sides {
            sub.line_to(pt(i));
        }
        sub.close();
        let mut p = Path::new();
        p.push(sub);
        p
    }

    pub fn star(r: Rect, points: u32, inner_ratio: f32) -> Self {
        let points = points.max(3);
        let ratio = inner_ratio.clamp(0.05, 1.0);
        let c = r.center();
        let (rx, ry) = (r.width * 0.5, r.height * 0.5);
        let n = points * 2;
        let pt = |i: u32| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / n as f32;
            let s = if i % 2 == 0 { 1.0 } else { ratio };
            Vec2::new(c.x + rx * s * a.cos(), c.y + ry * s * a.sin())
        };
        let mut sub = SubPath::new(pt(0));
        for i in 1..n {
            sub.line_to(pt(i));
        }
        sub.close();
        let mut p = Path::new();
        p.push(sub);
        p
    }

    pub fn line(from: Vec2, to: Vec2) -> Self {
        let mut sub = SubPath::new(from);
        sub.line_to(to);
        let mut p = Path::new();
        p.push(sub);
        p
    }
}

/// Boolean operations offered by the Shape tool (SPEC §5.19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BooleanOp {
    Unite,
    Subtract,
    Intersect,
    Exclude,
}

impl BooleanOp {
    pub const ALL: [BooleanOp; 4] =
        [BooleanOp::Unite, BooleanOp::Subtract, BooleanOp::Intersect, BooleanOp::Exclude];

    pub fn label(self) -> &'static str {
        match self {
            BooleanOp::Unite => "Unite",
            BooleanOp::Subtract => "Subtract",
            BooleanOp::Intersect => "Intersect",
            BooleanOp::Exclude => "Exclude",
        }
    }
}

/// A shape layer's geometry. Primitives keep their parameters so the shape
/// stays editable — dragging a rounded rectangle's corner handle should change
/// its radius, not push anchor points around.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShapeGeometry {
    Rectangle {
        rect: Rect,
    },
    RoundedRectangle {
        rect: Rect,
        radius: f32,
    },
    Ellipse {
        rect: Rect,
    },
    Polygon {
        rect: Rect,
        sides: u32,
    },
    Star {
        rect: Rect,
        points: u32,
        inner_ratio: f32,
    },
    Line {
        from: Vec2,
        to: Vec2,
    },
    /// A freeform path, either drawn with the Pen tools or produced by making a
    /// primitive editable.
    Custom {
        path: Path,
    },
    /// A pending boolean combination, kept live so it can be re-edited.
    Combined {
        op: BooleanOp,
        operands: Vec<ShapeGeometry>,
    },
}

impl ShapeGeometry {
    /// Realise the geometry as a path.
    ///
    /// Boolean combination is not implemented yet: a true path-boolean needs a
    /// planar-subdivision pass, and approximating it here would silently
    /// produce wrong outlines. Until then a `Combined` shape renders as its
    /// operands' subpaths with the even-odd rule, which is exact for
    /// `Exclude` and a visible placeholder for the rest.
    pub fn to_path(&self) -> Path {
        match self {
            ShapeGeometry::Rectangle { rect } => Path::rect(*rect),
            ShapeGeometry::RoundedRectangle { rect, radius } => {
                Path::rounded_rect(*rect, *radius)
            }
            ShapeGeometry::Ellipse { rect } => Path::ellipse(*rect),
            ShapeGeometry::Polygon { rect, sides } => Path::polygon(*rect, *sides),
            ShapeGeometry::Star { rect, points, inner_ratio } => {
                Path::star(*rect, *points, *inner_ratio)
            }
            ShapeGeometry::Line { from, to } => Path::line(*from, *to),
            ShapeGeometry::Custom { path } => path.clone(),
            ShapeGeometry::Combined { operands, .. } => {
                let mut p = Path { subpaths: Vec::new(), fill_rule: FillRule::EvenOdd };
                for o in operands {
                    p.subpaths.extend(o.to_path().subpaths);
                }
                p
            }
        }
    }

    pub fn bounds(&self) -> Rect {
        self.to_path().bounds()
    }

    /// Convert to an editable custom path — the Shape tool's `Make Editable`.
    pub fn into_editable(self) -> ShapeGeometry {
        match self {
            ShapeGeometry::Custom { .. } => self,
            other => ShapeGeometry::Custom { path: other.to_path() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_path_has_four_anchors() {
        let p = Path::rect(Rect::new(0.0, 0.0, 10.0, 20.0));
        assert_eq!(p.subpaths.len(), 1);
        assert!(p.subpaths[0].closed);
        assert_eq!(p.subpaths[0].anchors().len(), 4);
        let b = p.bounds();
        assert!((b.width - 10.0).abs() < 1e-3 && (b.height - 20.0).abs() < 1e-3);
    }

    #[test]
    fn ellipse_bounds_match_its_box() {
        let r = Rect::new(5.0, 5.0, 40.0, 20.0);
        let b = Path::ellipse(r).bounds();
        assert!((b.x - r.x).abs() < 0.2, "{b:?}");
        assert!((b.width - r.width).abs() < 0.5, "{b:?}");
    }

    #[test]
    fn hit_testing_a_rectangle() {
        let p = Path::rect(Rect::new(0.0, 0.0, 10.0, 10.0));
        assert!(p.contains(Vec2::new(5.0, 5.0)));
        assert!(!p.contains(Vec2::new(15.0, 5.0)));
        assert!(!p.contains(Vec2::new(-1.0, 5.0)));
    }

    #[test]
    fn even_odd_rule_makes_a_hole() {
        let mut p = Path::rect(Rect::new(0.0, 0.0, 20.0, 20.0));
        p.subpaths.extend(Path::rect(Rect::new(5.0, 5.0, 10.0, 10.0)).subpaths);
        p.fill_rule = FillRule::EvenOdd;
        assert!(p.contains(Vec2::new(2.0, 10.0)), "outer ring should be inside");
        assert!(!p.contains(Vec2::new(10.0, 10.0)), "inner square should be a hole");
    }

    #[test]
    fn polygon_and_star_vertex_counts() {
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert_eq!(Path::polygon(r, 6).subpaths[0].anchors().len(), 6);
        assert_eq!(Path::star(r, 5, 0.5).subpaths[0].anchors().len(), 10);
        // Degenerate inputs are clamped, not panicked on.
        assert_eq!(Path::polygon(r, 0).subpaths[0].anchors().len(), 3);
    }

    #[test]
    fn rounded_rect_clamps_oversized_radius() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Path::rounded_rect(r, 999.0).bounds();
        assert!((b.width - 10.0).abs() < 0.5);
    }

    #[test]
    fn transform_moves_the_whole_path() {
        let mut p = Path::rect(Rect::new(0.0, 0.0, 10.0, 10.0));
        p.transform(&Transform::translate(Vec2::new(5.0, 7.0)));
        let b = p.bounds();
        assert!((b.x - 5.0).abs() < 1e-3 && (b.y - 7.0).abs() < 1e-3);
    }

    #[test]
    fn flatten_respects_tolerance() {
        let mut sub = SubPath::new(Vec2::ZERO);
        sub.cubic_to(Vec2::new(0.0, 100.0), Vec2::new(100.0, 100.0), Vec2::new(100.0, 0.0));
        let coarse = sub.flatten(10.0).len();
        let fine = sub.flatten(0.1).len();
        assert!(fine > coarse, "finer tolerance should subdivide more: {fine} vs {coarse}");
    }

    #[test]
    fn make_editable_preserves_shape() {
        let g = ShapeGeometry::Ellipse { rect: Rect::new(0.0, 0.0, 30.0, 30.0) };
        let before = g.bounds();
        let after = g.into_editable().bounds();
        assert!((before.width - after.width).abs() < 1e-3);
    }
}
