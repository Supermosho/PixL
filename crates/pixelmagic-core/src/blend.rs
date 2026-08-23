//! Blend modes.
//!
//! The roster and grouping mirror Pixelmator Pro's blend-mode pop-up exactly
//! (see `docs/SPEC.md` §1): 26 modes in six functional groups. The discriminant
//! of each variant is the index handed to the compositing shader, so the order
//! here is load-bearing — `composite.frag` switches on it directly.

use serde::{Deserialize, Serialize};

/// Functional grouping used to build separators in the blend-mode menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlendGroup {
    Normal,
    Darkening,
    Lightening,
    Contrast,
    Comparative,
    Component,
}

impl BlendGroup {
    pub const ALL: [BlendGroup; 6] = [
        BlendGroup::Normal,
        BlendGroup::Darkening,
        BlendGroup::Lightening,
        BlendGroup::Contrast,
        BlendGroup::Comparative,
        BlendGroup::Component,
    ];

    /// Menu heading. `Normal` is ungrouped in the UI, hence the empty label.
    pub fn label(self) -> &'static str {
        match self {
            BlendGroup::Normal => "",
            BlendGroup::Darkening => "Darkening",
            BlendGroup::Lightening => "Lightening",
            BlendGroup::Contrast => "Contrast",
            BlendGroup::Comparative => "Comparative",
            BlendGroup::Component => "Component",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(u32)]
pub enum BlendMode {
    #[default]
    Normal = 0,

    // Darkening
    Darken = 1,
    Multiply = 2,
    ColorBurn = 3,
    LinearBurn = 4,
    DarkerColor = 5,

    // Lightening
    Lighten = 6,
    Screen = 7,
    ColorDodge = 8,
    LinearDodge = 9,
    LighterColor = 10,

    // Contrast
    Overlay = 11,
    SoftLight = 12,
    HardLight = 13,
    VividLight = 14,
    LinearLight = 15,
    PinLight = 16,
    HardMix = 17,

    // Comparative
    Difference = 18,
    Exclusion = 19,
    Subtract = 20,
    Divide = 21,

    // Component
    Hue = 22,
    Saturation = 23,
    Color = 24,
    Luminosity = 25,
}

impl BlendMode {
    pub const ALL: [BlendMode; 26] = [
        BlendMode::Normal,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::ColorBurn,
        BlendMode::LinearBurn,
        BlendMode::DarkerColor,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::ColorDodge,
        BlendMode::LinearDodge,
        BlendMode::LighterColor,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::VividLight,
        BlendMode::LinearLight,
        BlendMode::PinLight,
        BlendMode::HardMix,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Subtract,
        BlendMode::Divide,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];

    /// Index passed to the compositing shader as `u_blend_mode`.
    pub fn shader_index(self) -> u32 {
        self as u32
    }

    pub fn from_shader_index(i: u32) -> Option<Self> {
        Self::ALL.get(i as usize).copied()
    }

    pub fn group(self) -> BlendGroup {
        use BlendMode::*;
        match self {
            Normal => BlendGroup::Normal,
            Darken | Multiply | ColorBurn | LinearBurn | DarkerColor => BlendGroup::Darkening,
            Lighten | Screen | ColorDodge | LinearDodge | LighterColor => {
                BlendGroup::Lightening
            }
            Overlay | SoftLight | HardLight | VividLight | LinearLight | PinLight | HardMix => {
                BlendGroup::Contrast
            }
            Difference | Exclusion | Subtract | Divide => BlendGroup::Comparative,
            Hue | Saturation | Color | Luminosity => BlendGroup::Component,
        }
    }

    /// Display name, matching Pixelmator Pro's menu wording.
    pub fn label(self) -> &'static str {
        use BlendMode::*;
        match self {
            Normal => "Normal",
            Darken => "Darken",
            Multiply => "Multiply",
            ColorBurn => "Color Burn",
            LinearBurn => "Linear Burn",
            DarkerColor => "Darker Color",
            Lighten => "Lighten",
            Screen => "Screen",
            ColorDodge => "Color Dodge",
            LinearDodge => "Linear Dodge",
            LighterColor => "Lighter Color",
            Overlay => "Overlay",
            SoftLight => "Soft Light",
            HardLight => "Hard Light",
            VividLight => "Vivid Light",
            LinearLight => "Linear Light",
            PinLight => "Pin Light",
            HardMix => "Hard Mix",
            Difference => "Difference",
            Exclusion => "Exclusion",
            Subtract => "Subtract",
            Divide => "Divide",
            Hue => "Hue",
            Saturation => "Saturation",
            Color => "Color",
            Luminosity => "Luminosity",
        }
    }

    /// Stable identifier used in the on-disk document format. Renaming a label
    /// must never change this.
    pub fn id(self) -> &'static str {
        use BlendMode::*;
        match self {
            Normal => "normal",
            Darken => "darken",
            Multiply => "multiply",
            ColorBurn => "color-burn",
            LinearBurn => "linear-burn",
            DarkerColor => "darker-color",
            Lighten => "lighten",
            Screen => "screen",
            ColorDodge => "color-dodge",
            LinearDodge => "linear-dodge",
            LighterColor => "lighter-color",
            Overlay => "overlay",
            SoftLight => "soft-light",
            HardLight => "hard-light",
            VividLight => "vivid-light",
            LinearLight => "linear-light",
            PinLight => "pin-light",
            HardMix => "hard-mix",
            Difference => "difference",
            Exclusion => "exclusion",
            Subtract => "subtract",
            Divide => "divide",
            Hue => "hue",
            Saturation => "saturation",
            Color => "color",
            Luminosity => "luminosity",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    /// `Shift-+` / `Shift--` cycle through modes (SPEC §5, Style tool shortcuts).
    pub fn next(self) -> Self {
        let i = (self as usize + 1) % Self::ALL.len();
        Self::ALL[i]
    }

    pub fn prev(self) -> Self {
        let i = (self as usize + Self::ALL.len() - 1) % Self::ALL.len();
        Self::ALL[i]
    }

    /// True for the separable modes — those whose result can be computed per
    /// channel. The non-separable ones (`Hue`/`Saturation`/`Color`/
    /// `Luminosity`) plus the two "whole-colour comparison" modes need the full
    /// RGB triple, which matters when we split work per channel.
    pub fn is_separable(self) -> bool {
        use BlendMode::*;
        !matches!(self, Hue | Saturation | Color | Luminosity | DarkerColor | LighterColor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_indices_are_dense_and_ordered() {
        for (i, m) in BlendMode::ALL.iter().enumerate() {
            assert_eq!(m.shader_index() as usize, i, "{m:?} out of order");
            assert_eq!(BlendMode::from_shader_index(i as u32), Some(*m));
        }
    }

    #[test]
    fn ids_round_trip_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for m in BlendMode::ALL {
            assert!(seen.insert(m.id()), "duplicate id {}", m.id());
            assert_eq!(BlendMode::from_id(m.id()), Some(m));
        }
    }

    #[test]
    fn cycling_visits_every_mode() {
        let mut m = BlendMode::Normal;
        for _ in 0..BlendMode::ALL.len() {
            m = m.next();
        }
        assert_eq!(m, BlendMode::Normal);
        assert_eq!(BlendMode::Normal.prev(), BlendMode::Luminosity);
    }
}
