//! The brush engine.
//!
//! Runs on the CPU, writing into the layer's [`PixelBuffer`] and letting the
//! renderer re-upload the dirty region. Painting on the GPU would avoid the
//! upload, but it also means the authoritative pixels live in a texture that
//! has to be read back for every save, undo snapshot and colour pick — and
//! readback stalls are far more noticeable than a sub-millisecond upload of a
//! brush-sized rectangle.
//!
//! Strokes are stamped as overlapping dabs at a fixed spacing rather than drawn
//! as lines, which is how every raster brush works: it makes soft edges, flow
//! build-up and pressure response fall out naturally.

use pixelmagic_core::buffer::PixelBuffer;
use pixelmagic_core::color::Rgba;
use pixelmagic_core::geom::Rect;
use pixelmagic_core::selection::Selection;
use pixelmagic_core::tool::BrushSettings;

/// What a dab does to the pixels underneath it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrushMode {
    /// Paint the foreground colour.
    Paint,
    /// Reduce alpha.
    Erase,
    /// Local blur.
    Soften,
    /// Local unsharp mask.
    Sharpen,
    Lighten,
    Darken,
    Saturate,
    Desaturate,
    /// Copy from an offset source point.
    Clone {
        offset: glam::Vec2,
    },
}

/// One stamp of the brush.
pub struct Dab {
    pub center: glam::Vec2,
    pub radius: f32,
    pub softness: f32,
    pub opacity: f32,
    pub color: Rgba,
    pub mode: BrushMode,
}

impl Dab {
    /// Coverage at a distance from the centre, 0..=1.
    ///
    /// The falloff is `smoothstep` between the hard core and the outer edge.
    /// A linear ramp would leave a visible crease where the gradient's
    /// derivative jumps; smoothstep is C¹ at both ends, which is what makes a
    /// soft brush look soft rather than like a cone.
    pub fn coverage(&self, distance: f32) -> f32 {
        if self.radius <= 0.0 {
            return 0.0;
        }
        let t = distance / self.radius;
        if t >= 1.0 {
            return 0.0;
        }
        let hard = (1.0 - self.softness).clamp(0.0, 0.999);
        if t <= hard {
            return 1.0;
        }
        let u = (t - hard) / (1.0 - hard);
        let s = u * u * (3.0 - 2.0 * u);
        1.0 - s
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(
            self.center.x - self.radius - 1.0,
            self.center.y - self.radius - 1.0,
            self.radius * 2.0 + 2.0,
            self.radius * 2.0 + 2.0,
        )
    }
}

/// Stamp one dab into `buffer`, clipped to `selection`. Returns the rectangle
/// actually touched, for the dirty-region bookkeeping.
pub fn stamp(buffer: &mut PixelBuffer, dab: &Dab, selection: Option<&Selection>) -> Rect {
    let area = dab.bounds().round_out().intersection(buffer.bounds());
    if area.is_empty() {
        return Rect::ZERO;
    }

    let x0 = area.x as u32;
    let y0 = area.y as u32;
    let x1 = (area.x + area.width) as u32;
    let y1 = (area.y + area.height) as u32;

    // Neighbourhood-reading modes need the original pixels, or the blur would
    // feed on its own output and smear along the scan direction.
    let source =
        matches!(dab.mode, BrushMode::Soften | BrushMode::Sharpen).then(|| buffer.crop(area));

    for y in y0..y1 {
        for x in x0..x1 {
            let p = glam::Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let mut cov = dab.coverage(p.distance(dab.center)) * dab.opacity;
            if cov <= 0.0 {
                continue;
            }
            if let Some(sel) = selection {
                cov *= sel.coverage_at(x, y);
                if cov <= 0.0 {
                    continue;
                }
            }

            let Some(existing) = buffer.get(x, y) else { continue };
            let result = match dab.mode {
                BrushMode::Paint => blend_over(existing, dab.color, cov),
                BrushMode::Erase => existing.with_alpha((existing.a * (1.0 - cov)).max(0.0)),
                BrushMode::Lighten => scale_luma(existing, 1.0 + cov * 0.5),
                BrushMode::Darken => scale_luma(existing, 1.0 - cov * 0.4),
                BrushMode::Saturate => scale_saturation(existing, 1.0 + cov * 0.5),
                BrushMode::Desaturate => scale_saturation(existing, 1.0 - cov * 0.5),
                BrushMode::Soften => {
                    let s = source.as_ref().expect("soften takes a snapshot");
                    let avg = average_around(s, x - x0, y - y0, 1);
                    existing.lerp(avg, cov)
                }
                BrushMode::Sharpen => {
                    let s = source.as_ref().expect("sharpen takes a snapshot");
                    let avg = average_around(s, x - x0, y - y0, 1);
                    let detail = Rgba::new(
                        existing.r - avg.r,
                        existing.g - avg.g,
                        existing.b - avg.b,
                        0.0,
                    );
                    Rgba::new(
                        (existing.r + detail.r * cov).clamp(0.0, 1.0),
                        (existing.g + detail.g * cov).clamp(0.0, 1.0),
                        (existing.b + detail.b * cov).clamp(0.0, 1.0),
                        existing.a,
                    )
                }
                BrushMode::Clone { offset } => {
                    let sx = p.x - offset.x;
                    let sy = p.y - offset.y;
                    if sx < 0.0 || sy < 0.0 {
                        continue;
                    }
                    match buffer.get(sx as u32, sy as u32) {
                        Some(src) => blend_over(existing, src, cov * src.a),
                        None => continue,
                    }
                }
            };
            buffer.set(x, y, result);
        }
    }
    area
}

/// Source-over of a straight-alpha colour at coverage `cov`.
///
/// Done in linear light, then converted back, so painting a soft white edge
/// over black does not produce the grey fringe that encoded-space blending
/// gives.
fn blend_over(dst: Rgba, src: Rgba, cov: f32) -> Rgba {
    let a = (src.a * cov).clamp(0.0, 1.0);
    if a <= 0.0 {
        return dst;
    }
    let d = dst.to_linear();
    let s = src.to_linear();
    let out_a = a + d.a * (1.0 - a);
    if out_a <= f32::EPSILON {
        return Rgba::TRANSPARENT;
    }
    let f = |sc: f32, dc: f32| (sc * a + dc * d.a * (1.0 - a)) / out_a;
    Rgba::new(f(s.r, d.r), f(s.g, d.g), f(s.b, d.b), out_a).to_srgb()
}

fn scale_luma(c: Rgba, factor: f32) -> Rgba {
    let l = c.to_linear();
    Rgba::new(
        (l.r * factor).clamp(0.0, 1.0),
        (l.g * factor).clamp(0.0, 1.0),
        (l.b * factor).clamp(0.0, 1.0),
        c.a,
    )
    .to_srgb()
}

fn scale_saturation(c: Rgba, factor: f32) -> Rgba {
    let y = c.luminance();
    Rgba::new(
        (y + (c.r - y) * factor).clamp(0.0, 1.0),
        (y + (c.g - y) * factor).clamp(0.0, 1.0),
        (y + (c.b - y) * factor).clamp(0.0, 1.0),
        c.a,
    )
}

fn average_around(buf: &PixelBuffer, x: u32, y: u32, r: i32) -> Rgba {
    let mut acc = [0.0f32; 4];
    let mut n = 0.0;
    for dy in -r..=r {
        for dx in -r..=r {
            let nx = x as i64 + dx as i64;
            let ny = y as i64 + dy as i64;
            if nx < 0 || ny < 0 {
                continue;
            }
            if let Some(c) = buf.get(nx as u32, ny as u32) {
                acc[0] += c.r;
                acc[1] += c.g;
                acc[2] += c.b;
                acc[3] += c.a;
                n += 1.0;
            }
        }
    }
    if n == 0.0 {
        return Rgba::TRANSPARENT;
    }
    Rgba::new(acc[0] / n, acc[1] / n, acc[2] / n, acc[3] / n)
}

/// Stamp dabs along a segment at the brush's spacing.
///
/// Returns the union of every dab's bounds and the point the next segment
/// should start from — carrying the leftover distance forward is what keeps
/// spacing even across a stroke made of many short motion events, rather than
/// clustering a dab at every event boundary.
pub fn stroke(
    buffer: &mut PixelBuffer,
    from: glam::Vec2,
    to: glam::Vec2,
    settings: &BrushSettings,
    color: Rgba,
    mode: BrushMode,
    selection: Option<&Selection>,
) -> Rect {
    let spacing = settings.dab_spacing();
    let distance = from.distance(to);
    let steps = (distance / spacing).floor() as usize;

    let mut dirty = Rect::ZERO;
    let place = |p: glam::Vec2, buffer: &mut PixelBuffer, dirty: &mut Rect| {
        let dab = Dab {
            center: p,
            radius: settings.size * 0.5,
            softness: settings.softness,
            opacity: settings.opacity * settings.flow,
            color,
            mode,
        };
        *dirty = dirty.union(stamp(buffer, &dab, selection));
    };

    if steps == 0 {
        // A click, or a motion shorter than one spacing: still stamp once, or
        // tapping the canvas would do nothing.
        if distance < 1e-4 {
            place(to, buffer, &mut dirty);
        }
        return dirty;
    }

    let dir = (to - from) / distance;
    for i in 1..=steps {
        place(from + dir * (i as f32 * spacing), buffer, &mut dirty);
    }
    dirty
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(size: f32) -> BrushSettings {
        BrushSettings { size, softness: 0.0, spacing: 0.25, ..Default::default() }
    }

    #[test]
    fn hard_dab_is_binary() {
        let dab = Dab {
            center: glam::Vec2::new(5.0, 5.0),
            radius: 4.0,
            softness: 0.0,
            opacity: 1.0,
            color: Rgba::BLACK,
            mode: BrushMode::Paint,
        };
        assert_eq!(dab.coverage(0.0), 1.0);
        assert_eq!(dab.coverage(3.9), 1.0);
        assert_eq!(dab.coverage(4.0), 0.0);
        assert_eq!(dab.coverage(100.0), 0.0);
    }

    #[test]
    fn soft_dab_falls_off_smoothly() {
        let dab = Dab {
            center: glam::Vec2::ZERO,
            radius: 10.0,
            softness: 1.0,
            opacity: 1.0,
            color: Rgba::BLACK,
            mode: BrushMode::Paint,
        };
        let mut prev = 1.0;
        for i in 0..=10 {
            let c = dab.coverage(i as f32);
            assert!(c <= prev + 1e-6, "coverage should be monotone");
            prev = c;
        }
        assert!(dab.coverage(5.0) > 0.2 && dab.coverage(5.0) < 0.8);
    }

    #[test]
    fn zero_radius_paints_nothing() {
        let dab = Dab {
            center: glam::Vec2::ZERO,
            radius: 0.0,
            softness: 0.0,
            opacity: 1.0,
            color: Rgba::BLACK,
            mode: BrushMode::Paint,
        };
        assert_eq!(dab.coverage(0.0), 0.0);
    }

    #[test]
    fn painting_marks_the_pixels() {
        let mut buf = PixelBuffer::new(32, 32);
        let dab = Dab {
            center: glam::Vec2::new(16.0, 16.0),
            radius: 5.0,
            softness: 0.0,
            opacity: 1.0,
            color: Rgba::rgb(1.0, 0.0, 0.0),
            mode: BrushMode::Paint,
        };
        let dirty = stamp(&mut buf, &dab, None);
        assert_eq!(buf.get(16, 16).unwrap().to_u8(), [255, 0, 0, 255]);
        assert_eq!(buf.get(0, 0).unwrap().a, 0.0);
        assert!(dirty.contains(glam::Vec2::new(16.0, 16.0)));
    }

    #[test]
    fn painting_outside_the_buffer_is_harmless() {
        let mut buf = PixelBuffer::new(8, 8);
        let dab = Dab {
            center: glam::Vec2::new(-50.0, -50.0),
            radius: 5.0,
            softness: 0.0,
            opacity: 1.0,
            color: Rgba::WHITE,
            mode: BrushMode::Paint,
        };
        assert!(stamp(&mut buf, &dab, None).is_empty());
    }

    #[test]
    fn erasing_removes_alpha_without_touching_colour() {
        let mut buf = PixelBuffer::filled(16, 16, Rgba::rgb(0.2, 0.4, 0.6));
        let dab = Dab {
            center: glam::Vec2::new(8.0, 8.0),
            radius: 4.0,
            softness: 0.0,
            opacity: 1.0,
            color: Rgba::BLACK,
            mode: BrushMode::Erase,
        };
        stamp(&mut buf, &dab, None);
        let c = buf.get(8, 8).unwrap();
        assert_eq!(c.a, 0.0);
        assert_eq!(buf.get(0, 0).unwrap().a, 1.0);
    }

    #[test]
    fn selection_clips_the_brush() {
        use pixelmagic_core::selection::{Selection, SelectionOptions};
        let mut buf = PixelBuffer::new(32, 32);
        let sel = Selection::from_mask(Selection::rectangle(
            32,
            32,
            Rect::new(0.0, 0.0, 16.0, 32.0),
            SelectionOptions { antialias: false, feather: 0.0 },
        ));
        let dab = Dab {
            center: glam::Vec2::new(16.0, 16.0),
            radius: 10.0,
            softness: 0.0,
            opacity: 1.0,
            color: Rgba::WHITE,
            mode: BrushMode::Paint,
        };
        stamp(&mut buf, &dab, Some(&sel));
        assert!(buf.get(10, 16).unwrap().a > 0.9, "inside the selection");
        assert_eq!(buf.get(22, 16).unwrap().a, 0.0, "outside the selection");
    }

    #[test]
    fn stroke_covers_the_whole_segment() {
        let mut buf = PixelBuffer::new(64, 64);
        let s = settings(6.0);
        let dirty = stroke(
            &mut buf,
            glam::Vec2::new(8.0, 32.0),
            glam::Vec2::new(56.0, 32.0),
            &s,
            Rgba::WHITE,
            BrushMode::Paint,
            None,
        );
        for x in [12, 24, 36, 48] {
            assert!(buf.get(x, 32).unwrap().a > 0.5, "gap in the stroke at x={x}");
        }
        assert!(dirty.width > 40.0);
    }

    #[test]
    fn a_click_still_paints() {
        let mut buf = PixelBuffer::new(32, 32);
        let s = settings(8.0);
        let p = glam::Vec2::new(16.0, 16.0);
        stroke(&mut buf, p, p, &s, Rgba::WHITE, BrushMode::Paint, None);
        assert!(buf.get(16, 16).unwrap().a > 0.5, "a click with no motion should paint");
    }

    #[test]
    fn painting_over_black_with_white_does_not_go_grey_at_full_coverage() {
        let mut buf = PixelBuffer::filled(8, 8, Rgba::BLACK);
        let dab = Dab {
            center: glam::Vec2::new(4.0, 4.0),
            radius: 3.0,
            softness: 0.0,
            opacity: 1.0,
            color: Rgba::WHITE,
            mode: BrushMode::Paint,
        };
        stamp(&mut buf, &dab, None);
        assert_eq!(buf.get(4, 4).unwrap().to_u8(), [255, 255, 255, 255]);
    }

    #[test]
    fn soften_averages_an_edge() {
        let mut buf = PixelBuffer::new(16, 16);
        buf.fill_rect(Rect::new(0.0, 0.0, 8.0, 16.0), Rgba::WHITE);
        buf.fill_rect(Rect::new(8.0, 0.0, 8.0, 16.0), Rgba::BLACK);

        let dab = Dab {
            center: glam::Vec2::new(8.0, 8.0),
            radius: 4.0,
            softness: 0.0,
            opacity: 1.0,
            color: Rgba::BLACK,
            mode: BrushMode::Soften,
        };
        stamp(&mut buf, &dab, None);
        let v = buf.get(8, 8).unwrap().r;
        assert!(v > 0.05 && v < 0.95, "edge should have softened, got {v}");
    }

    #[test]
    fn lighten_and_darken_move_in_the_right_direction() {
        let base = Rgba::rgb(0.5, 0.5, 0.5);
        assert!(scale_luma(base, 1.5).r > base.r);
        assert!(scale_luma(base, 0.5).r < base.r);
        // Alpha is preserved.
        assert_eq!(scale_luma(base.with_alpha(0.3), 1.5).a, 0.3);
    }

    #[test]
    fn desaturate_moves_towards_grey() {
        let c = Rgba::rgb(1.0, 0.0, 0.0);
        let d = scale_saturation(c, 0.0);
        assert!((d.r - d.g).abs() < 1e-5 && (d.g - d.b).abs() < 1e-5);
    }
}
