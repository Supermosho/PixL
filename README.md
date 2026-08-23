# Pixelmagic

A GTK4-native, GPU-accelerated, non-destructive image editor for Linux.

Pixelmagic is a clean-room reimplementation of the *interaction model* of
Pixelmator Pro — layer-based, non-destructive, with adjustments and effects that
stay editable forever — built on GTK4, libadwaita and OpenGL. It is not a port,
a wrapper, or a compatibility layer. It shares no code, no assets and no file
format with Pixelmator Pro.

> **Status: early.** The engine works and is tested. The application shell runs,
> opens images, paints, selects, adjusts, undoes and exports. Large parts of the
> feature surface are catalogued but not yet implemented, and the app tells you
> which — see [Honest status](#honest-status).

---

## Why

Pixelmator Pro is macOS-only, and there is no compatibility layer that runs it.
The gap on Linux is not raw capability — GIMP and Krita are both more powerful
in places — it is the *shape* of the tool: a single non-destructive layer stack
where every adjustment and effect remains a live, re-editable node, with an
interface that stays out of the way.

That model is reproducible. The rest of this document is about how.

---

## Building

Requires Rust 1.80+, GTK 4.14+, libadwaita 1.5+ and OpenGL 3.3 (or GLES 3.0).

```sh
# Debian / Ubuntu
sudo apt install libgtk-4-dev libadwaita-1-dev libepoxy-dev \
                 libgl1-mesa-dev build-essential pkg-config

# Fedora
sudo dnf install gtk4-devel libadwaita-devel libepoxy-devel mesa-libGL-devel

# Arch
sudo pacman -S gtk4 libadwaita libepoxy

cargo build --release
./target/release/pixelmagic
```

### Verifying a build

```sh
cargo test                          # model, engine and I/O
./target/release/pixelmagic --check-shaders   # compiles all 41 shaders headlessly
./scripts/smoke-test.sh             # launches the app under Xvfb and drives it
```

`--check-shaders` brings up a surfaceless EGL context, compiles the entire
shader library and exits non-zero on the first failure. It needs no display and
no GPU — Mesa's software rasteriser is enough — which makes it usable in CI.

---

## Architecture

Four crates, layered so that each one can be replaced without touching the
others.

| Crate | Responsibility | Depends on |
|---|---|---|
| `pixelmagic-core` | Document model: layers, adjustments, effects, masks, selections, history. Pure data and pure functions — no GTK, no GL, no I/O. | — |
| `pixelmagic-gpu` | OpenGL shader library and the render graph. Owns no GTK types. | core |
| `pixelmagic-io` | Image decode/encode and the `.pxm` container. | core |
| `pixelmagic-app` | GTK4 front end: canvas widget, sidebars, tools, actions. | all |

The core crate having no dependency on a display server is what makes the model
testable: 169 of the project's tests run without GTK, GL, or a window.

### The rendering pipeline

Everything composites in **premultiplied, linear-light RGBA16F**.

```
source PNG (sRGB, straight alpha)
      │  uploaded as SRGB8_ALPHA8 — the texture unit decodes to linear for free
      ▼
  place  ─── inverse-maps the layer transform, premultiplies
      ▼
  mask   ─── multiplies coverage
      ▼
  adjustments ─── chain of fullscreen passes
      ▼
  effects     ─── chain of fullscreen passes
      ▼
  composite ─── 26 blend modes, Porter-Duff over the accumulator
      ▼
  present ─── linear → sRGB, over a transparency checkerboard
```

Three decisions in there are load-bearing, and each cost a bug to learn:

- **Source textures are `SRGB8_ALPHA8`, not `RGBA8`.** Treating gamma-encoded
  source data as linear makes every blend and every blur subtly wrong.
- **Premultiplication happens after the sRGB decode**, in the shader, because
  `encode(c) · a ≠ encode(c · a)`.
- **Blend *functions* run on encoded values by default** (`u_blend_gamma`),
  matching Photoshop and Core Image, while alpha compositing stays linear.
  Running `Overlay` in linear light pivots it around a mid-grey that is no
  longer perceptually mid.

### Why OpenGL rather than Vulkan or wgpu

The renderer has to hand frames to GTK. `GtkGLArea` provides a GL context that
GTK already composites from, so rendering into it is zero-copy. Reaching the
same point from wgpu means either reading every frame back to the CPU, or
exporting Vulkan memory as a dma-buf and importing it as a `GdkDmabufTexture` —
which works beautifully on Mesa and is fragile elsewhere. GL runs everywhere GTK
runs, including software rasterisation.

`Renderer` owns no GTK types, so a future Vulkan backend is a rewrite of one
crate, not of the app.

---

## Honest status

The tool roster and effect catalogue are complete *as data* — all 50 tools and
~75 effects are catalogued from Apple's published documentation, with their real
names, categories and parameters. Implementation lags that catalogue, and the
app is explicit about the gap rather than hiding it: unimplemented tools are
visible but disabled, unimplemented effects say so in their tooltip, and the
About window reports both counts.

| Area | State |
|---|---|
| Layer tree, groups, blend modes, opacity, masks | Working |
| Non-destructive adjustment and effect stacks | Working |
| GPU render graph, 41 shaders | Working, pixel-tested |
| Painting, erasing, dodge/burn, saturate, soften, sharpen | Working |
| Rectangular / oval / row / column selections, feather, boolean ops | Working (no marching-ants overlay yet — selections clip tools but are invisible) |
| Undo/redo with gesture coalescing and region snapshots | Working |
| Open PNG/JPEG/TIFF/WebP/BMP/GIF, save `.pxm`, export | Working |
| Adjustments with generated panels (10 of 16) | Working |
| Levels, Curves, colour wheels, Channel Mixer | Model + shaders exist; need bespoke editors |
| Effects with shaders (41 of ~75) | Working |
| Shape, text and vector layers | Modelled, not rendered |
| ML features (Super Resolution, Remove Background, …) | Not started — see the roadmap |
| RAW, PSD, SVG, PDF import | Not started |
| `.pxd` (Pixelmator's format) | Not started, and undocumented |

`docs/ROADMAP.md` has the sequencing. `docs/SPEC.md` is the 1,700-line
reimplementation reference the feature set is drawn from, with every unverified
claim marked as such.

---

## The `.pxm` document format

A Zip archive, deliberately inspectable:

```
mimetype          stored uncompressed — "application/x-pixelmagic"
document.json     the whole model minus pixel data
layers/<n>.png    one PNG per pixel layer, depth-first order
masks/<n>.png     one greyscale PNG per bitmap mask
```

A document is recoverable with `unzip` even if this code disappears. That is the
point.

---

## A note on Pixelmator Pro

Pixelmagic reimplements *behaviour* — what a tool does, what a slider is called,
how a non-destructive stack composes. It contains none of Apple's code, icons,
artwork or trade dress, and it does not read or write `.pxd`. Every name and
description in `docs/SPEC.md` is cited to Apple's public user guide, and every
number the guide does not publish is marked as our own choice rather than
presented as theirs.

Pixelmator and Pixelmator Pro are trademarks of Apple Inc. This project is not
affiliated with, endorsed by, or connected to Apple.

---

## Licence

GPL-3.0-or-later. See [LICENSE](LICENSE).
