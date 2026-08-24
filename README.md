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
./target/release/pixelmagic --check-shaders   # compiles all 43 shaders headlessly
./scripts/smoke-test.sh             # launches the app under Xvfb and drives it
```

`--check-shaders` brings up a surfaceless EGL context, compiles the entire
shader library and exits non-zero on the first failure. It needs no display and
no GPU — Mesa's software rasteriser is enough — which makes it usable in CI.

The headless tests run on desktop GL. GTK hands the application a **GLES**
context on many systems, and the two do not have the same entry points, so the
smoke test — which drives the real app against the real context — is not
optional. It is the only thing that would have caught a desktop-only function
call in the histogram readback, and it did.

---

## Architecture

Four crates, layered so that each one can be replaced without touching the
others.

| Crate | Responsibility | Depends on |
|---|---|---|
| `pixelmagic-core` | Document model: layers, adjustments, effects, masks, selections, history. Pure data and pure functions — no GTK, no GL, no I/O. | — |
| `pixelmagic-gpu` | OpenGL shader library (fragment + compute) and the render graph. Owns no GTK types. | core |
| `pixelmagic-io` | Image decode/encode and the `.pxm` container. | core |
| `pixelmagic-app` | GTK4 front end: canvas widget, sidebars, tools, actions. | all |

The core crate having no dependency on a display server is what makes the model
testable: 169 of the project's 260 tests run without GTK, GL, or a window. The
app touches exactly five symbols from the GPU crate, which is what makes the
rendering backend genuinely replaceable rather than nominally so.

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

### Compute shaders

Kernel-based passes run as compute shaders where the driver supports them
(GL 4.3 / GLES 3.1), staging a tile of the image into workgroup shared memory so
that a blur reads each pixel roughly once instead of once per tap.

Compute is **not** unconditionally faster, and the code says so rather than
assuming. Measured on llvmpipe at 1024², gaussian:

| radius | fragment | compute | ratio |
|-------:|---------:|--------:|------:|
|      8 |   370 ms |  486 ms | 0.76× |
|     24 |   670 ms |  625 ms | 1.07× |
|     48 |  1110 ms |  747 ms | 1.49× |

A dispatch has fixed overhead while the shared-memory saving scales with radius,
so below a crossover the fragment path wins. `Renderer` uses compute only at or
above `PIXELMAGIC_COMPUTE_BLUR_MIN` (default 12). A software rasteriser is the
pessimistic case — it has no on-die scratchpad — so real hardware should cross
over lower; `cargo run --release -p pixelmagic-gpu --example bench` measures it
on yours.

The histogram is the other half of the story, and the more interesting one: a
fragment shader *cannot* compute one, because every invocation can only write to
its own pixel. Compute plus atomics can, which is what puts a live histogram at
the top of the Color Adjustments pane and what will make Auto Contrast and
Auto Color possible.

Every compute path has a fragment fallback that produces the same image, and the
test suite renders both ways and diffs them — two implementations of one blur
drift apart silently otherwise.

### Why OpenGL rather than Vulkan or wgpu

The decisive constraint is not preference, it is that **GTK4 has no Vulkan
surface**. There is no `GtkVulkanArea`; GTK's own maintainers say so on the
Vulkan Roadmap issue. GTK 4.15+ renders *itself* with Vulkan, but that is
internal to GSK — an application cannot draw into it.

So a Vulkan renderer's output has to reach GTK as a `GdkTexture`, either by
exporting a dma-buf (zero-copy, but dependent on `VK_EXT_image_drm_format_modifier`
and historically fragile on NVIDIA's proprietary driver) or by reading every
frame back to the CPU (~32 MB per frame at 4K). `GtkGLArea` gives us a context
GTK already composites from, so GL gets that handoff for free.

What Vulkan would genuinely buy is compute — and GL 4.3 compute, which is what
this crate now uses, delivers most of that at none of the interop risk. If the
remaining gap ever matters (timeline semaphores, descriptor indexing, sharing a
device with ONNX Runtime), `Renderer` owns no GTK types and the app touches five
symbols from this crate, so a Vulkan backend is a rewrite of one crate rather
than of the application.

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
| GPU render graph, 43 shaders (41 fragment + 2 compute) | Working, pixel-tested |
| Compute-shader blur with fragment fallback, benchmarked crossover | Working |
| Live histogram in the Color Adjustments pane | Working |
| Painting, erasing, dodge/burn, saturate, soften, sharpen | Working |
| Rectangular / oval / row / column selections, feather, boolean ops | Working (no marching-ants overlay yet — selections clip tools but are invisible) |
| Undo/redo with gesture coalescing and region snapshots | Working |
| Open PNG/JPEG/TIFF/WebP/BMP/GIF, save `.pxm`, export | Working |
| Adjustments with generated panels (10 of 16) | Working |
| Levels, Curves, colour wheels, Channel Mixer | Model, shaders and histogram exist; need bespoke editors |
| Effects with shaders (41 of ~75) | Working |
| Auto Contrast / Auto Color | Not started — the histogram they need now exists |
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
