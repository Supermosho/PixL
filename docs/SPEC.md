# Pixelmator Pro — Reimplementation Reference for Pixelmagic

**Source of record:** Apple Support — *Pixelmator Pro User Guide for Mac*, base URL
`https://support.apple.com/guide/pixelmator-pro/<slug>/mac`.
Documentation covers **Pixelmator Pro 4.0 or later**; stated system requirement **macOS 26 or later**.

**Research date:** 2026-08-23.

## How to read this document

- Names in `Title Case` are **exact UI strings** transcribed from the Apple Support guide.
- Numeric ranges and defaults are given **only where the guide states them**. Where the guide is
  silent, the cell reads **not documented**. Do not substitute guesses — a fabricated range is worse
  than a missing one.
- **⚠️ unverified** marks any item I could not confirm from a fetched guide page (inferred from
  cross-references, or partially reported by the page).
- `support.pixelmator.com` was not consulted (blocked by robots.txt). Anything only obtainable there
  is marked unverified.

### Global caveat on numeric ranges

The Apple Support guide is written procedurally ("drag the slider to the right to…") and **almost
never publishes slider minimums, maximums, or defaults**. The handful of numeric facts that *are*
published are collected here in one place, because they are the only numbers in this document that
are safe to hard-code:

| Fact | Value | Where documented |
|---|---|---|
| Layer opacity range | 0%–100% | Adjust the opacity of a layer |
| Shadow / Inner Shadow `Distance` | up to 100 px; Option-drag extends past 100 px | Add a drop shadow / Add an inner shadow |
| White Balance `Temperature`, `Tint` | Option-drag extends "beyond 100%" | White balance an image |
| `Vibrance` | Option-drag extends beyond 0–100% | Adjust hue, saturation, and vibrance |
| `Exposure`, `Brightness`, `Contrast`, `Black Point` (Basic) | 0–200% with Option key | Adjust exposure, brightness, and contrast |
| Vignette `Exposure`, `Black Point` | Option-drag extends beyond 100% | Add stylized finishing effects |
| Grain `Size` | Option-drag extends beyond 200% | Add stylized finishing effects |
| Black & White `Intensity` | 0–100% | Convert an image to monochrome |
| Black & White R+G+B total | keep at or below 100% (guidance) | Convert an image to monochrome |
| Channel Mixer R+G+B+Constant total | keep at 100% (guidance) | Mix color channels |
| Sharpen adjustment `Radius` | 0.5–2 px fine detail / 3–10 px landscape (guidance, not limits) | Sharpen an image |
| Sharpen adjustment `Intensity` | 10–30% subtle / 60–100% dramatic (guidance, not limits) | Sharpen an image |
| Pixel Paint `Pixel Size` | Option-drag extends beyond 500 px | Paint and erase with the Pixel Paint tool |
| Color depth | 8 bits/channel ("true color", 16 million values); 16 bits/channel ("deep color", 281 trillion values) | Change the color depth of an image |
| GIF export | 8-bit color depth, 256 colors | Export photos, videos, and documents |
| PNG Export for Web palette reduction | 256 colors | Export a document for the web |

Everything else is qualitative.

---

# 1. Blend modes

Source: *Change the blend mode of a layer* (`change-the-blend-mode-of-a-layer-pix4a1f5998b`).

**UI location:** at the top of the Layers sidebar, a Blend Mode pop-up menu sits next to `Opacity`.
The same blend-mode pop-up is reached from a tool's Opacity pop-up menu in the Style pane, Paint
pane, Clone pane, Gradient Fill pane, Color Fill pane, Pixel Paint pane, and in Fill/Generator/High
Pass/Low Pass/Frequency Separation effects.

The list is presented in six functional groups, in this order. There are **27 modes**.

| # | Group | Mode | Documented behavior |
|---|---|---|---|
| 1 | (Normal) | `Normal` | "The default blend mode for all layers. No color mixing occurs between the blend and the base layers." |
| 2 | Darkening | `Darken` | "Compares the luminance of blend layer and base layer colors and keeps only the darker colors." |
| 3 | Darkening | `Multiply` | "Keeps only the darkest colors of the blend layer, evenly mixing the midtones of both layers. The result is always a darker image." |
| 4 | Darkening | `Color Burn` | "Intensifies the darker areas of a base layer by saturating the midtones and reducing the highlights." |
| 5 | Darkening | `Linear Burn` | "Similar to Multiply, except the midtones are slightly darker than Multiply and less saturated than Color Burn." |
| 6 | Darkening | `Darker Color` | "Compares the color values of the blend layer and base layer and retains only the darker values." |
| 7 | Lightening | `Lighten` | "Emphasizes the highlights of each overlapping layer by making the darker color values translucent and keeping the lighter color values fully opaque." |
| 8 | Lightening | `Screen` | "Emphasizes the highlights of each overlapping layer, evenly mixing the midtones of both layers. The result is always a lighter image." |
| 9 | Lightening | `Color Dodge` | "Intensifies the lighter areas of a base layer by saturating the midtones and increasing highlights." |
| 10 | Lightening | `Linear Dodge` | "The opposite of the Linear Burn blend mode and similar to Screen, except that lighter midtones in overlapping regions become more intense." |
| 11 | Lightening | `Lighter Color` | "Compares the color values of the blend and the base layers and retains only the color values that are lighter." |
| 12 | Contrast | `Overlay` | "Intensifies contrast by darkening colors that are darker than 50% gray and washing out colors that are lighter than 50% gray." |
| 13 | Contrast | `Soft Light` | "Similar to the Overlay blend mode, but offers slightly milder contrast and more even tinting." |
| 14 | Contrast | `Hard Light` | "Intensifies contrast by mixing colors depending on the brightness of the base color values." |
| 15 | Contrast | `Vivid Light` | "Similar to the Hard Light blend mode. Colors darker than 50% gray are darkened by increasing contrast, and colors lighter than 50% gray are lightened by decreasing contrast." |
| 16 | Contrast | `Linear Light` | "Similar to the Hard Light blend mode, except that overlapping midrange color values are mixed together with higher contrast." |
| 17 | Contrast | `Pin Light` | Creates tinted or solarized effects by conditionally replacing color values depending on whether blend-layer colors exceed a 50% gray threshold. |
| 18 | Contrast | `Hard Mix` | "Increases contrast by boosting saturation of the overlapping midrange color values." |
| 19 | Comparative | `Difference` | "Looks at the difference between the color values of the base and the blend layers. The larger the difference between the base and the blend layer colors, the brighter the resulting color." |
| 20 | Comparative | `Exclusion` | "Similar to Difference but with a slightly lower contrast." |
| 21 | Comparative | `Subtract` | Darkens base layers that are lighter than the blend layer; inverts colors where the base is darker. |
| 22 | Comparative | `Divide` | "The opposite of the Subtract blend mode. In areas where the base layer is darker than the blend layer, the base layer colors are lightened." |
| 23 | Component | `Hue` | "Mixes the luminance and saturation of the base layer colors and the hue of the blend layer colors." |
| 24 | Component | `Saturation` | "Mixes the luminance and hue of the base layer and the saturation of the blend layer." |
| 25 | Component | `Color` | "Mixes the luminance of the base layer and the hue and saturation of the blend layer." |
| 26 | Component | `Luminosity` | "The opposite of the Color mode. Mixes the hue and saturation of the base layer and the luminance of the blend layer." |

> **Note on count:** the guide's grouped presentation yields 26 named modes above (Normal + 5
> darkening + 5 lightening + 7 contrast + 4 comparative + 4 component). Some Pixelmator Pro builds
> additionally expose `Dissolve` and `Behind`; **⚠️ unverified** — neither appears in the Apple
> Support blend-mode page.

**Keyboard shortcuts for blend modes:** none documented.

---

# 2. Effects

Source: *Intro to effect categories* and the ten category pages.

## 2.1 Effect categories

| Category | Guide description |
|---|---|
| `Blur` | "soften and defocus images." |
| `Distortion` | "warp and reshape images." |
| `Sharpen` | "enhance image detail and clarity." |
| `Color Adjustment` | "modify colors, brightness, and contrast." |
| `Tile` | "create repeating patterns." |
| `Stylize` | "add creative visual treatments." |
| `Halftone` | "add printing-style dot patterns." |
| `Generator` | "create patterns and shapes." |
| `Fill` | "add colors, gradients, patterns or images." |
| `Other` | "professional image correction and blending techniques for compositing." |

## 2.2 Shared effect controls

From *Adjust effects* and the category pages:

| Control | Meaning |
|---|---|
| `Radius` | "A slider that defines the area of spread for an effect." |
| `Amount` / `Intensity` / `Sunniness` | Strength adjusters; the name varies per effect. |
| `Angle` | "A slider control that sets directional properties for effects with inherent orientation." (rendered as an angle wheel in several effects) |
| `Scale` / `Size` / `Width` | "the size of generated elements or imported content." |
| `Transition` | "gradual changes between different states" — used by focus-type effects. |
| `Opacity` | "A slider that changes the transparency of an effect." |
| Blend mode | Chosen from the pop-up menu next to `Opacity`. Present on Fill, Generator, High Pass, Low Pass, Frequency Separation, Gradient Map. |
| **Effect ropes** | "Graphical controls that appear over your image in the canvas for applied effects that have positioning features." Used to place/size/orient an effect's center, direction, or extent on-canvas. |

**Effect stacking:** "Effects are applied from bottom to top in the Effects pane." Drag to reorder.

**Management commands** (Control-click a layer → `Effects`):
`Copy Effects`, `Paste Effects`, `Reset Effects`, `Flatten Effects`.
`Flatten Effects` permanently merges effects into layer content and reduces file size.

**Adding effects:** select layer(s) → `Effects` button in Tools sidebar → `Add Effect` → category →
effect. Multiple effects may be applied to one layer.

**Effects layer:** Layers sidebar `Add (+)` → `Effects` creates a layer that applies effects
nondestructively to **all layers beneath it**.

**Restrictions:** effects cannot be added to color adjustment layers or empty layers.

**Presets:** an effects preset browser with collections; the guide names `Chroma` and `Photographic`
as examples. Full built-in collection list **⚠️ unverified**.

## 2.3 Blur effects

| Effect | Description | Parameters |
|---|---|---|
| `Gaussian` | "creates a smooth, even blur" | `Radius` |
| `Box` | "creates a harder blur that preserves square corners" | `Radius` |
| `Disc` | "uses disc shapes to simulate camera bokeh blur" | `Radius` |
| `Motion` | "creates directional blur that simulates movement" | `Radius`, `Angle` |
| `Zoom` | "simulates the look of zooming in or out while capturing a photo" | `Amount`, effect ropes |
| `Spin` | "creates rotational blur around a center point like a spinning wheel" | `Amount`, effect ropes |
| `Bokeh` | "creates realistic out-of-focus areas that mimic real-world camera lenses" | `Radius`, `Ring Amount`, `Ring Size` |
| `Tilt-Shift` | "mimics camera focal plane movements to imply shallow depth of field" | `Transition`, effect ropes |
| `Focus` | "creates a tunnel blur around circular in-focus areas" | `Transition`, effect ropes |

All ranges/defaults: **not documented**.

## 2.4 Distortion effects

| Effect | Description | Parameters |
|---|---|---|
| `Bump` | "creates inward or outward bumps in an area" | `Radius`, `Scale`, effect ropes |
| `Pinch` | "squeezes the layer toward a point for a compressed look" | `Radius`, `Scale`, effect ropes |
| `Twirl` | "twists the layer clockwise or counterclockwise around a center point" | `Radius`, `Angle`, effect ropes |
| `Vortex` | (no description text on the page) **⚠️ unverified description** | `Radius`, `Amount`, effect ropes |
| `Displacement Map` | "distorts the layer based on values from an imported grayscale map to create realistic texture effects" | `Scale`, `Angle`, `Smoothness`, imported map image |
| `Circle Splash` | "stretches the layer from a circular area toward the edges" | `Radius`, effect ropes |
| `Hole` | "makes a hole in the layer, pushing and distorting the surrounding area outward" | `Radius`, effect ropes |
| `Light Tunnel` | "stretches the layer with a twistable tunnel effect from an adjustable center point" | `Rotation`, effect ropes |

## 2.5 Sharpen effects

| Effect | Description | Parameters |
|---|---|---|
| `Sharpen` | "improves detail and perceived sharpness by increasing contrast around edges" | `Radius` (size of area around edges affected), `Intensity` (amount of added contrast) |
| `Sharpen Luminance` | "increases detail without affecting color saturation" | `Radius`, `Sharpness` |

## 2.6 Color Adjustment effects

These are the **effect-pane** color operations, distinct from the Color Adjustments pane (§3).

| Effect | Description | Parameters |
|---|---|---|
| `Exposure` | "brightens or darkens the entire image using EV (exposure value)" | `EV` slider |
| `Color Controls` | adjusts saturation, brightness, contrast | `Saturation`, `Brightness`, `Contrast` |
| `Hue Adjust` | "Shifts all colors along the color spectrum using an angle wheel" | `Angle` wheel |
| `Color Monochrome` | "Converts to monochrome with color picker options." | color well, `Intensity` |
| `Sepia Tone` | "Maps colors to various shades of brown." | `Intensity` |
| `False Color` | "Recolors images using a color picker to define highlight and shadow color." | `Color 0` (shadows), `Color 1` (highlights) |
| `Gradient Map` | "Replaces image colors with colors from a selected gradient." | gradient well + color stops, `Opacity`, blend mode |
| `Invert` | "Inverts all image colors to appear negative." | none |
| `Threshold` | "Converts colors to high-contrast black and white." | `Threshold` |

## 2.7 Tile effects

15 effects. All produce repeating patterns; most share `Angle` and `Width`.

| Effect | Description | Parameters |
|---|---|---|
| `Kaleidoscope` | "creates symmetrical reflections in kaleidoscopic patterns" | `Angle`, `Width`, `Count` |
| `Triangle Kaleidoscope` | "creates kaleidoscopic patterns with triangular segments" | `Size`, `Decay`, `Rotation` |
| `Snowflake` | "produces patterns using eight-way reflected symmetry resembling snowflakes" | `Angle`, `Width` |
| `Tessera` | "creates patterns by reflecting in parallelograms with angled segments" | `Angle`, `Width`, `Acute Angle` |
| `Pinwheel` | "rotates images at 90° increments for pinwheel-like patterns" | `Angle`, `Width` |
| `Shutters` | "produces patterns with four adjustable angles" | `Angle`, `Width`, `Acute Angle` |
| `Brickwork` | "creates patterns in a brick wall style with customizable tile sizes" | `Angle`, `Width` |
| `Op` | "creates optical art-style patterns by segmenting and transforming image pieces" | `Scale` |
| `Funhouse` | "makes tiled parallelograms that mimic a funhouse mirror effect" | `Angle`, `Width`, `Acute Angle` |
| `Lattice` | "sections images into hexagonal fragments for mosaic effects" | `Angle`, `Width` |
| `Windmill` | "rotates images at 60-degree increments resembling windmill sails" | `Angle`, `Width` |
| `Triangle Tiles` | "triangular segments of your image with size and decay controls" | `Angle`, `Width` |
| `Hexagon` | "creates patterns by rotating at 30-degree increments with hexagonal shapes" | `Angle`, `Width` |
| `Affine Tile` | "stretches, skews, and rotates images before tiling the transformed result" | `Angle`, `Width`, `Scale`, `Stretch`, `Skew` |
| `Perspective Tile` | "changes image perspective then tiles using the perspective-adjusted version" | `Angle`, `Width` |

> `Triangle Tiles` is described with "size and decay controls" but the parameter table lists
> `Angle`/`Width`. **⚠️ unverified** which is authoritative.

## 2.8 Stylize effects

| Effect | Description | Parameters |
|---|---|---|
| `Light Leak` | "imitates analog photography errors with colorful, glowing streaks of light" | `Amount`, `Sunniness`, effect ropes |
| `Bokeh` | "adds artistic, colorful bokeh shapes" | `Amount`, `Hue`, effect ropes |
| `Vignette` | "gradually darkens outer edges to draw attention to the center" | `Radius`, `Intensity`, `Falloff` |
| `Pixelate` | "creates a pixel effect by increasing the perceived size of the image's pixels" | `Scale` |
| `Pointillize` | "transforms images into pointillist painting styles with adjustable dot sizes" | `Radius` |
| `Crystallize` | "renders images as crystal-inspired geometric shapes" | `Radius` |
| `Bloom` | "softens edges and applies an ethereal glow around bright areas" | `Radius`, `Intensity` |
| `Gloom` | "dulls highlights and applies dark, moody atmospheric effects" | `Radius`, `Intensity` |
| `Spot Light` | "adds adjustable spotlights with precise illumination and shadow control" | `Radius`, `Light Color`, `Background Color`, `Concentration`, effect ropes |
| `Posterize` | "recreates images using fewer colors for silk-screen poster effects" | `Levels` |
| `Grain` | "adds realistic analog film texture to digital images" | `Intensity`, `Size` |
| `Noise` | "adds a color or monochrome digital texture" | `Amount`, `Monochrome` (toggle) |
| `Comics` | "simulates comic book drawings with edge outlines and color halftones" | none listed |

## 2.9 Halftone effects

| Effect | Description | Parameters |
|---|---|---|
| `Circular Screen` | "adds circular black-and-white halftone patterns over a layer" | `Width`, `Sharpness`, effect ropes |
| `CMYK Halftone` | "recreates images using red, yellow, magenta, and black as in four-color printing" | `Width`, `Sharpness`, `Angle`, `Gray Component Replacement`, `Under Color Removal`, effect ropes |
| `Dot Screen` | "recreates images using black-and-white dots as in a halftone screen" | `Width`, `Sharpness`, `Angle`, effect ropes |
| `Hatched Screen` | "recreates images using crosshatched black-and-white halftone lines" | `Width`, `Sharpness`, `Angle`, effect ropes |
| `Line Screen` | "recreates images using parallel black and white line screens" | `Width`, `Sharpness`, `Angle`, effect ropes |

`Width` = "the size of dots or lines in the pattern". `Sharpness` = edge definition; higher = crisper.

## 2.10 Generator effects

All generator effects carry `Opacity`, a blend mode pop-up, and effect ropes.

| Effect | Description | Parameters |
|---|---|---|
| `Checkerboard` | "generates customizable checkerboard patterns" | color well, `Width`, `Sharpness`, `Opacity`, blend mode, effect ropes |
| `Stripes` | "creates adjustable stripe patterns" | color well, `Width`, `Sharpness`, `Opacity`, blend mode, effect ropes |
| `Halo` | "adds colored halo effects" | color well, `Halo Width`, `Halo Radius`, `Time`, `Halo Overlap`, `Striation Strength`, `Striation Contrast`, `Opacity`, blend mode, effect ropes |
| `Star` | "generates starbursts with customizable cross patterns and spikes" | color well, `Cross Width`, `Radius`, `Epsilon`, `Cross Scale`, `Cross Angle`, `Cross Opacity`, `Opacity`, blend mode, effect ropes |
| `Sunbeams` | "creates customizable sun effects" | color well, `Sun Radius`, `Maximum Striation Radius`, `Time`, `Striation Strength`, `Striation Contrast`, `Opacity`, blend mode, effect ropes |
| `Clouds` | "generates two-color soft cloud patterns" | color well, `Width`, `Opacity`, blend mode, effect ropes |

## 2.11 Fill effects

| Effect | Description | Parameters |
|---|---|---|
| `Color` | "fills a layer with a custom solid color." | color well, `Opacity`, blend mode |
| `Gradient` | "creates smooth color transitions across a layer." | gradient well, `Scale`, `Angle`, `Opacity`, blend mode, effect ropes |
| `Pattern` | "fills a layer with a repeating image pattern using imported images." | source image, `Scale`, `Angle`, `Opacity`, blend mode, effect ropes |
| `Image` | "fills a layer with an imported image." | source image, `Scale`, `Angle`, `Opacity`, blend mode, effect ropes |

## 2.12 Other effects

| Effect | Description | Parameters |
|---|---|---|
| `Perspective Transform` | "reshapes a layer to correct lens distortion or create a realistic perspective change." | four-corner effect ropes |
| `Mask to Alpha` | "converts images to black and white while adding transparency to dark areas for compositing work." | none listed |
| `High Pass` | "isolates edges and fine detail by turning uniform areas neutral gray." | `Radius`, `Opacity`, blend mode |
| `Low Pass` | "smooths textures and removes noise, ideal for portrait retouching and skin smoothing." | `Radius`, `Opacity`, blend mode |
| `Frequency Separation` | "separates fine details (high frequency) from colors and tones (low frequency), combining High Pass and Low Pass effects." | `High Pass`, `Low Pass`, `Opacity`, blend mode |

---

# 3. Color adjustments

Source: *Intro to color adjustments*, *Apply a color adjustment*, and the per-adjustment pages.

## 3.1 The Color Adjustments pane

- Opened from the `Color Adjustments` button in the Tools sidebar (shortcut **A**).
- A **histogram** sits at the top of the pane; it can be pinned so it stays visible while scrolling.
- An `Add` button in the pane creates a **color adjustments layer**, inserted above the selected
  layer, which "changes the appearance of all layers below it". Adjustment layers can be reordered,
  masked, hidden/shown, and given their own opacity.
- Adjustments cannot be applied to an effects layer or an empty layer.
- A **Texture-Aware Algorithm** is enabled by default and can be toggled from the pane's `More`
  button menu.
- A `Customize` button at the bottom of the pane adds/removes adjustments from the pane. Notably,
  **`Invert` is not shown in the pane by default** and must be added via `Customize`.
- Presets: color adjustment preset collections exist; the guide names `Classic Films` and `Vintage`
  as examples. Full collection list **⚠️ unverified**.

**Ordered list of adjustment sections as shown in the pane:** the guide never publishes the pane's
complete ordered section list. From *Auto Enhance* and *Match Colors*, the sections named explicitly
are, in this order: `White Balance`, `Basic`, `Hue & Saturation`, `Selective Color`, `Color Balance`.
The remaining sections' exact order in the pane is **⚠️ unverified**.

## 3.2 Histogram modes

Located at the top of the Color Adjustments pane.

| Mode | Description |
|---|---|
| `RGB` (default) | "displays the RGB channels in an image, and their distribution from pure black (left side) to pure white (right side)." |
| `Luminance` | perceived brightness, without analyzing individual color channels separately. |
| `Color` | "the overall distribution of colors in an image organized by spectrum colors—red, orange, yellow, green, cyan, blue, violet, and magenta." |

## 3.3 White Balance

| Control | Behavior | Range |
|---|---|---|
| `Temperature` | drag left to cool, right to warm | Option-drag extends beyond 100% |
| `Tint` | drag left toward green, right toward magenta | Option-drag extends beyond 100% |
| Auto-adjust button | automatic color-cast correction | — |
| Color Picker button | sets white balance from a neutral gray area sampled in the image | — |
| Reset button | restores defaults | — |

No named white-balance presets (Daylight/Tungsten/etc.) are documented.

## 3.4 Hue & Saturation

| Control | Behavior | Range |
|---|---|---|
| `Hue` | "shift all colors along the spectrum" | not documented |
| `Saturation` | right intensifies, left reduces, across the whole image | not documented |
| `Vibrance` | right enhances subtle tones; left reduces oversaturated colors while preserving subtle tones | Option-drag extends beyond 0–100% |

Per-color-range controls are **not** part of this section — those live in `Selective Color` (§3.6).

## 3.5 Basic (Exposure / Lightness)

Page: *Adjust exposure, brightness, and contrast*. Eight sliders in the `Basic` section:

| # | Slider | Behavior | Range |
|---|---|---|---|
| 1 | `Exposure` | "Lightens or darkens the image uniformly" | 0–200% with Option |
| 2 | `Highlights` | "Adjusts exposure in the lightest areas" — recover overexposed detail | not documented |
| 3 | `Shadows` | "Adjusts exposure in the darkest areas" — recover underexposed detail | not documented |
| 4 | `Brightness` | "Adjusts the overall brightness of the image" | 0–200% with Option |
| 5 | `Contrast` | "Sets the relative amount of contrast between light and dark areas" | 0–200% with Option |
| 6 | `Black Point` | "Adjusts the point at which black areas become completely black" | 0–200% with Option |
| 7 | `Texture` | "Enhances surface details and textures without affecting overall contrast" | not documented |
| 8 | `Clarity` | "Enhances local contrast around edges, making images appear sharper" | not documented |

There is **no** separate `Whites` or `Blacks` slider — only `Black Point`.

## 3.6 Color Balance & Selective Color (Color Grading)

Page: *Color grade an image*. Two related sections.

### Color Balance

| Control | Options / behavior |
|---|---|
| Wheel mode | `Master` — "a single color wheel for adjusting overall color tint in the image"; `3-Way Color` — "three color wheels for adjusting color tints in shadows, midtones, and highlights individually" |
| Color wheel | drag the center point toward an edge color to add that hue to the tonal range |
| Brightness slider | on the right side of each wheel |
| Saturation slider | on the left side of each wheel |
| Per-range sliders | `Red/Cyan`, `Green/Magenta`, `Yellow/Blue` for each tonal range |

Tonal ranges in 3-Way mode: **Shadows**, **Midtones**, **Highlights**.

### Selective Color

Eight isolated color ranges: **reds, oranges, yellows, greens, cyans, blues, violets, magentas**.
Each range displays a histogram showing its prevalence in the image.

| Slider | Behavior | Range |
|---|---|---|
| `Hue` | shifts the color's appearance | Option extends range |
| `Saturation` | adjusts intensity within the selected range | not documented |
| `Brightness` | lightens/darkens colors in the range | Option extends range |

## 3.7 Selective Clarity

Page: *Selectively adjust clarity and texture*.

Tonal range selector: `Shadows`, `Midtones`, `Highlights`.

| Slider | Behavior | Range |
|---|---|---|
| `Clarity` | right enhances sharpness/local contrast, left softens | not documented |
| `Texture` | right emphasizes surface details, left smooths | not documented |

## 3.8 Levels

| Element | Options |
|---|---|
| Histogram channel view | `RGB` ("Adjustments affect all three color channels equally"), `Red`, `Green`, `Blue`, `Luminance` ("Adjustments affect brightness and contrast without affecting color saturation") |
| `Black point` handle | "Drag right to make dark areas darker and increase overall contrast" |
| `Midtones` handle | "Drag left to brighten midtones or right to darken them without affecting pure blacks and whites" |
| `White point` handle | "Drag left to make bright areas brighter and increase contrast" |
| `Quarter-tone` handles | "adjust tones between shadows and midtones, or between midtones and highlights, without affecting the main tonal points" |
| Eyedroppers | black point, gray point, white point |
| `More` button | `Auto Contrast`, `Auto Color` |

Numeric input/output value ranges: **not documented**.

## 3.9 Curves

| Element | Options |
|---|---|
| Channels | `RGB` (combined), `Red`, `Green`, `Blue` |
| Point editing | "Click the tonal curve to add a new point, then drag it to adjust"; remove a point by dragging it off the graph |
| Tonal regions (RGB mode) | Highlights (top), Midtones (middle), Shadows (bottom) |
| Eyedroppers | black point, gray point, white point |
| Auto | `Auto Contrast` ("Optimizes brightness and contrast"), `Auto Color` ("Optimizes brightness and contrast in red, green, and blue channels") |

Preset curve shapes (S-curve etc.) are described as techniques, not as UI presets.
A `Luminance` channel is **not** documented for Curves (unlike Levels).

## 3.10 Channel Mixer

Output channel tabs: `Red`, `Green`, `Blue`.
For each output channel, four sliders:

| Slider | Behavior |
|---|---|
| `Red` | "increase or decrease the red contribution to the selected channel" |
| `Green` | "increase or decrease the green contribution to the selected channel" |
| `Blue` | "increase or decrease the blue contribution to the selected channel" |
| `Constant` | "adjust the brightness of the selected channel" |

Guidance: "Keep the total values of the four values in any channel at 100% to maintain brightness."
Explicit slider min/max: **not documented**.

## 3.11 Vignette and Grain (stylized finishing effects)

### Vignette

| Slider | Behavior | Range |
|---|---|---|
| `Exposure` | "Creates exposure-based vignetting that mimics natural lens behavior. Drag right to darken edges or left to brighten them." | Option extends beyond 100% |
| `Black Point` | adjusts vignette darkness | Option extends beyond 100% |
| `Softness` | "Controls how gradually the vignette transitions from the center to edges." | not documented |

### Grain

| Slider | Behavior | Range |
|---|---|---|
| `Size` | "Adjusts the size of individual grain particles." | Option extends beyond 200% |
| `Intensity` | "Controls how visible the grain is." | not documented |

## 3.12 Sharpen (adjustment)

| Slider | Behavior | Guidance |
|---|---|---|
| `Radius` | "Determines how far from each edge the sharpening effect extends." | 0.5–2 px for fine detail/portraits; 3–10 px for landscapes/architecture |
| `Intensity` | "Controls the strength of the sharpening effect." | 10–30% subtle; 60–100% dramatic |

These are **recommended working ranges**, not documented slider limits.

## 3.13 Black & White (monochrome)

| Control | Behavior | Range |
|---|---|---|
| `Red` | red channel's contribution | not documented |
| `Green` | green channel's contribution | not documented |
| `Blue` | blue channel's contribution | not documented |
| `Tone` | "increase the brightness in areas of saturated color" | not documented |
| `Intensity` | "control the strength of the black-and-white effect"; at 100% the image is fully black and white | 0–100% |

Guidance: "Keep the total percentage of the red, green, and blue channels at or below 100% to prevent
blowing out the highlights."
No monochrome film presets are documented in this section.

## 3.14 Replace Color

| Control | Behavior |
|---|---|
| Left color well | the color to replace; an eyedropper samples it from the image |
| Right color well | the replacement color |
| `Range` | "adjust how many similar colors are removed. The higher the value, the broader the range of colors affected" |
| `Intensity` | "control how much the replacement color blends with the original. Lower values create subtle color shifts, while higher values produce complete color replacement" |

Works on images **and videos**. No decontaminate option is documented here (that is a separate
`Decontaminate Colors` command — §7.9).

## 3.15 Invert

| Control | Behavior |
|---|---|
| `Intensity` | "adjust the strength of the inversion" |

Applied via `Format > Color Adjustments > Invert`. Not in the pane by default — add it with the
`Customize` button.

## 3.16 LUT

| Aspect | Detail |
|---|---|
| Supported formats | "Pixelmator Pro supports 1D and 3D LUTs in the .cube file format." |
| `Intensity` | "control how completely the LUT is applied" |
| Import | options menu next to the LUT pop-up → `Choose LUT` → pick a `.cube` file; imported LUTs then appear in the LUT collections menu |
| Built-in | "five pre-installed LUT collections". The individual collection names are **⚠️ unverified**. |

## 3.17 Denoise and Deband

These are modal, ML-driven commands rather than pane sections — see §7.5 and §7.6.

---

# 4. Layer styles

Source: *Apply styles to layers* + the four style pages.

The `Style` tool (Tools sidebar, shortcut **S**) exposes **four style categories** plus layer-level
opacity and blend mode.

| Style | Guide description |
|---|---|
| `Fill` | "Fills the layer with a color, gradient, or pattern." |
| `Stroke` | "Adds an outline to a layer." |
| `Shadow` | "Adds a drop shadow to a layer." |
| `Inner Shadow` | "Adds an inner shadow to a layer." |

Additional Style-pane controls: layer `Opacity` and blend mode.
A Remove button next to each style deletes that style. `Flatten Styles` rasterizes applied styles
into the layer. Copy/paste via `Styles > Copy Styles` / `Styles > Paste Styles`.

**Restriction:** layer styles cannot be applied to effect layers, color adjustment layers, or empty
layers.

## 4.1 Fill

| Control | Options |
|---|---|
| Fill type pop-up | `Color` — "Adds a solid color fill"; `Gradient` — "Adds a gradient fill"; `Pattern` — "Adds a pattern fill based on a source file" |
| Color well | for `Color` type |
| Gradient well + gradient fill bar | color stops for `Gradient` type |
| Source image import | for `Pattern` type |
| `Opacity` | "adjust fill transparency" |
| Blend mode | from the pop-up menu next to `Opacity` |

Gradient *type* (linear/radial/angle) and an angle control are **not documented** on this page for
the Fill *style*; the Gradient Fill *tool* does document them (§5.6). **⚠️ unverified** whether the
Fill style exposes gradient type/angle.

## 4.2 Stroke (outline)

| Control | Options |
|---|---|
| Outline type pop-up | `Color` (solid color outline, with color well); `Gradient` (with color stops in the gradient fill bar); `Pattern` (based on a source file) |
| Stroke width | slider — range **not documented** |
| `Opacity` | slider — range not documented |
| Secondary pop-up ("with the line") | line `Style`, `Position`, `Spacing` |

The enumerated values of `Position` (inside / outside / center) and `Style` (solid / dashed /
dotted) are **⚠️ unverified** — the guide names the controls but not their option lists.

## 4.3 Inner Shadow

| Control | Behavior | Range |
|---|---|---|
| `Blur` | "Drag the slider to adjust the shadow diffusion." | not documented |
| `Distance` | "Drag the slider to adjust how far the shadow is from the object. Press and hold Option while dragging to extend the range past 100 pixels." | 0–100 px, extendable |
| `Angle` | "Drag the wheel to change the angle of the shadow." | wheel; degrees not documented |
| Color well | "Click the color well, then choose a color." | — |
| `Opacity` | "Drag the slider to adjust the transparency of the shadow." | not documented |
| Blend mode | "Click the Opacity pop-up menu, then choose a blend mode." | — |

There is **no** `Spread`/`Choke`/`Size` control documented (unlike Photoshop).

## 4.4 Shadow (drop shadow)

Identical control set to Inner Shadow:

| Control | Behavior | Range |
|---|---|---|
| `Blur` | shadow diffusion | not documented |
| `Distance` | distance from the object; Option-drag extends past 100 px | 0–100 px, extendable |
| `Angle` | angle wheel | not documented |
| Color well | shadow color | — |
| `Opacity` | shadow transparency | not documented |
| Blend mode | via the Opacity pop-up | — |

## 4.5 Layer style presets

- Preset collections exist; the guide names `Gradients` and `Color Outlines` as examples. Complete
  list **⚠️ unverified**.
- Collections export/import as **`.layerstyles`** files.

---

# 5. Tools

Source: *Pixelmator Pro tools*, *Keyboard shortcuts and gestures*, plus per-tool pages.

## 5.1 Tool roster and shortcuts

The Tools sidebar groups tools into seven categories. Shortcuts below are from the *Keyboard
shortcuts and gestures* page.

### Basic tools

| Tool | Shortcut | Description |
|---|---|---|
| `Style` | **S** | "Add fills, strokes, and shadows to layers" |
| `Arrange` | **V** | Select, move, align, resize, rotate, flip layers |
| `Color Adjustments` | **A** | Photo editing / color adjustment controls |
| `Effects` | **F** | Apply visual effects |
| `Crop` | **C** | "Crop and straighten images" |
| `Export for Web` | **K** or **Shift-Command-E** | Optimize images for online use |
| `Color Picker` | **I** | "Sample colors from images" |
| `Zoom` | **Z** | Magnify or reduce the view |
| `Hand` | **H** | Pan the canvas |

### Selection tools (9)

| Tool | Shortcut | Description |
|---|---|---|
| `Rectangular Selection` | **M** | "Makes square and rectangular selections." |
| `Oval Selection` | **Y** (guide labels the shortcut "Elliptical Selection") | "Makes circular and elliptical selections." |
| `Row Selection` | not documented | "Makes horizontal selections of a custom height and the full width of the canvas." |
| `Column Selection` | not documented | "Makes vertical selections of a custom width and the full height of the canvas." |
| `Free Selection` | **L** | "Allows you to draw freehand selections." |
| `Polygonal Selection` | **Shift-L** cycles Free/Polygonal/Magnetic | "Allows you to draw polygonal, jagged selections." |
| `Magnetic Selection` | **Shift-L** cycle | "Makes selections that intelligently snap to edges in the document, including edges within an image." |
| `Color Selection` | **W** | "Selects similarly colored areas in an image." |
| `Quick Selection` | **Q** | "Intelligently selects part of an image as you drag over it." |

### Painting tools (6)

| Tool | Shortcut | Description |
|---|---|---|
| `Paint` | **B** | Brush-based painting |
| `Pixel Paint` | not documented | "Paint using square pixel blocks" |
| `Color Fill` | **N** | Solid color fill of similar areas |
| `Gradient Fill` | **G** | Gradient fill |
| `Erase` | **E** | Brush-based erasing |
| `Smart Erase` | not documented | Erase similarly colored areas |

### Retouching tools (13)

| Tool | Shortcut | Description |
|---|---|---|
| `Repair` | **R** | Repair/remove objects |
| `Clone` | **O** | Clone-stamp from a sampled source |
| `Sharpen` | not documented | "makes areas clearer and better defined" |
| `Soften` | not documented | "creates a blur effect" |
| `Smudge` | not documented | Push pixels like wet paint |
| `Lighten` | not documented | Lighten painted areas |
| `Darken` | not documented | Darken painted areas |
| `Saturate` | not documented | Increase saturation locally |
| `Desaturate` | not documented | Decrease saturation locally |
| `Distort` | not documented | "push and pull pixels of an image in any direction" |
| `Bump` | not documented | Move pixels outward — "bloating effect" |
| `Pinch` | not documented | "pull pixels inwards towards the center creating a 'squeezing' effect" |
| `Twirl` | not documented | "rotate pixels in a circular motion" |

`Distort`, `Bump`, `Pinch`, `Twirl` are collectively the **reshape** tools.

### Drawing tools

| Tool | Shortcut | Description |
|---|---|---|
| `Shape` | **U** (**Shift-U** cycles shape tools) | Insert a shape from the shape browser |
| `Pen` | **P** (**Shift-P** cycles pen tools) | "Draw vector lines by connecting anchor points." |
| `Freeform Pen` | **Shift-P** cycle | Draw freehand vector paths |
| `Rectangle` | **Shift-U** cycle | — |
| `Rounded Rectangle` | **Shift-U** cycle | — |
| `Oval` | **Shift-U** cycle | — |
| `Polygon` | **Shift-U** cycle | — |
| `Star` | **Shift-U** cycle | — |
| `Line` | **Shift-U** cycle | — |

### Type tools (4)

| Tool | Shortcut | Description |
|---|---|---|
| `Type` | **T** (**Shift-T** cycles type tools) | Standard text box |
| `Circular Type` | **Shift-T** cycle | "add curved or circular text lines" |
| `Path Type` | **Shift-T** cycle | Text along a pen-drawn path |
| `Freeform Type` | **Shift-T** cycle | "draw a freeform path and insert text along it" |

**Total:** 9 basic + 9 selection + 6 painting + 13 retouching + 9 drawing + 4 type = **50 tool
entries** (the guide's own summary says "40+ tools"; some drawing entries are variants of `Shape`).

## 5.2 Paint tool options

| Option | Behavior | Range |
|---|---|---|
| Color well | "Click the color well, then select a new paint color" | — |
| `Brush Size` | "Drag the slider to adjust brush size" | not documented |
| `Softness` | "Drag the slider to the right to soften the edges" | not documented |
| `Opacity` | "Drag the slider to adjust brush transparency" | not documented |
| Blend mode | pop-up next to Opacity | — |
| `Advanced Settings` | opens the full brush settings (§5.3) | — |

**Flow** is not documented as a Paint-tool option. **⚠️ unverified** whether one exists.

## 5.3 Brush settings (Advanced Settings)

Source: *Customize brush settings*. Grouped exactly as follows:

| Group | Settings |
|---|---|
| General | `Brush Spacing`, `Shape Angle`, `Smudge`, `Shape`, `Grain`, `Smooth Textures`, `Merge Brush Marks`, `Wetness` |
| Shape | `Shape Direction`, `Scale Shape Horizontally`, `Scale Shape Vertically`, `Initial Direction` |
| Grain | `Grain Spacing`, `Grain Scale`, `Grain Rotation` |
| Stroke | `Tail Start Dynamic`, `Tail End Dynamics`, `Tail Opacity`, `Tail Size` |
| Dynamics | `Opacity by Speed`, `Size by Speed` |
| Scatter | `Brush Scatter`, `Angle Scatter`, `Size Scatter`, `Opacity Scatter`, `Hue Scatter`, `Saturation Scatter`, `Lightness Scatter` |
| Pressure | `Size by Pressure`, `Opacity by Pressure`, `Angle by Tilt`, `Scale Horizontally by Tilt`, `Scale Vertically by Tilt` |

"Pressure settings are used only with graphics tablets."
Ranges/defaults for all brush settings: **not documented**.

## 5.4 Erase tool options

`Brush Size`, `Softness`, `Opacity`, `Advanced Settings`.
The Erase tool has **no blend-mode option** (unlike Paint).

## 5.5 Pixel Paint tool options

| Option | Behavior | Range |
|---|---|---|
| Color well | choose paint color | — |
| `Pixel Size` | "adjust the size of the Pixel Paint tool block" | Option-drag extends beyond 500 px |
| `Opacity` | tool transparency | not documented |
| Blend mode | pop-up next to Opacity | — |
| `Eraser Mode` | toggles paint ↔ erase | — |
| Quick toggle | press **~** (tilde) while dragging to swap paint/erase | — |

## 5.6 Gradient Fill tool options

| Option | Values |
|---|---|
| Gradient type | `Linear` — "blend colors linearly between the color stops"; radial/circular — "blend colors in a circular pattern"; angle — "blend colors at an angle" |
| Color stops | click a stop to open the Colors window; click the gradient fill bar to add a stop; drag a stop away to remove (minimum two stops) |
| Reverse button | reverses stop order |
| Preset library | via the gradient color well; collections can be created, renamed, deleted, imported, exported as **`.gradients`** files |
| `Opacity` | "adjust the transparency of the fill" |
| Blend mode | "how the fill blends with the content or layers below it" |
| Canvas interaction | drag in the canvas to set size and rotation |
| Reset button | discard changes |

The exact menu labels for the three gradient types are given only as "Linear" plus descriptive
phrases; `Radial` and `Angle` as literal menu strings are **⚠️ unverified**.

## 5.7 Color Fill tool options

| Option | Behavior |
|---|---|
| Color well | "Click the color well, and select a new fill color." |
| Blend mode | pop-up |
| `Opacity` | "adjust the transparency of the fill" |
| `Sample all layers` | "have the color fill affect all layers in your composition" |
| `Smooth edges` | "naturally smooth the fill outline" |
| `Preserve transparency` | "fill only the opaque areas of an image, leaving the transparent areas untouched" |

**No tolerance/range slider and no contiguous option are documented** for Color Fill.

## 5.8 Smart Erase tool options

| Option | Behavior |
|---|---|
| `Opacity` | "adjust the transparency of the Smart Erase tool" |
| `Sample all layers` | "make the color fill account for all layers in the canvas" |
| `Smooth Edges` | (guide text on this page reads "fill only the opaque areas of an image" — apparently a doc error; elsewhere Smooth Edges smooths the outline) **⚠️ unverified** |

## 5.9 Repair tool options

`Brush Size`, `Sample all layers`. No softness/opacity/blend mode documented.

## 5.10 Clone tool options

| Option | Behavior |
|---|---|
| `Brush size` | "Drag the slider to adjust the brush size." |
| `Softness` | "Drag the slider to soften brush edges for blending." |
| `Opacity` | "Drag the slider to control the transparency of the cloned area." |
| Blend mode | pop-up next to Opacity |
| `Sample all layers` | sample from all visible layers |
| `Fix source position` | "start cloning from the source marker each time you click or drag" |
| `Show source marker` | display the clone source location |

No "aligned" option is documented.

## 5.11 Lighten / Darken / Sharpen / Soften / Saturate / Desaturate

These six share the same option set:

| Option | Behavior |
|---|---|
| `Brush Size` | "Drag the slider to change brush size." |
| `Softness` | "Drag the slider to soften brush edges for blending." |
| `Strength` | "Drag the slider to adjust the intensity of the effect." (for Sharpen/Soften this is behind the `More` button) |
| Tone range | `All` — "Affects shadows, midtones, and highlights equally"; `Shadows` — "Affects only the darkest areas"; `Midtones` — "Affects only the middle tones"; `Highlights` — "Affects only the brightest areas" |
| Reset button | at the bottom of the pane |

## 5.12 Smudge tool options

`Brush Size`, `Softness`, `Strength`. **No tone-range selector** is documented for Smudge.

## 5.13 Reshape tools (Distort, Bump, Pinch, Twirl)

| Option | Applies to | Behavior |
|---|---|---|
| Brush size | all four | coverage area |
| `Strength` | all four | "the intensity of the effect" |
| Direction | `Twirl` only | `Twirl Right` / `Twirl Left` |

Reached via `Tools > Reshape` or the reshape group icon in the Tools sidebar.

## 5.14 Selection tool options

### Geometric selections (Rectangular, Oval, Row, Column)

- Rectangular / Oval: drag in the canvas. Shift-drag constrains to a perfect square or circle.
- Row / Column: enter a row height or column width, then click in the canvas, or drag to reposition.
- **Modes (new/add/subtract/intersect), antialias, and feather are not documented on the geometric
  selections page.** The selection *modes* are documented on the *Modify selections* page (below).
  Antialias and feather as per-tool options: **⚠️ unverified**.

### Drawing selections (Free, Polygonal, Magnetic)

| Tool | Interaction |
|---|---|
| `Free Selection` | "Drag in the canvas to draw a selection." |
| `Polygonal Selection` | "Click in the canvas to make an anchor point, then click again to add additional anchor points." Close by clicking the origin or double-clicking. |
| `Magnetic Selection` | "Click in the canvas on the edge of an element … then move the pointer to guide your selection around the element's edge." Click to lock additional anchor points; click the start point to close. |

Numeric options (edge width, contrast, frequency) for Magnetic Selection: **not documented**.

### Quick Selection

| Option | Behavior |
|---|---|
| `Brush Size` | brush dimensions |
| `Sample all layers` | include/exclude all visible layers |

### Color Selection

| Option | Behavior |
|---|---|
| `Sample all layers` | select across all enabled layers or limit to the active layer |
| `Smooth edges` | "naturally smooth the selection outline" |

### Select Color Range (Edit menu)

`Sample all layers`, `Smooth edges`, and a `Range` slider that "adjusts the range of color selected".

### Selection modes

| Mode | Behavior |
|---|---|
| `Add` | "Adds areas to the existing selection" |
| `Subtract` | "Subtracts areas from the existing selection" |
| `Intersect` | "Reduces the new selection area within the bounds of the existing selection" |

### Select and Mask (`Edit > Select and Mask`)

| Control | Behavior |
|---|---|
| `Roundness` | "Make the edges rounder" |
| `Softness` | "Make the edges softer" |
| `Expand` | "drag to the right to move the edges outward, and make the selection larger" |
| Refine Edge brush | `Add` / `Subtract` modes, brush size, brush softness |
| `Smart Refine` | one click; "automatically refines the edges to account for image detail" |

### Other selection commands

`Edit > Invert Selection`, `Edit > Deselect`, `Format > Convert to Shape`.
Move the selection outline by dragging or with arrow keys (1 px, or 10 px with Shift).
A dedicated numeric `Feather` dialog is **⚠️ unverified** — the guide exposes softness through
`Select and Mask` rather than a Feather command.

## 5.15 Crop tool options

| Option | Values |
|---|---|
| `Constrain` pop-up | aspect ratio presets or custom. **The individual preset names are not published — ⚠️ unverified.** |
| `Straighten` | "Drag the Straighten slider to the left or right to rotate the image left or right." Also drag outside the crop box to rotate. |
| Perspective | `Vertical` and `Horizontal` sliders |
| Overlays (7) | Rule of thirds (9-rectangle grid), Grid (equal-size squares), Diagonal, Triangle, Golden ratio, Golden spiral, Center (crosshair). **Command-G** cycles overlays. |
| `Delete cropped pixels` | checkbox; by default cropped pixels are preserved/hidden |
| `Auto Crop` | button — automatic edge removal |
| `Auto Straighten` | button — automatic leveling |
| Apply button | confirms |

The Crop tool "affects all layers in your composition."

## 5.16 Arrange tool

Documented capabilities: select one or multiple layers directly in the canvas; move or align layers
manually, automatically, or by entering precise coordinates; resize, rotate, and flip layers.
It is also the tool used to make vector paths editable and to drag guides.

**The exact field names (X/Y/W/H), rotation field, flip buttons, alignment and distribution option
lists, and snapping toggles are not published in the guide — ⚠️ unverified.**

## 5.17 Transform (Arrange tool transform modes)

| Mode | Behavior |
|---|---|
| Resize | "drag any of the layer handles to resize the layer", optionally proportionally |
| `Skew` | "drag a corner layer handle to skew the layer"; "drag a midpoint handle to slant two adjacent corners" |
| `Distort` | "drag a corner layer handle to distort the layer", or manipulate midpoint handles |
| `Perspective` | "drag a corner layer handle to change the perspective vertically or horizontally"; "drag a midpoint handle to slant the layer" |

Applying `Perspective` to a text layer converts it to a shape layer (text becomes non-editable). The
nondestructive `Perspective Transform` **effect** is the alternative.

## 5.18 Warp tool (new in 4.0)

| Control | Behavior |
|---|---|
| `Vertical` | "Drag this slider to the left to make the top of the layer appear closer. Drag it to the right to make the bottom appear closer." |
| `Horizontal` | "Drag this slider to the left to make the left side appear closer. Drag it to the right to make the right side appear closer." |
| `Bend` | "Drag this slider to the left to bend the layer downwards. Drag it to the right to bend the layer upwards." Unavailable for the Cylinder warp type. |
| `Split into Grid` | `3 × 3 Grid` (9 sections), `4 × 4 Grid` (16), `5 × 5 Grid` (25) |
| Split types | crosswise split (intersecting horizontal + vertical), vertical split, horizontal split |
| Warp orientation | horizontal or vertical (not available for all warp types) |
| `Make Editable` | direct handle manipulation on canvas |

The full list of warp *types* (Cylinder is named) is **⚠️ unverified**.

## 5.19 Shape tools

Common shapes: `Rectangle`, `Rounded Rectangle`, `Oval`, `Polygon`, `Star`, `Line`.

Smart-shape on-canvas handles:

| Shape | Adjustable via handles |
|---|---|
| Rounded Rectangle | corner radius (white handles) |
| Polygon / Rhombus | number of sides (drag the white handle clockwise/counterclockwise) |
| Star | number of points (outermost green handle); point width (innermost green handle) |
| Arrows | rod thickness (green handle) |
| Speech bubbles | pointer thickness (innermost green handle), pointer length (outermost green handle) |

General shape editing: resize by dragging handles (optionally proportional); Command-drag to
reposition; Command-drag a handle to rotate; modify fill color or gradient; customize stroke.

Numeric fields for corner radius / sides / points are **⚠️ unverified** — the guide documents only
handle dragging.

### Boolean shape operations

| Command | Behavior |
|---|---|
| `Unite Shapes` | "Combines multiple shapes into a single shape layer (without changing their appearance in the canvas)." |
| `Subtract Shapes` | "Removes any part of the lower layer that's covered by the top layer, then combines the result into one shape." |
| `Intersect Shapes` | "Removes all areas of selected shapes that don't overlap in the canvas, combining the result into a single shape layer." |
| `Exclude Shapes` | "Removes areas where the shapes overlap, combining the result into one shape layer." |

### Vector path editing

- Anchor point types: `Make Sharp Point` and `Make Smooth Point` (Control-click an anchor point).
  Smooth points have "direction handles to reshape its connected paths"; sharp points do not.
- Add a point: "Double-click any part of a vector path between two anchor points."
- Remove points: select and delete.
- `Divide Path`: Control-click a selected anchor point → splits the path.
- `Close`: Control-click an open path → "Pixelmator Pro draws a straight line from endpoint to
  endpoint".
- Workflow: select the `Arrange` tool → Control-click → make path editable.
- Visual feedback: unselected anchor points have a red outline; selected points are solid red.

## 5.20 Type tools

| Aspect | Detail |
|---|---|
| Basic formatting | bold, italic, underline, strikethrough, in the Type pane |
| `Advanced Options` button | character spacing, capitalization, "other typographic refinements" |
| Scope | apply to a whole text layer (select it in the Layers sidebar) or to specific characters (double-click the text box, drag to select) |
| Text on a path | `Freeform Type` (draw a freeform path, press Return); `Path Type` (click anchor points like the Pen tool, press Return); `Circular Type` (draw a circle like the Shape tool) |
| Path text repositioning | Shift-drag the text box handles to move text along the path |
| Overflow | a clipping indicator can be adjusted to display additional text |
| Conversions | `Convert text into an outline`; `Convert text into a shape or pixel layer` |

**Exact names of kerning, tracking, leading, and baseline-shift controls are not published in the
guide — ⚠️ unverified.**

## 5.21 Color Picker tool

| Option | Values |
|---|---|
| Sample size | `1 point sample`, `3 by 3 average`, `5 by 5 average` |
| Color code display | RGB or HEX |
| Color naming | optional display of the nearest color name at the pointer |

## 5.22 Export for Web tool

| Option | Values |
|---|---|
| Formats | PNG, JPEG, GIF, WebP; MP4 for video |
| Quality | slider for JPEG, WebP, MP4 |
| PNG advanced | toggle for a newer compression algorithm; option to "reduce the color palette in your exported file to 256 colors only" |
| Scale factor | scale up/down by specified factors (PNG, JPEG, GIF, WebP) |
| `Add Format` | export multiple versions in different formats/sizes side by side for comparison |
| Presets | built-in presets for "high-quality photographs, web graphics, and vector graphics"; custom presets can be saved |
| Slices | see §6.7 |

---

# 6. Layer types and document model

## 6.1 Layer types

| Layer type | Notes |
|---|---|
| Image layer | photographs and image files; RAW layers carry a dedicated icon |
| Shape layer | vector shapes and graphics |
| Text layer | text; converts to shape layer under Perspective transform |
| Mask layer | bitmap or vector, shown nested under its parent layer |
| Color adjustments layer | affects all layers below it |
| Effects layer | affects all layers below it |
| Video layer | MP4 and QuickTime Movie |
| Group | container for related layers |
| Empty layer | pixel layer with no content; cannot take effects, adjustments, or styles |

Layers behave as "stacked transparent sheets"; "the stacking order of layers in the Layers sidebar
determines which layers appear in front of others in the canvas."

## 6.2 Layers sidebar `Add (+)` menu

`Empty Layer`, `Color Adjustments`, `Effects`, `Text`, `Circular Text`, `Shape`, `Generate Shape`,
Browse (Content Hub), `Photos or Videos`, `Take Photo`, `Generate Image`, `Image Playground`,
`Choose`.

## 6.3 Layer management

| Operation | Command |
|---|---|
| Group | Control-click → `Group`; `Ungroup` to reverse |
| Lock / Unlock | Control-click → `Lock` / `Unlock`, or click the lock icon on hover |
| Hide / show | visibility checkbox next to the layer or group |
| Rename | double-click the layer name, type, Return |
| Color tags | Control-click → choose a color tag |
| Merge | select layers → `Arrange > Merge` (destructive) |
| Flatten all | `Arrange > Merge All` |
| Delete | select and press Delete, or Control-click → `Delete` |
| Filter / search | search field at the bottom of the Layers sidebar; filter icon filters by type, tag, or other criteria |
| Opacity | slider at the top of the Layers sidebar, 0%–100% |
| Blend mode | pop-up next to Opacity |

Layer duplication is **⚠️ unverified** — not covered on the organize-and-manage page.

## 6.4 Masks

| Mask type | Definition |
|---|---|
| Bitmap mask | "a pixel-based mask that can be manually adjusted by painting in the canvas using a brush tool" |
| Vector mask | "a vector-based shape mask added to a layer as any shape or by drawing in the canvas using a vector pen tool" |
| Clipping mask | "linking two or more layers, using the bottom layer's shape to define visibility in the layers above it" |

Masks appear in the Layers sidebar as black-and-white thumbnails beneath their layer; **black hides,
white shows**.

### Bitmap mask commands

- Layers sidebar `Mask` button → `Add Mask` (creates a white mask — whole layer visible).
- `Mask` button → `Hide Background` — automatically detects and masks the background.
- Canvas bottom toolbar: `Paint Mask`, `Erase Mask`, `Settings` (brush size, softness, opacity),
  `Adjust Mask` (mask opacity, density, feather), `Invert Mask`.
- Paint tool method: paint black to hide, white to reveal, gray for partial transparency.
- Control-click a mask → `Refine Mask` (roundness, softness, expand, `Smart Refine`).
- Control-click a mask → `Replace Image Mask`.

### Vector mask commands

- `Mask` button → preset shapes `Rectangle`, `Rounded Rectangle`, `Oval`, `Polygon`, `Star`; or a
  custom shape from the shape browser; or draw with the `Pen` / `Freeform Pen` tool.
- Areas inside the shape are visible; outside becomes transparent.
- Editing: `Shape` button (swap shape); `Adjust Mask` → `Color Mask` / `Gradient Mask`; opacity,
  density, feather sliders; invert.
- `Refine Mask` on a vector mask **converts it to a bitmap mask**.

### Clipping mask commands

- Control-click the upper layer → `Create Clipping Mask`; the result is a "clipping set".
- Control-click any layer in the set → `Release Clipping Mask`.
- No dedicated keyboard shortcut is documented.

### Selection → mask

`Convert a selection to a mask` is a documented workflow (page
`convert-a-selection-to-a-mask-d75p183x6p5k`). Exact command wording **⚠️ unverified**.

## 6.5 Non-destructive model

- **Color adjustments layer** and **Effects layer** each affect *all layers below them*.
- Adjustments and effects may alternatively be attached **directly to a layer**, remaining editable.
- The `.pxd` format "was designed to save these adjustments with your file, so you can modify or
  reset them after closing and reopening your images", retaining text, color adjustments, effects,
  custom layer styles, and editing history so you can return to "your original image, or previous
  editing states".
- `Flatten Effects` and `Flatten Styles` bake changes in permanently.
- RAW layers can be reprocessed to revert destructive edits (Control-click → reprocess).
- `Restore an earlier document version` uses macOS document versions.

**PXD internal structure, format version numbers, and cross-version compatibility are not documented
by Apple Support — ⚠️ unverified.** Any Pixelmagic `.pxd` reader must be reverse-engineered.

## 6.6 Sidecar files

| Aspect | Detail |
|---|---|
| Purpose | "let you save layers and nondestructive edits to standard image file formats such as JPEG, PNG, TIFF, HEIF, or WebP" |
| Extension | **`.pxd-sidecar`** |
| Creation | generated automatically on save, placed alongside the original image |
| Use | opening that image in Pixelmator Pro automatically picks up its sidecar |
| Management | "once you save them, there's no need to update or move them" |
| Settings | `Preserve Edits`, `Save Sidecar Files In` (location pop-up), `Sidecar Disk Usage` (access/remove) |

## 6.7 Supported media formats

### Open / import

`PXD`, `JPEG`, `PNG`, `TIFF`, `HEIC`, `PSD`, `SVG`, `PDF`, `GIF`, `BMP`, `TGA`, `WebP`,
`JPEG-2000`, plus supported RAW files.
Video: `MP4`, `QuickTime Movie`.

RAW: the guide defers to Apple's "Digital camera RAW formats supported by iOS 26, iPadOS 26,
macOS Tahoe 26, and visionOS 26" list. RAW files can be exported "in the original RAW format,
without any adjustments".

### Export

| Format | Guide description |
|---|---|
| `JPEG` | "A compressed format that creates small files suitable for use with websites" |
| `HEIC` | "A compressed file format that creates smaller files than JPEG" |
| `AVIF` | "A royalty-free file format with a powerful compression algorithm to make images suitable for web use" |
| `PNG` | "A lossless file format, with transparency enabled, popular for web images" |
| `WebP` | transparency; "file sizes 25 percent smaller than similar-quality JPEG or PNG" |
| `TIFF` | "A lossless file format that supports 16-bit color and transparency" |
| `SVG` | "A vector file format with properties defined in XML files" |
| `PDF` | "A file format primarily used for reading and producing documents" |
| `JPEG-2000` | "A flexible raster image format with better compression performance than JPEG" |
| `GIF` | 8-bit color depth, 256 colors |
| `BMP` | "An uncompressed raster image format" |
| `OpenEXR` | "An HDR video format" |
| `Pixelmator Pro Document (.PXD)` | native |
| `Photoshop Document (.PSD)` | Adobe native |
| `Motion Project (.motn)` | Motion project |

Export settings include "color profile, quality, dimensions, and more"; per-format option lists are
**not enumerated** in the guide — **⚠️ unverified** beyond Export for Web (§5.22).

### Slices

Divide an image into slices for export: drag across the canvas, or drag layers/groups from the
Layers sidebar into the Export for Web pane. Each slice is listed in Export for Web with its own
export settings. Slices can be moved, resized (black handles), renamed, reordered (`Bring to Front`,
`Send to Back`), deleted individually, or all cleared. Export all slices or a selected subset.

## 6.8 Color management

| Aspect | Detail |
|---|---|
| Profiles | "any RGB profile installed on your Mac"; named examples: `Adobe RGB` ("a wide range of colors … softer and subtler tones") and `sRGB` ("designed for displaying content on digital screens and the web"). Custom profiles go in the ColorSync profiles folder. |
| `Assign Profile` | changes "how the colors are interpreted and displayed, not changing the color values in your image" |
| `Match to Profile` | modifies actual color values so the image "retains as much of its original look as possible"; **permanent** and not reversible by reassigning |
| Access | toolbar `More` button, or the `Image` menu |
| Soft proofing | `View > Soft Proof Colors` |

**Color model:** the guide only ever mentions RGB profiles. CMYK and grayscale document modes are
**not documented** — Pixelmator Pro appears to be RGB-only. **⚠️ unverified** as an explicit
statement.

## 6.9 Color depth

| Depth | Guide description |
|---|---|
| 8 bits per channel | "true color", "16 million color values"; "sufficient for most digital photo editing … basic edits and compositional changes" |
| 16 bits per channel | "deep color", "281 trillion color values"; "recommended for color-sensitive work, advanced corrections, and print workflows where color accuracy is paramount" |

Change via toolbar `More` → `Color Depth`, or `Image > Color Depth`, then OK.
Selectable when creating a new document.
**32-bit float is not documented** (despite OpenEXR export being listed) — **⚠️ unverified**.

## 6.10 Image resize

| Field | Values |
|---|---|
| `Width`, `Height` | numeric |
| Unit pop-up | pixels, inches, centimeters |
| `Resolution` | numeric |
| `Scale proportionally` | checkbox, on by default |
| Resampling algorithm | `Bilinear` — "Good for resizing images in most use cases"; `Lanczos` — "Good for resizing images with small details"; `Nearest Neighbor` — "Copies the color of the nearest pixel when resizing, resulting in a blocky, pixellated look"; `Super Resolution` — "Preserves sharpness and details intelligently. Ideal for making an image larger" |

Related canvas commands: `Change the canvas size`, `Trim the edges of the canvas`,
`Reveal parts of an image beyond the canvas`, `Rotate or flip an image`.

## 6.11 Video layers

| Aspect | Detail |
|---|---|
| Formats | MP4, QuickTime Movie |
| Playback | play, pause, mute, loop in the canvas; Space starts/stops all video playback; selected-layer playback via menu |
| `Edit Video` dialog | scrub, set poster frame (static thumbnail), trim with handles, reposition the clip on the timeline without changing its length |
| End behavior | `Hold Frame`, `Hide Video`, `Loop Video`, `Bounce Video` |
| Editing | video layers accept masks, color adjustments, effects, and styles like image layers |
| Export | MP4 via Export for Web with a quality slider; broader video export capability **⚠️ unverified** |

## 6.12 New document

The New Document dialog offers a `Preset` pop-up menu with "common paper sizes, digital documents,
TV formats, and more", plus settings for document orientation, size, resolution, and color depth.

**Exact preset category names, unit list, resolution defaults, color profile choices, and background
fill options are not published — ⚠️ unverified.**

---

# 7. ML / automatic features

Source: *Intro to automatic image editing* + per-feature pages, plus *What's new*.

| # | Feature | What it does |
|---|---|---|
| 1 | Super Resolution | increase resolution "without sacrificing quality" |
| 2 | Remove Background | removes or hides an image's background |
| 3 | Auto Enhance | automatically improves color quality |
| 4 | Match Colors | "Match the colors of one image to another" |
| 5 | Deband | eliminates "color-banding artifacts" |
| 6 | Denoise | automatically reduces image noise |
| 7 | Auto Crop / Auto Straighten | crop and straighten, "conform them to specific aspect ratios" |
| 8 | Decontaminate Colors | "blend layers seamlessly" by cleaning edge contamination |

Plus the intelligent selection tools (§5.14) and, new in 4.0, the generative features (§7.10, §7.11).

## 7.1 Super Resolution

- "intelligently upscale images while preserving details often lost with traditional scaling methods."
- Invoked from **`Image > Super Resolution`** — runs automatically, **no parameters**.
- For targeted dimensions, use `Super Resolution` as the resampling algorithm in the Image Resize
  dialog (§6.10).
- Maximum size / scale factor limits: **not documented**.

## 7.2 Remove Background

- Automatically detects and removes or hides the background. "works best for images with solid
  colored backgrounds, such as a product shot taken in a studio, or a studio portrait."
- Invoked from a **Remove Background button in the toolbar** (also present as `Hide Background` on
  the Layers sidebar `Mask` button).
- **Two modes:** click → deletes the background; **Option-click** → "automatically add a layer mask
  to hide the background".
- Applied to a **layer group**, it creates a mask rather than deleting; to fully remove within a
  group, apply per-layer.

## 7.3 Auto Enhance

- "an intelligent feature that instantly optimizes the look of an image."
- Invoked from the `Auto Enhance` control in the Color Adjustments pane (Tools sidebar →
  `Color Adjustments`).
- **Exposes no controls of its own.** It writes values into the existing adjustment sliders, which
  remain editable afterward.
- Sections it modifies: `White Balance`, `Basic`, `Hue & Saturation`, `Selective Color`,
  `Color Balance`.

## 7.4 Match Colors

- "copies the color palette of one image to another."
- Invoked from **`Format > Color Adjustments > Match Colors`**.
- Only control: choose a **source image** via a file browser, then click `Match Colors`.
- Modifies the same five sections as Auto Enhance: `White Balance`, `Basic`, `Hue & Saturation`,
  `Selective Color`, `Color Balance`, all of which remain editable.
- No intensity slider is documented.

## 7.5 Deband

- "automatically smooths banded areas in the image", for "images with limited color information or
  … heavily compressed" images.
- Invoked from **`Format > Deband`**.
- UI: a **before/after split view** — original on the left, debanded on the right — with a draggable
  vertical divider, then a `Done` button.
- **No intensity slider is documented for Deband.**
- Recommendation: convert to 16 bits per channel first for best results.

## 7.6 Denoise

- Removes visible grain from low-light or heavily compressed photos.
- Invoked from **`Format > Denoise`**.
- UI: before/after split view with a draggable vertical divider, plus a **`Denoise Intensity`**
  slider in the bottom toolbar, then `Done`.
- Slider range: **not documented**.

## 7.7 Auto Crop

- "remove unwanted edges from your photo".
- Invoked from the **Crop tool** pane → `Auto Crop` button.
- Additional control: an **aspect ratio pop-up** available when Auto Crop is enabled, which
  "adjusts the crop automatically to the selected aspect ratio".
- "intelligently align all layers in your composition."

## 7.8 Auto Straighten

- "level tilted photos and correct uneven horizon lines".
- Invoked from the **Crop tool** pane → `Auto Straighten` button. No further controls.

## 7.9 Decontaminate Colors

- Removes unwanted color artifacts where layers meet, for seamless compositing.
- Invoked from **`Format > Image > Decontaminate Colors`**.
- **No adjustable controls documented** — a one-shot command on the selected layer.

## 7.10 Select Subject / Smart Refine / intelligent selections

| Feature | Detail |
|---|---|
| `Select Subject` | "analyzes the image and selects the primary subject"; a modifier key optionally considers all layers |
| `Smart Refine` | single click that "automatically refines the edges to account for image detail"; available in `Select and Mask` and in `Refine Mask` |
| `Quick Selection` | brush-driven intelligent selection; options `Brush Size`, `Sample all layers` |
| `Magnetic Selection` | anchor-point selection that snaps to edges, including edges *within* an image |
| `Color Selection` | options `Sample all layers`, `Smooth edges` |

## 7.11 Generate Image (generative AI)

| Aspect | Detail |
|---|---|
| Invoked from | Layers sidebar `Add (+)` → `Generate Image` |
| Prompt | text description field |
| Style | `Photorealistic`, `Illustration`, `Line`, and others (full list **⚠️ unverified**) |
| View / aspect ratio | configurable |
| Refinement | edit categories `Details`, `Background`, `Mood`, and others, with suggested phrases or custom text |
| Insert | `Insert` adds the result as a new layer |
| Requirements | **Apple Creator Studio subscription**; subject to usage limits — check via `Pixelmator Pro > Intelligence Features > Show Usage Status` |
| Caveat | "uses third-party generative AI models, and outputs may vary" |

`Image Playground` is also present in the `Add (+)` menu as a separate generation entry point.

## 7.12 Generate Shape (generative vector shapes)

| Aspect | Detail |
|---|---|
| Invoked from | Layers sidebar `Add (+)` → `Generate Shape` |
| Input | text description field |
| Output | multiple shape options, "automatically saved in the shapes browser" |
| Controls | pick a shape to add it as a new layer; reset button regenerates; the prompt can be revised |
| Requirements | Apple Creator Studio subscription; usage limits via `Show Usage Status` |

---

# 8. Interface layout

Source: *Pixelmator Pro interface*, *Use the Pixelmator Pro toolbar*, *Use rulers, guides, and other
canvas overlays*, *Workspace settings*.

Pixelmator Pro is a **single-window application**: "all your tools for editing images and creating
graphic designs are accessible in one space."

```
┌──────────────────────────────────────────────────────────────────────────┐
│  Toolbar (top of window)                                                 │
├────────────────┬────────────────────────────────────┬────────────────────┤
│                │            (ruler)                 │                    │
│ Layers sidebar │                                    │  Tools sidebar     │
│   (LEFT)       │            CANVAS                  │    (RIGHT)         │
│                │   (rulers, grid, guides,           │                    │
│  Add (+)       │    selection handles,              │  Tool groups +     │
│  Mask button   │    quick canvas controls)          │  active tool's     │
│  Opacity       │                                    │  options pane      │
│  Blend Mode    │                                    │                    │
│  layer list    │                                    │                    │
│  search/filter │                                    │                    │
├────────────────┴────────────────────────────────────┴────────────────────┤
│  Canvas bottom toolbar (context-sensitive; e.g. mask editing controls)   │
└──────────────────────────────────────────────────────────────────────────┘
```

## 8.1 Layers sidebar (left)

Displays "elements (images, shapes, color adjustments, effects, masks, text, and so on), also known
as *layers*". Contains, top to bottom:

- `Add (+)` button (§6.2)
- `Mask` button (`Add Mask`, `Hide Background`, vector mask shapes)
- `Opacity` slider (0%–100%) and the Blend Mode pop-up beside it
- the layer list, with visibility checkboxes, lock icons on hover, mask thumbnails nested under
  their layers, and color tags
- at the bottom: a search field for name filtering, and a filter icon (filter by type, tag, or other
  criteria)

## 8.2 Canvas (center)

"the visual workspace where images, brushstrokes, and other elements you've added as layers are
combined to create your composition."

Optional overlays (`View` menu):

| Overlay | Command |
|---|---|
| Rulers | `View > Show Rulers` (left and top edges) |
| Grid | `View > Overlays > Grid` |
| Guides | `View > Overlays > Guides`; add via `View > Guides > Add Guide` or by dragging from a ruler |
| Pixel Grid | `View > Overlays > Pixel Grid` |
| Shape Outline | `View > Overlays > Shape Outline` |
| Layer and Selection Handles | `View > Overlays > Layer and Selection Handles` |
| Quick Canvas Controls | `View > Overlays > Quick Canvas Controls` |

Smart Guides (`View > Smart Guides`): `Show Guides at Object Center`, `Show Guides at Object Edges`,
`Show Relative Sizes`, `Show Relative Spacing`.

Guide management: move guides with the `Arrange` tool; `View > Guides > Lock Guides`;
`View > Guides > Clear Guides`; drag diagonally from the ruler intersection to move the zero origin.

There is also a **context-sensitive bottom toolbar** over the canvas — documented explicitly for
mask editing (`Paint Mask`, `Erase Mask`, `Settings`, `Adjust Mask`, `Invert Mask`) and for Denoise
(`Denoise Intensity`, `Done`).

## 8.3 Tools sidebar (right)

Contains "tools for image editing, composition and layout, graphic design, and digital painting and
illustration", organized by function with a customizable default toolset. Selecting a tool reveals
that tool's **options pane** in the same sidebar (e.g. the Paint pane, Crop pane, Color Adjustments
pane, Effects pane, Style pane, Export for Web pane).

Customization: *Customize the Tools sidebar* describes adding, removing, and rearranging tools, but
**does not publish the full add-a-tool inventory — ⚠️ unverified** (use §5.1 as the working roster).

## 8.4 Toolbar (top)

Default items:

1. Layers Sidebar toggle button
2. Interface elements disclosure menu (tools, rulers, presets, guides)
3. Zoom slider
4. Document name field
5. Color picker
6. Remove Background button
7. Rotate Left / Rotate Right buttons
8. Content Hub button
9. Share / Export button
10. More actions menu

The toolbar is customizable ("Drag an icon from the dialog to the toolbar"); the **complete list of
available toolbar items is not published — ⚠️ unverified**.

> **"Info bar":** the task brief mentions an info bar. The Apple Support interface page does **not**
> name an info bar as a distinct region. The nearest documented equivalents are the toolbar's zoom
> slider / document name field and the context-sensitive canvas bottom toolbar. **⚠️ unverified.**

## 8.5 Workspace settings

Preset workspace layouts optimized for different workflows are documented, including **Photography**,
**Design**, **Illustration**, and a default layout, plus other presets whose names are **⚠️
unverified**. Interface elements outlined in blue can be dragged to new positions to build a custom
workspace.

Appearance/theme (light/dark) options are **not documented** on the workspace settings page —
**⚠️ unverified**.

## 8.6 Other settings panes

| Pane | Documented contents |
|---|---|
| `General` | **⚠️ unverified** (page not fetched in detail) |
| `Editing` | `Open Images In` (Pixelmator Pro format vs. original format), `Preserve Edits`, `Save Sidecar Files In`, `Sidecar Disk Usage`, `Auto Save and Versions` |
| `Ruler, grid, and guide` | **⚠️ unverified** |
| `Workspace` | see §8.5 |

## 8.7 Colors window

Pixelmator Pro uses the standard macOS Colors window, with five picker modes: **color wheel, color
sliders, color palettes, image palettes, pencils**. It includes a screen eyedropper and a swatch row
saved across applications.

Pixelmator Pro ships preset swatch collections named **`Basic`, `Cool`, `Subdued`**. Collections can
be created, added to, redefined, reset to defaults, and imported/exported as **`.colorpalette`**
files.

---

# 9. Keyboard shortcuts

## 9.1 Tools

| Tool | Shortcut |
|---|---|
| Style | S |
| Arrange | V |
| Color Picker | I |
| Free Selection | L |
| Cycle Free / Polygonal / Magnetic Selection | Shift-L |
| Rectangular Selection | M |
| Elliptical (Oval) Selection | Y |
| Color Selection | W |
| Quick Selection | Q |
| Paint | B |
| Erase | E |
| Color Fill | N |
| Gradient Fill | G |
| Repair | R |
| Clone | O |
| Shape | U |
| Cycle Shape tools | Shift-U |
| Pen | P |
| Cycle Pen tools | Shift-P |
| Type | T |
| Cycle Type tools | Shift-T |
| Color Adjustments | A |
| Effects | F |
| Crop | C |
| Export for Web | K or Shift-Command-E |
| Zoom | Z |
| Hand | H |

## 9.2 Canvas, file, edit, layers

| Action | Shortcut |
|---|---|
| Zoom in | Command-+ |
| Zoom out | Command-− |
| Fit to window | Command-0 |
| Actual size | Command-1 |
| Cycle crop overlays | Command-G |
| New document | Command-N |
| Open | Command-O |
| Save | Command-S |
| Export | Command-E |
| Undo | Command-Z |
| Redo | Shift-Command-Z |
| Cut | Command-X |
| Copy | Command-C |
| Paste | Command-V |
| New layer | Shift-Command-N |
| Delete layer | Backspace |
| Select all layers | Option-Command-A |
| Toggle Pixel Paint paint/erase while dragging | ~ (tilde) |
| Constrain to square/circle while dragging a selection | Shift |
| Move selection outline 1 px / 10 px | Arrow keys / Shift-Arrow |
| Start/stop all video playback | Space |
| Extend a slider's range | Option-drag |

Retouch/reshape tool shortcuts, brush size shortcuts (`[` / `]`), and per-blend-mode shortcuts are
**not documented — ⚠️ unverified**.

---

# 10. Implementation notes for Pixelmagic (GTK4)

These are engineering observations, not documented Pixelmator behavior.

1. **Layout mirroring.** Pixelmator Pro puts Layers on the **left** and Tools on the **right** — the
   inverse of GIMP/Photoshop convention. A GTK4 clone should use `GtkPaned` with the tool options
   pane living *inside* the right sidebar rather than as a separate horizontal options bar.
2. **Blend modes.** 26 documented modes; all except `Pin Light`, `Subtract`, `Divide`, `Hard Mix`,
   `Darker Color`, `Lighter Color` map directly to standard Porter-Duff/PDF separable blend formulas.
   The four component modes (`Hue`/`Saturation`/`Color`/`Luminosity`) are the CSS/PDF non-separable
   set.
3. **Two parallel color systems.** The `Color Adjustment` **effects** (§2.6) are a distinct,
   Core-Image-derived set from the `Color Adjustments` **pane** (§3). Pixelmagic needs both, and must
   not merge them.
4. **Effect ropes** are the distinguishing interaction affordance: on-canvas draggable handles that
   set an effect's center/direction/extent. Implement as a per-effect overlay widget contract, not
   ad-hoc per effect.
5. **Non-destructive stack ordering.** Effects apply bottom-to-top within a layer's Effects pane;
   effect layers and adjustment layers apply to everything beneath them in the layer stack. Two
   distinct compositing scopes.
6. **Sidecar model** (`.pxd-sidecar`) is a genuinely useful pattern to copy: it lets a Linux clone
   ship layered editing on top of PNG/JPEG without forcing users into a proprietary container.
7. **Ranges must come from the app, not the docs.** Apple's guide publishes almost no slider
   bounds. Every "not documented" cell in §2–§5 is a value that must be measured against the real
   application (or chosen deliberately and documented as a Pixelmagic decision) before shipping.

---

# 11. Source index

| Section | Page slug |
|---|---|
| Interface | `pixelmator-pro-interface-pix96e754af4` |
| What's new | `whats-new-pix298vw3pm` |
| Blend modes | `change-the-blend-mode-of-a-layer-pix4a1f5998b` |
| Layer opacity | `adjust-the-opacity-of-a-layer-pix6eb83450d` |
| Effect categories | `intro-to-effect-categories-pix502jmg4lg` |
| Add/manage effects | `add-and-manage-effects-pix6zmny4xl5` |
| Adjust effects | `adjust-effects-pix5v36yym9j` |
| Blur | `blur-effects-pix29c8be33c` |
| Distortion | `distortion-effects-pixrw43z36ln` |
| Sharpen (effects) | `sharpen-effects-pix6g0vn51xp` |
| Color Adjustment (effects) | `color-adjustment-effects-pixz2l6335km` |
| Tile | `tile-effects-pixwz35vkyne` |
| Stylize | `stylize-effects-pix3jz9xnzvj` |
| Halftone | `halftone-effects-pix694yex43e` |
| Generator | `generator-effects-pix5vzgmrpny` |
| Fill | `fill-effects-pix5g3w5gx46` |
| Other | `other-effects-pix5p2wyz83g` |
| Effects presets | `use-effects-presets-pixbd56cea3f` |
| Intro to color adjustments | `intro-to-color-adjustments-pix1e05b7354` |
| Apply a color adjustment | `apply-a-color-adjustment-hd2gpemy5qre` |
| White Balance | `white-balance-an-image-pixc1256c396` |
| Hue & Saturation | `adjust-hue-saturation-and-vibrance-pix44f000742` |
| Basic / Exposure | `adjust-exposure-brightness-and-contrast-pix1033797c9` |
| Selective Clarity | `selectively-adjust-clarity-and-texture-pixba90f8dc5` |
| Color Grading | `color-grade-an-image-pix341b49983` |
| Levels | `adjust-tonal-levels-pixd88337f00` |
| Curves | `adjust-tonal-curves-pix0accab39b` |
| Channel Mixer | `mix-color-channels-pix5315004bc` |
| Vignette / Grain | `add-stylized-finishing-effects-pixb8f3ebca` |
| Sharpen (adjustment) | `sharpen-an-image-pix5b2ce3c61` |
| Color adjustment presets | `apply-color-adjustment-presets-pix20726fa4e` |
| Black & White | `convert-an-image-to-monochrome-pix96b22cf98` |
| Replace Color | `remove-replace-colors-image-video-pixc649f3ead` |
| Invert | `invert-the-colors-of-an-image-pixb98a65d80` |
| LUTs | `apply-luts-pixb0deeff19` |
| Histograms | `use-histograms-pix5e1f7d9e1` |
| Layer styles overview | `apply-styles-to-layers-pix1a597f9fb` |
| Stroke | `add-an-outline-around-a-layer-pixbc50c6bb2` |
| Fill style | `fill-a-layer-with-a-color-or-gradient-pix38ba7f0c9` |
| Inner Shadow | `add-an-inner-shadow-to-a-layer-pixe33eaab68` |
| Drop Shadow | `add-a-drop-shadow-to-a-layer-pix5a414d7b8` |
| Layer style presets | `use-layer-style-presets-pix0e9718d51` |
| Tools list | `pixelmator-pro-tools-pixe9d86732d` |
| Customize Tools sidebar | `customize-the-tools-sidebar-pix9368187ed` |
| Toolbar | `use-the-pixelmator-pro-toolbar-pixbb01478cd` |
| Keyboard shortcuts | `keyboard-shortcuts-and-gestures-pix71a3304c4` |
| Color controls | `use-color-controls-pix6960fa2d3` |
| Paint / Erase | `paint-and-erase-using-brushes-pixdcaa7cd75` |
| Brush settings | `customize-brush-settings-pixde4369e5e` |
| Pixel Paint | `paint-and-erase-with-the-pixel-paint-tool-pix112147674` |
| Color Fill tool | `fill-specific-areas-of-an-image-with-color-pix627a4729e` |
| Gradient Fill tool | `fill-a-layer-with-a-color-gradient-pixd232f3f3e` |
| Smart Erase | `erase-similarly-colored-areas-of-an-image-pixa11aaf853` |
| Repair / Clone | `repair-remove-and-clone-objects-in-images-pixc5f9d789e` |
| Lighten / Darken | `lighten-or-darken-areas-of-an-image-pix0cc72b2fc` |
| Sharpen / Soften tools | `sharpen-and-soften-areas-of-images-pix55d4027d6` |
| Smudge | `smudge-an-image-pix42f1fe0a7` |
| Saturate / Desaturate | `adjust-color-saturation-in-areas-of-an-image-pix818ea49a1` |
| Reshape tools | `reshape-areas-of-an-image-pixab55580d5` |
| Transform | `transform-a-layer-pix57476609d` |
| Warp | `warp-a-layer-md49514rvgv2` |
| Crop | `crop-and-straighten-images-pixb0ea7e75d` |
| Image resize | `resize-an-image-pix1db05dd71` |
| Selection tools intro | `intro-to-selection-tools-pix320ebf970` |
| Intelligent selections | `make-intelligent-selections-pixefcaa405e` |
| Geometric selections | `make-geometric-selections-pix92eae1855` |
| Drawing selections | `select-areas-by-drawing-in-the-canvas-pix5b65e22a7` |
| Modify selections | `modify-selections-pix6e781ba66` |
| Shapes | `draw-vector-shapes-and-graphics-pixe136a0db0` |
| Combine shapes | `combine-shapes-pixfd33bcf51` |
| Vector paths | `edit-vector-paths-pix96f612bcc` |
| Text on a path | `add-text-on-a-path-pixbf412d0c7` |
| Format text | `format-text-characters-pix2919203b3` |
| Layers intro | `intro-to-layers-pixea4964220` |
| Create layers | `create-layers-pix6923d1210` |
| Organize layers | `organize-and-manage-layers-pix8f2d79cea` |
| Masks intro | `intro-to-masks-pix3d249bed5` |
| Bitmap masks | `add-and-edit-bitmap-masks-pix9feaf6a04` |
| Vector masks | `add-and-edit-vector-masks-pix0dfc363fb` |
| Clipping masks | `make-a-clipping-mask-pix8c3811aa7` |
| Video layers | `work-with-video-layers-pix7fee5c66d` |
| Canvas overlays | `use-rulers-guides-and-other-canvas-overlays-s783je260z13` |
| PXD format | `about-the-pixelmator-pro-file-format-pixf61bcbc50` |
| Sidecar files | `about-pixelmator-pro-sidecar-files-pix1a7b07504` |
| Supported media formats | `supported-media-formats-a7veyq8q24zq` |
| RAW files | `work-with-raw-files-pix260d78823` |
| Color management | `about-color-management-pix91010c515` |
| Color profile | `change-the-color-profile-of-an-image-pixb20fc44a5` |
| Color depth | `change-the-color-depth-of-an-image-pixa0ae0513d` |
| Export | `export-photos-videos-and-documents-pixa7f02d291` |
| Export for Web | `export-a-document-for-the-web-pixdae75632d` |
| Slices | `slice-documents-into-individual-image-exports-pix11e423acc` |
| Automatic editing intro | `intro-to-automatic-image-editing-xd2gpv04mgy5` |
| Super Resolution | `automatically-increase-image-resolution-pix1d6f0eac3` |
| Remove Background | `remove-or-hide-an-image-background-pix9c48d501c` |
| Auto Enhance | `automatically-enhance-image-color-pix72e5d7b09` |
| Match Colors | `automatically-match-image-colors-pixe0f950ccd` |
| Deband | `remove-color-banding-in-an-image-pix8742a8b3c` |
| Denoise | `automatically-reduce-image-noise-pixb11dfe403` |
| Auto Crop / Straighten | `automatically-crop-and-straighten-images-pixc21dd3a40` |
| Decontaminate Colors | `decontaminate-image-colors-pixb5695f282` |
| Generate Image | `generate-an-image-from-a-text-description-m70xgqkg89n5` |
| Generate Shape | `generate-custom-shapes-z75p04z3wg0m` |
| Editing settings | `editing-settings-pix36c86f487` |
| Workspace settings | `workspace-settings-pix37f268c8c` |
