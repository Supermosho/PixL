//! The visual language.
//!
//! Pixelmator Pro's window is a dark canvas with **floating, translucent
//! panels** laid over it — not a set of docked panes with dividers, which is
//! what GTK gives you by default and what Pixelmagic started as. Getting that
//! right is mostly a matter of three things: force dark, put the panels in a
//! `GtkOverlay` instead of a `GtkPaned`, and give them a rounded translucent
//! background with a hairline border and a real shadow.
//!
//! Values here are read off a screenshot of the original at 2× and converted to
//! logical pixels. They are approximations of someone else's design, not
//! measurements from a spec, so treat them as "matches to the eye" rather than
//! exact.

/// Panel and rail geometry, shared between the CSS and the layout code so the
/// two cannot disagree about how much room to leave.
pub mod metrics {
    /// Layers panel, left edge.
    pub const LAYERS_WIDTH: i32 = 284;
    /// Tool options panel, right, inboard of the rail.
    pub const OPTIONS_WIDTH: i32 = 272;
    /// Tool rail, far right.
    pub const RAIL_WIDTH: i32 = 44;
    /// Gap between a floating panel and the window edge.
    pub const PANEL_MARGIN: i32 = 8;
    /// Corner radius of a floating panel. Must stay in step with the
    /// `border-radius` in [`CSS`] *and* with the renderer's backdrop corner —
    /// the frosting and the border trace the same curve, and a mismatch shows
    /// up as a bright fringe around every panel.
    pub const PANEL_CORNER: f32 = 10.0;
}

/// Application stylesheet.
///
/// Deliberately not a theme override of every widget — libadwaita's dark
/// palette is already close, and fighting it wholesale produces something that
/// looks wrong the moment the user changes their accent colour. Only the parts
/// that define the look are restated.
pub const CSS: &str = r#"
/* ---------------------------------------------------------------------------
   Palette
   --------------------------------------------------------------------------- */
:root {
    --pm-window-bg:   #1c1c1e;
    --pm-panel-bg:    rgba(44, 44, 46, 0.62);
    --pm-panel-line:  rgba(255, 255, 255, 0.10);
    --pm-text:        #f2f2f7;
    --pm-text-dim:    #98989d;
    --pm-row-active:  rgba(255, 255, 255, 0.12);
    --pm-control-bg:  rgba(255, 255, 255, 0.09);
}

window.pixelmagic,
window.pixelmagic .main-area {
    background-color: #1c1c1e;
    color: #f2f2f7;
}

/* ---------------------------------------------------------------------------
   Toolbar
   --------------------------------------------------------------------------- */
.pm-toolbar {
    background-color: #1c1c1e;
    box-shadow: none;
    border: none;
    min-height: 48px;
    padding: 0 8px;
}
.pm-toolbar button.flat {
    min-width: 30px;
    min-height: 26px;
    padding: 2px 6px;
    border-radius: 6px;
    color: #d8d8dc;
}
.pm-toolbar button.flat:hover { background-color: rgba(255,255,255,0.10); }
.pm-doc-title    { font-weight: 600; font-size: 0.95rem; color: #f2f2f7; }
.pm-doc-subtitle { font-size: 0.78rem; color: #98989d; }

/* The zoom slider: short, with a coloured fill to the left of the handle. */
.pm-zoom trough  { min-height: 4px; background-color: rgba(255,255,255,0.16); }
.pm-zoom highlight { background-color: @accent_bg_color; }
.pm-zoom slider  {
    min-width: 14px; min-height: 14px;
    background-color: #f2f2f7;
    border: none;
    box-shadow: 0 1px 2px rgba(0,0,0,0.4);
}

/* ---------------------------------------------------------------------------
   Floating panels
   --------------------------------------------------------------------------- */
.pm-panel {
    /* Deliberately translucent: the renderer draws a blurred, tinted copy of
       the canvas underneath this exact rectangle (see `backdrop.frag`), and
       this layer is only the sheen and border on top of it. If the GL frosting
       is ever unavailable the panel is still 62% opaque, which stays legible
       over a photograph — that is why this is not lower. */
    background-color: rgba(44, 44, 46, 0.62);
    border: 1px solid rgba(255, 255, 255, 0.10);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
}
.pm-panel-title {
    font-weight: 600;
    font-size: 0.92rem;
    color: #f2f2f7;
}
.pm-panel-header {
    padding: 8px 10px 4px 10px;
}
.pm-panel separator { background-color: rgba(255,255,255,0.08); }

/* Everything inside a panel must let the frosting through. GTK's containers
   default to painting the theme's window colour, and a single opaque child
   turns the panel back into a flat rectangle. */
.pm-panel scrolledwindow,
.pm-panel viewport,
.pm-panel list,
.pm-panel listview,
.pm-panel row,
.pm-panel-body {
    background-color: transparent;
    background-image: none;
}
.pm-layer-list { background-color: transparent; }

/* ---------------------------------------------------------------------------
   Layers panel
   --------------------------------------------------------------------------- */
.pm-layer-row {
    border-radius: 7px;
    padding: 5px 6px;
    margin: 1px 6px;
}
.pm-layer-row:selected,
.pm-layer-row.selected {
    background-color: rgba(255, 255, 255, 0.13);
}
.pm-layer-name { font-size: 0.9rem; color: #f2f2f7; }
.pm-layer-meta { font-size: 0.75rem; color: #98989d; }
.pm-thumb {
    border-radius: 4px;
    background-color: rgba(255,255,255,0.06);
}
/* Pixelmator's visibility control is a filled accent checkbox, not GTK's
   outline style. */
.pm-visible check {
    min-width: 15px; min-height: 15px;
    border-radius: 4px;
}
.pm-search {
    margin: 6px;
    border-radius: 7px;
    background-color: rgba(255,255,255,0.07);
    min-height: 28px;
}

/* ---------------------------------------------------------------------------
   Tool rail
   --------------------------------------------------------------------------- */
/* A little more opaque than the wide panels: the rail is only 44px across, so
   the frosting underneath averages a much smaller patch of canvas and stays
   colourful. The icons need a calmer ground than that to read against. */
.pm-rail {
    background-color: rgba(44, 44, 46, 0.74);
    border: 1px solid rgba(255, 255, 255, 0.10);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
    padding: 4px 0;
}
.pm-rail button.tool-button {
    min-width: 34px;
    min-height: 30px;
    margin: 1px 3px;
    padding: 0;
    border-radius: 7px;
    font-size: 14px;
    color: #d8d8dc;
    background: none;
    border: none;
    box-shadow: none;
}
.pm-rail button.tool-button:hover   { background-color: rgba(255,255,255,0.10); }
.pm-rail button.tool-button:checked { background-color: rgba(255,255,255,0.18); color: #ffffff; }
.pm-rail button.tool-button:disabled { color: rgba(216,216,220,0.28); }

/* ---------------------------------------------------------------------------
   Controls inside panels
   --------------------------------------------------------------------------- */
.pm-panel .param-row { margin: 1px 0; }
.pm-panel label { color: #d8d8dc; font-size: 0.85rem; }
.pm-panel .dim-label { color: #98989d; }
.pm-panel button {
    border-radius: 6px;
    min-height: 26px;
    font-size: 0.85rem;
}
.pm-panel dropdown > button { min-height: 26px; border-radius: 6px; }
.pm-panel spinbutton { min-height: 26px; border-radius: 6px; }

.pm-histogram { border-radius: 6px; margin: 2px 8px 0 8px; }
.pm-section {
    font-size: 0.82rem;
    font-weight: 600;
    color: #98989d;
}

/* Pixelmator's visibility checkbox and opacity slider use the system accent,
   which in the reference screenshot is red. Restated explicitly so the look
   holds regardless of the user's GNOME accent colour. */
.pm-visible check:checked,
.pm-visible check:indeterminate {
    background-color: #ff453a;
    border-color: #ff453a;
    color: #ffffff;
}
.pm-opacity highlight { background-color: #ff453a; }
.pm-opacity trough    { min-height: 4px; background-color: rgba(255,255,255,0.16); }
.pm-opacity slider {
    min-width: 14px; min-height: 14px;
    background-color: #f2f2f7;
    border: none;
    box-shadow: 0 1px 2px rgba(0,0,0,0.4);
}

/* The info strip along the bottom of the window. */
/* The strip sits directly on the canvas with nothing behind it, so on a light
   image the dim grey would vanish. A shadow costs nothing and keeps it legible
   over anything. */
.pm-status {
    font-size: 0.78rem;
    color: #c8c8cd;
    padding: 3px 12px;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.9);
}
"#;
