//! The tool roster.
//!
//! All 50 entries from `docs/SPEC.md` §5.1, with their categories and
//! shortcuts. Like the effect catalogue this is a static table rather than a
//! type per tool, so the Tools sidebar, the keyboard map and the "what does
//! this tool actually do yet" status all read from one source.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolCategory {
    Basic,
    Selection,
    Painting,
    Retouching,
    Drawing,
    Type,
}

impl ToolCategory {
    pub const ALL: [ToolCategory; 6] = [
        ToolCategory::Basic,
        ToolCategory::Selection,
        ToolCategory::Painting,
        ToolCategory::Retouching,
        ToolCategory::Drawing,
        ToolCategory::Type,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ToolCategory::Basic => "Basic",
            ToolCategory::Selection => "Selection",
            ToolCategory::Painting => "Painting",
            ToolCategory::Retouching => "Retouching",
            ToolCategory::Drawing => "Drawing",
            ToolCategory::Type => "Type",
        }
    }
}

/// Tools that share a slot in the sidebar and cycle with Shift + the shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolGroup {
    None,
    /// Free / Polygonal / Magnetic — Shift-L.
    LassoSelection,
    /// Shape primitives — Shift-U.
    Shapes,
    /// Pen / Freeform Pen — Shift-P.
    Pens,
    /// The four type tools — Shift-T.
    TypeTools,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tool {
    // Basic
    Style,
    Arrange,
    ColorAdjustments,
    Effects,
    Crop,
    ExportForWeb,
    ColorPicker,
    Zoom,
    Hand,
    // Selection
    RectangularSelection,
    OvalSelection,
    RowSelection,
    ColumnSelection,
    FreeSelection,
    PolygonalSelection,
    MagneticSelection,
    ColorSelection,
    QuickSelection,
    // Painting
    Paint,
    PixelPaint,
    ColorFill,
    GradientFill,
    Erase,
    SmartErase,
    // Retouching
    Repair,
    Clone,
    Sharpen,
    Soften,
    Smudge,
    Lighten,
    Darken,
    Saturate,
    Desaturate,
    Distort,
    Bump,
    Pinch,
    Twirl,
    // Drawing
    Shape,
    Pen,
    FreeformPen,
    Rectangle,
    RoundedRectangle,
    Oval,
    Polygon,
    Star,
    Line,
    // Type
    Type,
    CircularType,
    PathType,
    FreeformType,
}

pub struct ToolInfo {
    pub tool: Tool,
    pub id: &'static str,
    pub label: &'static str,
    pub category: ToolCategory,
    pub group: ToolGroup,
    /// Single-key shortcut, where the guide documents one.
    pub shortcut: Option<char>,
    pub description: &'static str,
    /// Whether the tool does anything yet. The sidebar dims the ones that do
    /// not, rather than offering a tool that silently no-ops.
    pub implemented: bool,
}

macro_rules! tools {
    ($($tool:ident $id:literal $label:literal $cat:ident $group:ident $key:expr, $impl:literal, $desc:literal);* $(;)?) => {
        pub static TOOLS: &[ToolInfo] = &[
            $(ToolInfo {
                tool: Tool::$tool,
                id: $id,
                label: $label,
                category: ToolCategory::$cat,
                group: ToolGroup::$group,
                shortcut: $key,
                description: $desc,
                implemented: $impl,
            }),*
        ];
    };
}

tools! {
    // -- Basic -----------------------------------------------------------
    Style "style" "Style" Basic None Some('s'), true,
        "Add fills, strokes, and shadows to layers";
    Arrange "arrange" "Arrange" Basic None Some('v'), true,
        "Move, rotate, resize, or change the position of layers";
    ColorAdjustments "color-adjustments" "Color Adjustments" Basic None Some('a'), true,
        "Access controls for basic photo editing";
    Effects "effects" "Effects" Basic None Some('f'), true,
        "Add visual effects to your document";
    Crop "crop" "Crop" Basic None Some('c'), true,
        "Crop and straighten images";
    ExportForWeb "export-for-web" "Export for Web" Basic None Some('k'), false,
        "Prepare and export images for the web";
    ColorPicker "color-picker" "Color Picker" Basic None Some('i'), true,
        "Sample colors from images";
    Zoom "zoom" "Zoom" Basic None Some('z'), true,
        "Zoom in and out of an image";
    Hand "hand" "Hand" Basic None Some('h'), true,
        "Scroll or pan an image";

    // -- Selection -------------------------------------------------------
    RectangularSelection "rect-select" "Rectangular Selection" Selection None Some('m'), true,
        "Makes square and rectangular selections";
    OvalSelection "oval-select" "Oval Selection" Selection None Some('y'), true,
        "Makes circular and elliptical selections";
    RowSelection "row-select" "Row Selection" Selection None None, true,
        "Makes horizontal selections of a custom height and the full width of the canvas";
    ColumnSelection "column-select" "Column Selection" Selection None None, true,
        "Makes vertical selections of a custom width and the full height of the canvas";
    FreeSelection "free-select" "Free Selection" Selection LassoSelection Some('l'), true,
        "Allows you to draw freehand selections";
    PolygonalSelection "polygonal-select" "Polygonal Selection" Selection LassoSelection None, true,
        "Allows you to draw polygonal, jagged selections";
    MagneticSelection "magnetic-select" "Magnetic Selection" Selection LassoSelection None, false,
        "Makes selections that intelligently snap to edges in the document";
    ColorSelection "color-select" "Color Selection" Selection None Some('w'), true,
        "Selects similarly colored areas in an image";
    QuickSelection "quick-select" "Quick Selection" Selection None Some('q'), false,
        "Intelligently selects part of an image as you drag over it";

    // -- Painting --------------------------------------------------------
    Paint "paint" "Paint" Painting None Some('b'), true,
        "Paint with a wide array of brushes";
    PixelPaint "pixel-paint" "Pixel Paint" Painting None None, true,
        "Paint using square pixel blocks";
    ColorFill "color-fill" "Color Fill" Painting None Some('n'), true,
        "Fill an entire layer, or part of one, with a solid color";
    GradientFill "gradient-fill" "Gradient Fill" Painting None Some('g'), true,
        "Fill an entire layer, or part of one, with a gradient";
    Erase "erase" "Erase" Painting None Some('e'), true,
        "Erase part of an image with brushes";
    SmartErase "smart-erase" "Smart Erase" Painting None None, true,
        "Erase similarly colored areas of an image";

    // -- Retouching ------------------------------------------------------
    Repair "repair" "Repair" Retouching None Some('r'), false,
        "Remove small parts or entire objects from an image";
    Clone "clone" "Clone" Retouching None Some('o'), true,
        "Copy one area of an image to another";
    Sharpen "sharpen-tool" "Sharpen" Retouching None None, true,
        "Sharpen part of an image";
    Soften "soften-tool" "Soften" Retouching None None, true,
        "Soften part of an image";
    Smudge "smudge" "Smudge" Retouching None None, true,
        "Smudge part of an image like wet paint";
    Lighten "lighten" "Lighten" Retouching None None, true,
        "Make part of an image lighter";
    Darken "darken" "Darken" Retouching None None, true,
        "Make part of an image darker";
    Saturate "saturate" "Saturate" Retouching None None, true,
        "Make color in part of an image more saturated";
    Desaturate "desaturate" "Desaturate" Retouching None None, true,
        "Make color in part of an image less saturated";
    Distort "distort" "Distort" Retouching None None, false,
        "Push and pull part of an image in any direction";
    Bump "bump-tool" "Bump" Retouching None None, false,
        "Make part of an image appear bulbous";
    Pinch "pinch-tool" "Pinch" Retouching None None, false,
        "Make part of an image appear to be squeezed";
    Twirl "twirl-tool" "Twirl" Retouching None None, false,
        "Rotate pixels in part of an image to look like a spiral";

    // -- Drawing ---------------------------------------------------------
    Shape "shape" "Shape" Drawing Shapes Some('u'), true,
        "Add a shape layer";
    Pen "pen" "Pen" Drawing Pens Some('p'), true,
        "Draw vector lines or shapes by connecting anchor points";
    FreeformPen "freeform-pen" "Freeform Pen" Drawing Pens None, true,
        "Draw vector lines or shapes freehand";
    Rectangle "rectangle" "Rectangle" Drawing Shapes None, true,
        "Add a rectangle";
    RoundedRectangle "rounded-rectangle" "Rounded Rectangle" Drawing Shapes None, true,
        "Add a rounded rectangle";
    Oval "oval" "Oval" Drawing Shapes None, true,
        "Add an oval";
    Polygon "polygon" "Polygon" Drawing Shapes None, true,
        "Add a polygon";
    Star "star" "Star" Drawing Shapes None, true,
        "Add a star";
    Line "line" "Line" Drawing Shapes None, true,
        "Add a line";

    // -- Type ------------------------------------------------------------
    Type "type" "Type" Type TypeTools Some('t'), true,
        "Add text to a document";
    CircularType "circular-type" "Circular Type" Type TypeTools None, false,
        "Add text on a circular path";
    PathType "path-type" "Path Type" Type TypeTools None, false,
        "Add text on a path drawn with anchor points";
    FreeformType "freeform-type" "Freeform Type" Type TypeTools None, false,
        "Add text on a freeform path";
}

impl Tool {
    pub fn info(self) -> &'static ToolInfo {
        TOOLS
            .iter()
            .find(|t| t.tool == self)
            .expect("every Tool variant must have a TOOLS entry")
    }

    pub fn label(self) -> &'static str {
        self.info().label
    }

    pub fn id(self) -> &'static str {
        self.info().id
    }

    pub fn category(self) -> ToolCategory {
        self.info().category
    }

    pub fn shortcut(self) -> Option<char> {
        self.info().shortcut
    }

    pub fn is_implemented(self) -> bool {
        self.info().implemented
    }

    pub fn from_id(id: &str) -> Option<Tool> {
        TOOLS.iter().find(|t| t.id == id).map(|t| t.tool)
    }

    /// The tool bound to a bare letter key.
    pub fn from_shortcut(key: char) -> Option<Tool> {
        let key = key.to_ascii_lowercase();
        TOOLS.iter().find(|t| t.shortcut == Some(key)).map(|t| t.tool)
    }

    /// Next tool in this one's Shift-cycle group, wrapping around. Returns
    /// `self` for tools that are not in a group.
    pub fn cycle(self) -> Tool {
        let group = self.info().group;
        if group == ToolGroup::None {
            return self;
        }
        let members: Vec<Tool> =
            TOOLS.iter().filter(|t| t.group == group).map(|t| t.tool).collect();
        let i = members.iter().position(|&t| t == self).unwrap_or(0);
        members[(i + 1) % members.len()]
    }

    /// Whether this tool paints into a pixel layer, and so requires one to be
    /// active.
    pub fn needs_pixel_layer(self) -> bool {
        matches!(self.category(), ToolCategory::Painting | ToolCategory::Retouching)
    }

    /// Whether the tool draws a marquee rather than modifying pixels.
    pub fn is_selection(self) -> bool {
        self.category() == ToolCategory::Selection
    }
}

pub fn tools_in(category: ToolCategory) -> impl Iterator<Item = &'static ToolInfo> {
    TOOLS.iter().filter(move |t| t.category == category)
}

/// Count of working tools out of the full roster.
pub fn implemented_count() -> (usize, usize) {
    (TOOLS.iter().filter(|t| t.implemented).count(), TOOLS.len())
}

// ---------------------------------------------------------------------------
// Brush settings
// ---------------------------------------------------------------------------

/// Shared options for every brush-driven tool (SPEC §5.2, §5.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrushSettings {
    pub size: f32,
    /// 0 = hard edge, 1 = fully feathered.
    pub softness: f32,
    pub opacity: f32,
    pub flow: f32,
    /// Dab spacing as a fraction of brush size.
    pub spacing: f32,
    pub angle: f32,
    /// 1.0 is round; lower values squash the dab along `angle`.
    pub roundness: f32,
    pub scatter: f32,
    /// Whether stylus pressure drives size and opacity.
    pub pressure_size: bool,
    pub pressure_opacity: bool,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            size: 40.0,
            softness: 0.5,
            opacity: 1.0,
            flow: 1.0,
            spacing: 0.08,
            angle: 0.0,
            roundness: 1.0,
            scatter: 0.0,
            pressure_size: true,
            pressure_opacity: false,
        }
    }
}

impl BrushSettings {
    /// `[` and `]` step the size. Steps are proportional so that the control
    /// feels the same at 5 px and at 500 px.
    pub fn step_size(&mut self, up: bool) {
        let factor = if up { 1.25 } else { 1.0 / 1.25 };
        self.size = (self.size * factor).clamp(1.0, 5000.0);
    }

    /// `Shift-[` and `Shift-]` step the hardness.
    pub fn step_softness(&mut self, softer: bool) {
        let delta = if softer { 0.1 } else { -0.1 };
        self.softness = (self.softness + delta).clamp(0.0, 1.0);
    }

    /// Distance between dabs in pixels, never less than a quarter pixel — a
    /// zero would make stroke interpolation loop forever.
    pub fn dab_spacing(&self) -> f32 {
        (self.size * self.spacing).max(0.25)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_an_entry() {
        // Exercising `info()` on each entry proves the table and the enum agree
        // in one direction; the count check below covers the other.
        for t in TOOLS {
            assert_eq!(t.tool.info().id, t.id);
        }
        assert_eq!(TOOLS.len(), 50, "SPEC §5.1 counts 50 tool entries");
    }

    #[test]
    fn ids_are_unique_and_round_trip() {
        let mut seen = std::collections::HashSet::new();
        for t in TOOLS {
            assert!(seen.insert(t.id), "duplicate tool id {}", t.id);
            assert_eq!(Tool::from_id(t.id), Some(t.tool));
        }
    }

    #[test]
    fn shortcuts_are_unique() {
        let mut seen = std::collections::HashMap::new();
        for t in TOOLS {
            if let Some(k) = t.shortcut {
                assert!(
                    seen.insert(k, t.id).is_none(),
                    "shortcut '{k}' is claimed by both {} and {}",
                    seen[&k],
                    t.id
                );
            }
        }
    }

    #[test]
    fn documented_shortcuts_resolve() {
        assert_eq!(Tool::from_shortcut('b'), Some(Tool::Paint));
        assert_eq!(Tool::from_shortcut('B'), Some(Tool::Paint));
        assert_eq!(Tool::from_shortcut('v'), Some(Tool::Arrange));
        assert_eq!(Tool::from_shortcut('q'), Some(Tool::QuickSelection));
        assert_eq!(Tool::from_shortcut('9'), None);
    }

    #[test]
    fn lasso_group_cycles_through_three() {
        let mut t = Tool::FreeSelection;
        let mut seen = vec![t];
        for _ in 0..2 {
            t = t.cycle();
            seen.push(t);
        }
        assert_eq!(
            seen,
            vec![Tool::FreeSelection, Tool::PolygonalSelection, Tool::MagneticSelection]
        );
        assert_eq!(t.cycle(), Tool::FreeSelection, "should wrap around");
    }

    #[test]
    fn type_group_cycles_through_four() {
        let mut t = Tool::Type;
        for _ in 0..4 {
            t = t.cycle();
        }
        assert_eq!(t, Tool::Type);
    }

    #[test]
    fn ungrouped_tools_do_not_cycle() {
        assert_eq!(Tool::Hand.cycle(), Tool::Hand);
        assert_eq!(Tool::Crop.cycle(), Tool::Crop);
    }

    #[test]
    fn category_counts_match_the_spec() {
        assert_eq!(tools_in(ToolCategory::Basic).count(), 9);
        assert_eq!(tools_in(ToolCategory::Selection).count(), 9);
        assert_eq!(tools_in(ToolCategory::Painting).count(), 6);
        assert_eq!(tools_in(ToolCategory::Retouching).count(), 13);
        assert_eq!(tools_in(ToolCategory::Drawing).count(), 9);
        assert_eq!(tools_in(ToolCategory::Type).count(), 4);
    }

    #[test]
    fn tool_requirements() {
        assert!(Tool::Paint.needs_pixel_layer());
        assert!(Tool::Smudge.needs_pixel_layer());
        assert!(!Tool::Arrange.needs_pixel_layer());
        assert!(Tool::RectangularSelection.is_selection());
        assert!(!Tool::Paint.is_selection());
    }

    #[test]
    fn brush_size_steps_are_proportional_and_clamped() {
        let mut b = BrushSettings { size: 40.0, ..Default::default() };
        b.step_size(true);
        assert!((b.size - 50.0).abs() < 1e-3);
        b.step_size(false);
        assert!((b.size - 40.0).abs() < 1e-3);

        b.size = 1.0;
        for _ in 0..20 {
            b.step_size(false);
        }
        assert_eq!(b.size, 1.0, "size must not go below one pixel");
    }

    #[test]
    fn softness_stays_in_range() {
        let mut b = BrushSettings::default();
        for _ in 0..20 {
            b.step_softness(true);
        }
        assert_eq!(b.softness, 1.0);
        for _ in 0..40 {
            b.step_softness(false);
        }
        assert_eq!(b.softness, 0.0);
    }

    #[test]
    fn dab_spacing_never_reaches_zero() {
        let b = BrushSettings { spacing: 0.0, size: 100.0, ..Default::default() };
        assert!(b.dab_spacing() > 0.0, "zero spacing would hang stroke interpolation");
    }

    #[test]
    fn implementation_status_is_reported() {
        let (done, total) = implemented_count();
        assert_eq!(total, 50);
        assert!(done > 25, "expected a majority of tools to be wired up");
        assert!(done < total, "the unimplemented ones should still be flagged");
    }
}
