//! Colour adjustments.
//!
//! One variant per section of Pixelmator Pro's Color Adjustments pane
//! (`docs/SPEC.md` §3). Control **names** are transcribed from Apple's guide;
//! control **ranges** are ours, because the guide does not publish them — see
//! the note at the top of [`crate::param`].
//!
//! Adjustments are non-destructive: they hang off a layer (or off a standalone
//! adjustment layer that affects everything beneath it) and are evaluated by
//! the render graph on every frame, never baked into pixels until export.

use crate::color::Rgba;
use crate::curve::Curve;
use crate::param::{ParamKind, ParamSpec, ParamValue, Parameterized};
use crate::parameterized;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Simple adjustments
// ---------------------------------------------------------------------------

parameterized! {
    /// SPEC §3.3. `Temperature` cools left / warms right; `Tint` runs green to
    /// magenta. Both are Option-draggable past the visible track.
    pub struct WhiteBalance {
        temperature: f32 = "temperature", "Temperature", ParamKind::bipolar_percent();
        tint: f32 = "tint", "Tint", ParamKind::bipolar_percent();
    }
}

parameterized! {
    /// SPEC §3.4.
    pub struct HueSaturation {
        hue: f32 = "hue", "Hue", ParamKind::Angle { default: 0.0 };
        saturation: f32 = "saturation", "Saturation", ParamKind::bipolar_percent();
        vibrance: f32 = "vibrance", "Vibrance", ParamKind::bipolar_percent();
    }
}

parameterized! {
    /// SPEC §3.5 — the eight-slider `Basic` section. Note there is no separate
    /// `Whites`/`Blacks` pair, only `Black Point`.
    pub struct Basic {
        exposure: f32 = "exposure", "Exposure", ParamKind::bipolar_percent();
        highlights: f32 = "highlights", "Highlights", ParamKind::bipolar_percent();
        shadows: f32 = "shadows", "Shadows", ParamKind::bipolar_percent();
        brightness: f32 = "brightness", "Brightness", ParamKind::bipolar_percent();
        contrast: f32 = "contrast", "Contrast", ParamKind::bipolar_percent();
        black_point: f32 = "black_point", "Black Point", ParamKind::bipolar_percent();
        texture: f32 = "texture", "Texture", ParamKind::bipolar_percent();
        clarity: f32 = "clarity", "Clarity", ParamKind::bipolar_percent();
    }
}

parameterized! {
    /// SPEC §3.7. Applies clarity and texture to one tonal range at a time.
    pub struct SelectiveClarity {
        range: u32 = "range", "Range",
            ParamKind::Choice { options: &["Shadows", "Midtones", "Highlights"], default: 1 };
        clarity: f32 = "clarity", "Clarity", ParamKind::bipolar_percent();
        texture: f32 = "texture", "Texture", ParamKind::bipolar_percent();
    }
}

parameterized! {
    /// SPEC §3.11.
    pub struct Vignette {
        exposure: f32 = "exposure", "Exposure", ParamKind::bipolar_percent();
        black_point: f32 = "black_point", "Black Point", ParamKind::bipolar_percent();
        softness: f32 = "softness", "Softness", ParamKind::unit_percent(0.5);
    }
}

parameterized! {
    /// SPEC §3.11.
    pub struct Grain {
        size: f32 = "size", "Size", ParamKind::Slider {
            min: 0.0, max: 8.0, soft_min: 0.0, soft_max: 2.0,
            default: 1.0, percent: true, unit: "",
        };
        intensity: f32 = "intensity", "Intensity", ParamKind::unit_percent(0.0);
    }
}

parameterized! {
    /// SPEC §3.12. The guide's stated working ranges (0.5–2 px for portraits,
    /// 3–10 px for landscapes) inform the soft maximum.
    pub struct SharpenAdjust {
        radius: f32 = "radius", "Radius", ParamKind::Slider {
            min: 0.0, max: 100.0, soft_min: 0.0, soft_max: 10.0,
            default: 1.0, percent: false, unit: "px",
        };
        intensity: f32 = "intensity", "Intensity", ParamKind::unit_percent(0.0);
    }
}

parameterized! {
    /// SPEC §3.13. The guide advises keeping R+G+B at or below 100%.
    pub struct BlackAndWhite {
        red: f32 = "red", "Red", ParamKind::Slider {
            min: -2.0, max: 2.0, soft_min: 0.0, soft_max: 1.0,
            default: 0.2126, percent: true, unit: "",
        };
        green: f32 = "green", "Green", ParamKind::Slider {
            min: -2.0, max: 2.0, soft_min: 0.0, soft_max: 1.0,
            default: 0.7152, percent: true, unit: "",
        };
        blue: f32 = "blue", "Blue", ParamKind::Slider {
            min: -2.0, max: 2.0, soft_min: 0.0, soft_max: 1.0,
            default: 0.0722, percent: true, unit: "",
        };
        tone: f32 = "tone", "Tone", ParamKind::unit_percent(0.0);
        intensity: f32 = "intensity", "Intensity", ParamKind::unit_percent(1.0);
    }
}

parameterized! {
    /// SPEC §3.14. Works on images and video alike.
    pub struct ReplaceColor {
        source: Rgba = "source", "Source", ParamKind::Color { default: Rgba::WHITE };
        target: Rgba = "target", "Target", ParamKind::Color { default: Rgba::WHITE };
        range: f32 = "range", "Range", ParamKind::unit_percent(0.25);
        intensity: f32 = "intensity", "Intensity", ParamKind::unit_percent(1.0);
    }
}

parameterized! {
    /// SPEC §3.15.
    pub struct Invert {
        intensity: f32 = "intensity", "Intensity", ParamKind::unit_percent(1.0);
    }
}

// ---------------------------------------------------------------------------
// Colour balance
// ---------------------------------------------------------------------------

/// One tonal range's worth of colour-balance controls: a wheel (stored as its
/// Cartesian offset, which is what the shader wants) plus the three
/// complementary-pair sliders and the wheel's own brightness and saturation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct BalanceWheel {
    /// Wheel handle offset from centre, each axis in −1..=1.
    pub offset_x: f32,
    pub offset_y: f32,
    pub red_cyan: f32,
    pub green_magenta: f32,
    pub yellow_blue: f32,
    pub brightness: f32,
    pub saturation: f32,
}

impl BalanceWheel {
    pub fn is_identity(&self) -> bool {
        *self == BalanceWheel::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BalanceMode {
    #[default]
    Master,
    ThreeWay,
}

/// SPEC §3.6.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ColorBalance {
    pub mode: BalanceMode,
    pub master: BalanceWheel,
    pub shadows: BalanceWheel,
    pub midtones: BalanceWheel,
    pub highlights: BalanceWheel,
}

impl ColorBalance {
    pub fn is_identity(&self) -> bool {
        match self.mode {
            BalanceMode::Master => self.master.is_identity(),
            BalanceMode::ThreeWay => {
                self.shadows.is_identity()
                    && self.midtones.is_identity()
                    && self.highlights.is_identity()
            }
        }
    }

    /// The wheels the UI should draw for the current mode.
    pub fn active_wheels(&self) -> Vec<(&'static str, &BalanceWheel)> {
        match self.mode {
            BalanceMode::Master => vec![("Master", &self.master)],
            BalanceMode::ThreeWay => vec![
                ("Shadows", &self.shadows),
                ("Midtones", &self.midtones),
                ("Highlights", &self.highlights),
            ],
        }
    }

    pub fn wheel_mut(&mut self, name: &str) -> Option<&mut BalanceWheel> {
        match name {
            "Master" => Some(&mut self.master),
            "Shadows" => Some(&mut self.shadows),
            "Midtones" => Some(&mut self.midtones),
            "Highlights" => Some(&mut self.highlights),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Selective colour
// ---------------------------------------------------------------------------

/// The eight isolated colour ranges of SPEC §3.6, in the order the pane lists
/// them. The index is also the hue-band index used by the shader.
pub const SELECTIVE_COLOR_RANGES: [&str; 8] =
    ["Reds", "Oranges", "Yellows", "Greens", "Cyans", "Blues", "Violets", "Magentas"];

/// Hue centre in degrees for each range, matching the order above.
pub const SELECTIVE_COLOR_HUES: [f32; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct SelectiveColorBand {
    pub hue: f32,
    pub saturation: f32,
    pub brightness: f32,
}

impl SelectiveColorBand {
    pub fn is_identity(&self) -> bool {
        *self == SelectiveColorBand::default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SelectiveColor {
    pub bands: [SelectiveColorBand; 8],
}

impl SelectiveColor {
    pub fn is_identity(&self) -> bool {
        self.bands.iter().all(|b| b.is_identity())
    }
}

// ---------------------------------------------------------------------------
// Levels
// ---------------------------------------------------------------------------

/// Which histogram a Levels or Curves edit applies to. Levels offers
/// `Luminance`; Curves, per the guide, does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ToneChannel {
    #[default]
    Rgb,
    Red,
    Green,
    Blue,
    Luminance,
}

impl ToneChannel {
    pub const LEVELS: [ToneChannel; 5] = [
        ToneChannel::Rgb,
        ToneChannel::Red,
        ToneChannel::Green,
        ToneChannel::Blue,
        ToneChannel::Luminance,
    ];
    pub const CURVES: [ToneChannel; 4] =
        [ToneChannel::Rgb, ToneChannel::Red, ToneChannel::Green, ToneChannel::Blue];

    pub fn label(self) -> &'static str {
        match self {
            ToneChannel::Rgb => "RGB",
            ToneChannel::Red => "Red",
            ToneChannel::Green => "Green",
            ToneChannel::Blue => "Blue",
            ToneChannel::Luminance => "Luminance",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// The handle positions for one channel. `quarter_low`/`quarter_high` are
/// Pixelmator's quarter-tone handles (SPEC §3.8), which bend the transfer curve
/// between the main points without moving them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LevelsChannel {
    pub black_in: f32,
    pub white_in: f32,
    pub gamma: f32,
    pub black_out: f32,
    pub white_out: f32,
    pub quarter_low: f32,
    pub quarter_high: f32,
}

impl Default for LevelsChannel {
    fn default() -> Self {
        Self {
            black_in: 0.0,
            white_in: 1.0,
            gamma: 1.0,
            black_out: 0.0,
            white_out: 1.0,
            quarter_low: 0.25,
            quarter_high: 0.75,
        }
    }
}

impl LevelsChannel {
    pub fn is_identity(&self) -> bool {
        *self == LevelsChannel::default()
    }

    /// Map one channel value through this channel's levels.
    pub fn apply(&self, v: f32) -> f32 {
        let span = (self.white_in - self.black_in).abs().max(1e-6);
        let t = ((v - self.black_in) / span).clamp(0.0, 1.0);
        let t = if (self.gamma - 1.0).abs() < 1e-6 {
            t
        } else {
            t.powf(1.0 / self.gamma.max(1e-3))
        };
        // Quarter-tone handles bend the curve as a pair of eased nudges around
        // the 25% and 75% marks, falling off to zero at the endpoints and mid.
        let t = apply_quarter(t, 0.25, self.quarter_low);
        let t = apply_quarter(t, 0.75, self.quarter_high);
        self.black_out + t.clamp(0.0, 1.0) * (self.white_out - self.black_out)
    }
}

/// Nudge the curve near `centre` by however far the handle has been dragged
/// from its home position, tapering to nothing half a window away.
fn apply_quarter(t: f32, centre: f32, handle: f32) -> f32 {
    let delta = centre - handle;
    if delta.abs() < 1e-6 {
        return t;
    }
    const WINDOW: f32 = 0.25;
    let d = ((t - centre) / WINDOW).clamp(-1.0, 1.0);
    // Smooth bump: 1 at the centre, 0 at the window edges, C¹ continuous.
    let falloff = (1.0 - d * d) * (1.0 - d * d);
    (t + delta * falloff).clamp(0.0, 1.0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Levels {
    pub active: ToneChannel,
    pub channels: [LevelsChannel; 5],
}

impl Levels {
    pub fn channel(&self, c: ToneChannel) -> &LevelsChannel {
        &self.channels[c.index()]
    }

    pub fn channel_mut(&mut self, c: ToneChannel) -> &mut LevelsChannel {
        &mut self.channels[c.index()]
    }

    pub fn is_identity(&self) -> bool {
        self.channels.iter().all(|c| c.is_identity())
    }
}

// ---------------------------------------------------------------------------
// Curves
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Curves {
    pub active: ToneChannel,
    /// Indexed by [`ToneChannel::index`]; the `Luminance` slot is unused for
    /// Curves but kept so the two adjustments share an indexing scheme.
    pub channels: [Curve; 5],
}

impl Default for Curves {
    fn default() -> Self {
        Self { active: ToneChannel::Rgb, channels: std::array::from_fn(|_| Curve::identity()) }
    }
}

impl Curves {
    pub fn channel(&self, c: ToneChannel) -> &Curve {
        &self.channels[c.index()]
    }

    pub fn channel_mut(&mut self, c: ToneChannel) -> &mut Curve {
        &mut self.channels[c.index()]
    }

    pub fn is_identity(&self) -> bool {
        self.channels.iter().all(|c| c.is_identity())
    }
}

// ---------------------------------------------------------------------------
// Channel mixer
// ---------------------------------------------------------------------------

/// One output channel's row of the mixing matrix (SPEC §3.10).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MixerRow {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub constant: f32,
}

impl MixerRow {
    pub fn identity(output: usize) -> Self {
        Self {
            red: if output == 0 { 1.0 } else { 0.0 },
            green: if output == 1 { 1.0 } else { 0.0 },
            blue: if output == 2 { 1.0 } else { 0.0 },
            constant: 0.0,
        }
    }

    /// The guide's advice is to keep the three weights summing to 100% so
    /// overall brightness is preserved; the UI surfaces this as a hint.
    pub fn weight_sum(&self) -> f32 {
        self.red + self.green + self.blue
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelMixer {
    /// 0 = red output, 1 = green, 2 = blue.
    pub active: u32,
    pub rows: [MixerRow; 3],
}

impl Default for ChannelMixer {
    fn default() -> Self {
        Self { active: 0, rows: std::array::from_fn(MixerRow::identity) }
    }
}

impl ChannelMixer {
    pub fn is_identity(&self) -> bool {
        self.rows.iter().enumerate().all(|(i, r)| *r == MixerRow::identity(i))
    }
}

// ---------------------------------------------------------------------------
// LUT
// ---------------------------------------------------------------------------

/// SPEC §3.16. Pixelmator Pro reads 1-D and 3-D `.cube` LUTs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LutAdjust {
    /// Display name of the selected LUT, or empty for none.
    pub name: String,
    /// Path to the `.cube` file, when it came from disk rather than a built-in
    /// collection.
    pub path: Option<std::path::PathBuf>,
    pub intensity: f32,
}

impl LutAdjust {
    pub fn is_identity(&self) -> bool {
        self.name.is_empty() || self.intensity <= 0.0
    }
}

// ---------------------------------------------------------------------------
// The adjustment enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Adjustment {
    WhiteBalance(WhiteBalance),
    HueSaturation(HueSaturation),
    Basic(Basic),
    ColorBalance(ColorBalance),
    SelectiveColor(SelectiveColor),
    SelectiveClarity(SelectiveClarity),
    Levels(Levels),
    Curves(Curves),
    ChannelMixer(ChannelMixer),
    Vignette(Vignette),
    Grain(Grain),
    Sharpen(SharpenAdjust),
    BlackAndWhite(BlackAndWhite),
    ReplaceColor(ReplaceColor),
    Invert(Invert),
    Lut(LutAdjust),
}

/// Identifies an adjustment without carrying its settings — used for menus,
/// the `Customize` pane list, and the document format's type tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AdjustmentKind {
    WhiteBalance,
    HueSaturation,
    Basic,
    ColorBalance,
    SelectiveColor,
    SelectiveClarity,
    Levels,
    Curves,
    ChannelMixer,
    Vignette,
    Grain,
    Sharpen,
    BlackAndWhite,
    ReplaceColor,
    Invert,
    Lut,
}

impl AdjustmentKind {
    /// Display order of the Color Adjustments pane.
    pub const ALL: [AdjustmentKind; 16] = [
        AdjustmentKind::WhiteBalance,
        AdjustmentKind::HueSaturation,
        AdjustmentKind::Basic,
        AdjustmentKind::SelectiveClarity,
        AdjustmentKind::ColorBalance,
        AdjustmentKind::SelectiveColor,
        AdjustmentKind::Levels,
        AdjustmentKind::Curves,
        AdjustmentKind::ChannelMixer,
        AdjustmentKind::BlackAndWhite,
        AdjustmentKind::ReplaceColor,
        AdjustmentKind::Sharpen,
        AdjustmentKind::Vignette,
        AdjustmentKind::Grain,
        AdjustmentKind::Lut,
        AdjustmentKind::Invert,
    ];

    pub fn label(self) -> &'static str {
        use AdjustmentKind::*;
        match self {
            WhiteBalance => "White Balance",
            HueSaturation => "Hue & Saturation",
            Basic => "Basic",
            ColorBalance => "Color Balance",
            SelectiveColor => "Selective Color",
            SelectiveClarity => "Selective Clarity",
            Levels => "Levels",
            Curves => "Curves",
            ChannelMixer => "Channel Mixer",
            Vignette => "Vignette",
            Grain => "Grain",
            Sharpen => "Sharpen",
            BlackAndWhite => "Black & White",
            ReplaceColor => "Replace Color",
            Invert => "Invert",
            Lut => "LUT",
        }
    }

    pub fn id(self) -> &'static str {
        use AdjustmentKind::*;
        match self {
            WhiteBalance => "white-balance",
            HueSaturation => "hue-saturation",
            Basic => "basic",
            ColorBalance => "color-balance",
            SelectiveColor => "selective-color",
            SelectiveClarity => "selective-clarity",
            Levels => "levels",
            Curves => "curves",
            ChannelMixer => "channel-mixer",
            Vignette => "vignette",
            Grain => "grain",
            Sharpen => "sharpen",
            BlackAndWhite => "black-and-white",
            ReplaceColor => "replace-color",
            Invert => "invert",
            Lut => "lut",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }

    /// Sections shown in the pane by default; the rest are added via
    /// `Customize` (SPEC §3.1, §3.15).
    pub fn in_default_pane(self) -> bool {
        !matches!(self, AdjustmentKind::Invert | AdjustmentKind::Lut)
    }

    /// Construct an adjustment of this kind at its defaults.
    ///
    /// Named `new` despite not returning `Self` because it reads naturally at
    /// the call site — `AdjustmentKind::Basic.new()` — and the alternative
    /// (`make`, `create`, `instantiate`) is worse everywhere it is used.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(self) -> Adjustment {
        use AdjustmentKind::*;
        match self {
            WhiteBalance => Adjustment::WhiteBalance(Default::default()),
            HueSaturation => Adjustment::HueSaturation(Default::default()),
            Basic => Adjustment::Basic(Default::default()),
            ColorBalance => Adjustment::ColorBalance(Default::default()),
            SelectiveColor => Adjustment::SelectiveColor(Default::default()),
            SelectiveClarity => Adjustment::SelectiveClarity(Default::default()),
            Levels => Adjustment::Levels(Default::default()),
            Curves => Adjustment::Curves(Default::default()),
            ChannelMixer => Adjustment::ChannelMixer(Default::default()),
            Vignette => Adjustment::Vignette(Default::default()),
            Grain => Adjustment::Grain(Default::default()),
            Sharpen => Adjustment::Sharpen(Default::default()),
            BlackAndWhite => Adjustment::BlackAndWhite(Default::default()),
            ReplaceColor => Adjustment::ReplaceColor(Default::default()),
            Invert => Adjustment::Invert(Default::default()),
            Lut => Adjustment::Lut(Default::default()),
        }
    }
}

impl Adjustment {
    pub fn kind(&self) -> AdjustmentKind {
        use Adjustment as A;
        match self {
            A::WhiteBalance(_) => AdjustmentKind::WhiteBalance,
            A::HueSaturation(_) => AdjustmentKind::HueSaturation,
            A::Basic(_) => AdjustmentKind::Basic,
            A::ColorBalance(_) => AdjustmentKind::ColorBalance,
            A::SelectiveColor(_) => AdjustmentKind::SelectiveColor,
            A::SelectiveClarity(_) => AdjustmentKind::SelectiveClarity,
            A::Levels(_) => AdjustmentKind::Levels,
            A::Curves(_) => AdjustmentKind::Curves,
            A::ChannelMixer(_) => AdjustmentKind::ChannelMixer,
            A::Vignette(_) => AdjustmentKind::Vignette,
            A::Grain(_) => AdjustmentKind::Grain,
            A::Sharpen(_) => AdjustmentKind::Sharpen,
            A::BlackAndWhite(_) => AdjustmentKind::BlackAndWhite,
            A::ReplaceColor(_) => AdjustmentKind::ReplaceColor,
            A::Invert(_) => AdjustmentKind::Invert,
            A::Lut(_) => AdjustmentKind::Lut,
        }
    }

    pub fn label(&self) -> &'static str {
        self.kind().label()
    }

    /// Whether this adjustment would leave the image untouched, letting the
    /// render graph skip its pass.
    pub fn is_identity(&self) -> bool {
        use Adjustment as A;
        match self {
            A::WhiteBalance(a) => a.is_identity(),
            A::HueSaturation(a) => a.is_identity(),
            A::Basic(a) => a.is_identity(),
            A::ColorBalance(a) => a.is_identity(),
            A::SelectiveColor(a) => a.is_identity(),
            A::SelectiveClarity(a) => a.is_identity(),
            A::Levels(a) => a.is_identity(),
            A::Curves(a) => a.is_identity(),
            A::ChannelMixer(a) => a.is_identity(),
            A::Vignette(a) => a.is_identity(),
            A::Grain(a) => a.is_identity(),
            A::Sharpen(a) => a.is_identity(),
            // Black & White and Invert are *not* no-ops at their defaults:
            // adding either one is meant to change the image immediately, the
            // way it does in Pixelmator. Their identity is zero intensity.
            A::BlackAndWhite(a) => a.intensity <= 0.0,
            A::ReplaceColor(a) => a.is_identity(),
            A::Invert(a) => a.intensity <= 0.0,
            A::Lut(a) => a.is_identity(),
        }
    }

    /// Flat controls, for the adjustments that have them. The structured ones
    /// (Levels, Curves, colour wheels, the mixer) get bespoke widgets and
    /// return an empty list here.
    pub fn specs(&self) -> Vec<ParamSpec> {
        use Adjustment as A;
        match self {
            A::WhiteBalance(a) => a.specs(),
            A::HueSaturation(a) => a.specs(),
            A::Basic(a) => a.specs(),
            A::SelectiveClarity(a) => a.specs(),
            A::Vignette(a) => a.specs(),
            A::Grain(a) => a.specs(),
            A::Sharpen(a) => a.specs(),
            A::BlackAndWhite(a) => a.specs(),
            A::ReplaceColor(a) => a.specs(),
            A::Invert(a) => a.specs(),
            _ => Vec::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<ParamValue> {
        use Adjustment as A;
        match self {
            A::WhiteBalance(a) => a.get(key),
            A::HueSaturation(a) => a.get(key),
            A::Basic(a) => a.get(key),
            A::SelectiveClarity(a) => a.get(key),
            A::Vignette(a) => a.get(key),
            A::Grain(a) => a.get(key),
            A::Sharpen(a) => a.get(key),
            A::BlackAndWhite(a) => a.get(key),
            A::ReplaceColor(a) => a.get(key),
            A::Invert(a) => a.get(key),
            _ => None,
        }
    }

    pub fn set(&mut self, key: &str, value: ParamValue) -> bool {
        use Adjustment as A;
        match self {
            A::WhiteBalance(a) => a.set(key, value),
            A::HueSaturation(a) => a.set(key, value),
            A::Basic(a) => a.set(key, value),
            A::SelectiveClarity(a) => a.set(key, value),
            A::Vignette(a) => a.set(key, value),
            A::Grain(a) => a.set(key, value),
            A::Sharpen(a) => a.set(key, value),
            A::BlackAndWhite(a) => a.set(key, value),
            A::ReplaceColor(a) => a.set(key, value),
            A::Invert(a) => a.set(key, value),
            _ => false,
        }
    }
}

/// An adjustment plus the per-instance state every adjustment carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdjustmentInstance {
    pub adjustment: Adjustment,
    pub enabled: bool,
}

impl AdjustmentInstance {
    pub fn new(kind: AdjustmentKind) -> Self {
        Self { adjustment: kind.new(), enabled: true }
    }

    /// True when the render graph can skip this instance entirely.
    pub fn is_noop(&self) -> bool {
        !self.enabled || self.adjustment.is_identity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_constructs_matching_variant() {
        for kind in AdjustmentKind::ALL {
            assert_eq!(kind.new().kind(), kind, "{kind:?} constructed the wrong variant");
        }
    }

    #[test]
    fn ids_are_unique_and_round_trip() {
        let mut seen = std::collections::HashSet::new();
        for kind in AdjustmentKind::ALL {
            assert!(seen.insert(kind.id()), "duplicate id {}", kind.id());
            assert_eq!(AdjustmentKind::from_id(kind.id()), Some(kind));
        }
    }

    /// Adding one of these is meant to change the image straight away, so they
    /// must not be optimised out before the user touches anything.
    const ACTIVE_AT_DEFAULTS: [AdjustmentKind; 2] =
        [AdjustmentKind::Invert, AdjustmentKind::BlackAndWhite];

    #[test]
    fn fresh_adjustments_are_no_ops_except_where_they_should_not_be() {
        for kind in AdjustmentKind::ALL {
            let inst = AdjustmentInstance::new(kind);
            if ACTIVE_AT_DEFAULTS.contains(&kind) {
                assert!(!inst.is_noop(), "{kind:?} should be active at its defaults");
            } else {
                assert!(inst.is_noop(), "{kind:?} is not identity at its defaults");
            }
        }
    }

    #[test]
    fn zero_intensity_disables_the_always_on_adjustments() {
        let mut a = AdjustmentKind::Invert.new();
        a.set("intensity", ParamValue::Float(0.0));
        assert!(a.is_identity());

        let mut a = AdjustmentKind::BlackAndWhite.new();
        a.set("intensity", ParamValue::Float(0.0));
        assert!(a.is_identity());
    }

    #[test]
    fn touching_a_control_makes_it_active() {
        let mut inst = AdjustmentInstance::new(AdjustmentKind::Basic);
        assert!(inst.is_noop());
        assert!(inst.adjustment.set("exposure", ParamValue::Float(0.3)));
        assert!(!inst.is_noop());
    }

    #[test]
    fn disabling_forces_a_no_op() {
        let mut inst = AdjustmentInstance::new(AdjustmentKind::Basic);
        inst.adjustment.set("contrast", ParamValue::Float(0.5));
        assert!(!inst.is_noop());
        inst.enabled = false;
        assert!(inst.is_noop());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let mut a = AdjustmentKind::Basic.new();
        assert!(!a.set("no_such_key", ParamValue::Float(1.0)));
        assert_eq!(a.get("no_such_key"), None);
    }

    #[test]
    fn slider_values_are_clamped_on_set() {
        let mut a = AdjustmentKind::BlackAndWhite.new();
        a.set("intensity", ParamValue::Float(99.0));
        assert_eq!(a.get("intensity"), Some(ParamValue::Float(1.0)));
    }

    #[test]
    fn black_and_white_defaults_to_rec709_luma() {
        let bw = BlackAndWhite::default();
        assert!((bw.red + bw.green + bw.blue - 1.0).abs() < 1e-5);
    }

    #[test]
    fn levels_identity_passes_values_through() {
        let ch = LevelsChannel::default();
        for i in 0..=10 {
            let v = i as f32 / 10.0;
            assert!((ch.apply(v) - v).abs() < 1e-4, "levels shifted {v}");
        }
    }

    #[test]
    fn levels_black_point_crushes_shadows() {
        let ch = LevelsChannel { black_in: 0.5, ..Default::default() };
        assert!(ch.apply(0.25) < 1e-6);
        assert!((ch.apply(1.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn levels_gamma_brightens_midtones() {
        let ch = LevelsChannel { gamma: 2.0, ..Default::default() };
        assert!(ch.apply(0.5) > 0.5);
        // Endpoints are unaffected by gamma.
        assert!(ch.apply(0.0).abs() < 1e-5);
        assert!((ch.apply(1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn quarter_tone_handle_is_local() {
        let ch = LevelsChannel { quarter_low: 0.15, ..Default::default() };
        // Lifts near the quarter tone...
        assert!(ch.apply(0.25) > 0.30);
        // ...but leaves the far end alone.
        assert!((ch.apply(0.9) - 0.9).abs() < 1e-4);
    }

    #[test]
    fn mixer_identity_detection() {
        let mut m = ChannelMixer::default();
        assert!(m.is_identity());
        m.rows[0].green = 0.5;
        assert!(!m.is_identity());
        assert!((m.rows[0].weight_sum() - 1.5).abs() < 1e-6);
    }

    #[test]
    fn color_balance_identity_respects_mode() {
        let mut cb = ColorBalance::default();
        cb.shadows.red_cyan = 0.4;
        // Still identity in Master mode, since the shadows wheel is not in play.
        assert!(cb.is_identity());
        cb.mode = BalanceMode::ThreeWay;
        assert!(!cb.is_identity());
        assert_eq!(cb.active_wheels().len(), 3);
    }

    #[test]
    fn selective_color_has_eight_named_bands() {
        let sc = SelectiveColor::default();
        assert_eq!(sc.bands.len(), SELECTIVE_COLOR_RANGES.len());
        assert_eq!(SELECTIVE_COLOR_HUES.len(), 8);
        assert!(sc.is_identity());
    }

    #[test]
    fn curves_default_to_identity_on_every_channel() {
        let c = Curves::default();
        assert!(c.is_identity());
        assert_eq!(ToneChannel::CURVES.len(), 4);
        assert_eq!(ToneChannel::LEVELS.len(), 5);
    }

    #[test]
    fn reset_restores_defaults() {
        let mut b = Basic { exposure: 0.9, clarity: -0.5, ..Default::default() };
        b.reset();
        assert_eq!(b, Basic::default());
    }
}
