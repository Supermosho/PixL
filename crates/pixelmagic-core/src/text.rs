//! Text layers.
//!
//! The model here is deliberately layout-engine agnostic: it stores what the
//! user typed and how they want it styled, and leaves shaping, bidi and line
//! breaking to Pango in the UI layer. Getting that boundary right matters —
//! text layout is one of the few places where reinventing the wheel is
//! guaranteed to be worse than the platform's.

use crate::color::Rgba;
use crate::style::PaintSource;
use crate::vector::Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

impl TextAlign {
    pub const ALL: [TextAlign; 4] =
        [TextAlign::Left, TextAlign::Center, TextAlign::Right, TextAlign::Justify];

    pub fn label(self) -> &'static str {
        match self {
            TextAlign::Left => "Left",
            TextAlign::Center => "Center",
            TextAlign::Right => "Right",
            TextAlign::Justify => "Justify",
        }
    }
}

/// How the text box relates to its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextFrame {
    /// The box grows with the text — created by clicking once with the Type
    /// tool.
    #[default]
    Auto,
    /// Fixed width, text wraps — created by dragging out a box.
    Fixed,
}

/// Character-level formatting applied to a run of text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub family: String,
    pub size: f32,
    pub weight: u16,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// Extra space between characters, in 1/1000 em — the usual typographic
    /// unit for tracking.
    pub tracking: f32,
    /// Baseline-to-baseline distance as a multiple of the font size.
    pub line_height: f32,
    /// Baseline offset in points; positive raises.
    pub baseline_shift: f32,
    pub fill: PaintSource,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: "Sans".into(),
            size: 64.0,
            weight: 400,
            italic: false,
            underline: false,
            strikethrough: false,
            tracking: 0.0,
            line_height: 1.2,
            baseline_shift: 0.0,
            fill: PaintSource::Color(Rgba::BLACK),
        }
    }
}

/// A styled span. `start`/`end` are byte offsets into the layer's string, so
/// they line up with what Pango expects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextRun {
    pub start: usize,
    pub end: usize,
    pub style: TextStyle,
}

/// The path a `Path Type`, `Circular Type` or `Freeform Type` layer flows
/// along.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum TextPath {
    /// Ordinary horizontal text.
    #[default]
    None,
    /// `Circular Type`: text around a circle of the given radius.
    Circle { radius: f32 },
    /// `Path Type` / `Freeform Type`.
    Custom { path: Path },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
    /// Base style; runs override it for their ranges.
    pub base: TextStyle,
    pub runs: Vec<TextRun>,
    pub align: TextAlign,
    pub frame: TextFrame,
    /// Box width in layer units; only meaningful when `frame` is `Fixed`.
    pub width: f32,
    pub path: TextPath,
    /// Offset of the text's start along `path`, 0..=1.
    pub path_offset: f32,
}

impl Default for TextContent {
    fn default() -> Self {
        Self {
            text: String::new(),
            base: TextStyle::default(),
            runs: Vec::new(),
            align: TextAlign::default(),
            frame: TextFrame::default(),
            width: 400.0,
            path: TextPath::default(),
            path_offset: 0.0,
        }
    }
}

impl TextContent {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), ..Default::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// The style in effect at a byte offset: the last run covering it, falling
    /// back to the base style.
    pub fn style_at(&self, offset: usize) -> &TextStyle {
        self.runs
            .iter()
            .rev()
            .find(|r| offset >= r.start && offset < r.end)
            .map(|r| &r.style)
            .unwrap_or(&self.base)
    }

    /// Apply a style to a byte range, dropping any runs it fully covers so the
    /// list does not grow without bound as the user restyles the same text.
    pub fn set_style(&mut self, start: usize, end: usize, style: TextStyle) {
        if start >= end {
            return;
        }
        self.runs.retain(|r| !(r.start >= start && r.end <= end));
        self.runs.push(TextRun { start, end, style });
        self.runs.sort_by_key(|r| r.start);
    }

    /// Clamp every run to the current text length. Call after editing the
    /// string so stale ranges cannot point past the end.
    pub fn clamp_runs(&mut self) {
        let len = self.text.len();
        for r in &mut self.runs {
            r.start = r.start.min(len);
            r.end = r.end.min(len);
        }
        self.runs.retain(|r| r.start < r.end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_falls_back_to_base() {
        let mut c = TextContent::new("hello world");
        assert_eq!(c.style_at(0).size, c.base.size);

        let big = TextStyle { size: 120.0, ..Default::default() };
        c.set_style(0, 5, big);
        assert_eq!(c.style_at(2).size, 120.0);
        assert_eq!(c.style_at(7).size, c.base.size);
    }

    #[test]
    fn restyling_the_same_range_does_not_accumulate_runs() {
        let mut c = TextContent::new("hello");
        for size in [10.0, 20.0, 30.0] {
            c.set_style(0, 5, TextStyle { size, ..Default::default() });
        }
        assert_eq!(c.runs.len(), 1);
        assert_eq!(c.style_at(0).size, 30.0);
    }

    #[test]
    fn empty_range_is_ignored() {
        let mut c = TextContent::new("hello");
        c.set_style(3, 3, TextStyle::default());
        assert!(c.runs.is_empty());
    }

    #[test]
    fn clamping_drops_stale_runs() {
        let mut c = TextContent::new("hello world");
        c.set_style(6, 11, TextStyle::default());
        c.text = "hi".into();
        c.clamp_runs();
        assert!(c.runs.is_empty(), "runs past the end should be dropped");
    }

    #[test]
    fn emptiness_ignores_whitespace() {
        assert!(TextContent::new("   \n ").is_empty());
        assert!(!TextContent::new("x").is_empty());
    }
}
