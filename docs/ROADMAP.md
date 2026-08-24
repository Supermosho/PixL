# Roadmap

Sequenced by what unblocks the most downstream work, not by what is most
visible. Each milestone is meant to leave the app in a shippable state.

An honest framing first: **Pixelmator Pro represents on the order of 100+
person-years.** Nothing here promises parity. What it promises is a coherent
tool that gets more useful at every step, built in an order where each piece
makes the next one cheaper.

---

## Done

- **Document model** — layer tree with groups, 26 blend modes, per-layer
  adjustment and effect stacks, bitmap and vector masks, clipping, styles,
  selections, colour tags, locking.
- **Render graph** — GPU compositing in premultiplied linear-light RGBA16F,
  41 shaders, target pooling, revision-keyed texture caching, pixel-verified
  against hand-computed expectations.
- **Undo/redo** — command objects with region snapshots and gesture coalescing,
  so a slider drag is one step and a brush stroke stores only the rectangle it
  touched.
- **Brush engine** — dab stamping with spacing, softness, flow, selection
  clipping; paint, erase, dodge, burn, saturate, desaturate, soften, sharpen.
- **Selections** — rectangular, oval, row, column, path; anti-aliasing, feather,
  and the four boolean modes.
- **I/O** — PNG, JPEG, TIFF, WebP, BMP, GIF; the `.pxm` container.
- **App shell** — canvas with pan and zoom, tool rail, layers sidebar,
  generated adjustment and effect panels, keyboard shortcuts, export.
- **Compute shaders** — capability detection with a fragment fallback for every
  path, a shared-memory separable blur, and a benchmarked crossover radius below
  which the fragment path is still faster.
- **Histogram** — 256 bins × RGB + luma via compute atomics, drawn at the top of
  the Color Adjustments pane. The thing a fragment shader cannot do at all.

---

## 1 — Close the gap between catalogue and implementation

The highest-value work, because the surface already exists and the panels
generate themselves.

- **Bespoke editors** for Levels, Curves, Channel Mixer and the colour wheels.
  The models, shaders and now the histogram are all in place; each needs a
  custom widget that draws on top of it.
- **Auto Contrast and Auto Color**, which are a short step from the histogram:
  find the black and white points per channel and write them into Levels.
- **Live histogram updates** during a slider drag. It currently refreshes when
  the pane is opened, because recomputing it means re-rendering the document and
  doing that per slider tick needs a debounce first.
- **The remaining ~34 effects.** Most are 20–40 lines of GLSL against an
  existing pass structure. `EffectDescriptor::implemented` is the checklist.
- **Layer styles rendered** — fill, stroke, inner and drop shadow. The model and
  bounds arithmetic exist; they need three shader passes.
- **Selection overlay.** Selections already clip every tool, but nothing draws
  them — there are no marching ants. This is the most jarring gap in the app
  today and the cheapest to close.
- **Repair tool** (content-aware fill). The one retouching tool with real
  algorithmic depth; PatchMatch is the standard approach.

## 2 — Vector and text

Currently modelled but not rendered, which is the largest single hole.

- **Path rasterisation.** Either tessellate to triangles on the GPU (`lyon`), or
  rasterise with cairo and upload. Cairo first — correctness beats speed here,
  and it unblocks vector masks and shape layers at once.
- **Text layout via Pango**, with the layer reporting real measured extents
  instead of the current estimate. Then text on a path for the three remaining
  type tools.
- **Pen and Freeform Pen** editing on canvas: anchor points, direction handles,
  the boolean operations.

## 3 — Colour management

Currently sRGB in, sRGB out, which is fine until it is not.

- **LittleCMS** for ICC profiles: embedded profiles on import, display transform
  to the monitor's profile, soft proofing.
- **Display P3 and Rec. 2020** working spaces; 16-bit and 32-bit documents (the
  renderer is already floating point, the CPU buffers are not).
- **LUTs** — `.cube` parsing, 1-D and 3-D, as a 3-D texture pass.

## 4 — The intelligent features

This is where Linux is closest to parity, because the open models are good.

| Pixelmator feature | Approach |
|---|---|
| Remove Background | BiRefNet or RMBG-2.0 via ONNX Runtime |
| Super Resolution | Real-ESRGAN or SwinIR |
| Denoise | SCUNet / NAFNet |
| Select Subject, Quick Selection | SAM 2 or MobileSAM |
| Auto Enhance, Auto Crop, Auto Straighten | Classical CV — histogram analysis, saliency, Hough lines |
| Deband | Dithered gradient reconstruction; no model needed |

Models ship separately and are downloaded on first use, so the binary stays
small and the licences stay clean. Everything runs locally.

## 5 — Interchange

- **RAW** via LibRaw — ~1,000 cameras, with a reprocessable RAW layer as
  Pixelmator has.
- **PSD** read and write. Well documented, and the format that actually matters
  for moving work between tools.
- **SVG and PDF** import once vector rendering lands.
- **`.pxd`** — undocumented, requires reverse engineering from sample files, and
  should only be attempted once there is a corpus to test against. A reader that
  silently drops adjustments is worse than none.

## 6 — Scale and polish

- **Tiled rendering.** Canvas-sized intermediates are the right first answer;
  they stop being right somewhere around 100-megapixel documents. Tiling is a
  change inside the renderer, invisible to everything else.
- **On-canvas effect controls** — Pixelmator's "ropes" — replacing the
  normalised spin buttons the `Point` parameter currently generates.
- **Video layers**, which the model already carries so documents round-trip.
- **Flatpak packaging**, and a real icon set.

---

## Not planned

- Reproducing Apple's icons, artwork or trade dress. Pixelmagic gets its own
  visual language.
- Any form of `.pxd` writing. Reading for interoperability is defensible;
  writing a proprietary format is not.
