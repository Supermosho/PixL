//! Region growing for the Quick Selection and Color Selection tools.
//!
//! ## What this is, and what it deliberately is not
//!
//! Quick Selection in Pixelmator (and Photoshop) grows a region outward from
//! where you point, stopping where the image stops looking like what you
//! started on. The polished versions of this are graph-cut segmentations with a
//! learned edge term; "Select Subject" goes further still and runs a salient
//! object model. Neither is implemented here and neither is claimed.
//!
//! What *is* here is the honest core of the technique: a flood fill in colour
//! space with a soft threshold, which is what the tool degrades to on flat and
//! near-flat regions — the illustration-style artwork it will most often be
//! pointed at. On a photograph of a person against a busy background it will
//! under-select, and the user will reach for the brush to add to it. That is a
//! real limitation, not a temporary one, and the tool's panel says so.
//!
//! ## Why a queue and not recursion
//!
//! A recursive flood fill overflows the stack on any region of interesting
//! size — a 4000×3000 image is twelve million pixels and the recursion depth is
//! bounded only by the region's diameter. The explicit queue below is the same
//! algorithm without that failure mode.
//!
//! ## Cost
//!
//! Each pixel is visited at most once, so this is O(area) with a small
//! constant, and the caller controls the area through `max_radius`. The hover
//! preview runs it on every pointer move, so that bound is what keeps the
//! interaction responsive rather than any cleverness in the inner loop.

use crate::buffer::{MaskBuffer, PixelBuffer};
use crate::color::Rgba;
use std::collections::VecDeque;

/// How the region is allowed to grow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrowOptions {
    /// Colour distance at which a pixel stops belonging, in 0..=1 over the
    /// unit RGB cube. Larger takes in more.
    pub tolerance: f32,
    /// Hard cap on how far from the seed the region may reach, in pixels.
    /// `None` means the whole canvas.
    ///
    /// This is what makes a hover preview affordable: without it, pointing at
    /// the sky in a landscape floods most of the image on every mouse move.
    pub max_radius: Option<f32>,
    /// Whether a pixel's similarity is judged against the seed colour
    /// (`false`, the classic magic wand — one global threshold) or against the
    /// neighbour it spread from (`true`, which follows gradients and is what
    /// makes a soft-shaded object come out whole).
    pub relative: bool,
    /// Include diagonal neighbours. Off by default: 8-connectivity leaks
    /// through single-pixel diagonal gaps, which on antialiased artwork means
    /// escaping through the seam between two shapes that merely touch.
    pub diagonal: bool,
}

impl Default for GrowOptions {
    fn default() -> Self {
        Self { tolerance: 0.12, max_radius: None, relative: false, diagonal: false }
    }
}

impl GrowOptions {
    /// Settings for the live hover preview: bounded reach so the cost per
    /// pointer move is predictable.
    pub fn preview(tolerance: f32, max_radius: f32) -> Self {
        Self { tolerance, max_radius: Some(max_radius), relative: true, diagonal: false }
    }
}

/// Squared Euclidean distance in RGB, alpha-aware.
///
/// Squared rather than the distance itself so the inner loop has no `sqrt`;
/// the caller's tolerance is squared once, up front, to match. Alpha
/// participates because a transparent pixel and an opaque one of the same RGB
/// are not the same thing to select — without this, growing from inside a
/// shape spills into the transparent surround, whose RGB is often whatever the
/// last blend left behind.
fn distance_sq(a: Rgba, b: Rgba) -> f32 {
    let dr = a.r - b.r;
    let dg = a.g - b.g;
    let db = a.b - b.b;
    let da = a.a - b.a;
    dr * dr + dg * dg + db * db + da * da
}

/// Grow a region outward from `seed`, returning a coverage mask.
///
/// The mask is binary — 0 or 255 — because a flood fill has no natural notion
/// of partial coverage. Callers wanting a soft edge run
/// [`crate::selection::feather`] over the result, which is also how the
/// geometric selection tools get theirs.
///
/// Returns an empty mask if the seed is outside the image.
pub fn grow(image: &PixelBuffer, seed: (u32, u32), options: GrowOptions) -> MaskBuffer {
    let (w, h) = (image.width(), image.height());
    let mut mask = MaskBuffer::new(w, h);
    let (sx, sy) = seed;
    if sx >= w || sy >= h || w == 0 || h == 0 {
        return mask;
    }
    let Some(seed_color) = image.get(sx, sy) else { return mask };

    let threshold_sq = {
        let t = options.tolerance.max(0.0);
        t * t * 4.0 // four channels, so the diagonal of the unit hypercube is 2
    };
    let radius_sq = options.max_radius.map(|r| r * r);

    // `visited` is separate from `mask` so a pixel rejected once is not tested
    // again from another direction. Without it a large flat region is examined
    // roughly four times over.
    let mut visited = vec![false; (w as usize) * (h as usize)];
    let mut queue: VecDeque<(u32, u32, Rgba)> = VecDeque::new();

    let index = |x: u32, y: u32| (y as usize) * (w as usize) + (x as usize);

    visited[index(sx, sy)] = true;
    mask.set(sx, sy, 255);
    queue.push_back((sx, sy, seed_color));

    // Built once rather than per dequeued pixel: this loop runs millions of
    // times on a large fill and allocating a neighbour list inside it was the
    // first version's mistake.
    const OFFSETS: [(i32, i32); 8] =
        [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
    let neighbours = &OFFSETS[..if options.diagonal { 8 } else { 4 }];

    while let Some((x, y, from_color)) = queue.pop_front() {
        for (dx, dy) in neighbours.iter().copied() {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                continue;
            }
            let (nx, ny) = (nx as u32, ny as u32);
            let i = index(nx, ny);
            if visited[i] {
                continue;
            }

            if let Some(r2) = radius_sq {
                let ddx = nx as f32 - sx as f32;
                let ddy = ny as f32 - sy as f32;
                if ddx * ddx + ddy * ddy > r2 {
                    // Marked visited so the boundary is not re-tested from
                    // every pixel along it.
                    visited[i] = true;
                    continue;
                }
            }

            visited[i] = true;
            let Some(color) = image.get(nx, ny) else { continue };
            let reference = if options.relative { from_color } else { seed_color };
            if distance_sq(color, reference) <= threshold_sq {
                mask.set(nx, ny, 255);
                queue.push_back((nx, ny, color));
            }
        }
    }

    mask
}

/// Select every pixel in the image similar to the one at `seed`, contiguous or
/// not — the "global" behaviour of a magic wand with `Contiguous` unticked.
///
/// A plain scan rather than a flood fill: there is no propagation, so the
/// `relative` and `diagonal` options do not apply and are ignored.
pub fn grow_global(image: &PixelBuffer, seed: (u32, u32), tolerance: f32) -> MaskBuffer {
    let (w, h) = (image.width(), image.height());
    let mut mask = MaskBuffer::new(w, h);
    let (sx, sy) = seed;
    if sx >= w || sy >= h {
        return mask;
    }
    let Some(seed_color) = image.get(sx, sy) else { return mask };
    let t = tolerance.max(0.0);
    let threshold_sq = t * t * 4.0;

    for y in 0..h {
        for x in 0..w {
            if let Some(c) = image.get(x, y) {
                if distance_sq(c, seed_color) <= threshold_sq {
                    mask.set(x, y, 255);
                }
            }
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two solid halves, split down the middle at `split`.
    fn two_tone(w: u32, h: u32, split: u32, left: Rgba, right: Rgba) -> PixelBuffer {
        let mut b = PixelBuffer::new(w, h);
        for y in 0..h {
            for x in 0..w {
                b.set(x, y, if x < split { left } else { right });
            }
        }
        b
    }

    fn count(mask: &MaskBuffer) -> usize {
        mask.data().iter().filter(|&&v| v > 0).count()
    }

    #[test]
    fn growth_stops_at_a_hard_colour_edge() {
        let image =
            two_tone(20, 10, 10, Rgba::new(1.0, 0.0, 0.0, 1.0), Rgba::new(0.0, 0.0, 1.0, 1.0));
        let mask = grow(&image, (2, 5), GrowOptions::default());

        assert_eq!(count(&mask), 10 * 10, "should take exactly the red half");
        assert_eq!(mask.get(9, 5), 255, "last red column selected");
        assert_eq!(mask.get(10, 5), 0, "first blue column not selected");
    }

    #[test]
    fn a_wide_enough_tolerance_crosses_the_edge() {
        let image =
            two_tone(20, 10, 10, Rgba::new(1.0, 0.0, 0.0, 1.0), Rgba::new(0.0, 0.0, 1.0, 1.0));
        let mask = grow(&image, (2, 5), GrowOptions { tolerance: 1.0, ..Default::default() });
        assert_eq!(count(&mask), 20 * 10, "everything is within tolerance now");
    }

    #[test]
    fn growth_is_contiguous_only() {
        // Two red bars separated by a blue gap: growing from one must not
        // jump the gap, however similar the far bar is.
        let mut image = PixelBuffer::new(30, 6);
        for y in 0..6 {
            for x in 0..30 {
                let red = !(10..20).contains(&x);
                image.set(
                    x,
                    y,
                    if red {
                        Rgba::new(1.0, 0.0, 0.0, 1.0)
                    } else {
                        Rgba::new(0.0, 0.0, 1.0, 1.0)
                    },
                );
            }
        }
        let mask = grow(&image, (2, 3), GrowOptions::default());
        assert_eq!(count(&mask), 10 * 6, "only the near bar");
        assert_eq!(mask.get(25, 3), 0, "the far bar is a separate region");

        // ...whereas the global variant takes both, which is the distinction
        // between the two functions.
        let global = grow_global(&image, (2, 3), 0.12);
        assert_eq!(count(&global), 20 * 6, "both bars");
    }

    #[test]
    fn max_radius_bounds_the_region() {
        let image = two_tone(64, 64, 64, Rgba::WHITE, Rgba::WHITE); // all one colour
        let unbounded = grow(&image, (32, 32), GrowOptions::default());
        assert_eq!(count(&unbounded), 64 * 64, "no cap means the whole canvas");

        let bounded =
            grow(&image, (32, 32), GrowOptions { max_radius: Some(5.0), ..Default::default() });
        assert!(count(&bounded) < 100, "a radius-5 disc, not the canvas");
        assert_eq!(bounded.get(32, 32), 255, "seed is in");
        assert_eq!(bounded.get(32, 50), 0, "well outside the radius is out");
    }

    #[test]
    fn relative_mode_follows_a_gradient_that_absolute_mode_stops_within() {
        // A left-to-right ramp: adjacent columns differ slightly, but the two
        // ends differ a lot. Absolute mode measures against the seed and
        // stops partway; relative mode measures against the neighbour and
        // walks the whole ramp. This is the difference that makes a
        // soft-shaded object come out in one piece.
        let (w, h) = (64u32, 4u32);
        let mut image = PixelBuffer::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = x as f32 / (w - 1) as f32;
                image.set(x, y, Rgba::new(v, v, v, 1.0));
            }
        }

        let absolute =
            grow(&image, (0, 2), GrowOptions { tolerance: 0.1, ..Default::default() });
        let relative = grow(
            &image,
            (0, 2),
            GrowOptions { tolerance: 0.1, relative: true, ..Default::default() },
        );

        assert!(count(&absolute) < (w * h) as usize, "absolute must stop inside the ramp");
        assert_eq!(count(&relative), (w * h) as usize, "relative walks the whole ramp");
    }

    #[test]
    fn transparency_is_a_boundary() {
        // Same RGB either side, different alpha. Growing from the opaque half
        // must not spill into the transparent one — the case that makes a
        // selection on a cut-out layer swallow the empty surround.
        let image =
            two_tone(20, 4, 10, Rgba::new(0.5, 0.5, 0.5, 1.0), Rgba::new(0.5, 0.5, 0.5, 0.0));
        let mask = grow(&image, (2, 2), GrowOptions::default());
        assert_eq!(count(&mask), 10 * 4, "stops at the alpha edge");
    }

    #[test]
    fn a_seed_outside_the_image_selects_nothing() {
        let image = two_tone(8, 8, 8, Rgba::WHITE, Rgba::WHITE);
        assert_eq!(count(&grow(&image, (99, 0), GrowOptions::default())), 0);
        assert_eq!(count(&grow_global(&image, (0, 99), 0.5)), 0);
    }

    #[test]
    fn diagonal_connectivity_crosses_a_corner_touch_that_orthogonal_does_not() {
        // Two blocks meeting at exactly one corner. This is the leak that
        // 8-connectivity causes on artwork where two shapes merely touch, and
        // the reason `diagonal` defaults to off.
        let mut image = PixelBuffer::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                image.set(x, y, Rgba::new(0.0, 0.0, 0.0, 1.0));
            }
        }
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1), (2, 2), (3, 2), (2, 3), (3, 3)] {
            image.set(x, y, Rgba::WHITE);
        }

        let ortho = grow(&image, (0, 0), GrowOptions::default());
        assert_eq!(count(&ortho), 4, "only the first block");

        let diag = grow(&image, (0, 0), GrowOptions { diagonal: true, ..Default::default() });
        assert_eq!(count(&diag), 8, "leaks through the corner into the second");
    }

    #[test]
    fn every_pixel_is_visited_at_most_once_on_a_large_flat_region() {
        // Guards the `visited` array: without it this still terminates but
        // does several times the work. A million-pixel fill finishing quickly
        // is the observable difference.
        let image = two_tone(1000, 1000, 1000, Rgba::WHITE, Rgba::WHITE);
        let start = std::time::Instant::now();
        let mask = grow(&image, (500, 500), GrowOptions::default());
        assert_eq!(count(&mask), 1000 * 1000);
        assert!(
            start.elapsed().as_secs_f32() < 5.0,
            "a flat megapixel fill should not take seconds"
        );
    }
}
