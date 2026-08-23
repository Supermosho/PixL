//! A tiny reflection layer for adjustment and effect parameters.
//!
//! Pixelmator Pro exposes on the order of 17 adjustments and ~75 effects, most
//! of which are "a title and three sliders". Hand-writing a settings panel for
//! each one would be several thousand lines of near-identical GTK code that
//! then has to be kept in step with the shaders. Instead every parameterised
//! node describes itself through [`Parameterized`], and the UI builds its panel
//! from that description while the renderer packs the same values into a
//! uniform block.
//!
//! ## On numeric ranges
//!
//! Apple's user guide almost never publishes slider bounds (see `docs/SPEC.md`,
//! "Global caveat on numeric ranges"). The ranges below are therefore **our**
//! choices, picked to be defensible and symmetric, not transcriptions of
//! Pixelmator Pro. Where the guide *does* state a number, the spec is cited in
//! a comment. Anything calibrated against the real app should be marked as
//! such when it is measured.

use crate::color::Rgba;
use crate::curve::Curve;
use glam::Vec2;
use serde::{Deserialize, Serialize};

/// What kind of control the UI should build, and the bounds it enforces.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamKind {
    /// A continuous value. `min`/`max` are the hard limits the control clamps
    /// to; `soft_min`/`soft_max` are where the slider track ends. Pixelmator
    /// lets you Option-drag past the visible end of many sliders, and this pair
    /// is how we model that.
    Slider {
        min: f32,
        max: f32,
        soft_min: f32,
        soft_max: f32,
        default: f32,
        /// Displayed as a percentage rather than a raw number.
        percent: bool,
        /// Suffix such as `"px"` or `"°"`. Empty when `percent` is set.
        unit: &'static str,
    },
    /// An angle in degrees, drawn as a rotary control.
    Angle {
        default: f32,
    },
    Toggle {
        default: bool,
    },
    Color {
        default: Rgba,
    },
    /// A pop-up menu. The value is an index into `options`.
    Choice {
        options: &'static [&'static str],
        default: u32,
    },
    /// An on-canvas control point in normalised 0..1 layer coordinates —
    /// Pixelmator calls these "effect ropes".
    Point {
        default: Vec2,
    },
    /// A tone curve editor.
    Curve,
}

impl ParamKind {
    /// Convenience for the overwhelmingly common "−100%..+100%, centred at 0"
    /// slider.
    pub const fn bipolar_percent() -> Self {
        ParamKind::Slider {
            min: -4.0,
            max: 4.0,
            soft_min: -1.0,
            soft_max: 1.0,
            default: 0.0,
            percent: true,
            unit: "",
        }
    }

    /// "0%..100%, defaulting to full strength" — intensity and opacity.
    pub const fn unit_percent(default: f32) -> Self {
        ParamKind::Slider {
            min: 0.0,
            max: 1.0,
            soft_min: 0.0,
            soft_max: 1.0,
            default,
            percent: true,
            unit: "",
        }
    }

    /// A pixel radius.
    pub const fn radius(default: f32, soft_max: f32) -> Self {
        ParamKind::Slider {
            min: 0.0,
            max: 4096.0,
            soft_min: 0.0,
            soft_max,
            default,
            percent: false,
            unit: "px",
        }
    }

    pub fn default_value(&self) -> ParamValue {
        match *self {
            ParamKind::Slider { default, .. } => ParamValue::Float(default),
            ParamKind::Angle { default } => ParamValue::Float(default),
            ParamKind::Toggle { default } => ParamValue::Bool(default),
            ParamKind::Color { default } => ParamValue::Color(default),
            ParamKind::Choice { default, .. } => ParamValue::Index(default),
            ParamKind::Point { default } => ParamValue::Point(default),
            ParamKind::Curve => ParamValue::Curve(Curve::identity()),
        }
    }

    /// Clamp a value to this parameter's hard limits. Out-of-range values can
    /// arrive from a hand-edited document or a preset written by a future
    /// version, and silently clamping beats rejecting the file.
    pub fn clamp(&self, v: ParamValue) -> ParamValue {
        match (self, v) {
            (ParamKind::Slider { min, max, .. }, ParamValue::Float(f)) => {
                ParamValue::Float(f.clamp(*min, *max))
            }
            (ParamKind::Angle { .. }, ParamValue::Float(f)) => {
                ParamValue::Float(f.rem_euclid(360.0))
            }
            (ParamKind::Choice { options, .. }, ParamValue::Index(i)) => {
                ParamValue::Index(i.min(options.len().saturating_sub(1) as u32))
            }
            (_, other) => other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParamValue {
    Float(f32),
    Bool(bool),
    Color(Rgba),
    Index(u32),
    Point(Vec2),
    Curve(Curve),
}

impl ParamValue {
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            ParamValue::Float(f) => Some(*f),
            ParamValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            ParamValue::Index(i) => Some(*i as f32),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ParamValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<Rgba> {
        match self {
            ParamValue::Color(c) => Some(*c),
            _ => None,
        }
    }

    pub fn as_index(&self) -> Option<u32> {
        match self {
            ParamValue::Index(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_point(&self) -> Option<Vec2> {
        match self {
            ParamValue::Point(p) => Some(*p),
            _ => None,
        }
    }

    pub fn as_curve(&self) -> Option<&Curve> {
        match self {
            ParamValue::Curve(c) => Some(c),
            _ => None,
        }
    }
}

/// A single named control.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamSpec {
    /// Stable key used in the document format and as the shader uniform name.
    pub key: &'static str,
    /// Human-readable label, matching Pixelmator Pro's wording where known.
    pub label: &'static str,
    pub kind: ParamKind,
    /// Optional grouping header, for nodes with more controls than fit
    /// comfortably in one flat list.
    pub group: Option<&'static str>,
}

impl ParamSpec {
    pub const fn new(key: &'static str, label: &'static str, kind: ParamKind) -> Self {
        Self { key, label, kind, group: None }
    }

    pub const fn grouped(
        key: &'static str,
        label: &'static str,
        kind: ParamKind,
        group: &'static str,
    ) -> Self {
        Self { key, label, kind, group: Some(group) }
    }
}

/// Implemented by every adjustment and effect so the UI and the renderer can
/// treat them uniformly.
pub trait Parameterized {
    /// The controls this node exposes, in display order.
    fn specs(&self) -> Vec<ParamSpec>;

    fn get(&self, key: &str) -> Option<ParamValue>;

    /// Returns `false` if `key` is unknown or the value has the wrong type.
    fn set(&mut self, key: &str, value: ParamValue) -> bool;

    /// Restore every control to its default.
    fn reset(&mut self) {
        for spec in self.specs() {
            self.set(spec.key, spec.kind.default_value());
        }
    }

    /// True when this node would leave its input untouched, letting the render
    /// graph drop the pass entirely. Worth getting right: a freshly added
    /// adjustment sits at its defaults until the user drags something, and a
    /// stack of six such no-ops should cost nothing.
    fn is_identity(&self) -> bool {
        self.specs().iter().all(|spec| {
            self.get(spec.key).map(|v| v == spec.kind.default_value()).unwrap_or(true)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_clamps_to_hard_limits() {
        let k = ParamKind::unit_percent(1.0);
        assert_eq!(k.clamp(ParamValue::Float(5.0)), ParamValue::Float(1.0));
        assert_eq!(k.clamp(ParamValue::Float(-5.0)), ParamValue::Float(0.0));
    }

    #[test]
    fn bipolar_allows_option_drag_past_the_track() {
        let ParamKind::Slider { soft_max, max, .. } = ParamKind::bipolar_percent() else {
            panic!("expected a slider");
        };
        assert!(max > soft_max, "hard limit must leave room past the visible track");
    }

    #[test]
    fn angle_wraps_rather_than_clamping() {
        let k = ParamKind::Angle { default: 0.0 };
        assert_eq!(k.clamp(ParamValue::Float(370.0)), ParamValue::Float(10.0));
        assert_eq!(k.clamp(ParamValue::Float(-90.0)), ParamValue::Float(270.0));
    }

    #[test]
    fn choice_clamps_to_last_option() {
        let k = ParamKind::Choice { options: &["a", "b"], default: 0 };
        assert_eq!(k.clamp(ParamValue::Index(9)), ParamValue::Index(1));
    }
}
