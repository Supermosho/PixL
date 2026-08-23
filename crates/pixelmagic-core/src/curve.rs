//! Tone curves.
//!
//! Used by the Curves adjustment, by Levels (which is a constrained curve with
//! a friendlier UI), and by Gradient Map. Control points are kept sorted by
//! `x`, and evaluation uses monotone cubic Hermite interpolation
//! (Fritsch–Carlson): a plain Catmull-Rom spline overshoots between closely
//! spaced points, which shows up as ugly contrast reversals — a curve that dips
//! *darker* between two points the user dragged *lighter*. Monotone
//! interpolation cannot do that.

use serde::{Deserialize, Serialize};

/// Number of entries baked into the 1-D texture handed to the shader. 1024 is
/// enough that banding stays below the noise floor even on 16-bit documents,
/// while costing 4 KB per channel.
pub const LUT_SIZE: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub x: f32,
    pub y: f32,
}

impl CurvePoint {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    points: Vec<CurvePoint>,
}

impl Default for Curve {
    fn default() -> Self {
        Curve::identity()
    }
}

impl Curve {
    /// The straight line y = x.
    pub fn identity() -> Self {
        Curve { points: vec![CurvePoint::new(0.0, 0.0), CurvePoint::new(1.0, 1.0)] }
    }

    pub fn from_points(mut points: Vec<CurvePoint>) -> Self {
        points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        if points.len() < 2 {
            return Curve::identity();
        }
        Curve { points }
    }

    pub fn points(&self) -> &[CurvePoint] {
        &self.points
    }

    pub fn is_identity(&self) -> bool {
        self.points.len() == 2
            && (self.points[0].x).abs() < 1e-6
            && (self.points[0].y).abs() < 1e-6
            && (self.points[1].x - 1.0).abs() < 1e-6
            && (self.points[1].y - 1.0).abs() < 1e-6
    }

    /// Insert a point, keeping the list sorted. Returns its index.
    ///
    /// A new point whose `x` collides with an existing one replaces it — that
    /// matches what dragging onto an existing point does, and it keeps the
    /// spline well-defined (two points at the same `x` would divide by zero).
    pub fn add_point(&mut self, p: CurvePoint) -> usize {
        let p = CurvePoint::new(p.x.clamp(0.0, 1.0), p.y);
        match self.points.iter().position(|q| (q.x - p.x).abs() < 1e-4) {
            Some(i) => {
                self.points[i] = p;
                i
            }
            None => {
                let i = self.points.iter().position(|q| q.x > p.x).unwrap_or(self.points.len());
                self.points.insert(i, p);
                i
            }
        }
    }

    /// Remove a point. Refuses to drop below two points, since a curve needs at
    /// least a start and an end to be evaluable.
    pub fn remove_point(&mut self, index: usize) -> bool {
        if self.points.len() <= 2 || index >= self.points.len() {
            return false;
        }
        self.points.remove(index);
        true
    }

    /// Move a point, keeping the ordering intact. The point is confined between
    /// its neighbours so the curve stays a function of `x`.
    pub fn move_point(&mut self, index: usize, x: f32, y: f32) -> bool {
        if index >= self.points.len() {
            return false;
        }
        const GAP: f32 = 1e-3;
        let lo = if index == 0 { 0.0 } else { self.points[index - 1].x + GAP };
        let hi =
            if index + 1 == self.points.len() { 1.0 } else { self.points[index + 1].x - GAP };
        // First and last points are pinned to the ends of the domain.
        let x = if index == 0 {
            0.0
        } else if index + 1 == self.points.len() {
            1.0
        } else {
            x.clamp(lo.min(hi), hi.max(lo))
        };
        self.points[index] = CurvePoint::new(x, y.clamp(0.0, 1.0));
        true
    }

    /// Evaluate at `x`, extending the end segments flat outside 0..1.
    pub fn eval(&self, x: f32) -> f32 {
        let pts = &self.points;
        let n = pts.len();
        if n < 2 {
            return x;
        }
        if x <= pts[0].x {
            return pts[0].y;
        }
        if x >= pts[n - 1].x {
            return pts[n - 1].y;
        }

        let i = match pts
            .binary_search_by(|p| p.x.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal))
        {
            Ok(i) => return pts[i].y,
            Err(i) => i - 1,
        };

        let tangents = self.tangents();
        let (p0, p1) = (pts[i], pts[i + 1]);
        let h = p1.x - p0.x;
        if h <= 0.0 {
            return p0.y;
        }
        let t = (x - p0.x) / h;
        let t2 = t * t;
        let t3 = t2 * t;

        // Cubic Hermite basis.
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;

        h00 * p0.y + h10 * h * tangents[i] + h01 * p1.y + h11 * h * tangents[i + 1]
    }

    /// Fritsch–Carlson tangents, which guarantee the interpolant is monotone on
    /// every interval where the data is.
    fn tangents(&self) -> Vec<f32> {
        let pts = &self.points;
        let n = pts.len();
        let mut slopes = Vec::with_capacity(n - 1);
        for i in 0..n - 1 {
            let h = pts[i + 1].x - pts[i].x;
            slopes.push(if h > 0.0 { (pts[i + 1].y - pts[i].y) / h } else { 0.0 });
        }

        let mut m = Vec::with_capacity(n);
        m.push(slopes[0]);
        for i in 1..n - 1 {
            // A local extremum in the data must stay an extremum in the curve.
            if slopes[i - 1] * slopes[i] <= 0.0 {
                m.push(0.0);
            } else {
                m.push((slopes[i - 1] + slopes[i]) * 0.5);
            }
        }
        m.push(slopes[n - 2]);

        // Clamp tangents into the Fritsch–Carlson monotonicity region.
        for i in 0..n - 1 {
            if slopes[i].abs() < 1e-9 {
                m[i] = 0.0;
                m[i + 1] = 0.0;
                continue;
            }
            let a = m[i] / slopes[i];
            let b = m[i + 1] / slopes[i];
            let s = a * a + b * b;
            if s > 9.0 {
                let t = 3.0 / s.sqrt();
                m[i] = t * a * slopes[i];
                m[i + 1] = t * b * slopes[i];
            }
        }
        m
    }

    /// Bake to a lookup table for upload as a 1-D texture.
    pub fn to_lut(&self) -> Vec<f32> {
        (0..LUT_SIZE).map(|i| self.eval(i as f32 / (LUT_SIZE - 1) as f32)).collect()
    }

    /// The classic contrast S-curve, useful as a preset and in tests.
    pub fn s_curve(strength: f32) -> Self {
        let k = strength.clamp(0.0, 1.0) * 0.25;
        Curve::from_points(vec![
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.25, 0.25 - k),
            CurvePoint::new(0.75, 0.75 + k),
            CurvePoint::new(1.0, 1.0),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_flat() {
        let c = Curve::identity();
        assert!(c.is_identity());
        for i in 0..=20 {
            let x = i as f32 / 20.0;
            assert!((c.eval(x) - x).abs() < 1e-4, "identity broke at {x}");
        }
    }

    #[test]
    fn passes_through_its_control_points() {
        let c = Curve::from_points(vec![
            CurvePoint::new(0.0, 0.1),
            CurvePoint::new(0.5, 0.8),
            CurvePoint::new(1.0, 0.9),
        ]);
        assert!((c.eval(0.0) - 0.1).abs() < 1e-4);
        assert!((c.eval(0.5) - 0.8).abs() < 1e-4);
        assert!((c.eval(1.0) - 0.9).abs() < 1e-4);
    }

    #[test]
    fn monotone_data_yields_monotone_curve() {
        // Closely spaced points like these are exactly what makes a
        // Catmull-Rom spline overshoot.
        let c = Curve::from_points(vec![
            CurvePoint::new(0.0, 0.0),
            CurvePoint::new(0.45, 0.05),
            CurvePoint::new(0.55, 0.95),
            CurvePoint::new(1.0, 1.0),
        ]);
        let mut prev = -1.0;
        for i in 0..=500 {
            let y = c.eval(i as f32 / 500.0);
            assert!(y >= prev - 1e-5, "curve went backwards at {i}: {y} < {prev}");
            assert!((-0.001..=1.001).contains(&y), "overshoot at {i}: {y}");
            prev = y;
        }
    }

    #[test]
    fn clamps_outside_the_domain() {
        let c = Curve::from_points(vec![CurvePoint::new(0.0, 0.2), CurvePoint::new(1.0, 0.8)]);
        assert!((c.eval(-1.0) - 0.2).abs() < 1e-6);
        assert!((c.eval(2.0) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn adding_at_an_existing_x_replaces() {
        let mut c = Curve::identity();
        c.add_point(CurvePoint::new(0.5, 0.7));
        assert_eq!(c.points().len(), 3);
        c.add_point(CurvePoint::new(0.5, 0.3));
        assert_eq!(c.points().len(), 3);
        assert!((c.eval(0.5) - 0.3).abs() < 1e-4);
    }

    #[test]
    fn cannot_remove_below_two_points() {
        let mut c = Curve::identity();
        assert!(!c.remove_point(0));
        c.add_point(CurvePoint::new(0.5, 0.5));
        assert!(c.remove_point(1));
        assert_eq!(c.points().len(), 2);
    }

    #[test]
    fn endpoints_stay_pinned_when_moved() {
        let mut c = Curve::identity();
        c.move_point(0, 0.4, 0.3);
        assert_eq!(c.points()[0].x, 0.0);
        assert!((c.points()[0].y - 0.3).abs() < 1e-6);
    }

    #[test]
    fn interior_points_cannot_cross_neighbours() {
        let mut c = Curve::identity();
        let i = c.add_point(CurvePoint::new(0.5, 0.5));
        c.move_point(i, 5.0, 0.5);
        assert!(c.points()[i].x < 1.0);
        c.move_point(i, -5.0, 0.5);
        assert!(c.points()[i].x > 0.0);
    }

    #[test]
    fn lut_has_the_right_shape() {
        let lut = Curve::s_curve(1.0).to_lut();
        assert_eq!(lut.len(), LUT_SIZE);
        assert!(lut[0] < 0.01);
        assert!(lut[LUT_SIZE - 1] > 0.99);
        // An S-curve darkens the low quarter and lightens the high quarter.
        assert!(lut[LUT_SIZE / 4] < 0.25);
        assert!(lut[LUT_SIZE * 3 / 4] > 0.75);
    }
}
