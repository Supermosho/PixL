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
}

pub struct Renderer {
    gl: Rc<glow::Context>,
    shaders: ShaderLibrary,
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
        let mut r = Self {
            shaders: ShaderLibrary::new(gl.clone(), flavor),
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
        self.shaders.compile_all()
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
