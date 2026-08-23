//! Layer styles: fill, stroke, drop shadow, inner shadow.
//!
//! Mirrors Pixelmator Pro's Style tool (`docs/SPEC.md` §4). Each style is
//! optional and independently toggleable, and — like adjustments — is
//! non-destructive until `Flatten Styles`.
//!
//! Notably absent, matching the original: shadows have no `Spread`/`Choke`, and
//! there is no bevel/emboss or satin. The style set really is just these four.

use crate::blend::BlendMode;
use crate::color::Rgba;
use crate::param::ParamKind;
use crate::parameterized;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Gradients
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GradientType {
    #[default]
    Linear,
    Radial,
    Angle,
}

impl GradientType {
    pub const ALL: [GradientType; 3] =
        [GradientType::Linear, GradientType::Radial, GradientType::Angle];

    pub fn label(self) -> &'static str {
        match self {
            GradientType::Linear => "Linear",
            GradientType::Radial => "Radial",
            GradientType::Angle => "Angle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Position along the gradient, 0..=1.
    pub position: f32,
    pub color: Rgba,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    pub kind: GradientType,
    /// Degrees; ignored for `Radial`.
    pub angle: f32,
    stops: Vec<GradientStop>,
}

impl Default for Gradient {
    fn default() -> Self {
        Gradient::two_stop(Rgba::BLACK, Rgba::WHITE)
    }
}

impl Gradient {
    pub fn two_stop(from: Rgba, to: Rgba) -> Self {
        Gradient {
            kind: GradientType::Linear,
            angle: 0.0,
            stops: vec![
                GradientStop { position: 0.0, color: from },
                GradientStop { position: 1.0, color: to },
            ],
        }
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    /// Insert a stop, keeping the list ordered by position. Returns its index.
    pub fn add_stop(&mut self, stop: GradientStop) -> usize {
        let stop = GradientStop { position: stop.position.clamp(0.0, 1.0), ..stop };
        let i = self
            .stops
            .iter()
            .position(|s| s.position > stop.position)
            .unwrap_or(self.stops.len());
        self.stops.insert(i, stop);
        i
    }

    /// Remove a stop. A gradient needs at least two, so this refuses to go
    /// below that.
    pub fn remove_stop(&mut self, index: usize) -> bool {
        if self.stops.len() <= 2 || index >= self.stops.len() {
            return false;
        }
        self.stops.remove(index);
        true
    }

    pub fn move_stop(&mut self, index: usize, position: f32) -> bool {
        if index >= self.stops.len() {
            return false;
        }
        self.stops[index].position = position.clamp(0.0, 1.0);
        self.stops.sort_by(|a, b| {
            a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal)
        });
        true
    }

    /// Sample the gradient at `t`, interpolating in the caller's colour space.
    pub fn sample(&self, t: f32) -> Rgba {
        if self.stops.is_empty() {
            return Rgba::TRANSPARENT;
        }
        let t = t.clamp(0.0, 1.0);
        if t <= self.stops[0].position {
            return self.stops[0].color;
        }
        let last = self.stops.len() - 1;
        if t >= self.stops[last].position {
            return self.stops[last].color;
        }
        for w in self.stops.windows(2) {
            let (a, b) = (w[0], w[1]);
            if t >= a.position && t <= b.position {
                let span = b.position - a.position;
                let f = if span <= 1e-6 { 0.0 } else { (t - a.position) / span };
                return a.color.lerp(b.color, f);
            }
        }
        self.stops[last].color
    }

    /// Bake to an RGBA ramp for upload as a 1-D texture.
    pub fn to_ramp(&self, size: usize) -> Vec<[f32; 4]> {
        (0..size).map(|i| self.sample(i as f32 / (size.max(2) - 1) as f32).to_array()).collect()
    }
}

// ---------------------------------------------------------------------------
// Fill
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PaintSource {
    Color(Rgba),
    Gradient(Gradient),
    Pattern { path: Option<PathBuf>, scale: f32, angle: f32 },
}

impl Default for PaintSource {
    fn default() -> Self {
        PaintSource::Color(Rgba::rgb(0.0, 0.45, 0.95))
    }
}

impl PaintSource {
    pub fn type_index(&self) -> u32 {
        match self {
            PaintSource::Color(_) => 0,
            PaintSource::Gradient(_) => 1,
            PaintSource::Pattern { .. } => 2,
        }
    }

    pub const TYPE_LABELS: [&'static str; 3] = ["Color", "Gradient", "Pattern"];

    /// Switch fill type, keeping whatever colour information carries over so
    /// flipping Color → Gradient → Color does not lose the user's choice.
    pub fn with_type_index(&self, index: u32) -> PaintSource {
        match (index, self) {
            (0, PaintSource::Gradient(g)) => PaintSource::Color(g.sample(0.0)),
            (0, _) => PaintSource::Color(match self {
                PaintSource::Color(c) => *c,
                _ => Rgba::default(),
            }),
            (1, PaintSource::Color(c)) => {
                PaintSource::Gradient(Gradient::two_stop(*c, c.with_alpha(0.0)))
            }
            (1, PaintSource::Gradient(g)) => PaintSource::Gradient(g.clone()),
            (1, _) => PaintSource::Gradient(Gradient::default()),
            (2, PaintSource::Pattern { .. }) => self.clone(),
            (2, _) => PaintSource::Pattern { path: None, scale: 1.0, angle: 0.0 },
            _ => self.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillStyle {
    pub source: PaintSource,
    pub opacity: f32,
    pub blend_mode: BlendMode,
}

impl Default for FillStyle {
    fn default() -> Self {
        Self { source: PaintSource::default(), opacity: 1.0, blend_mode: BlendMode::Normal }
    }
}

// ---------------------------------------------------------------------------
// Stroke
// ---------------------------------------------------------------------------

/// ⚠️ The guide names a `Position` control but not its options; inside /
/// centre / outside is the conventional set and what we implement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StrokePosition {
    Inside,
    #[default]
    Center,
    Outside,
}

impl StrokePosition {
    pub const ALL: [StrokePosition; 3] =
        [StrokePosition::Inside, StrokePosition::Center, StrokePosition::Outside];

    pub fn label(self) -> &'static str {
        match self {
            StrokePosition::Inside => "Inside",
            StrokePosition::Center => "Center",
            StrokePosition::Outside => "Outside",
        }
    }
}

/// ⚠️ As with [`StrokePosition`], the option list is our reading of a control
/// the guide names but does not enumerate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StrokeStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl StrokeStyle {
    pub const ALL: [StrokeStyle; 3] =
        [StrokeStyle::Solid, StrokeStyle::Dashed, StrokeStyle::Dotted];

    pub fn label(self) -> &'static str {
        match self {
            StrokeStyle::Solid => "Solid",
            StrokeStyle::Dashed => "Dashed",
            StrokeStyle::Dotted => "Dotted",
        }
    }

    /// Dash pattern in multiples of the stroke width, empty for a solid line.
    pub fn dash_pattern(self) -> &'static [f32] {
        match self {
            StrokeStyle::Solid => &[],
            StrokeStyle::Dashed => &[3.0, 2.0],
            StrokeStyle::Dotted => &[0.0, 2.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrokeStyleSettings {
    pub source: PaintSource,
    pub width: f32,
    pub position: StrokePosition,
    pub style: StrokeStyle,
    pub spacing: f32,
    pub opacity: f32,
    pub blend_mode: BlendMode,
}

impl Default for StrokeStyleSettings {
    fn default() -> Self {
        Self {
            source: PaintSource::Color(Rgba::BLACK),
            width: 2.0,
            position: StrokePosition::default(),
            style: StrokeStyle::default(),
            spacing: 1.0,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
        }
    }
}

// ---------------------------------------------------------------------------
// Shadows
// ---------------------------------------------------------------------------

parameterized! {
    /// Shared by the drop shadow and the inner shadow, which the guide gives
    /// identical control sets (SPEC §4.3, §4.4). `Distance` is documented as
    /// 0–100 px with Option-drag extending past that.
    pub struct ShadowSettings {
        blur: f32 = "blur", "Blur", ParamKind::radius(6.0, 100.0);
        distance: f32 = "distance", "Distance", ParamKind::Slider {
            min: 0.0, max: 1000.0, soft_min: 0.0, soft_max: 100.0,
            default: 6.0, percent: false, unit: "px",
        };
        angle: f32 = "angle", "Angle", ParamKind::Angle { default: 135.0 };
        opacity: f32 = "opacity", "Opacity", ParamKind::unit_percent(0.5);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shadow {
    pub settings: ShadowSettings,
    pub color: Rgba,
    pub blend_mode: BlendMode,
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            settings: ShadowSettings::default(),
            color: Rgba::BLACK,
            blend_mode: BlendMode::Normal,
        }
    }
}

impl Shadow {
    /// Offset in layer space implied by `distance` and `angle`.
    ///
    /// Angles are measured counter-clockwise from east, as on the wheel; y is
    /// negated because document space grows downwards.
    pub fn offset(&self) -> glam::Vec2 {
        let r = self.settings.angle.to_radians();
        glam::Vec2::new(self.settings.distance * r.cos(), -self.settings.distance * r.sin())
    }

    /// How far past the layer's own bounds this shadow can reach — needed to
    /// size the intermediate texture before rendering it.
    pub fn bounds_expansion(&self) -> f32 {
        self.settings.distance + self.settings.blur * 1.5
    }
}

// ---------------------------------------------------------------------------
// The style set
// ---------------------------------------------------------------------------

/// The complete style state of a layer. All four are `Option`, matching the
/// Style pane where each is added and removed independently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LayerStyle {
    pub fill: Option<FillStyle>,
    pub stroke: Option<StrokeStyleSettings>,
    pub shadow: Option<Shadow>,
    pub inner_shadow: Option<Shadow>,
}

impl LayerStyle {
    pub fn is_empty(&self) -> bool {
        self.fill.is_none()
            && self.stroke.is_none()
            && self.shadow.is_none()
            && self.inner_shadow.is_none()
    }

    /// How far the styled result extends beyond the layer's own bounds.
    /// Drop shadows and outside strokes both grow it.
    pub fn bounds_expansion(&self) -> f32 {
        let mut e: f32 = 0.0;
        if let Some(s) = &self.shadow {
            e = e.max(s.bounds_expansion());
        }
        if let Some(s) = &self.stroke {
            e = e.max(match s.position {
                StrokePosition::Outside => s.width,
                StrokePosition::Center => s.width * 0.5,
                StrokePosition::Inside => 0.0,
            });
        }
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_samples_endpoints_and_midpoint() {
        let g = Gradient::two_stop(Rgba::BLACK, Rgba::WHITE);
        assert_eq!(g.sample(0.0), Rgba::BLACK);
        assert_eq!(g.sample(1.0), Rgba::WHITE);
        assert!((g.sample(0.5).r - 0.5).abs() < 1e-5);
        // Out of range clamps rather than wrapping.
        assert_eq!(g.sample(-1.0), Rgba::BLACK);
        assert_eq!(g.sample(2.0), Rgba::WHITE);
    }

    #[test]
    fn gradient_stops_stay_sorted() {
        let mut g = Gradient::two_stop(Rgba::BLACK, Rgba::WHITE);
        g.add_stop(GradientStop { position: 0.7, color: Rgba::rgb(1.0, 0.0, 0.0) });
        g.add_stop(GradientStop { position: 0.3, color: Rgba::rgb(0.0, 1.0, 0.0) });
        let ps: Vec<f32> = g.stops().iter().map(|s| s.position).collect();
        assert_eq!(ps, vec![0.0, 0.3, 0.7, 1.0]);

        g.move_stop(1, 0.9);
        let ps: Vec<f32> = g.stops().iter().map(|s| s.position).collect();
        assert!(ps.windows(2).all(|w| w[0] <= w[1]), "not sorted: {ps:?}");
    }

    #[test]
    fn gradient_keeps_at_least_two_stops() {
        let mut g = Gradient::two_stop(Rgba::BLACK, Rgba::WHITE);
        assert!(!g.remove_stop(0));
        g.add_stop(GradientStop { position: 0.5, color: Rgba::WHITE });
        assert!(g.remove_stop(1));
        assert_eq!(g.stops().len(), 2);
    }

    #[test]
    fn ramp_has_requested_length() {
        let ramp = Gradient::default().to_ramp(256);
        assert_eq!(ramp.len(), 256);
        assert!(ramp[0][0] < 0.01 && ramp[255][0] > 0.99);
    }

    #[test]
    fn fill_type_switch_preserves_colour() {
        let src = PaintSource::Color(Rgba::rgb(0.2, 0.4, 0.6));
        let grad = src.with_type_index(1);
        assert_eq!(grad.type_index(), 1);
        let back = grad.with_type_index(0);
        match back {
            PaintSource::Color(c) => assert!((c.r - 0.2).abs() < 1e-5),
            other => panic!("expected a colour, got {other:?}"),
        }
    }

    #[test]
    fn shadow_offset_follows_the_angle_wheel() {
        let mut s = Shadow::default();
        s.settings.angle = 0.0;
        s.settings.distance = 10.0;
        let o = s.offset();
        assert!((o.x - 10.0).abs() < 1e-4 && o.y.abs() < 1e-4);

        s.settings.angle = 90.0;
        let o = s.offset();
        // 90° on the wheel points up, which is −y in document space.
        assert!(o.x.abs() < 1e-3 && (o.y + 10.0).abs() < 1e-3);
    }

    #[test]
    fn style_bounds_expansion_covers_shadow_and_stroke() {
        let mut style = LayerStyle::default();
        assert_eq!(style.bounds_expansion(), 0.0);
        assert!(style.is_empty());

        style.stroke = Some(StrokeStyleSettings {
            width: 8.0,
            position: StrokePosition::Outside,
            ..Default::default()
        });
        assert_eq!(style.bounds_expansion(), 8.0);

        let mut sh = Shadow::default();
        sh.settings.distance = 20.0;
        sh.settings.blur = 10.0;
        style.shadow = Some(sh);
        assert_eq!(style.bounds_expansion(), 35.0);
        assert!(!style.is_empty());
    }

    #[test]
    fn inside_stroke_does_not_grow_bounds() {
        let style = LayerStyle {
            stroke: Some(StrokeStyleSettings {
                width: 20.0,
                position: StrokePosition::Inside,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(style.bounds_expansion(), 0.0);
    }
}
