//! The effect registry.
//!
//! Pixelmator Pro ships around 75 effects across ten categories
//! (`docs/SPEC.md` §2). Modelling each as its own Rust type would mean ~75
//! structs, ~75 `Parameterized` impls and ~75 UI panels for what is, in every
//! case, "a name and a handful of sliders". So effects are **data**: a static
//! descriptor table gives each one its category, label and parameter list, and
//! an [`Effect`] instance is just a descriptor id plus the values the user has
//! changed from their defaults.
//!
//! That buys three things. The settings panel is generated, so adding an effect
//! is a table row plus a shader. The document format serialises as
//! `{id, values}` and tolerates unknown ids from a newer version. And
//! [`EffectDescriptor::implemented`] makes the gap between "catalogued" and
//! "working" explicit rather than hiding it behind an effect that silently
//! does nothing.

use crate::color::Rgba;
use crate::param::{ParamKind, ParamSpec, ParamValue};
use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectCategory {
    Blur,
    Distortion,
    Sharpen,
    ColorAdjustment,
    Tile,
    Stylize,
    Halftone,
    Generator,
    Fill,
    Other,
}

impl EffectCategory {
    /// Display order of the Effects browser.
    pub const ALL: [EffectCategory; 10] = [
        EffectCategory::Blur,
        EffectCategory::Distortion,
        EffectCategory::Sharpen,
        EffectCategory::ColorAdjustment,
        EffectCategory::Tile,
        EffectCategory::Stylize,
        EffectCategory::Halftone,
        EffectCategory::Generator,
        EffectCategory::Fill,
        EffectCategory::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EffectCategory::Blur => "Blur",
            EffectCategory::Distortion => "Distortion",
            EffectCategory::Sharpen => "Sharpen",
            EffectCategory::ColorAdjustment => "Color Adjustment",
            EffectCategory::Tile => "Tile",
            EffectCategory::Stylize => "Stylize",
            EffectCategory::Halftone => "Halftone",
            EffectCategory::Generator => "Generator",
            EffectCategory::Fill => "Fill",
            EffectCategory::Other => "Other",
        }
    }
}

/// Static description of one effect.
pub struct EffectDescriptor {
    /// Stable id: the document-format tag and the shader-program key.
    pub id: &'static str,
    pub label: &'static str,
    pub category: EffectCategory,
    pub params: &'static [ParamSpec],
    /// Whether a shader for this effect exists yet. Effects with `false` are
    /// listed in the browser but disabled, so the catalogue stays honest about
    /// what actually renders.
    pub implemented: bool,
}

// Shorthands for the parameters that recur across dozens of effects.
const fn radius(default: f32, soft_max: f32) -> ParamKind {
    ParamKind::radius(default, soft_max)
}

const fn amount(default: f32) -> ParamKind {
    ParamKind::unit_percent(default)
}

const fn angle(default: f32) -> ParamKind {
    ParamKind::Angle { default }
}

const fn centre() -> ParamKind {
    ParamKind::Point { default: Vec2::new(0.5, 0.5) }
}

const fn width(default: f32) -> ParamKind {
    ParamKind::Slider {
        min: 0.5,
        max: 2048.0,
        soft_min: 1.0,
        soft_max: 200.0,
        default,
        percent: false,
        unit: "px",
    }
}

const fn scale(default: f32) -> ParamKind {
    ParamKind::Slider {
        min: 0.0,
        max: 64.0,
        soft_min: 0.0,
        soft_max: 4.0,
        default,
        percent: true,
        unit: "",
    }
}

macro_rules! effects {
    ($($cat:ident $id:literal $label:literal $impl:literal [
        $($key:literal $plabel:literal $kind:expr),* $(,)?
    ]);* $(;)?) => {
        pub static EFFECTS: &[EffectDescriptor] = &[
            $(EffectDescriptor {
                id: $id,
                label: $label,
                category: EffectCategory::$cat,
                implemented: $impl,
                params: &[$(ParamSpec::new($key, $plabel, $kind)),*],
            }),*
        ];
    };
}

// The roster below is transcribed from SPEC §2. Parameter *names* are Apple's;
// ranges and defaults are ours (see the note in `crate::param`).
effects! {
    // -- Blur ------------------------------------------------------------
    Blur "gaussian-blur" "Gaussian" true ["radius" "Radius" radius(10.0, 100.0)];
    Blur "box-blur" "Box" true ["radius" "Radius" radius(10.0, 100.0)];
    Blur "disc-blur" "Disc" true ["radius" "Radius" radius(10.0, 100.0)];
    Blur "motion-blur" "Motion" true [
        "radius" "Radius" radius(20.0, 200.0),
        "angle" "Angle" angle(0.0),
    ];
    Blur "zoom-blur" "Zoom" true [
        "amount" "Amount" amount(0.3),
        "center" "Center" centre(),
    ];
    Blur "spin-blur" "Spin" true [
        "amount" "Amount" amount(0.3),
        "center" "Center" centre(),
    ];
    Blur "bokeh-blur" "Bokeh" false [
        "radius" "Radius" radius(20.0, 100.0),
        "ring_amount" "Ring Amount" amount(0.3),
        "ring_size" "Ring Size" amount(0.3),
    ];
    Blur "tilt-shift" "Tilt-Shift" true [
        "transition" "Transition" amount(0.3),
        "center" "Center" centre(),
        "angle" "Angle" angle(0.0),
        "radius" "Radius" radius(20.0, 100.0),
    ];
    Blur "focus-blur" "Focus" true [
        "transition" "Transition" amount(0.3),
        "center" "Center" centre(),
        "radius" "Radius" radius(20.0, 100.0),
    ];

    // -- Distortion ------------------------------------------------------
    Distortion "bump-distort" "Bump" true [
        "radius" "Radius" radius(200.0, 1000.0),
        "scale" "Scale" ParamKind::bipolar_percent(),
        "center" "Center" centre(),
    ];
    Distortion "pinch-distort" "Pinch" true [
        "radius" "Radius" radius(200.0, 1000.0),
        "scale" "Scale" ParamKind::bipolar_percent(),
        "center" "Center" centre(),
    ];
    Distortion "twirl-distort" "Twirl" true [
        "radius" "Radius" radius(200.0, 1000.0),
        "angle" "Angle" angle(90.0),
        "center" "Center" centre(),
    ];
    Distortion "vortex-distort" "Vortex" false [
        "radius" "Radius" radius(200.0, 1000.0),
        "amount" "Amount" ParamKind::bipolar_percent(),
        "center" "Center" centre(),
    ];
    Distortion "displacement-map" "Displacement Map" false [
        "scale" "Scale" scale(1.0),
        "angle" "Angle" angle(0.0),
        "smoothness" "Smoothness" amount(0.5),
    ];
    Distortion "circle-splash" "Circle Splash" false [
        "radius" "Radius" radius(150.0, 1000.0),
        "center" "Center" centre(),
    ];
    Distortion "hole-distort" "Hole" false [
        "radius" "Radius" radius(150.0, 1000.0),
        "center" "Center" centre(),
    ];
    Distortion "light-tunnel" "Light Tunnel" false [
        "rotation" "Rotation" angle(0.0),
        "center" "Center" centre(),
    ];

    // -- Sharpen ---------------------------------------------------------
    Sharpen "sharpen" "Sharpen" true [
        "radius" "Radius" radius(1.5, 20.0),
        "intensity" "Intensity" amount(0.5),
    ];
    Sharpen "sharpen-luminance" "Sharpen Luminance" true [
        "radius" "Radius" radius(1.5, 20.0),
        "sharpness" "Sharpness" amount(0.5),
    ];

    // -- Color Adjustment ------------------------------------------------
    ColorAdjustment "exposure-effect" "Exposure" true [
        "ev" "EV" ParamKind::Slider {
            min: -10.0, max: 10.0, soft_min: -4.0, soft_max: 4.0,
            default: 0.0, percent: false, unit: "EV",
        },
    ];
    ColorAdjustment "color-controls" "Color Controls" true [
        "saturation" "Saturation" ParamKind::bipolar_percent(),
        "brightness" "Brightness" ParamKind::bipolar_percent(),
        "contrast" "Contrast" ParamKind::bipolar_percent(),
    ];
    ColorAdjustment "hue-adjust" "Hue Adjust" true ["angle" "Angle" angle(0.0)];
    ColorAdjustment "color-monochrome" "Color Monochrome" true [
        "color" "Color" ParamKind::Color { default: Rgba::rgb(0.6, 0.45, 0.3) },
        "intensity" "Intensity" amount(1.0),
    ];
    ColorAdjustment "sepia-tone" "Sepia Tone" true ["intensity" "Intensity" amount(1.0)];
    ColorAdjustment "false-color" "False Color" true [
        "color0" "Color 0" ParamKind::Color { default: Rgba::BLACK },
        "color1" "Color 1" ParamKind::Color { default: Rgba::WHITE },
    ];
    ColorAdjustment "gradient-map" "Gradient Map" true [
        "opacity" "Opacity" amount(1.0),
    ];
    ColorAdjustment "invert-effect" "Invert" true [];
    ColorAdjustment "threshold" "Threshold" true ["threshold" "Threshold" amount(0.5)];

    // -- Tile ------------------------------------------------------------
    Tile "kaleidoscope" "Kaleidoscope" true [
        "angle" "Angle" angle(0.0),
        "width" "Width" width(100.0),
        "count" "Count" ParamKind::Slider {
            min: 2.0, max: 64.0, soft_min: 2.0, soft_max: 24.0,
            default: 6.0, percent: false, unit: "",
        },
        "center" "Center" centre(),
    ];
    Tile "triangle-kaleidoscope" "Triangle Kaleidoscope" false [
        "size" "Size" width(100.0),
        "decay" "Decay" amount(0.85),
        "rotation" "Rotation" angle(0.0),
    ];
    Tile "snowflake-tile" "Snowflake" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
    ];
    Tile "tessera-tile" "Tessera" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
        "acute_angle" "Acute Angle" angle(60.0),
    ];
    Tile "pinwheel-tile" "Pinwheel" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
    ];
    Tile "shutters-tile" "Shutters" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
        "acute_angle" "Acute Angle" angle(60.0),
    ];
    Tile "brickwork-tile" "Brickwork" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
    ];
    Tile "op-tile" "Op" false ["scale" "Scale" scale(1.0)];
    Tile "funhouse-tile" "Funhouse" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
        "acute_angle" "Acute Angle" angle(60.0),
    ];
    Tile "lattice-tile" "Lattice" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
    ];
    Tile "windmill-tile" "Windmill" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
    ];
    Tile "triangle-tile" "Triangle Tiles" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
    ];
    Tile "hexagon-tile" "Hexagon" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
    ];
    Tile "affine-tile" "Affine Tile" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
        "scale" "Scale" scale(1.0),
        "stretch" "Stretch" scale(1.0),
        "skew" "Skew" ParamKind::bipolar_percent(),
    ];
    Tile "perspective-tile" "Perspective Tile" false [
        "angle" "Angle" angle(0.0), "width" "Width" width(100.0),
    ];

    // -- Stylize ---------------------------------------------------------
    Stylize "light-leak" "Light Leak" false [
        "amount" "Amount" amount(0.5),
        "sunniness" "Sunniness" amount(0.5),
    ];
    Stylize "bokeh-stylize" "Bokeh" false [
        "amount" "Amount" amount(0.5),
        "hue" "Hue" angle(0.0),
    ];
    Stylize "vignette-effect" "Vignette" true [
        "radius" "Radius" amount(0.7),
        "intensity" "Intensity" amount(0.5),
        "falloff" "Falloff" amount(0.5),
    ];
    Stylize "pixelate" "Pixelate" true ["scale" "Scale" width(12.0)];
    Stylize "pointillize" "Pointillize" false ["radius" "Radius" radius(10.0, 60.0)];
    Stylize "crystallize" "Crystallize" true ["radius" "Radius" radius(10.0, 60.0)];
    Stylize "bloom" "Bloom" true [
        "radius" "Radius" radius(20.0, 100.0),
        "intensity" "Intensity" amount(0.5),
    ];
    Stylize "gloom" "Gloom" true [
        "radius" "Radius" radius(20.0, 100.0),
        "intensity" "Intensity" amount(0.5),
    ];
    Stylize "spot-light" "Spot Light" false [
        "radius" "Radius" radius(200.0, 1000.0),
        "light_color" "Light Color" ParamKind::Color { default: Rgba::WHITE },
        "background_color" "Background Color" ParamKind::Color { default: Rgba::BLACK },
        "concentration" "Concentration" amount(0.5),
        "center" "Center" centre(),
    ];
    Stylize "posterize" "Posterize" true [
        "levels" "Levels" ParamKind::Slider {
            min: 2.0, max: 64.0, soft_min: 2.0, soft_max: 16.0,
            default: 6.0, percent: false, unit: "",
        },
    ];
    Stylize "grain-effect" "Grain" true [
        "intensity" "Intensity" amount(0.3),
        "size" "Size" scale(1.0),
    ];
    Stylize "noise-effect" "Noise" true [
        "amount" "Amount" amount(0.3),
        "monochrome" "Monochrome" ParamKind::Toggle { default: false },
    ];
    Stylize "comics" "Comics" false [];

    // -- Halftone --------------------------------------------------------
    Halftone "circular-screen" "Circular Screen" true [
        "width" "Width" width(6.0),
        "sharpness" "Sharpness" amount(0.7),
        "center" "Center" centre(),
    ];
    Halftone "cmyk-halftone" "CMYK Halftone" false [
        "width" "Width" width(6.0),
        "sharpness" "Sharpness" amount(0.7),
        "angle" "Angle" angle(0.0),
        "gcr" "Gray Component Replacement" amount(1.0),
        "ucr" "Under Color Removal" amount(0.5),
    ];
    Halftone "dot-screen" "Dot Screen" true [
        "width" "Width" width(6.0),
        "sharpness" "Sharpness" amount(0.7),
        "angle" "Angle" angle(0.0),
    ];
    Halftone "hatched-screen" "Hatched Screen" true [
        "width" "Width" width(6.0),
        "sharpness" "Sharpness" amount(0.7),
        "angle" "Angle" angle(0.0),
    ];
    Halftone "line-screen" "Line Screen" true [
        "width" "Width" width(6.0),
        "sharpness" "Sharpness" amount(0.7),
        "angle" "Angle" angle(0.0),
    ];

    // -- Generator -------------------------------------------------------
    Generator "checkerboard" "Checkerboard" true [
        "color" "Color" ParamKind::Color { default: Rgba::BLACK },
        "width" "Width" width(32.0),
        "sharpness" "Sharpness" amount(1.0),
        "opacity" "Opacity" amount(1.0),
    ];
    Generator "stripes" "Stripes" true [
        "color" "Color" ParamKind::Color { default: Rgba::BLACK },
        "width" "Width" width(32.0),
        "sharpness" "Sharpness" amount(1.0),
        "angle" "Angle" angle(0.0),
        "opacity" "Opacity" amount(1.0),
    ];
    Generator "halo" "Halo" false [
        "color" "Color" ParamKind::Color { default: Rgba::WHITE },
        "halo_width" "Halo Width" width(60.0),
        "halo_radius" "Halo Radius" radius(200.0, 1000.0),
        "halo_overlap" "Halo Overlap" amount(0.5),
        "striation_strength" "Striation Strength" amount(0.5),
        "striation_contrast" "Striation Contrast" amount(0.5),
        "opacity" "Opacity" amount(1.0),
    ];
    Generator "star-generator" "Star" false [
        "color" "Color" ParamKind::Color { default: Rgba::WHITE },
        "cross_width" "Cross Width" width(4.0),
        "radius" "Radius" radius(60.0, 500.0),
        "cross_scale" "Cross Scale" scale(1.0),
        "cross_angle" "Cross Angle" angle(0.0),
        "cross_opacity" "Cross Opacity" amount(0.5),
        "opacity" "Opacity" amount(1.0),
    ];
    Generator "sunbeams" "Sunbeams" false [
        "color" "Color" ParamKind::Color { default: Rgba::WHITE },
        "sun_radius" "Sun Radius" radius(40.0, 400.0),
        "max_striation_radius" "Maximum Striation Radius" radius(300.0, 1000.0),
        "striation_strength" "Striation Strength" amount(0.5),
        "striation_contrast" "Striation Contrast" amount(0.5),
        "opacity" "Opacity" amount(1.0),
    ];
    Generator "clouds" "Clouds" true [
        "color" "Color" ParamKind::Color { default: Rgba::WHITE },
        "width" "Width" width(200.0),
        "opacity" "Opacity" amount(1.0),
    ];

    // -- Fill ------------------------------------------------------------
    Fill "fill-color" "Color" true [
        "color" "Color" ParamKind::Color { default: Rgba::rgb(0.0, 0.45, 0.95) },
        "opacity" "Opacity" amount(1.0),
    ];
    Fill "fill-gradient" "Gradient" true [
        "scale" "Scale" scale(1.0),
        "angle" "Angle" angle(0.0),
        "opacity" "Opacity" amount(1.0),
    ];
    Fill "fill-pattern" "Pattern" false [
        "scale" "Scale" scale(1.0),
        "angle" "Angle" angle(0.0),
        "opacity" "Opacity" amount(1.0),
    ];
    Fill "fill-image" "Image" false [
        "scale" "Scale" scale(1.0),
        "angle" "Angle" angle(0.0),
        "opacity" "Opacity" amount(1.0),
    ];

    // -- Other -----------------------------------------------------------
    Other "perspective-transform" "Perspective Transform" false [];
    Other "mask-to-alpha" "Mask to Alpha" true [];
    Other "high-pass" "High Pass" true [
        "radius" "Radius" radius(10.0, 100.0),
        "opacity" "Opacity" amount(1.0),
    ];
    Other "low-pass" "Low Pass" true [
        "radius" "Radius" radius(10.0, 100.0),
        "opacity" "Opacity" amount(1.0),
    ];
    Other "frequency-separation" "Frequency Separation" false [
        "high_pass" "High Pass" radius(4.0, 50.0),
        "low_pass" "Low Pass" radius(10.0, 100.0),
        "opacity" "Opacity" amount(1.0),
    ];
}

pub fn descriptor(id: &str) -> Option<&'static EffectDescriptor> {
    EFFECTS.iter().find(|d| d.id == id)
}

pub fn descriptors_in(
    category: EffectCategory,
) -> impl Iterator<Item = &'static EffectDescriptor> {
    EFFECTS.iter().filter(move |d| d.category == category)
}

/// How many effects currently have a shader behind them, out of the full
/// catalogue. Surfaced in the about window so the number cannot quietly rot.
pub fn implemented_count() -> (usize, usize) {
    (EFFECTS.iter().filter(|d| d.implemented).count(), EFFECTS.len())
}

/// One configured effect on a layer or effects layer.
///
/// Only values that differ from the descriptor's defaults are stored, which
/// keeps documents small and lets a later version change a default without
/// rewriting every file that used it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub id: String,
    pub enabled: bool,
    values: BTreeMap<String, ParamValue>,
}

impl Effect {
    pub fn new(id: &str) -> Option<Self> {
        descriptor(id)?;
        Some(Self { id: id.to_string(), enabled: true, values: BTreeMap::new() })
    }

    pub fn descriptor(&self) -> Option<&'static EffectDescriptor> {
        descriptor(&self.id)
    }

    pub fn label(&self) -> &str {
        self.descriptor().map(|d| d.label).unwrap_or(&self.id)
    }

    pub fn specs(&self) -> &'static [ParamSpec] {
        self.descriptor().map(|d| d.params).unwrap_or(&[])
    }

    /// Current value of a parameter, falling back to the descriptor default.
    pub fn get(&self, key: &str) -> Option<ParamValue> {
        if let Some(v) = self.values.get(key) {
            return Some(v.clone());
        }
        self.specs().iter().find(|s| s.key == key).map(|s| s.kind.default_value())
    }

    pub fn set(&mut self, key: &str, value: ParamValue) -> bool {
        let Some(spec) = self.specs().iter().find(|s| s.key == key) else {
            return false;
        };
        let clamped = spec.kind.clamp(value);
        if clamped == spec.kind.default_value() {
            self.values.remove(key);
        } else {
            self.values.insert(key.to_string(), clamped);
        }
        true
    }

    pub fn reset(&mut self) {
        self.values.clear();
    }

    /// True when this effect is off, unknown, or entirely at its defaults.
    ///
    /// Note that "at defaults" is not the same as "does nothing" for
    /// generators: a Checkerboard at its defaults still draws a checkerboard.
    /// Only effects that transform their input can be skipped.
    pub fn is_noop(&self) -> bool {
        if !self.enabled {
            return true;
        }
        match self.descriptor() {
            None => true,
            Some(d) if !d.implemented => true,
            Some(d) => {
                let transforms_input =
                    !matches!(d.category, EffectCategory::Generator | EffectCategory::Fill);
                transforms_input && self.values.is_empty() && !d.params.is_empty()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_id_is_unique() {
        let mut seen = std::collections::HashSet::new();
        for d in EFFECTS {
            assert!(seen.insert(d.id), "duplicate effect id {}", d.id);
        }
    }

    #[test]
    fn every_effect_param_key_is_unique_within_its_effect() {
        for d in EFFECTS {
            let mut seen = std::collections::HashSet::new();
            for p in d.params {
                assert!(seen.insert(p.key), "{}: duplicate param {}", d.id, p.key);
            }
        }
    }

    #[test]
    fn catalogue_covers_every_category() {
        for cat in EffectCategory::ALL {
            assert!(descriptors_in(cat).next().is_some(), "{cat:?} has no effects");
        }
    }

    #[test]
    fn catalogue_size_matches_the_spec() {
        // SPEC §2 enumerates roughly 75 effects; guard against accidental
        // deletions from the table.
        assert!(EFFECTS.len() >= 70, "catalogue shrank to {}", EFFECTS.len());
        let (done, total) = implemented_count();
        assert!(done > 0 && done <= total);
    }

    #[test]
    fn unknown_effect_ids_are_rejected() {
        assert!(Effect::new("no-such-effect").is_none());
        assert!(Effect::new("gaussian-blur").is_some());
    }

    #[test]
    fn values_fall_back_to_defaults() {
        let e = Effect::new("gaussian-blur").unwrap();
        assert_eq!(e.get("radius"), Some(ParamValue::Float(10.0)));
        assert_eq!(e.get("nope"), None);
    }

    #[test]
    fn setting_back_to_default_drops_the_override() {
        let mut e = Effect::new("gaussian-blur").unwrap();
        assert!(e.set("radius", ParamValue::Float(25.0)));
        assert_eq!(e.values.len(), 1);
        e.set("radius", ParamValue::Float(10.0));
        assert!(e.values.is_empty(), "default value should not be stored");
    }

    #[test]
    fn untouched_transforming_effects_are_no_ops() {
        let mut e = Effect::new("gaussian-blur").unwrap();
        assert!(e.is_noop());
        e.set("radius", ParamValue::Float(25.0));
        assert!(!e.is_noop());
        e.enabled = false;
        assert!(e.is_noop());
    }

    #[test]
    fn generators_are_never_no_ops_at_defaults() {
        let e = Effect::new("checkerboard").unwrap();
        assert!(!e.is_noop(), "a generator at defaults still draws something");
    }

    #[test]
    fn parameterless_effects_are_not_no_ops() {
        let e = Effect::new("invert-effect").unwrap();
        assert!(!e.is_noop());
    }

    #[test]
    fn unimplemented_effects_are_skipped_by_the_renderer() {
        let mut e = Effect::new("comics").unwrap();
        e.enabled = true;
        assert!(e.is_noop(), "unimplemented effects must not affect output");
    }

    #[test]
    fn set_clamps_out_of_range_values() {
        let mut e = Effect::new("posterize").unwrap();
        e.set("levels", ParamValue::Float(9999.0));
        assert_eq!(e.get("levels"), Some(ParamValue::Float(64.0)));
    }
}
