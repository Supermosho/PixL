//! Colour types, working space and transfer functions.
//!
//! **Working space.** Everything inside the render graph is premultiplied
//! linear-light RGBA in `f16` (see `pixelmagic-gpu`). Compositing in a gamma
//! space is the single most common source of wrong-looking blends — halos on
//! `Screen`, muddy `Multiply` — so decoded pixels are linearised on the way in
//! and re-encoded only when we hand a frame to the display or an encoder.
//!
//! **Alpha.** `Rgba` here is *un*premultiplied, because that is what a user
//! types into a colour field. Premultiplication happens at upload time.

use serde::{Deserialize, Serialize};

/// An un-premultiplied colour. Components are nominally 0..=1 but values
/// outside that range are legal and meaningful: HDR highlights and
/// out-of-gamut results from adjustments must survive until the final clamp.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for Rgba {
    fn default() -> Self {
        Rgba::BLACK
    }
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const BLACK: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Rgba = Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    /// The 50% grey that `Overlay`, `Soft Light` and friends pivot around —
    /// perceptually mid, i.e. 0.5 in *encoded* sRGB, not in linear light.
    pub const MID_GREY: Rgba = Rgba { r: 0.5, g: 0.5, b: 0.5, a: 1.0 };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub fn from_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    pub fn to_u8(self) -> [u8; 4] {
        [
            (self.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.b.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.a.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }

    /// Parse `#rgb`, `#rrggbb` or `#rrggbbaa` (leading `#` optional).
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        let nyb = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
        let b = s.as_bytes();
        match b.len() {
            3 => {
                let (r, g, bl) = (nyb(b[0])?, nyb(b[1])?, nyb(b[2])?);
                Some(Rgba::from_u8(r * 17, g * 17, bl * 17, 255))
            }
            6 | 8 => {
                let byte = |i: usize| Some(nyb(b[i * 2])? * 16 + nyb(b[i * 2 + 1])?);
                let a = if b.len() == 8 { byte(3)? } else { 255 };
                Some(Rgba::from_u8(byte(0)?, byte(1)?, byte(2)?, a))
            }
            _ => None,
        }
    }

    /// `#rrggbb`, or `#rrggbbaa` when not fully opaque.
    pub fn to_hex(self) -> String {
        let [r, g, b, a] = self.to_u8();
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    pub fn premultiplied(self) -> Self {
        Self { r: self.r * self.a, g: self.g * self.a, b: self.b * self.a, a: self.a }
    }

    pub fn unpremultiplied(self) -> Self {
        if self.a <= f32::EPSILON {
            Rgba::TRANSPARENT
        } else {
            Self { r: self.r / self.a, g: self.g / self.a, b: self.b / self.a, a: self.a }
        }
    }

    /// Rec. 709 relative luminance. This is the weighting used for `Luminosity`
    /// blending, the luma histogram and the default Black & White mix, and it
    /// expects *linear* input.
    pub fn luminance(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    /// Interpret the components as encoded sRGB and convert to linear light.
    pub fn to_linear(self) -> Self {
        Self {
            r: srgb_to_linear(self.r),
            g: srgb_to_linear(self.g),
            b: srgb_to_linear(self.b),
            a: self.a,
        }
    }

    /// Inverse of [`Rgba::to_linear`].
    pub fn to_srgb(self) -> Self {
        Self {
            r: linear_to_srgb(self.r),
            g: linear_to_srgb(self.g),
            b: linear_to_srgb(self.b),
            a: self.a,
        }
    }

    pub fn lerp(self, other: Rgba, t: f32) -> Self {
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// sRGB electro-optical transfer function (encoded → linear).
///
/// Odd inputs are mirrored through the origin rather than clamped, so that
/// negative excursions produced by wide-gamut conversions round-trip instead of
/// collapsing to zero.
pub fn srgb_to_linear(c: f32) -> f32 {
    let s = c.signum();
    let c = c.abs();
    s * if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Inverse sRGB transfer function (linear → encoded).
pub fn linear_to_srgb(c: f32) -> f32 {
    let s = c.signum();
    let c = c.abs();
    s * if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// Working colour space of a document.
///
/// Pixelmator Pro is RGB-only (SPEC §6) — no CMYK or Lab documents — so this
/// enumerates RGB spaces plus a slot for an embedded ICC profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColorSpace {
    #[default]
    Srgb,
    DisplayP3,
    AdobeRgb,
    Rec2020,
    /// Linear-light sRGB primaries. Useful for compositing-heavy work.
    LinearSrgb,
    /// An ICC profile carried with the document, identified by its description.
    Icc {
        name: String,
    },
}

impl ColorSpace {
    pub const BUILT_IN: [ColorSpace; 5] = [
        ColorSpace::Srgb,
        ColorSpace::DisplayP3,
        ColorSpace::AdobeRgb,
        ColorSpace::Rec2020,
        ColorSpace::LinearSrgb,
    ];

    pub fn label(&self) -> &str {
        match self {
            ColorSpace::Srgb => "sRGB IEC61966-2.1",
            ColorSpace::DisplayP3 => "Display P3",
            ColorSpace::AdobeRgb => "Adobe RGB (1998)",
            ColorSpace::Rec2020 => "Rec. ITU-R BT.2020",
            ColorSpace::LinearSrgb => "Linear sRGB",
            ColorSpace::Icc { name } => name,
        }
    }

    /// Whether values in this space are already linear-light.
    pub fn is_linear(&self) -> bool {
        matches!(self, ColorSpace::LinearSrgb)
    }

    /// CIE 1931 xy chromaticities: `[red, green, blue, white]`.
    /// `None` for an embedded profile, whose primaries come from the ICC data.
    pub fn primaries(&self) -> Option<[[f32; 2]; 4]> {
        const D65: [f32; 2] = [0.3127, 0.3290];
        Some(match self {
            ColorSpace::Srgb | ColorSpace::LinearSrgb => {
                [[0.640, 0.330], [0.300, 0.600], [0.150, 0.060], D65]
            }
            ColorSpace::DisplayP3 => [[0.680, 0.320], [0.265, 0.690], [0.150, 0.060], D65],
            ColorSpace::AdobeRgb => [[0.640, 0.330], [0.210, 0.710], [0.150, 0.060], D65],
            ColorSpace::Rec2020 => [[0.708, 0.292], [0.170, 0.797], [0.131, 0.046], D65],
            ColorSpace::Icc { .. } => return None,
        })
    }
}

/// Bits per channel. Pixelmator Pro documents are 8- or 16-bit (SPEC §6); we
/// additionally allow 32-bit float, which costs nothing to support given the
/// renderer already works in floating point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BitDepth {
    #[default]
    Eight,
    Sixteen,
    ThirtyTwoFloat,
}

impl BitDepth {
    pub const ALL: [BitDepth; 3] =
        [BitDepth::Eight, BitDepth::Sixteen, BitDepth::ThirtyTwoFloat];

    pub fn label(self) -> &'static str {
        match self {
            BitDepth::Eight => "8 bits per channel",
            BitDepth::Sixteen => "16 bits per channel",
            BitDepth::ThirtyTwoFloat => "32 bits per channel (float)",
        }
    }

    pub fn bytes_per_pixel(self) -> usize {
        match self {
            BitDepth::Eight => 4,
            BitDepth::Sixteen => 8,
            BitDepth::ThirtyTwoFloat => 16,
        }
    }
}

/// HSL, with `h` in degrees 0..360 and `s`/`l` in 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsl {
    pub h: f32,
    pub s: f32,
    pub l: f32,
}

impl Hsl {
    pub fn from_rgb(c: Rgba) -> Self {
        let max = c.r.max(c.g).max(c.b);
        let min = c.r.min(c.g).min(c.b);
        let d = max - min;
        let l = (max + min) * 0.5;

        if d.abs() < 1e-7 {
            return Hsl { h: 0.0, s: 0.0, l };
        }

        let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-7);
        let h = if max == c.r {
            60.0 * (((c.g - c.b) / d) % 6.0)
        } else if max == c.g {
            60.0 * ((c.b - c.r) / d + 2.0)
        } else {
            60.0 * ((c.r - c.g) / d + 4.0)
        };
        Hsl { h: h.rem_euclid(360.0), s: s.clamp(0.0, 1.0), l }
    }

    pub fn to_rgb(self, alpha: f32) -> Rgba {
        let c = (1.0 - (2.0 * self.l - 1.0).abs()) * self.s;
        let h = self.h.rem_euclid(360.0) / 60.0;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = self.l - c * 0.5;
        Rgba::new(r + m, g + m, b + m, alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_transfer_round_trips() {
        for i in 0..=100 {
            let v = i as f32 / 100.0;
            assert!((linear_to_srgb(srgb_to_linear(v)) - v).abs() < 1e-5, "failed at {v}");
        }
    }

    #[test]
    fn srgb_transfer_hits_known_anchors() {
        assert!((srgb_to_linear(0.0)).abs() < 1e-6);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
        // Encoded mid-grey is much darker than half in linear light.
        assert!((srgb_to_linear(0.5) - 0.2140).abs() < 1e-3);
    }

    #[test]
    fn srgb_transfer_is_odd_for_negatives() {
        assert!((srgb_to_linear(-0.5) + srgb_to_linear(0.5)).abs() < 1e-6);
    }

    #[test]
    fn hex_parsing() {
        assert_eq!(Rgba::from_hex("#ff0000"), Some(Rgba::rgb(1.0, 0.0, 0.0)));
        assert_eq!(Rgba::from_hex("f00"), Some(Rgba::rgb(1.0, 0.0, 0.0)));
        assert_eq!(Rgba::from_hex("#00ff0080").unwrap().a, 128.0 / 255.0);
        assert_eq!(Rgba::from_hex("nope"), None);
        assert_eq!(Rgba::from_hex("#12345"), None);
    }

    #[test]
    fn hex_round_trips() {
        let c = Rgba::from_hex("#3a7fd5").unwrap();
        assert_eq!(c.to_hex(), "#3a7fd5");
        let t = Rgba::from_hex("#3a7fd580").unwrap();
        assert_eq!(t.to_hex(), "#3a7fd580");
    }

    #[test]
    fn premultiply_round_trips() {
        let c = Rgba::new(0.8, 0.4, 0.2, 0.5);
        let back = c.premultiplied().unpremultiplied();
        assert!((back.r - c.r).abs() < 1e-6 && (back.a - c.a).abs() < 1e-6);
        assert_eq!(Rgba::TRANSPARENT.premultiplied().unpremultiplied(), Rgba::TRANSPARENT);
    }

    #[test]
    fn hsl_round_trips() {
        for c in [
            Rgba::rgb(1.0, 0.0, 0.0),
            Rgba::rgb(0.0, 1.0, 0.0),
            Rgba::rgb(0.0, 0.0, 1.0),
            Rgba::rgb(0.3, 0.6, 0.9),
            Rgba::rgb(0.5, 0.5, 0.5),
        ] {
            let back = Hsl::from_rgb(c).to_rgb(1.0);
            assert!(
                (back.r - c.r).abs() < 1e-4
                    && (back.g - c.g).abs() < 1e-4
                    && (back.b - c.b).abs() < 1e-4,
                "{c:?} -> {back:?}"
            );
        }
    }

    #[test]
    fn greys_have_zero_saturation() {
        let hsl = Hsl::from_rgb(Rgba::rgb(0.42, 0.42, 0.42));
        assert!(hsl.s.abs() < 1e-6);
    }

    #[test]
    fn luminance_weights_sum_to_one() {
        assert!((Rgba::WHITE.luminance() - 1.0).abs() < 1e-6);
        assert!(Rgba::rgb(0.0, 1.0, 0.0).luminance() > Rgba::rgb(1.0, 0.0, 0.0).luminance());
    }
}
