//! The render graph.
//!
//! Rendering a document is a post-order walk of the layer tree. Each layer
//! becomes a canvas-sized RGBA16F target; its adjustments and effects run as a
//! chain of fullscreen passes over that target; then it is composited onto the
//! accumulated backdrop with its blend mode and opacity.
//!
//! ## Why canvas-sized intermediates
//!
//! Rendering each layer at its own bounds and compositing sub-rectangles would
//! use less memory and less fill rate. It also makes every effect that reads
//! neighbouring pixels — which is most of them — need bounds expansion logic,
//! and makes a layer that extends past the canvas a special case in a dozen
//! places. Canvas-sized targets are the boring choice, and boring is the right
//! trade until profiling says otherwise. The [`TargetPool`] keeps the
//! allocation cost near zero; the fill-rate cost is the real one, and it is
//! bounded by (layer count × canvas area).
//!
//! ## Adjustment and effects layers
//!
//! These do not composite. They take the accumulated backdrop as input and
//! replace it — which is exactly what "affects all layers below it" means.

use glam::{Mat3, Vec2};
use pixelmagic_core::adjust::{Adjustment, AdjustmentInstance, BalanceMode, ToneChannel};
use pixelmagic_core::blend::BlendMode;
use pixelmagic_core::curve::LUT_SIZE;
use pixelmagic_core::document::Document;
use pixelmagic_core::effect::{Effect, EffectCategory};
use pixelmagic_core::layer::{Layer, LayerId, LayerKind, Mask};
use pixelmagic_core::param::ParamValue;
use std::collections::HashMap;
use std::rc::Rc;

use crate::compute::{Capabilities, ComputeLibrary, StorageBuffer};
use crate::program::{GlFlavor, ShaderLibrary};
use crate::texture::{Filter, Format, RenderTarget, TargetPool, Texture, Wrap};
use crate::{GpuError, Result};

/// Cached GPU copy of a layer's content, with the revision it was built from.
struct LayerCache {
    texture: Texture,
    /// Bumped by the app whenever the layer's pixels change, so the renderer
    /// knows to re-upload without having to diff megabytes.
    revision: u64,
}

/// Statistics from the last frame, surfaced in the diagnostics panel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub layers_drawn: usize,
    pub layers_skipped: usize,
    pub passes: usize,
    pub uploads: usize,
    /// Compute dispatches, as opposed to fragment passes.
    pub dispatches: usize,
}

/// A panel rectangle to frost, in top-left-origin widget pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackdropRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// How the frosted-glass backdrop looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackdropStyle {
    /// Blur radius in widget pixels, before downscaling.
    pub radius: f32,
    /// Downsample factor for the blur. Larger is cheaper and, at these radii,
    /// indistinguishable.
    pub scale: u32,
    /// Corner radius in widget pixels. Must match the panel's CSS, or the
    /// frosting and the border will trace different curves.
    pub corner: f32,
    /// Panel colour and how much of it covers the blurred backdrop.
    pub tint: [f32; 4],
    /// Overall strength, so the whole effect can be turned down or off.
    pub opacity: f32,
}

impl Default for BackdropStyle {
    fn default() -> Self {
        Self {
            radius: 64.0,
            scale: 4,
            corner: 10.0,
            // Grey of `--pm-panel-bg`, at the share of the tint this layer
            // is responsible for. The stylesheet paints the rest on top, and
            // the two together leave roughly a quarter of the blurred canvas
            // showing — enough to read as glass, not enough to fight the text.
            tint: [0.172, 0.172, 0.180, 0.50],
            opacity: 1.0,
        }
    }
}

/// How a selection overlay is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionOverlayStyle {
    /// Dash animation phase in device pixels. Advance it every frame to make
    /// the ants crawl; the direction of travel is the sign of the increment.
    pub phase: f32,
    /// Length of one light-plus-dark dash pair, in device pixels.
    pub dash: f32,
    /// Tint over the selected interior, straight (not premultiplied). Alpha 0
    /// gives an outline only — what a committed selection wants. The Quick
    /// Selection hover preview passes a translucent yellow.
    pub fill: [f32; 4],
    /// Coverage at which a pixel counts as selected.
    pub threshold: f32,
}

impl Default for SelectionOverlayStyle {
    fn default() -> Self {
        Self { phase: 0.0, dash: 8.0, fill: [0.0; 4], threshold: 0.5 }
    }
}

impl SelectionOverlayStyle {
    /// The committed selection: ants, no fill.
    pub fn ants(phase: f32) -> Self {
        Self { phase, ..Self::default() }
    }

    /// The Quick Selection hover preview: what you would get if you clicked.
    /// Yellow because that is what the reference does, and because it is the
    /// hue least likely to be confused with a selection that already exists.
    pub fn preview(phase: f32) -> Self {
        Self { phase, fill: [1.0, 0.85, 0.1, 0.38], ..Self::default() }
    }
}

/// A 256-bin histogram of the composited image.
///
/// Channels are red, green, blue and luminance, binned on **encoded** sRGB
/// values — the space a histogram is read in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Histogram {
    pub bins: [[u32; 256]; 4],
    /// Non-transparent pixels counted.
    pub total: u32,
}

impl Default for Histogram {
    fn default() -> Self {
        Self { bins: [[0; 256]; 4], total: 0 }
    }
}

impl Histogram {
    pub const RED: usize = 0;
    pub const GREEN: usize = 1;
    pub const BLUE: usize = 2;
    pub const LUMA: usize = 3;

    /// Largest bin across a channel, for scaling the plot.
    pub fn peak(&self, channel: usize) -> u32 {
        self.bins.get(channel).map(|b| b.iter().copied().max().unwrap_or(0)).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }
}

pub struct Renderer {
    gl: Rc<glow::Context>,
    shaders: ShaderLibrary,
    compute: ComputeLibrary,
    caps: Capabilities,
    /// What the driver reported at start-up, so the test toggle can restore it.
    caps_detected: Capabilities,
    /// Blur radius at or above which the compute path is used. See
    /// [`Renderer::compute_blur_min_radius`].
    compute_blur_min_radius: f32,
    /// 4 × 256 `u32` bins, reused between histogram queries.
    histogram_buffer: Option<StorageBuffer>,
    pool: TargetPool,
    vao: glow::VertexArray,
    layer_cache: HashMap<LayerId, LayerCache>,
    /// Reusable 1×N and 5×N lookup textures for curves and levels.
    lut: Option<Texture>,
    ramp: Option<Texture>,
    /// 1×1 fully-opaque white, bound where a shader expects a mask but the
    /// layer has none. Cheaper than compiling a masked and unmasked variant of
    /// every shader.
    white: Option<Texture>,
    /// See `composite.frag`: whether blend functions run on encoded values.
    pub blend_in_gamma: bool,
    pub stats: FrameStats,
}

impl Renderer {
    pub fn new(gl: Rc<glow::Context>, flavor: GlFlavor) -> Result<Self> {
        use glow::HasContext;
        let vao = unsafe { gl.create_vertex_array().map_err(GpuError::Gl)? };
        let caps = Capabilities::detect(&gl, flavor);
        log::info!("renderer capabilities: {}", caps.describe());
        let mut r = Self {
            shaders: ShaderLibrary::new(gl.clone(), flavor),
            compute: ComputeLibrary::new(gl.clone(), flavor),
            caps,
            caps_detected: caps,
            compute_blur_min_radius: default_compute_blur_min_radius(),
            histogram_buffer: None,
            pool: TargetPool::new(gl.clone()),
            vao,
            layer_cache: HashMap::new(),
            lut: None,
            ramp: None,
            white: None,
            blend_in_gamma: true,
            stats: FrameStats::default(),
            gl,
        };
        r.init_helpers()?;
        Ok(r)
    }

    fn init_helpers(&mut self) -> Result<()> {
        let white =
            Texture::new(self.gl.clone(), 1, 1, Format::R8, Filter::Linear, Wrap::Clamp)?;
        white.upload_raw(&[255])?;
        self.white = Some(white);

        self.lut = Some(Texture::new(
            self.gl.clone(),
            LUT_SIZE as u32,
            5,
            Format::R32f,
            Filter::Linear,
            Wrap::Clamp,
        )?);
        self.ramp = Some(Texture::new(
            self.gl.clone(),
            256,
            1,
            Format::Rgba8,
            Filter::Linear,
            Wrap::Clamp,
        )?);
        Ok(())
    }

    /// Compile every shader up front. Slower to start, but a driver that
    /// rejects one of them says so immediately instead of when the user
    /// reaches for that effect.
    pub fn precompile(&mut self) -> Result<usize> {
        let fragment = self.shaders.compile_all()?;
        let compute = if self.caps.compute { self.compute.compile_all()? } else { 0 };
        Ok(fragment + compute)
    }

    pub fn capabilities(&self) -> Capabilities {
        self.caps
    }

    /// Force the fragment fallback even where compute is available.
    ///
    /// Exists so the test suite can render the same scene both ways and assert
    /// they agree, which is the only way to keep the two implementations from
    /// drifting apart.
    pub fn set_compute_enabled(&mut self, enabled: bool) {
        self.caps.compute = enabled && self.caps_detected.compute;
    }

    /// Radius at or above which a blur is dispatched as a compute shader.
    ///
    /// Compute is not unconditionally faster. A dispatch has fixed overhead —
    /// pipeline switch, barrier, image binding — while the shared-memory saving
    /// grows with radius, so below some crossover the fragment path wins.
    /// Measured on llvmpipe (`examples/bench.rs`), 1024², gaussian:
    ///
    /// | radius | fragment | compute | ratio |
    /// |-------:|---------:|--------:|------:|
    /// |      8 |   370 ms |  486 ms | 0.76× |
    /// |     24 |   670 ms |  625 ms | 1.07× |
    /// |     48 |  1110 ms |  747 ms | 1.49× |
    ///
    /// A software rasteriser is the pessimistic case — it has no on-die
    /// scratchpad, so shared memory buys nothing there and the crossover sits
    /// high. Real hardware should cross over lower. The default of 12 is a
    /// deliberately conservative middle; `PIXELMAGIC_COMPUTE_BLUR_MIN`
    /// overrides it for anyone who benchmarks their own GPU.
    pub fn compute_blur_min_radius(&self) -> f32 {
        self.compute_blur_min_radius
    }

    pub fn set_compute_blur_min_radius(&mut self, radius: f32) {
        self.compute_blur_min_radius = radius.max(0.0);
    }

    /// Drop cached textures for layers that no longer exist.
    pub fn evict(&mut self, doc: &Document) {
        self.layer_cache.retain(|id, _| doc.layers.get(*id).is_some());
    }

    pub fn invalidate(&mut self, id: LayerId) {
        self.layer_cache.remove(&id);
    }

    pub fn invalidate_all(&mut self) {
        self.layer_cache.clear();
        self.pool.clear();
    }

    /// Bytes held in cached layer textures and the target pool.
    pub fn memory_estimate(&self) -> usize {
        let layers: usize = self
            .layer_cache
            .values()
            .map(|c| {
                c.texture.width as usize
                    * c.texture.height as usize
                    * c.texture.format.bytes_per_pixel()
            })
            .sum();
        layers + self.pool.bytes()
    }

    fn draw_quad(&self) {
        use glow::HasContext;
        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }

    // -- document rendering -------------------------------------------------

    /// Render the whole document into a canvas-sized target.
    pub fn render_document(
        &mut self,
        doc: &Document,
        revisions: &HashMap<LayerId, u64>,
    ) -> Result<RenderTarget> {
        self.stats = FrameStats::default();
        let (w, h) = (doc.width, doc.height);
        let mut acc = self.pool.acquire(w, h, Format::Rgba16f)?;
        acc.clear();
        acc = self.render_children(doc, None, acc, revisions)?;
        Ok(acc)
    }

    /// Composite one level of the tree onto `acc`, back to front.
    fn render_children(
        &mut self,
        doc: &Document,
        parent: Option<LayerId>,
        mut acc: RenderTarget,
        revisions: &HashMap<LayerId, u64>,
    ) -> Result<RenderTarget> {
        for id in doc.layers.render_order(parent) {
            let Some(layer) = doc.layers.get(id) else { continue };
            if layer.is_hidden() {
                self.stats.layers_skipped += 1;
                continue;
            }

            match layer.kind {
                // Adjustment and effects layers rewrite the backdrop in place.
                LayerKind::ColorAdjustments => {
                    acc = self.apply_adjustments(acc, &layer.adjustments, doc)?;
                    self.stats.layers_drawn += 1;
                    continue;
                }
                LayerKind::Effects => {
                    acc = self.apply_effects(acc, &layer.effects, doc)?;
                    self.stats.layers_drawn += 1;
                    continue;
                }
                _ => {}
            }

            let Some(content) = self.render_layer(doc, layer, revisions)? else {
                self.stats.layers_skipped += 1;
                continue;
            };
            acc = self.composite(acc, &content, layer)?;
            self.pool.release(content);
            self.stats.layers_drawn += 1;
        }
        Ok(acc)
    }

    /// Produce a canvas-sized target holding one layer's finished content:
    /// pixels placed, masked, adjusted and filtered, but not yet composited.
    fn render_layer(
        &mut self,
        doc: &Document,
        layer: &Layer,
        revisions: &HashMap<LayerId, u64>,
    ) -> Result<Option<RenderTarget>> {
        let (w, h) = (doc.width, doc.height);

        let mut target = match &layer.kind {
            LayerKind::Group => {
                let mut t = self.pool.acquire(w, h, Format::Rgba16f)?;
                t.clear();
                t = self.render_children(doc, Some(layer.id), t, revisions)?;
                t
            }
            LayerKind::Pixel { buffer } => {
                if buffer.is_empty() {
                    return Ok(None);
                }
                let revision = revisions.get(&layer.id).copied().unwrap_or(0);
                self.ensure_layer_texture(layer.id, buffer, revision)?;
                let t = self.pool.acquire(w, h, Format::Rgba16f)?;
                self.place(&t, layer, buffer.width(), buffer.height())?;
                t
            }
            // Shape, text and video layers are rasterised by the UI layer,
            // which owns Pango and cairo, and handed back as pixel content.
            // Until that path is wired up they contribute nothing rather than
            // rendering something misleading.
            LayerKind::Shape { .. } | LayerKind::Text { .. } | LayerKind::Video { .. } => {
                return Ok(None)
            }
            LayerKind::ColorAdjustments | LayerKind::Effects => return Ok(None),
        };

        if let Some(mask) = &layer.mask {
            target = self.apply_mask(target, mask, layer, doc)?;
        }
        if layer.adjustments.iter().any(|a| !a.is_noop()) {
            target = self.apply_adjustments(target, &layer.adjustments, doc)?;
        }
        if layer.effects.iter().any(|e| !e.is_noop()) {
            target = self.apply_effects(target, &layer.effects, doc)?;
        }
        Ok(Some(target))
    }

    fn ensure_layer_texture(
        &mut self,
        id: LayerId,
        buffer: &pixelmagic_core::buffer::PixelBuffer,
        revision: u64,
    ) -> Result<()> {
        let needs_upload = match self.layer_cache.get(&id) {
            Some(c) => {
                c.revision != revision
                    || c.texture.width != buffer.width()
                    || c.texture.height != buffer.height()
            }
            None => true,
        };
        if !needs_upload {
            return Ok(());
        }
        let texture = Texture::new(
            self.gl.clone(),
            buffer.width(),
            buffer.height(),
            Format::Srgb8,
            Filter::Linear,
            Wrap::Clamp,
        )?;
        texture.upload_srgb8_straight(buffer.data())?;
        self.layer_cache.insert(id, LayerCache { texture, revision });
        self.stats.uploads += 1;
        Ok(())
    }

    /// Draw a cached layer texture into a canvas-sized target under the
    /// layer's transform.
    fn place(&mut self, target: &RenderTarget, layer: &Layer, lw: u32, lh: u32) -> Result<()> {
        let handle = self
            .layer_cache
            .get(&layer.id)
            .ok_or_else(|| GpuError::Invalid("layer texture missing".into()))?
            .texture
            .handle();

        let inv = layer.transform.inverse().to_cols_array();
        let nearest = layer.transform.is_integer_translation();
        let canvas = [target.width() as f32, target.height() as f32];

        target.clear();
        let p = self.shaders.get("place")?;
        p.bind();
        p.set_texture("u_layer", 0, handle);
        p.set_mat3("u_inv_transform", &inv);
        p.set_vec2("u_canvas_size", canvas);
        p.set_vec2("u_layer_size", [lw as f32, lh as f32]);
        p.set_bool("u_nearest", nearest);
        self.draw_quad();
        self.stats.passes += 1;
        Ok(())
    }

    fn apply_mask(
        &mut self,
        src: RenderTarget,
        mask: &Mask,
        layer: &Layer,
        doc: &Document,
    ) -> Result<RenderTarget> {
        let Mask::Bitmap { buffer, offset, inverted, opacity, density, .. } = mask else {
            // Vector masks need path rasterisation, which lives in the UI
            // layer; leaving the layer unmasked is the honest fallback.
            return Ok(src);
        };

        let tex = Texture::new(
            self.gl.clone(),
            buffer.width(),
            buffer.height(),
            Format::R8,
            Filter::Linear,
            Wrap::Clamp,
        )?;
        tex.upload_raw(buffer.data())?;

        let dst = self.pool.acquire(doc.width, doc.height, Format::Rgba16f)?;
        dst.bind();

        let placement =
            layer.transform.then(&pixelmagic_core::geom::Transform::translate(*offset));
        let inv = placement.inverse().to_cols_array();
        let src_handle = src.texture.handle();

        let p = self.shaders.get("mask_apply")?;
        p.bind();
        p.set_texture("u_image", 0, src_handle);
        p.set_texture("u_mask", 1, tex.handle());
        p.set_mat3("u_inv_transform", &inv);
        p.set_vec2("u_canvas_size", [doc.width as f32, doc.height as f32]);
        p.set_vec2("u_mask_size", [buffer.width() as f32, buffer.height() as f32]);
        p.set_bool("u_inverted", *inverted);
        p.set_f32("u_opacity", *opacity);
        p.set_f32("u_density", *density);
        self.draw_quad();
        self.stats.passes += 1;

        self.pool.release(src);
        Ok(dst)
    }

    fn composite(
        &mut self,
        backdrop: RenderTarget,
        source: &RenderTarget,
        layer: &Layer,
    ) -> Result<RenderTarget> {
        // `Normal` at full opacity with nothing else going on is by far the
        // most common case, but it still needs a pass because we cannot write
        // into the texture we are reading from. Ping-pong through the pool.
        let dst = self.pool.acquire(backdrop.width(), backdrop.height(), Format::Rgba16f)?;
        dst.bind();

        let (bh, sh) = (backdrop.texture.handle(), source.texture.handle());
        let white = self.white_handle()?;
        let gamma = if self.blend_in_gamma { 1.0 } else { 0.0 };
        let mode = layer.blend_mode.shader_index() as i32;
        let opacity = layer.opacity;

        let p = self.shaders.get("composite")?;
        p.bind();
        p.set_texture("u_backdrop", 0, bh);
        p.set_texture("u_source", 1, sh);
        p.set_texture("u_mask", 2, white);
        p.set_bool("u_use_mask", false);
        p.set_i32("u_blend_mode", mode);
        p.set_f32("u_opacity", opacity);
        p.set_f32("u_blend_gamma", gamma);
        self.draw_quad();
        self.stats.passes += 1;

        self.pool.release(backdrop);
        Ok(dst)
    }

    fn white_handle(&self) -> Result<glow::Texture> {
        self.white
            .as_ref()
            .map(|t| t.handle())
            .ok_or_else(|| GpuError::Invalid("helper textures not initialised".into()))
    }

    // -- adjustment and effect chains ---------------------------------------

    fn apply_adjustments(
        &mut self,
        mut target: RenderTarget,
        adjustments: &[AdjustmentInstance],
        doc: &Document,
    ) -> Result<RenderTarget> {
        for inst in adjustments {
            if inst.is_noop() {
                continue;
            }
            target = self.apply_adjustment(target, &inst.adjustment, doc)?;
        }
        Ok(target)
    }

    fn apply_adjustment(
        &mut self,
        src: RenderTarget,
        adj: &Adjustment,
        doc: &Document,
    ) -> Result<RenderTarget> {
        let (w, h) = (src.width(), src.height());
        let aspect = aspect_of(w, h);

        // Adjustments that need a blurred reference produce it first.
        let helper = match adj {
            Adjustment::Basic(b) if b.clarity != 0.0 || b.texture != 0.0 => {
                Some((self.blur(&src, 24.0)?, self.blur(&src, 4.0)?))
            }
            Adjustment::Sharpen(s) => {
                let b = self.blur(&src, s.radius.max(0.5))?;
                Some((b, self.pool.acquire(1, 1, Format::Rgba16f)?))
            }
            _ => None,
        };

        let dst = self.pool.acquire(w, h, Format::Rgba16f)?;
        dst.bind();
        let src_handle = src.texture.handle();

        match adj {
            Adjustment::Basic(b) => {
                let (coarse, fine) = helper
                    .as_ref()
                    .map(|(a, b)| (a.texture.handle(), b.texture.handle()))
                    .unwrap_or((src_handle, src_handle));
                let p = self.shaders.get("adjust.basic")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_texture("u_blur_coarse", 1, coarse);
                p.set_texture("u_blur_fine", 2, fine);
                p.set_f32("u_exposure", b.exposure);
                p.set_f32("u_highlights", b.highlights);
                p.set_f32("u_shadows", b.shadows);
                p.set_f32("u_brightness", b.brightness);
                p.set_f32("u_contrast", b.contrast);
                p.set_f32("u_black_point", b.black_point);
                p.set_f32("u_texture", b.texture);
                p.set_f32("u_clarity", b.clarity);
            }
            Adjustment::WhiteBalance(a) => {
                let p = self.shaders.get("adjust.white_balance")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_temperature", a.temperature);
                p.set_f32("u_tint", a.tint);
            }
            Adjustment::HueSaturation(a) => {
                let p = self.shaders.get("adjust.hue_saturation")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_hue", a.hue);
                p.set_f32("u_saturation", a.saturation);
                p.set_f32("u_vibrance", a.vibrance);
            }
            Adjustment::BlackAndWhite(a) => {
                let p = self.shaders.get("adjust.black_white")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_red", a.red);
                p.set_f32("u_green", a.green);
                p.set_f32("u_blue", a.blue);
                p.set_f32("u_tone", a.tone);
                p.set_f32("u_intensity", a.intensity);
            }
            Adjustment::Invert(a) => {
                let p = self.shaders.get("adjust.invert")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_intensity", a.intensity);
            }
            Adjustment::Vignette(a) => {
                let p = self.shaders.get("adjust.vignette")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_exposure", a.exposure);
                p.set_f32("u_black_point", a.black_point);
                p.set_f32("u_softness", a.softness);
                p.set_vec2("u_aspect", aspect);
            }
            Adjustment::Grain(a) => {
                let p = self.shaders.get("adjust.grain")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_size_px", [w as f32, h as f32]);
                p.set_f32("u_size", a.size);
                p.set_f32("u_intensity", a.intensity);
            }
            Adjustment::Sharpen(a) => {
                let blur =
                    helper.as_ref().map(|(b, _)| b.texture.handle()).unwrap_or(src_handle);
                let p = self.shaders.get("adjust.sharpen")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_texture("u_blur", 1, blur);
                p.set_f32("u_intensity", a.intensity);
                p.set_bool("u_luminance_only", false);
            }
            Adjustment::ChannelMixer(a) => {
                // glam's Mat3 is column-major, and GLSL multiplies
                // matrix * vector, so each *column* here is one output row.
                let m = [
                    a.rows[0].red,
                    a.rows[1].red,
                    a.rows[2].red,
                    a.rows[0].green,
                    a.rows[1].green,
                    a.rows[2].green,
                    a.rows[0].blue,
                    a.rows[1].blue,
                    a.rows[2].blue,
                ];
                let c = [a.rows[0].constant, a.rows[1].constant, a.rows[2].constant];
                let p = self.shaders.get("adjust.channel_mixer")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_mat3("u_matrix", &m);
                p.set_vec3("u_constant", c);
            }
            Adjustment::ColorBalance(a) => {
                let wheels = match a.mode {
                    BalanceMode::Master => [&a.master, &a.master, &a.master],
                    BalanceMode::ThreeWay => [&a.shadows, &a.midtones, &a.highlights],
                };
                let mut lift = [0.0f32; 12];
                let mut sat = [0.0f32; 3];
                for (i, wheel) in wheels.iter().enumerate() {
                    // The wheel handle and the complementary sliders both feed
                    // the same RGB lift.
                    lift[i * 4] = wheel.offset_x + wheel.red_cyan;
                    lift[i * 4 + 1] = wheel.offset_y + wheel.green_magenta;
                    lift[i * 4 + 2] = -wheel.offset_x - wheel.offset_y + wheel.yellow_blue;
                    lift[i * 4 + 3] = wheel.brightness;
                    sat[i] = wheel.saturation;
                    if a.mode == BalanceMode::Master {
                        break;
                    }
                }
                let master = a.mode == BalanceMode::Master;
                let p = self.shaders.get("adjust.color_balance")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec4_array("u_lift", &lift);
                p.set_vec3("u_saturation", sat);
                p.set_bool("u_master", master);
            }
            Adjustment::SelectiveColor(a) => {
                let mut bands = [0.0f32; 24];
                for (i, b) in a.bands.iter().enumerate() {
                    bands[i * 3] = b.hue;
                    bands[i * 3 + 1] = b.saturation;
                    bands[i * 3 + 2] = b.brightness;
                }
                let centers = pixelmagic_core::adjust::SELECTIVE_COLOR_HUES;
                let p = self.shaders.get("adjust.selective_color")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec3_array("u_bands", &bands);
                p.set_f32_array("u_centers", &centers);
            }
            Adjustment::ReplaceColor(a) => {
                let p = self.shaders.get("adjust.replace_color")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec3("u_source", [a.source.r, a.source.g, a.source.b]);
                p.set_vec3("u_target", [a.target.r, a.target.g, a.target.b]);
                p.set_f32("u_range", a.range);
                p.set_f32("u_intensity", a.intensity);
            }
            Adjustment::Levels(a) => {
                let mut data = vec![0.0f32; LUT_SIZE * 5];
                for ch in ToneChannel::LEVELS {
                    let c = a.channel(ch);
                    for i in 0..LUT_SIZE {
                        let v = i as f32 / (LUT_SIZE - 1) as f32;
                        data[ch.index() * LUT_SIZE + i] = c.apply(v);
                    }
                }
                let lum_only = a.active == ToneChannel::Luminance;
                self.upload_lut(&data)?;
                let lut = self.lut_handle()?;
                let p = self.shaders.get("adjust.levels")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_texture("u_lut", 1, lut);
                p.set_bool("u_luminance_only", lum_only);
            }
            Adjustment::Curves(a) => {
                let mut data = vec![0.0f32; LUT_SIZE * 5];
                for ch in ToneChannel::CURVES {
                    let baked = a.channel(ch).to_lut();
                    data[ch.index() * LUT_SIZE..(ch.index() + 1) * LUT_SIZE]
                        .copy_from_slice(&baked);
                }
                // Leave the unused luminance row as identity.
                for i in 0..LUT_SIZE {
                    data[4 * LUT_SIZE + i] = i as f32 / (LUT_SIZE - 1) as f32;
                }
                self.upload_lut(&data)?;
                let lut = self.lut_handle()?;
                let p = self.shaders.get("adjust.curves")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_texture("u_lut", 1, lut);
            }
            // Not yet wired to a shader; pass through unchanged rather than
            // producing something wrong.
            Adjustment::SelectiveClarity(_) | Adjustment::Lut(_) => {
                self.pool.release(dst);
                let _ = doc;
                if let Some((a, b)) = helper {
                    self.pool.release(a);
                    self.pool.release(b);
                }
                return Ok(src);
            }
        }

        self.draw_quad();
        self.stats.passes += 1;

        if let Some((a, b)) = helper {
            self.pool.release(a);
            self.pool.release(b);
        }
        self.pool.release(src);
        Ok(dst)
    }

    fn apply_effects(
        &mut self,
        mut target: RenderTarget,
        effects: &[Effect],
        doc: &Document,
    ) -> Result<RenderTarget> {
        for effect in effects {
            if effect.is_noop() {
                continue;
            }
            target = self.apply_effect(target, effect, doc)?;
        }
        Ok(target)
    }

    fn apply_effect(
        &mut self,
        src: RenderTarget,
        effect: &Effect,
        _doc: &Document,
    ) -> Result<RenderTarget> {
        let (w, h) = (src.width(), src.height());
        let size = [w as f32, h as f32];
        let aspect = aspect_of(w, h);
        let f = |key: &str| effect.get(key).and_then(|v| v.as_f32()).unwrap_or(0.0);
        let point =
            |key: &str| effect.get(key).and_then(|v| v.as_point()).unwrap_or(Vec2::splat(0.5));
        let color = |key: &str| {
            effect
                .get(key)
                .and_then(|v| v.as_color())
                .map(|c| c.to_array())
                .unwrap_or([0.0, 0.0, 0.0, 1.0])
        };
        let flag = |key: &str| matches!(effect.get(key), Some(ParamValue::Bool(true)));

        // Effects that need a blurred copy build it before taking a target.
        let blur_helper = match effect.id.as_str() {
            "sharpen" | "sharpen-luminance" => Some(self.blur(&src, f("radius").max(0.5))?),
            "bloom" | "gloom" => Some(self.blur(&src, f("radius"))?),
            "high-pass" | "low-pass" => Some(self.blur(&src, f("radius"))?),
            "tilt-shift" | "focus-blur" => Some(self.blur(&src, f("radius").max(8.0))?),
            _ => None,
        };

        // Separable blurs run their own two passes and return early.
        match effect.id.as_str() {
            "gaussian-blur" | "box-blur" => {
                let kernel = if effect.id == "box-blur" { 1 } else { 0 };
                let out = self.blur_kernel(&src, f("radius"), kernel)?;
                self.pool.release(src);
                return Ok(out);
            }
            "motion-blur" => {
                let out = self.blur_directional(&src, f("radius"), f("angle").to_radians())?;
                self.pool.release(src);
                return Ok(out);
            }
            _ => {}
        }

        let dst = self.pool.acquire(w, h, Format::Rgba16f)?;
        dst.bind();
        let src_handle = src.texture.handle();
        let helper_handle =
            blur_helper.as_ref().map(|t| t.texture.handle()).unwrap_or(src_handle);

        match effect.id.as_str() {
            "disc-blur" => {
                let p = self.shaders.get("effect.blur_disc")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_texel", [1.0 / size[0], 1.0 / size[1]]);
                p.set_f32("u_radius", f("radius"));
            }
            "zoom-blur" | "spin-blur" => {
                let spin = effect.id == "spin-blur";
                let c = point("center");
                let p = self.shaders.get("effect.blur_radial")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_center", [c.x, c.y]);
                p.set_vec2("u_aspect", aspect);
                p.set_f32("u_amount", f("amount"));
                p.set_bool("u_spin", spin);
            }
            "tilt-shift" | "focus-blur" => {
                let radial = effect.id == "focus-blur";
                let c = point("center");
                let angle = f("angle").to_radians();
                let p = self.shaders.get("effect.blur_mask")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_texture("u_blur", 1, helper_handle);
                p.set_vec2("u_center", [c.x, c.y]);
                p.set_vec2("u_aspect", aspect);
                p.set_f32("u_transition", f("transition"));
                p.set_f32("u_angle", angle);
                p.set_bool("u_radial", radial);
            }
            "bump-distort" | "pinch-distort" | "twirl-distort" => {
                let mode = match effect.id.as_str() {
                    "bump-distort" => 0,
                    "pinch-distort" => 1,
                    _ => 2,
                };
                let c = point("center");
                let amount = if mode == 2 { f("angle").to_radians() } else { f("scale") };
                let p = self.shaders.get("effect.distort")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_center", [c.x, c.y]);
                p.set_vec2("u_size_px", size);
                p.set_f32("u_radius", f("radius"));
                p.set_f32("u_amount", amount);
                p.set_i32("u_mode", mode);
            }
            "kaleidoscope" => {
                let c = point("center");
                let p = self.shaders.get("effect.kaleidoscope")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_center", [c.x, c.y]);
                p.set_vec2("u_aspect", aspect);
                p.set_f32("u_angle", f("angle").to_radians());
                p.set_f32("u_count", f("count").max(2.0));
            }
            "sharpen" | "sharpen-luminance" => {
                let lum = effect.id == "sharpen-luminance";
                let intensity = if lum { f("sharpness") } else { f("intensity") };
                let p = self.shaders.get("adjust.sharpen")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_texture("u_blur", 1, helper_handle);
                p.set_f32("u_intensity", intensity);
                p.set_bool("u_luminance_only", lum);
            }
            "exposure-effect" => {
                let p = self.shaders.get("effect.exposure")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_ev", f("ev"));
            }
            "color-controls" => {
                let p = self.shaders.get("effect.color_controls")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_saturation", f("saturation"));
                p.set_f32("u_brightness", f("brightness"));
                p.set_f32("u_contrast", f("contrast"));
            }
            "hue-adjust" => {
                let p = self.shaders.get("effect.hue_adjust")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_angle", f("angle"));
            }
            "sepia-tone" | "color-monochrome" | "false-color" => {
                let (shadow, highlight, intensity) = match effect.id.as_str() {
                    "sepia-tone" => ([0.17, 0.09, 0.03], [1.0, 0.90, 0.70], f("intensity")),
                    "color-monochrome" => {
                        let c = color("color");
                        ([0.0, 0.0, 0.0], [c[0], c[1], c[2]], f("intensity"))
                    }
                    _ => {
                        let a = color("color0");
                        let b = color("color1");
                        ([a[0], a[1], a[2]], [b[0], b[1], b[2]], 1.0)
                    }
                };
                let p = self.shaders.get("effect.tint")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec3("u_shadow_color", shadow);
                p.set_vec3("u_highlight_color", highlight);
                p.set_f32("u_intensity", intensity);
            }
            "invert-effect" => {
                let p = self.shaders.get("adjust.invert")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_intensity", 1.0);
            }
            "threshold" => {
                let p = self.shaders.get("effect.threshold")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_threshold", f("threshold"));
            }
            "posterize" => {
                let p = self.shaders.get("effect.posterize")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_levels", f("levels"));
            }
            "pixelate" => {
                let p = self.shaders.get("effect.pixelate")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_size_px", size);
                p.set_f32("u_scale", f("scale"));
            }
            "crystallize" => {
                let p = self.shaders.get("effect.crystallize")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_size_px", size);
                p.set_f32("u_radius", f("radius"));
            }
            "vignette-effect" => {
                let p = self.shaders.get("adjust.vignette")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_f32("u_exposure", f("intensity"));
                p.set_f32("u_black_point", 0.0);
                p.set_f32("u_softness", f("falloff"));
                p.set_vec2("u_aspect", aspect);
            }
            "grain-effect" => {
                let p = self.shaders.get("adjust.grain")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_size_px", size);
                p.set_f32("u_size", f("size").max(0.05));
                p.set_f32("u_intensity", f("intensity"));
            }
            "noise-effect" => {
                let mono = flag("monochrome");
                let p = self.shaders.get("effect.noise")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_size_px", size);
                p.set_f32("u_amount", f("amount"));
                p.set_bool("u_monochrome", mono);
            }
            "bloom" | "gloom" => {
                let gloom = effect.id == "gloom";
                let p = self.shaders.get("effect.bloom")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_texture("u_blur", 1, helper_handle);
                p.set_f32("u_intensity", f("intensity"));
                p.set_bool("u_gloom", gloom);
            }
            "dot-screen" | "line-screen" | "hatched-screen" | "circular-screen" => {
                let mode = match effect.id.as_str() {
                    "dot-screen" => 0,
                    "line-screen" => 1,
                    "hatched-screen" => 2,
                    _ => 3,
                };
                let c = point("center");
                let p = self.shaders.get("effect.halftone")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_size_px", size);
                p.set_vec2("u_center", [c.x, c.y]);
                p.set_f32("u_width", f("width"));
                p.set_f32("u_sharpness", f("sharpness"));
                p.set_f32("u_angle", f("angle").to_radians());
                p.set_i32("u_mode", mode);
            }
            "checkerboard" | "stripes" => {
                let stripes = effect.id == "stripes";
                let c = color("color");
                let p = self.shaders.get("effect.checkerboard")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_size_px", size);
                p.set_vec4("u_color", c);
                p.set_f32("u_width", f("width"));
                p.set_f32("u_sharpness", f("sharpness"));
                p.set_f32("u_angle", f("angle").to_radians());
                p.set_f32("u_opacity", f("opacity"));
                p.set_bool("u_stripes", stripes);
            }
            "clouds" => {
                let c = color("color");
                let p = self.shaders.get("effect.clouds")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_vec2("u_size_px", size);
                p.set_vec4("u_color", c);
                p.set_f32("u_width", f("width"));
                p.set_f32("u_opacity", f("opacity"));
            }
            "fill-color" | "fill-gradient" => {
                let gradient = effect.id == "fill-gradient";
                let c = color("color");
                let ramp = self.ramp_handle()?;
                let p = self.shaders.get("effect.fill_color")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_texture("u_ramp", 1, ramp);
                p.set_vec4("u_color", c);
                p.set_f32("u_opacity", f("opacity"));
                p.set_f32("u_angle", f("angle").to_radians());
                p.set_f32("u_scale", f("scale").max(0.01));
                p.set_i32("u_type", if gradient { 1 } else { 0 });
            }
            "gradient-map" => {
                let ramp = self.ramp_handle()?;
                let p = self.shaders.get("effect.gradient_map")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_texture("u_ramp", 1, ramp);
                p.set_f32("u_opacity", f("opacity"));
            }
            "high-pass" | "low-pass" => {
                let high = effect.id == "high-pass";
                let p = self.shaders.get("effect.frequency")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
                p.set_texture("u_blur", 1, helper_handle);
                p.set_f32("u_opacity", f("opacity"));
                p.set_bool("u_high", high);
            }
            "mask-to-alpha" => {
                let p = self.shaders.get("effect.mask_to_alpha")?;
                p.bind();
                p.set_texture("u_image", 0, src_handle);
            }
            _ => {
                // Catalogued but not implemented. `Effect::is_noop` already
                // filters these out, so reaching here means the registry and
                // this match have drifted apart.
                log::warn!("effect `{}` has no renderer branch", effect.id);
                self.pool.release(dst);
                if let Some(t) = blur_helper {
                    self.pool.release(t);
                }
                return Ok(src);
            }
        }

        self.draw_quad();
        self.stats.passes += 1;

        if let Some(t) = blur_helper {
            self.pool.release(t);
        }
        self.pool.release(src);
        Ok(dst)
    }

    // -- blur helpers -------------------------------------------------------

    fn blur(&mut self, src: &RenderTarget, radius: f32) -> Result<RenderTarget> {
        self.blur_kernel(src, radius, 0)
    }

    fn blur_kernel(
        &mut self,
        src: &RenderTarget,
        radius: f32,
        kernel: i32,
    ) -> Result<RenderTarget> {
        if self.caps.compute && radius >= self.compute_blur_min_radius {
            return self.blur_kernel_compute(src, radius, kernel);
        }
        self.blur_kernel_fragment(src, radius, kernel)
    }

    /// Shared-memory blur. Both axes are still separate passes — that is what
    /// makes a blur O(r) instead of O(r²) — but each pass now reads the image
    /// once into shared memory instead of once per tap.
    fn blur_kernel_compute(
        &mut self,
        src: &RenderTarget,
        radius: f32,
        kernel: i32,
    ) -> Result<RenderTarget> {
        let (w, h) = (src.width(), src.height());
        let tmp = self.pool.acquire(w, h, Format::Rgba16f)?;
        let out = self.pool.acquire(w, h, Format::Rgba16f)?;

        // Horizontal: workgroups tile along x, one row of groups per image row.
        {
            let (s_tex, d_tex) = (src.texture.handle(), tmp.texture.handle());
            let p = self.compute.get("blur_separable")?;
            p.bind();
            p.set_texture("u_src", 0, s_tex);
            p.bind_image(0, d_tex, glow::RGBA16F, true);
            p.set_ivec2("u_size", [w as i32, h as i32]);
            p.set_ivec2("u_direction", [1, 0]);
            p.set_f32("u_radius", radius);
            p.set_i32("u_kernel", kernel);
            p.dispatch_covering(w, h, (128, 1));
            p.barrier_image_to_texture();
        }

        // Vertical: the roles of the two dispatch axes swap.
        {
            let (s_tex, d_tex) = (tmp.texture.handle(), out.texture.handle());
            let p = self.compute.get("blur_separable")?;
            p.bind();
            p.set_texture("u_src", 0, s_tex);
            p.bind_image(0, d_tex, glow::RGBA16F, true);
            p.set_ivec2("u_size", [w as i32, h as i32]);
            p.set_ivec2("u_direction", [0, 1]);
            p.set_f32("u_radius", radius);
            p.set_i32("u_kernel", kernel);
            p.dispatch_covering(h, w, (128, 1));
            p.barrier_image_to_texture();
        }

        self.stats.dispatches += 2;
        self.pool.release(tmp);
        Ok(out)
    }

    fn blur_kernel_fragment(
        &mut self,
        src: &RenderTarget,
        radius: f32,
        kernel: i32,
    ) -> Result<RenderTarget> {
        let (w, h) = (src.width(), src.height());
        let texel = [1.0 / w as f32, 1.0 / h as f32];

        let tmp = self.pool.acquire(w, h, Format::Rgba16f)?;
        tmp.bind();
        let src_handle = src.texture.handle();
        {
            let p = self.shaders.get("effect.blur_separable")?;
            p.bind();
            p.set_texture("u_image", 0, src_handle);
            p.set_vec2("u_texel", texel);
            p.set_vec2("u_direction", [1.0, 0.0]);
            p.set_f32("u_radius", radius);
            p.set_i32("u_kernel", kernel);
        }
        self.draw_quad();

        let out = self.pool.acquire(w, h, Format::Rgba16f)?;
        out.bind();
        let tmp_handle = tmp.texture.handle();
        {
            let p = self.shaders.get("effect.blur_separable")?;
            p.bind();
            p.set_texture("u_image", 0, tmp_handle);
            p.set_vec2("u_texel", texel);
            p.set_vec2("u_direction", [0.0, 1.0]);
            p.set_f32("u_radius", radius);
            p.set_i32("u_kernel", kernel);
        }
        self.draw_quad();

        self.stats.passes += 2;
        self.pool.release(tmp);
        Ok(out)
    }

    /// Single-pass box blur along an arbitrary direction — motion blur.
    fn blur_directional(
        &mut self,
        src: &RenderTarget,
        radius: f32,
        angle: f32,
    ) -> Result<RenderTarget> {
        let (w, h) = (src.width(), src.height());
        let out = self.pool.acquire(w, h, Format::Rgba16f)?;
        out.bind();
        let src_handle = src.texture.handle();
        let p = self.shaders.get("effect.blur_separable")?;
        p.bind();
        p.set_texture("u_image", 0, src_handle);
        p.set_vec2("u_texel", [1.0 / w as f32, 1.0 / h as f32]);
        p.set_vec2("u_direction", [angle.cos(), angle.sin()]);
        p.set_f32("u_radius", radius);
        p.set_i32("u_kernel", 1);
        self.draw_quad();
        self.stats.passes += 1;
        Ok(out)
    }

    fn upload_lut(&mut self, data: &[f32]) -> Result<()> {
        self.lut
            .as_ref()
            .ok_or_else(|| GpuError::Invalid("LUT texture missing".into()))?
            .upload_f32(data)
    }

    fn lut_handle(&self) -> Result<glow::Texture> {
        self.lut
            .as_ref()
            .map(|t| t.handle())
            .ok_or_else(|| GpuError::Invalid("LUT texture missing".into()))
    }

    fn ramp_handle(&self) -> Result<glow::Texture> {
        self.ramp
            .as_ref()
            .map(|t| t.handle())
            .ok_or_else(|| GpuError::Invalid("ramp texture missing".into()))
    }

    /// Upload a gradient ramp for the next fill or gradient-map pass.
    pub fn set_ramp(&mut self, stops: &[[f32; 4]]) -> Result<()> {
        let mut data = Vec::with_capacity(256 * 4);
        for i in 0..256 {
            let s = stops.get(i * stops.len() / 256).copied().unwrap_or([0.0, 0.0, 0.0, 1.0]);
            for c in s {
                data.push((c.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        self.ramp
            .as_ref()
            .ok_or_else(|| GpuError::Invalid("ramp texture missing".into()))?
            .upload_raw(&data)
    }

    /// Read a rendered target back as straight-alpha sRGB8.
    ///
    /// Goes through a shader encode pass into an 8-bit target and reads that,
    /// rather than asking for floats directly. See `encode_srgb.frag` for why:
    /// float ReadPixels is not portable to GLES, and this is.
    pub fn read_image(&mut self, image: &RenderTarget) -> Result<Vec<u8>> {
        let (w, h) = (image.width(), image.height());
        let encoded = self.pool.acquire(w, h, Format::Rgba8)?;
        encoded.bind();

        let handle = image.texture.handle();
        let p = self.shaders.get("encode_srgb")?;
        p.bind();
        p.set_texture("u_image", 0, handle);
        self.draw_quad();
        self.stats.passes += 1;

        let bytes = encoded.read_rgba8();
        self.pool.release(encoded);
        bytes
    }

    /// Compute a 256-bin histogram of a rendered target.
    ///
    /// Uses a compute shader with per-workgroup shared bins where available.
    /// The CPU fallback reads the frame back and bins it in software: slower by
    /// a wide margin, but it means the Levels and Curves panes still show a
    /// histogram on a driver without compute rather than showing nothing.
    pub fn histogram(&mut self, image: &RenderTarget) -> Result<Histogram> {
        if self.caps.compute {
            self.histogram_compute(image)
        } else {
            self.histogram_cpu(image)
        }
    }

    fn histogram_compute(&mut self, image: &RenderTarget) -> Result<Histogram> {
        const BINS: usize = 256 * 4;
        if self.histogram_buffer.is_none() {
            self.histogram_buffer = Some(StorageBuffer::new(self.gl.clone(), BINS * 4)?);
        }
        let buffer = self.histogram_buffer.as_ref().expect("just created");
        buffer.clear_to_zero();
        buffer.bind(0);

        let (w, h) = (image.width(), image.height());
        let tex = image.texture.handle();
        {
            let p = self.compute.get("histogram")?;
            p.bind();
            p.set_texture("u_src", 0, tex);
            p.set_ivec2("u_size", [w as i32, h as i32]);
            p.dispatch_covering(w, h, (16, 16));
            p.barrier_storage();
        }
        self.stats.dispatches += 1;

        let raw = self.histogram_buffer.as_ref().expect("just created").read_u32();

        let mut hist = Histogram::default();
        for channel in 0..4 {
            for bin in 0..256 {
                hist.bins[channel][bin] = raw.get(channel * 256 + bin).copied().unwrap_or(0);
            }
        }
        // Every counted pixel contributes exactly once to each channel, so any
        // one channel's sum is the pixel count.
        hist.total = hist.bins[Histogram::LUMA].iter().sum();
        Ok(hist)
    }

    fn histogram_cpu(&mut self, image: &RenderTarget) -> Result<Histogram> {
        let pixels = self.read_image(image)?;
        let mut hist = Histogram::default();
        for px in pixels.chunks_exact(4) {
            if px[3] == 0 {
                continue;
            }
            hist.bins[Histogram::RED][px[0] as usize] += 1;
            hist.bins[Histogram::GREEN][px[1] as usize] += 1;
            hist.bins[Histogram::BLUE][px[2] as usize] += 1;
            let luma = 0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32;
            hist.bins[Histogram::LUMA][(luma.round() as usize).min(255)] += 1;
            hist.total += 1;
        }
        Ok(hist)
    }

    /// Draw a finished document target to a framebuffer.
    ///
    /// `target` is `None` for the window's default framebuffer. It matters that
    /// this is a parameter: GTK's `GLArea` does not render to framebuffer 0, it
    /// renders to one it owns and then composites, so hard-coding 0 here would
    /// draw the canvas into the void.
    pub fn present(
        &mut self,
        image: &RenderTarget,
        viewport: (i32, i32, i32, i32),
        show_checker: bool,
        target: Option<glow::Framebuffer>,
    ) -> Result<()> {
        use glow::HasContext;
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, target);
            self.gl.viewport(viewport.0, viewport.1, viewport.2, viewport.3);
        }
        let handle = image.texture.handle();
        let vp = [viewport.2 as f32, viewport.3 as f32];
        let p = self.shaders.get("present")?;
        p.bind();
        p.set_texture("u_image", 0, handle);
        p.set_f32("u_checker_size", 8.0);
        p.set_vec2("u_viewport", vp);
        p.set_bool("u_show_checker", show_checker);
        self.draw_quad();
        self.stats.passes += 1;
        Ok(())
    }

    /// Blur the framebuffer behind a set of rectangles, in place.
    ///
    /// This is the frosted-glass effect under the floating panels. It has to
    /// happen here rather than in the toolkit: GTK4 has no `backdrop-filter`,
    /// and a translucent widget just shows the pixels underneath unaltered.
    /// The GNOME "Blur My Shell" extension does not reach it either — that
    /// blurs what is behind a *window*, and this is compositing inside one.
    ///
    /// Rectangles are in top-left-origin widget pixels, matching how the UI
    /// lays panels out; the conversion to GL's bottom-left origin happens here
    /// so no caller has to think about it. Call it after [`Renderer::present`]
    /// and before GTK draws the panel widgets over the top.
    ///
    /// `scale` divides the snapshot before blurring — 4 means a quarter-size
    /// blur, which is invisible at these radii and sixteen times cheaper.
    pub fn blur_backdrop(
        &mut self,
        widget: (i32, i32),
        rects: &[BackdropRect],
        style: BackdropStyle,
        target: Option<glow::Framebuffer>,
    ) -> Result<()> {
        use glow::HasContext;

        let (fw, fh) = widget;
        if rects.is_empty() || fw <= 0 || fh <= 0 || style.opacity <= 0.0 {
            return Ok(());
        }

        let scale = style.scale.max(1);
        let (sw, sh) = ((fw as u32 / scale).max(1), (fh as u32 / scale).max(1));

        // Snapshot what has been drawn so far. `blit_framebuffer` rather than
        // `copy_tex_sub_image_2d` because only the blit can rescale, and doing
        // the downsample here means the blur never touches full resolution.
        let snapshot = self.pool.acquire(sw, sh, Format::Rgba8)?;
        unsafe {
            self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, target);
            self.gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(snapshot.framebuffer()));
            self.gl.blit_framebuffer(
                0,
                0,
                fw,
                fh,
                0,
                0,
                sw as i32,
                sh as i32,
                glow::COLOR_BUFFER_BIT,
                glow::LINEAR,
            );
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, target);
        }

        // The radius is in widget pixels, so it shrinks with the snapshot.
        let blurred = self.blur(&snapshot, (style.radius / scale as f32).max(0.5))?;
        self.pool.release(snapshot);

        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, target);
            self.gl.viewport(0, 0, fw, fh);
            self.gl.enable(glow::BLEND);
            // Premultiplied source, which is what the shader writes.
            self.gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        }

        let handle = blurred.texture.handle();
        for rect in rects {
            if rect.width <= 0.0 || rect.height <= 0.0 {
                continue;
            }
            let p = self.shaders.get("backdrop")?;
            p.bind();
            p.set_texture("u_image", 0, handle);
            p.set_vec2("u_resolution", [fw as f32, fh as f32]);
            p.set_vec4(
                "u_rect",
                [
                    rect.x,
                    // Top-left origin in, bottom-left origin out.
                    fh as f32 - rect.y - rect.height,
                    rect.width,
                    rect.height,
                ],
            );
            p.set_f32("u_corner", style.corner);
            p.set_vec4("u_tint", style.tint);
            p.set_f32("u_opacity", style.opacity);
            self.draw_quad();
            self.stats.passes += 1;
        }

        unsafe { self.gl.disable(glow::BLEND) };
        self.pool.release(blurred);
        Ok(())
    }

    /// Draw a selection overlay: marching ants, optionally over a tinted fill.
    ///
    /// `viewport` is the document's rectangle on screen, in the same
    /// bottom-left-origin device pixels [`Renderer::present`] takes, so the
    /// overlay lands exactly on the image it describes at any zoom or pan.
    ///
    /// The mask is a single-channel texture at *document* resolution; the
    /// shader turns it into a screen-space outline, which is why this cannot
    /// simply be another effect pass in the graph — it has to run after the
    /// document has been placed on screen and know where that was.
    pub fn draw_selection_overlay(
        &mut self,
        mask: &Texture,
        viewport: (i32, i32, i32, i32),
        style: SelectionOverlayStyle,
        target: Option<glow::Framebuffer>,
    ) -> Result<()> {
        use glow::HasContext;
        if viewport.2 <= 0 || viewport.3 <= 0 {
            return Ok(());
        }
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, target);
            self.gl.viewport(viewport.0, viewport.1, viewport.2, viewport.3);
            self.gl.enable(glow::BLEND);
            // Premultiplied, matching what the shader writes.
            self.gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
        }

        // Device pixels per document pixel, taken from the viewport rather
        // than passed in: the two can only disagree if a caller computes zoom
        // differently from how it sized the viewport, and then the ants would
        // sit at the wrong thickness with nothing to explain why.
        let scale = viewport.2 as f32 / (mask.width.max(1)) as f32;

        let handle = mask.handle();
        let p = self.shaders.get("selection_overlay")?;
        p.bind();
        p.set_texture("u_mask", 0, handle);
        p.set_vec2("u_doc_size", [mask.width as f32, mask.height as f32]);
        p.set_f32("u_scale", scale);
        p.set_f32("u_phase", style.phase);
        p.set_f32("u_dash", style.dash);
        p.set_vec4("u_fill", style.fill);
        p.set_f32("u_threshold", style.threshold);
        self.draw_quad();
        self.stats.passes += 1;

        unsafe { self.gl.disable(glow::BLEND) };
        Ok(())
    }

    /// The GL context this renderer was built on, so a host can create
    /// textures that will be valid to pass back in — a selection mask, for
    /// instance. Sharing the handle is the point: a texture made on any other
    /// context would be rejected here.
    pub fn context(&self) -> Rc<glow::Context> {
        self.gl.clone()
    }

    /// An 8-bit target from the pool, for a caller that needs somewhere to
    /// present into that behaves like the window's framebuffer. Tests use this
    /// to stand in for the one GTK owns.
    pub fn acquire_rgba8(&mut self, width: u32, height: u32) -> Result<RenderTarget> {
        self.pool.acquire(width, height, Format::Rgba8)
    }

    pub fn release(&mut self, target: RenderTarget) {
        self.pool.release(target);
    }

    /// The framebuffer currently bound for drawing.
    ///
    /// GTK's `GLArea` renders into a framebuffer it owns and then composites
    /// that into the window, so "draw to the screen" is not framebuffer zero.
    /// The host asks for this before rendering and hands it back to
    /// [`Renderer::present`], which is the only way to put the canvas where
    /// GTK expects it.
    pub fn current_framebuffer(&self) -> Option<glow::Framebuffer> {
        use glow::HasContext;
        let id = unsafe { self.gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) };
        std::num::NonZeroU32::new(id as u32).map(glow::NativeFramebuffer)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        use glow::HasContext;
        unsafe { self.gl.delete_vertex_array(self.vao) };
    }
}

/// Default crossover radius for the compute blur, overridable by environment
/// variable so a user can tune it to their own hardware without a rebuild.
fn default_compute_blur_min_radius() -> f32 {
    std::env::var("PIXELMAGIC_COMPUTE_BLUR_MIN")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .unwrap_or(12.0)
}

/// Aspect correction so radial effects stay circular on non-square canvases.
fn aspect_of(w: u32, h: u32) -> [f32; 2] {
    if w >= h {
        [w as f32 / h as f32, 1.0]
    } else {
        [1.0, h as f32 / w as f32]
    }
}

/// Compile-time sanity: the shader's blend-mode branches must cover the enum.
const _: () = {
    assert!(BlendMode::ALL.len() == 26);
};

/// Effects whose category means they replace rather than filter their input.
pub fn is_generator(category: EffectCategory) -> bool {
    matches!(category, EffectCategory::Generator | EffectCategory::Fill)
}

/// Convert a `glam::Mat3` to the column-major array GL wants.
pub fn mat3_to_array(m: Mat3) -> [f32; 9] {
    m.to_cols_array()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_is_one_for_square() {
        assert_eq!(aspect_of(100, 100), [1.0, 1.0]);
    }

    #[test]
    fn aspect_favours_the_long_axis() {
        assert_eq!(aspect_of(200, 100), [2.0, 1.0]);
        assert_eq!(aspect_of(100, 200), [1.0, 2.0]);
    }

    #[test]
    fn generator_categories() {
        assert!(is_generator(EffectCategory::Generator));
        assert!(is_generator(EffectCategory::Fill));
        assert!(!is_generator(EffectCategory::Blur));
    }
}
